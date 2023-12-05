use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct CachedPost {
    pub version: u32,
    pub post: Post,
}

#[derive(Deserialize, Serialize, Default, Debug)]
pub struct Post {
    pub id: u32,
    pub title: String,
    pub content: String,
    pub created_at: Option<String>, // time string in edge-local time
}
