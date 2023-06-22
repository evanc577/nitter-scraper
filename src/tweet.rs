use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Tweet {
    pub id: u128,
    pub id_str: String,
    pub created_at: String,
    pub created_at_ts: i64,
    pub user: User,
    pub full_text: String,
    pub images: Vec<String>,
    pub links: Vec<String>,
    pub retweet: bool,
    pub reply: bool,
    pub quote: bool,
    pub pinned: bool,
}

#[derive(Debug, Serialize)]
pub struct User {
    pub screen_name: String,
}
