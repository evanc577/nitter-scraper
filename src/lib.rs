mod error;
mod parse;
mod tweet;

use std::collections::VecDeque;
use std::time::Duration;

#[cfg(feature = "bin")]
use clap::Subcommand;
use error::NitterError;
use futures_util::Stream;
use parse::parse_nitter_html;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::header::COOKIE;
use reqwest::{Client, StatusCode};
use tweet::Tweet;
use typed_builder::TypedBuilder;

#[derive(TypedBuilder)]
pub struct NitterScraper<'a> {
    client: &'a Client,

    #[builder(setter(into))]
    instance: String,

    query: NitterQuery,

    #[builder(default)]
    limit: Option<usize>,

    #[builder(default)]
    reorder_pinned: bool,

    #[builder(default)]
    skip_retweets: bool,

    #[builder(default)]
    min_id: Option<u128>,

    #[builder(setter(skip), default)]
    state: NitterSearchState,
}

#[derive(Debug, Default)]
struct NitterSearchState {
    tweets: VecDeque<Tweet>,
    cursor: NitterCursor,
    count: usize,
    errored: bool,
    pinned: Option<Tweet>,
}

#[derive(Debug)]
pub enum NitterCursor {
    Initial,
    More(String),
    End,
}

impl Default for NitterCursor {
    fn default() -> Self {
        Self::Initial
    }
}

#[cfg_attr(feature = "bin", derive(Subcommand))]
pub enum NitterQuery {
    Search { query: String },
    User { user: String },
    UserWithReplies { user: String },
    UserMedia { user: String },
    UserSearch { user: String, query: String },
}

impl NitterQuery {
    fn encode_get_params(&self) -> String {
        match self {
            Self::Search { query } => {
                let encoded = utf8_percent_encode(query, NON_ALPHANUMERIC);
                format!("?f=tweets&q={}", encoded)
            }
            Self::User { .. } => "".into(),
            Self::UserWithReplies { .. } => "".into(),
            Self::UserMedia { .. } => "".into(),
            Self::UserSearch { query, .. } => {
                let encoded = utf8_percent_encode(query, NON_ALPHANUMERIC);
                format!("?f=tweets&q={}", encoded)
            }
        }
    }

    fn url_path(&self) -> String {
        match self {
            Self::Search { .. } => "/search".into(),
            Self::User { user } => format!("/{}", user),
            Self::UserWithReplies { user } => format!("/{}/with_replies", user),
            Self::UserMedia { user } => format!("/{}/media", user),
            Self::UserSearch { user, .. } => format!("/{}/search", user),
        }
    }
}

enum ReturnedTweet {
    Pinned,
    Normal,
    None,
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

            // Since skip-retweets may cause entire page to be empty, loop until cursor doesn't
            // exist anymore
            loop {
                // Return tweet if available
                if let Some(tweet) = state.state.tweets.iter().next() {
                    match Self::should_return_tweet(
                        tweet,
                        &state.state.pinned,
                        state.min_id,
                        state.reorder_pinned,
                    ) {
                        ReturnedTweet::Normal => {
                            state.state.count += 1;
                            return Some((Ok(state.state.tweets.pop_front().unwrap()), state));
                        }
                        ReturnedTweet::Pinned => {
                            state.state.count += 1;
                            return Some((Ok(state.state.pinned.take().unwrap()), state));
                        }
                        ReturnedTweet::None => break,
                    }
                }

                if let NitterCursor::End = state.state.cursor {
                    break;
                }

                // Scrape nitter
                match state.scrape_page().await {
                    Ok(tweets) => {
                        state.state.tweets.extend(tweets.into_iter());
                    }
                    Err(
                        NitterError::ProtectedAccount
                        | NitterError::SuspendedAccount
                        | NitterError::NotFound,
                    ) => {
                        return None;
                    }
                    Err(e) => {
                        state.state.errored = true;
                        return Some((Err(e), state));
                    }
                }
            }

            // Return pinned tweet if needed
            if let Some(t) = state.state.pinned.take() {
                return Some((Ok(t), state));
            }

            None
        })
    }

    fn should_return_tweet(
        tweet: &Tweet,
        pinned: &Option<Tweet>,
        min_id: Option<u128>,
        reorder_pinned: bool,
    ) -> ReturnedTweet {
        if reorder_pinned {
            if let Some(p) = pinned {
                // Should use tweet id here but nitter doesn't expose it for retweets
                if p.created_at_ts > tweet.created_at_ts {
                    return ReturnedTweet::Pinned;
                }
            }
        }

        // Stop if minimum tweet id reached
        if let Some(min_id) = min_id {
            if tweet.id < min_id {
                return ReturnedTweet::None;
            }
        }

        // Return next tweet
        ReturnedTweet::Normal
    }

    async fn scrape_page(&mut self) -> Result<Vec<Tweet>, NitterError> {
        // Use cursor if it exists
        let get_params = match self.state.cursor {
            NitterCursor::Initial => self.query.encode_get_params(),
            NitterCursor::More(ref c) => c.clone(),
            NitterCursor::End => return Ok(vec![]),
        };

        // Send request
        let url = format!("{}{}{}", self.instance, self.query.url_path(), get_params);
        let mut i = 0;
        let response = loop {
            let response = self
                .client
                .get(&url)
                .header(COOKIE, "replaceTwitter=; replaceYouTube=; replaceReddit=")
                .send()
                .await
                .map_err(|e| NitterError::Network(e.to_string()))?;

            if response.status() == StatusCode::TOO_MANY_REQUESTS {
                // Retry if 429
                if i < 10 {
                    i += 1;
                    let sleep_s = std::cmp::min(300, 1 << i);
                    eprintln!(
                        "Received status code {}, sleeping for {} seconds",
                        response.status().as_u16(),
                        sleep_s
                    );
                    tokio::time::sleep(Duration::from_secs(sleep_s)).await;
                    continue;
                } else {
                    return Err(NitterError::Network(format!(
                        "received status code {}",
                        response.status().as_u16()
                    )));
                }
            } else if response.status() == StatusCode::NOT_FOUND {
                // Return nothing on 404
                return Err(NitterError::NotFound);
            } else if !response.status().is_success() {
                // Error if bad status code
                return Err(NitterError::Network(format!(
                    "received status code {}",
                    response.status().as_u16()
                )));
            }

            break response;
        };

        let text = response.text().await.unwrap();

        // Parse html and update cursor
        let (tweets, cursor) = parse_nitter_html(text)?;
        self.state.cursor = cursor;

        let tweets = if self.skip_retweets {
            // Filter out retweets
            tweets.into_iter().filter(|t| !t.retweet).collect()
        } else {
            tweets
        };

        if self.reorder_pinned {
            // Extract pinned tweet
            let (mut pinned, unpinned): (Vec<_>, Vec<_>) =
                tweets.into_iter().partition(|t| t.pinned);
            if let Some(t) = pinned.pop() {
                if let Some(min_id) = self.min_id {
                    if t.id >= min_id {
                        self.state.pinned = Some(t);
                    }
                } else {
                    self.state.pinned = Some(t);
                }
            }
            Ok(unpinned)
        } else {
            Ok(tweets)
        }
    }
}
