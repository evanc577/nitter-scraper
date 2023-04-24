mod error;
mod tweet;

use std::collections::VecDeque;

use error::NitterError;
use futures_util::Stream;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use regex::Regex;
use reqwest::header::COOKIE;
use reqwest::Client;
use scraper::{Html, Selector};
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::PrimitiveDateTime;
use tweet::Tweet;
use typed_builder::TypedBuilder;

use crate::tweet::User;

#[derive(TypedBuilder)]
pub struct NitterScraper<'a> {
    client: &'a Client,

    #[builder(setter(into))]
    instance: String,

    #[builder(setter(into))]
    query: String,

    #[builder(setter(strip_option), default)]
    limit: Option<usize>,

    #[builder(setter(strip_option), default)]
    min_id: Option<u128>,

    #[builder(setter(skip), default)]
    state: NitterSearchState,
}

#[derive(Default, Debug)]
struct NitterSearchState {
    tweets: VecDeque<Tweet>,
    cursor: Option<String>,
    count: usize,
    errored: bool,
}

impl<'a> NitterScraper<'a> {
    pub async fn search(&'a mut self) -> impl Stream<Item = Result<Tweet, NitterError>> + '_ {
        // Reset internal state
        self.state = Default::default();

        futures_util::stream::unfold(self, |state| async {
            // Stop if previously errored
            if state.state.errored {
                return None;
            }

            // Stop if limit reached
            if let Some(limit) = state.limit {
                if state.state.count >= limit {
                    return None;
                }
            }

            let should_return_tweet = |tweet: Tweet, min_id| {
                // Stop if minimum tweet id reached
                if let Some(min_id) = min_id {
                    if tweet.id < min_id {
                        return None;
                    }
                }

                // Return next tweet
                Some(Ok(tweet))
            };

            // Try returning next tweet if available
            if let Some(tweet) = state.state.tweets.pop_front() {
                if let Some(r) = should_return_tweet(tweet, state.min_id) {
                    state.state.count += 1;
                    return Some((r, state));
                }
            }

            // Scrape nitter
            match state.scrape_page().await {
                Ok(tweets) => {
                    state.state.tweets.extend(tweets.into_iter());
                }
                Err(e) => {
                    state.state.errored = true;
                    return Some((Err(e), state));
                }
            }

            // Try returning next tweet if available
            if let Some(tweet) = state.state.tweets.pop_front() {
                if let Some(r) = should_return_tweet(tweet, state.min_id) {
                    state.state.count += 1;
                    return Some((r, state));
                }
            }

            None
        })
    }

    async fn scrape_page(&mut self) -> Result<Vec<Tweet>, NitterError> {
        // Use cursor if it exists
        let get_params = match self.state.cursor {
            Some(ref c) => c.clone(),
            None => {
                let encoded = utf8_percent_encode(&self.query, NON_ALPHANUMERIC);
                format!("?f=tweets&q={}", encoded)
            }
        };

        // Send request
        let url = format!("{}/search{}", self.instance, get_params);
        let response = self
            .client
            .get(url)
            .header(COOKIE, "replaceTwitter=; replaceYouTube=; replaceReddit=")
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let text = response.text().await.unwrap();

        // Parse html and update cursor
        let (tweets, cursor) = parse_nitter_html(text).unwrap();
        self.state.cursor = Some(cursor);

        Ok(tweets)
    }
}

fn parse_nitter_html(html: String) -> Result<(Vec<Tweet>, String), NitterError> {
    let document = Html::parse_document(&html);
    let tweet_selector = Selector::parse(".timeline-item:not(.show-more)").unwrap();
    let tweet_link_selector = Selector::parse("a.tweet-link").unwrap();
    let tweet_link_re = Regex::new(r"^/(?P<screen_name>\w+)/status/(?P<id>\d+)").unwrap();
    let tweet_body_selector = Selector::parse(".tweet-content").unwrap();
    let images_selector = Selector::parse(".attachment.image a.still-image").unwrap();
    let image_id_re = Regex::new(r"^/pic/\w+/media%2F(?P<url>\w+\.\w+)$").unwrap();
    let tweet_date_selector = Selector::parse("span.tweet-date a").unwrap();
    let time_format_description = format_description!(
        "[month repr:short] [day padding:none], [year] · [hour repr:12 padding:none]:[minute] [period] UTC"
    );

    let mut tweets = vec![];

    for element in document.select(&tweet_selector) {
        // Parse tweet author screen_name and tweet id
        let (screen_name, id_str) = {
            let tweet_link_element = element.select(&tweet_link_selector).next().unwrap();
            let tweet_link = tweet_link_element.value().attr("href").unwrap();
            let caps = tweet_link_re.captures(tweet_link).unwrap();
            let screen_name = caps.name("screen_name").unwrap().as_str();
            let id_str = caps.name("id").unwrap().as_str();
            (screen_name, id_str)
        };
        let id = id_str.parse().unwrap();

        // Parse tweet body
        let full_text: String = element
            .select(&tweet_body_selector)
            .next()
            .unwrap()
            .text()
            .into_iter()
            .collect();

        // Parse images
        let images: Vec<_> = element
            .select(&images_selector)
            .into_iter()
            .filter_map(|e| {
                let link = e.value().attr("href").unwrap();
                match image_id_re.captures(link) {
                    Some(caps) => Some(format!(
                        "https://pbs.twimg.com/media/{}",
                        caps.name("url").unwrap().as_str()
                    )),
                    None => None,
                }
            })
            .collect();

        // Parse date
        let created_at = {
            let tweet_date_element = element.select(&tweet_date_selector).next().unwrap();
            let time_str = tweet_date_element.value().attr("title").unwrap();
            let time = PrimitiveDateTime::parse(time_str, time_format_description).unwrap();
            let time = time.assume_utc();
            time.format(&Rfc3339).unwrap()
        };

        tweets.push(Tweet {
            id,
            id_str: id_str.to_owned(),
            created_at,
            full_text: full_text.to_owned(),
            images,
            user: User {
                screen_name: screen_name.to_owned(),
            },
        })
    }

    // Parse cursor
    let cursor_selector = Selector::parse(".show-more a").unwrap();
    let cursor = document
        .select(&cursor_selector)
        .last()
        .unwrap()
        .value()
        .attr("href")
        .unwrap()
        .to_owned();

    Ok((tweets, cursor))
}
