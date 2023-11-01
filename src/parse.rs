use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use time::format_description::well_known::Rfc2822;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::PrimitiveDateTime;

use crate::error::NitterError;
use crate::nitter_scraper::NitterCursor;
use crate::tweet::{Stats, Tweet, User};

pub fn parse_nitter_html(html: String) -> Result<(Vec<Tweet>, NitterCursor), NitterError> {
    static TWEET_SELECTOR: Lazy<Selector> = Lazy::new(|| {
        Selector::parse(".timeline-item:not(.show-more):not(.unavailable):not(.threadunavailable)")
            .unwrap()
    });

    let mut document = Html::parse_document(&html);

    // Check if user is protected
    if parse_protected(document.root_element()) {
        return Err(NitterError::ProtectedAccount);
    }

    // Check if user is suspended
    if parse_suspended(document.root_element()) {
        return Err(NitterError::SuspendedAccount);
    }

    // Check if user not found
    if parse_not_found(document.root_element()) {
        return Err(NitterError::NotFound);
    }

    // Remove all quotes
    static QUOTE_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".quote > *:not(.quote-link)").unwrap());
    let ids: Vec<_> = document
        .select(&QUOTE_SELECTOR)
        .map(|p_node| p_node.id())
        .collect();
    for id in ids {
        document.tree.get_mut(id).unwrap().detach();
    }

    let mut tweets = vec![];
    for element in document.select(&TWEET_SELECTOR) {
        tweets.push(parse_tweet(element)?);
    }

    // Parse pagination cursor
    let cursor = parse_cursor(document.root_element());

    Ok((tweets, cursor))
}

pub fn parse_nitter_single(html: String) -> Result<(Tweet, NitterCursor), NitterError> {
    let mut document = Html::parse_document(&html);

    // Remove all quotes
    static QUOTE_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".quote > *:not(.quote-link)").unwrap());
    let ids: Vec<_> = document
        .select(&QUOTE_SELECTOR)
        .map(|p_node| p_node.id())
        .collect();
    for id in ids {
        document.tree.get_mut(id).unwrap().detach();
    }

    let main_tweet = main_tweet(document.root_element());

    Ok((parse_tweet(main_tweet)?, NitterCursor::End))
}

fn parse_tweet(element: ElementRef) -> Result<Tweet, NitterError> {
    // Parse individual tweets
    let full_name = parse_tweet_full_name(element)?;
    let screen_name = parse_tweet_screen_name(element)?;
    let id_str = parse_tweet_id_str(element)?;
    let id = id_str
        .parse()
        .map_err(|_| NitterError::Parse(format!("invalid id {:?}", id_str)))?;
    let full_text = parse_tweet_body(element)?;
    let links = parse_links(element)?;
    let images = parse_tweet_images(element);
    let (created_at, created_at_ts) = parse_tweet_time(element)?;
    let retweet = parse_tweet_retweet(element);
    let reply = parse_tweet_reply(element);
    let quote = parse_tweet_quote(element);
    let pinned = parse_tweet_pinned(element);
    let stats = Stats {
        comment: parse_tweet_stat(element, TweetStat::Comment),
        retweet: parse_tweet_stat(element, TweetStat::Retweet),
        quote: parse_tweet_stat(element, TweetStat::Quote),
        heart: parse_tweet_stat(element, TweetStat::Heart),
    };

    Ok(Tweet {
        id,
        id_str,
        created_at,
        created_at_ts,
        full_text,
        links,
        images,
        retweet,
        reply,
        quote,
        pinned,
        user: User {
            screen_name,
            full_name,
        },
        stats,
    })
}

fn main_tweet(element: ElementRef) -> ElementRef {
    static MAIN_TWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("div.main-tweet > .timeline-item").unwrap());
    element.select(&MAIN_TWEET_SELECTOR).next().unwrap()
}

fn parse_protected(element: ElementRef) -> bool {
    static PROTECTED_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("div.timeline-protected").unwrap());

    element.select(&PROTECTED_SELECTOR).next().is_some()
}

static ERROR_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("div.error-panel").unwrap());

fn parse_suspended(element: ElementRef) -> bool {
    element
        .select(&ERROR_SELECTOR)
        .next()
        .and_then(|element| element.text().next())
        .map(|text| text.contains("has been suspended"))
        .eq(&Some(true))
}

fn parse_not_found(element: ElementRef) -> bool {
    element
        .select(&ERROR_SELECTOR)
        .next()
        .and_then(|element| element.text().next())
        .map(|text| text.contains("not found"))
        .eq(&Some(true))
}

static TWEET_LINK_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".tweet-date > a").unwrap());
static TWEET_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/(?P<screen_name>\w+)/status/(?P<id>\d+)").unwrap());

fn parse_tweet_full_name(element: ElementRef) -> Result<String, NitterError> {
    static FULLNAME_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("a.fullname").unwrap());
    element
        .select(&FULLNAME_SELECTOR)
        .next()
        .and_then(|fullname_link_element| fullname_link_element.value().attr("title"))
        .map(|fullname| fullname.to_owned())
        .ok_or_else(|| NitterError::Parse("missing full_name".into()))
}

fn parse_tweet_screen_name(element: ElementRef) -> Result<String, NitterError> {
    element
        .select(&TWEET_LINK_SELECTOR)
        .next()
        .and_then(|tweet_link_element| tweet_link_element.value().attr("href"))
        .and_then(|tweet_link| TWEET_LINK_RE.captures(tweet_link))
        .and_then(|caps| caps.name("screen_name"))
        .map(|cap| cap.as_str().to_owned())
        .ok_or_else(|| NitterError::Parse("missing screen_name".into()))
}

