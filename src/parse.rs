use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use time::format_description::well_known::Rfc3339;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::PrimitiveDateTime;

use crate::error::NitterError;
use crate::tweet::{Tweet, User};

pub fn parse_nitter_html(html: String) -> Result<(Vec<Tweet>, String), NitterError> {
    static TWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".timeline-item:not(.show-more)").unwrap());

    let mut tweets = vec![];

    let document = Html::parse_document(&html);
    for element in document.select(&TWEET_SELECTOR) {
        // Parse individual tweets
        let screen_name = parse_tweet_screen_name(&element)?;
        let id_str = parse_tweet_id_str(&element)?;
        let id = id_str.parse().unwrap();
        let full_text = parse_tweet_body(&element)?;
        let images = parse_tweet_images(&element)?;
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

static TWEET_LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a.tweet-link").unwrap());
static TWEET_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/(?P<screen_name>\w+)/status/(?P<id>\d+)").unwrap());

fn parse_tweet_screen_name(element: &ElementRef) -> Result<String, NitterError> {
    let tweet_link_element = element.select(&TWEET_LINK_SELECTOR).next().unwrap();
    let tweet_link = tweet_link_element.value().attr("href").unwrap();
    let caps = TWEET_LINK_RE.captures(tweet_link).unwrap();
    let screen_name = caps.name("screen_name").unwrap().as_str();
    Ok(screen_name.to_owned())
}

fn parse_tweet_id_str(element: &ElementRef) -> Result<String, NitterError> {
    let tweet_link_element = element.select(&TWEET_LINK_SELECTOR).next().unwrap();
    let tweet_link = tweet_link_element.value().attr("href").unwrap();
    let caps = TWEET_LINK_RE.captures(tweet_link).unwrap();
    let id = caps.name("id").unwrap().as_str();
    Ok(id.to_owned())
}

fn parse_tweet_body(element: &ElementRef) -> Result<String, NitterError> {
    static TWEET_BODY_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".tweet-content").unwrap());

    let full_text: String = element
        .select(&TWEET_BODY_SELECTOR)
        .next()
        .unwrap()
        .text()
        .into_iter()
        .collect();
    Ok(full_text)
}

fn parse_tweet_images(element: &ElementRef) -> Result<Vec<String>, NitterError> {
    static IMAGES_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".attachment.image a.still-image").unwrap());
    static IMAGE_ID_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^/pic/\w+/media%2F(?P<url>\w+\.\w+)$").unwrap());

    let images: Vec<_> = element
        .select(&IMAGES_SELECTOR)
        .into_iter()
        .filter_map(|e| {
            let link = e.value().attr("href").unwrap();
            match IMAGE_ID_RE.captures(link) {
                Some(caps) => Some(format!(
                    "https://pbs.twimg.com/media/{}",
                    caps.name("url").unwrap().as_str()
                )),
                None => None,
            }
        })
        .collect();
    Ok(images)
}

fn parse_tweet_time(element: &ElementRef) -> Result<String, NitterError> {
    static TWEET_DATE_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("span.tweet-date a").unwrap());
    static TIME_FORMAT_DESCRIPTION: &[FormatItem<'_>] = format_description!(
        "[month repr:short] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period] UTC"
    );

    let created_at = {
        let tweet_date_element = element.select(&TWEET_DATE_SELECTOR).next().unwrap();
        let time_str = tweet_date_element.value().attr("title").unwrap();
        let time = PrimitiveDateTime::parse(time_str, TIME_FORMAT_DESCRIPTION).unwrap();
        let time = time.assume_utc();
        time.format(&Rfc3339).unwrap()
    };
    Ok(created_at)
}

fn parse_tweet_retweet(element: &ElementRef) -> bool {
    static RETWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".retweet-header").unwrap());

    element.select(&RETWEET_SELECTOR).next().is_some()
}

fn parse_tweet_pinned(element: &ElementRef) -> bool {
    static PINNED_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".pinned").unwrap());

    element.select(&PINNED_SELECTOR).next().is_some()
}

fn parse_cursor(element: &ElementRef) -> Result<String, NitterError> {
    static CURSOR_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".show-more a").unwrap());

    let cursor = element
        .select(&CURSOR_SELECTOR)
        .last()
        .unwrap()
        .value()
        .attr("href")
        .unwrap()
        .to_owned();
    Ok(cursor)
}
