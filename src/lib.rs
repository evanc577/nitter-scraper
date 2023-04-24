mod error;
mod parse;
mod tweet;

use std::collections::VecDeque;

use error::NitterError;
use futures_util::Stream;
use parse::parse_nitter_html;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::header::COOKIE;
use reqwest::Client;
use tweet::Tweet;
use typed_builder::TypedBuilder;

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
