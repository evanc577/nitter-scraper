use std::io::Write;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use futures_util::StreamExt;
use nitter_scraper::{NitterError, NitterQuery, NitterScraper};
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
async fn main() -> ExitCode {
    let args = Args::parse();

    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
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
        let tweet = match tweet_result {
            Err(NitterError::NotFound) => {
                eprintln!("account not found");
                return ExitCode::from(10);
            }
            Err(NitterError::SuspendedAccount) => {
                eprintln!("account suspended");
                return ExitCode::from(10);
            }
            Err(NitterError::ProtectedAccount) => {
                eprintln!("account is protected");
                return ExitCode::from(10);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                return ExitCode::FAILURE;
            }
            Ok(t) => t,
        };

        if let Err(e) = writeln!(
            std::io::stdout(),
            "{}",
            serde_json::to_string(&tweet).unwrap()
        ) {
            match e.kind() {
                std::io::ErrorKind::BrokenPipe => break,
                _ => {
                    eprintln!("e");
                    return ExitCode::FAILURE;
                }
            }
        }
    }

    ExitCode::SUCCESS
}
