use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Tweet {
    pub id: u128,
    pub id_str: String,
    pub created_at: String,
    pub user: User,
    pub full_text: String,
    pub images: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct User {
    pub screen_name: String,
}
