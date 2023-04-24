use futures_util::StreamExt;
use nitter_scraper::NitterScraper;
use reqwest::Client;

#[tokio::main]
async fn main() {
    let client = Client::new();
    let mut nitter_scraper = NitterScraper::builder()
        .client(&client)
        .instance("http://0.0.0.0:8080")
        .query("from:sua_cab")
        .limit(100)
        .build();
    let nitter_search = nitter_scraper.search().await;
    futures_util::pin_mut!(nitter_search);

    while let Some(tweet_result) = nitter_search.next().await {
        let tweet = tweet_result.unwrap();
        println!("{}", serde_json::to_string(&tweet).unwrap());
    }
}
