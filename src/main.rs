use clap::Parser;
use futures_util::StreamExt;
use nitter_scraper::{NitterQuery, NitterScraper};
use reqwest::Client;

#[derive(Parser)]
struct Args {
    /// Nitter instance URL
    instance: String,

    /// Max number of tweets to return
    #[arg(short, long)]
    limit: Option<usize>,

    /// Should reorder pinned tweet to chronological order
    #[arg(long)]
    reorder_pinned: bool,

    /// Skip retweets
    #[arg(long)]
    skip_retweets: bool,

    /// Minimum tweet ID to return
    #[arg(short, long)]
    min_id: Option<u128>,

    #[command(subcommand)]
    query: NitterQuery,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let client = Client::new();
    let mut nitter_scraper = NitterScraper::builder()
        .client(&client)
        .instance(args.instance)
        .query(args.query)
        .reorder_pinned(args.reorder_pinned)
        .skip_retweets(args.skip_retweets)
        .limit(args.limit)
        .min_id(args.min_id)
        .build();
    let nitter_search = nitter_scraper.search().await;
    futures_util::pin_mut!(nitter_search);

    while let Some(tweet_result) = nitter_search.next().await {
        let tweet = tweet_result.unwrap();
        println!("{}", serde_json::to_string(&tweet).unwrap());
    }
}