fn parse_tweet_id_str(element: ElementRef) -> Result<String, NitterError> {
    element
        .select(&TWEET_LINK_SELECTOR)
        .next()
        .and_then(|tweet_link_element| tweet_link_element.value().attr("href"))
        .and_then(|tweet_link| TWEET_LINK_RE.captures(tweet_link))
        .and_then(|caps| caps.name("id"))
        .map(|cap| cap.as_str().to_owned())
        .ok_or_else(|| NitterError::Parse("missing id".into()))
}

static TWEET_BODY_SELECTOR: Lazy<Selector> =
    Lazy::new(|| Selector::parse(".tweet-content").unwrap());

fn parse_tweet_body(element: ElementRef) -> Result<String, NitterError> {
    let full_text = element
        .select(&TWEET_BODY_SELECTOR)
        .next()
        .ok_or_else(|| NitterError::Parse("missing body".into()))?
        .text()
        .collect();
    Ok(full_text)
}

fn parse_links(element: ElementRef) -> Result<Vec<String>, NitterError> {
    static LINK_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse("a").unwrap());

    let links = element
        .select(&TWEET_BODY_SELECTOR)
        .next()
        .ok_or_else(|| NitterError::Parse("missing body".into()))?
        .select(&LINK_SELECTOR)
        .filter_map(|l| l.value().attr("href"))
        .filter(|l| !l.starts_with('/'))
        .map(|l| l.to_owned())
        .collect();
    Ok(links)
}

fn parse_tweet_images(element: ElementRef) -> Vec<String> {
    static IMAGES_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".attachment.image a.still-image").unwrap());
    static IMAGE_ID_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"^/pic/\w+/media%2F(?P<url>[\w\-]+\.\w+)$").unwrap());

    let images: Vec<_> = element
        .select(&IMAGES_SELECTOR)
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

fn parse_tweet_time(element: ElementRef) -> Result<(String, i64), NitterError> {
    static TWEET_DATE_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse("span.tweet-date a").unwrap());
    static TIME_FORMAT_DESCRIPTION: &[FormatItem<'_>] = format_description!(
        "[month repr:short] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period] UTC"
    );

    let time = element
        .select(&TWEET_DATE_SELECTOR)
        .next()
        .and_then(|tweet_date_element| tweet_date_element.value().attr("title"))
        .and_then(|time_str| PrimitiveDateTime::parse(time_str, TIME_FORMAT_DESCRIPTION).ok())
        .map(|time| time.assume_utc());

    if let Some(t) = time {
        Ok((t.format(&Rfc2822).unwrap(), t.unix_timestamp()))
    } else {
        Err(NitterError::Parse("missing time".into()))
    }
}

fn parse_tweet_retweet(element: ElementRef) -> bool {
    static RETWEET_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".retweet-header").unwrap());

    element.select(&RETWEET_SELECTOR).next().is_some()
}

fn parse_tweet_pinned(element: ElementRef) -> bool {
    static PINNED_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".pinned").unwrap());

    element.select(&PINNED_SELECTOR).next().is_some()
}

fn parse_tweet_reply(element: ElementRef) -> bool {
    static REPLY_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".replying-to").unwrap());

    element.select(&REPLY_SELECTOR).next().is_some()
}

fn parse_tweet_quote(element: ElementRef) -> bool {
    static QUOTE_SELECTOR: Lazy<Selector> = Lazy::new(|| Selector::parse(".quote").unwrap());

    element.select(&QUOTE_SELECTOR).next().is_some()
}

enum TweetStat {
    Comment,
    Retweet,
    Quote,
    Heart,
}

impl TweetStat {
    fn selector(&self) -> &'static Lazy<Selector> {
        static COMMENT_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse(".icon-comment").unwrap());
        static RETWEET_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse(".icon-retweet").unwrap());
        static QUOTE_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse(".icon-quote").unwrap());
        static HEART_SELECTOR: Lazy<Selector> =
            Lazy::new(|| Selector::parse(".icon-heart").unwrap());
        match self {
            Self::Comment => &COMMENT_SELECTOR,
            Self::Retweet => &RETWEET_SELECTOR,
            Self::Quote => &QUOTE_SELECTOR,
            Self::Heart => &HEART_SELECTOR,
        }
    }
}

fn parse_tweet_stat(element: ElementRef, stat: TweetStat) -> u64 {
    static TWEET_STAT_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".tweet-stat > .icon-container").unwrap());
    for e in element.select(&TWEET_STAT_SELECTOR) {
        if e.select(stat.selector()).next().is_some() {
            return e
                .text()
                .next()
                .and_then(|t| t.trim().replace(',', "").parse().ok())
                .unwrap_or(0);
        }
    }
    0
}

fn parse_cursor(element: ElementRef) -> NitterCursor {
    static CURSOR_SELECTOR: Lazy<Selector> =
        Lazy::new(|| Selector::parse(".show-more:not(.timeline-item) a").unwrap());

    let cursor = element
        .select(&CURSOR_SELECTOR)
        .last()
        .and_then(|cursor_element| cursor_element.value().attr("href"))
        .map(|cursor| cursor.to_owned());
    match cursor {
        Some(c) => NitterCursor::More(c),
        None => NitterCursor::End,
    }
}
