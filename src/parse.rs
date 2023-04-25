use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use time::format_description::well_known::Rfc2822;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::PrimitiveDateTime;

use crate::error::NitterError;
use crate::tweet::{Tweet, User};

pub fn parse_nitter_html(html: String) -> Result<(Vec<Tweet>, String), NitterError> {
    static TWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".timeline-item:not(.show-more)").unwrap());

    let document = Html::parse_document(&html);

    // Check if user is protected
    if parse_protected(&document.root_element()) {
        return Err(NitterError::ProtectedAccount);
    }

    // Check if user is suspended
    if parse_suspended(&document.root_element()) {
        return Err(NitterError::SuspendedAccount);
    }

    let mut tweets = vec![];
    for element in document.select(&TWEET_SELECTOR) {
        // Parse individual tweets
        let screen_name = parse_tweet_screen_name(&element)?;
        let id_str = parse_tweet_id_str(&element)?;
        let id = id_str
            .parse()
            .map_err(|_| NitterError::Parse(format!("invalid id {:?}", id_str)))?;
        let full_text = parse_tweet_body(&element)?;
        let images = parse_tweet_images(&element);
        let created_at = parse_tweet_time(&element)?;
        let retweet = parse_tweet_retweet(&element);
        let pinned = parse_tweet_pinned(&element);

        tweets.push(Tweet {
            id,
            id_str,
            created_at,
            full_text,
            images,
            retweet,
            pinned,
            user: User { screen_name },
        })
    }

    // Parse pagination cursor
    let cursor = parse_cursor(&document.root_element())?;

    Ok((tweets, cursor))
}

fn parse_protected(element: &ElementRef) -> bool {
    static PROTECTED_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("div.timeline-protected").unwrap());

    element.select(&PROTECTED_SELECTOR).next().is_some()
}

fn parse_suspended(element: &ElementRef) -> bool {
    static ERROR_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("div.error-panel").unwrap());

    element
        .select(&ERROR_SELECTOR)
        .next()
        .and_then(|element| element.text().next())
        .and_then(|text| Some(text.contains("has been suspended")))
        .eq(&Some(true))
}

static TWEET_LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a.tweet-link").unwrap());
static TWEET_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/(?P<screen_name>\w+)/status/(?P<id>\d+)").unwrap());

fn parse_tweet_screen_name(element: &ElementRef) -> Result<String, NitterError> {
    element
        .select(&TWEET_LINK_SELECTOR)
        .next()
        .and_then(|tweet_link_element| tweet_link_element.value().attr("href"))
        .and_then(|tweet_link| TWEET_LINK_RE.captures(tweet_link))
        .and_then(|caps| caps.name("screen_name"))
        .and_then(|cap| Some(cap.as_str().to_owned()))
        .ok_or_else(|| NitterError::Parse("missing screen_name".into()))
}

fn parse_tweet_id_str(element: &ElementRef) -> Result<String, NitterError> {
    element
        .select(&TWEET_LINK_SELECTOR)
        .next()
        .and_then(|tweet_link_element| tweet_link_element.value().attr("href"))
        .and_then(|tweet_link| TWEET_LINK_RE.captures(tweet_link))
        .and_then(|caps| caps.name("id"))
        .and_then(|cap| Some(cap.as_str().to_owned()))
        .ok_or_else(|| NitterError::Parse("missing id".into()))
}

fn parse_tweet_body(element: &ElementRef) -> Result<String, NitterError> {
    static TWEET_BODY_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".tweet-content").unwrap());

    let full_text: String = element
        .select(&TWEET_BODY_SELECTOR)
        .next()
        .ok_or_else(|| NitterError::Parse("missing body".into()))?
        .text()
        .into_iter()
        .collect();
    Ok(full_text)
}

fn parse_tweet_images(element: &ElementRef) -> Vec<String> {
    static IMAGES_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".attachment.image a.still-image").unwrap());
    static IMAGE_ID_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^/pic/\w+/media%2F(?P<url>\w+\.\w+)$").unwrap());

    let images: Vec<_> = element
        .select(&IMAGES_SELECTOR)
        .into_iter()
        .filter_map(|e| {
            let link = e.value().attr("href")?;
            match IMAGE_ID_RE.captures(link) {
                Some(caps) => Some(format!(
                    "https://pbs.twimg.com/media/{}",
                    caps.name("url")?.as_str()
                )),
                None => None,
            }
        })
        .collect();
    images
}

fn parse_tweet_time(element: &ElementRef) -> Result<String, NitterError> {
    static TWEET_DATE_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("span.tweet-date a").unwrap());
    static TIME_FORMAT_DESCRIPTION: &[FormatItem<'_>] = format_description!(
        "[month repr:short] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period] UTC"
    );

    element
        .select(&TWEET_DATE_SELECTOR)
        .next()
        .and_then(|tweet_date_element| tweet_date_element.value().attr("title"))
        .and_then(|time_str| PrimitiveDateTime::parse(time_str, TIME_FORMAT_DESCRIPTION).ok())
        .and_then(|time| time.assume_utc().format(&Rfc2822).ok())
        .ok_or_else(|| NitterError::Parse("missing time".into()))
}

fn parse_tweet_retweet(element: &ElementRef) -> bool {
    static RETWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".retweet-header").unwrap());

    element.select(&RETWEET_SELECTOR).next().is_some()
}

fn parse_tweet_pinned(element: &ElementRef) -> bool {
    static PINNED_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".pinned").unwrap());

    element.select(&PINNED_SELECTOR).next().is_some()
}

fn parse_cursor(element: &ElementRef) -> Result<String, NitterError> {
    static CURSOR_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".show-more a").unwrap());

    element
        .select(&CURSOR_SELECTOR)
        .last()
        .and_then(|cursor_element| cursor_element.value().attr("href"))
        .and_then(|cursor| Some(cursor.to_owned()))
        .ok_or_else(|| NitterError::Parse("missing cursor".into()))
}
