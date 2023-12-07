use askama::Template;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::{wasm_bindgen, JsValue};

#[derive(Deserialize, Serialize, Default, Debug)]
pub struct Post {
    pub id: u32,
    pub title: String,
    pub content: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    pub posts: &'a Vec<Post>,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {}

/// Returns an HTML page with all posts.
#[wasm_bindgen]
pub async fn get_index_template(posts_js: Vec<JsValue>) -> String {
    let posts: Vec<Post> = posts_js
        .into_iter()
        .map(|post_js| serde_wasm_bindgen::from_value(post_js).unwrap())
        .collect();
    let template = IndexTemplate { posts: &posts };
    template.render().unwrap()
}

/// Returns an HTML page with a simple About page.
#[wasm_bindgen]
pub async fn get_about_template() -> String {
    let template = AboutTemplate {};
    template.render().unwrap()
}

/// Returns the read/write set of posts needed by `get_posts`.
#[wasm_bindgen]
pub async fn get_index_rw_set(page_num: u32, page_size: u32) -> JsValue {
    // Does not do boundary checking.
    let keys: Vec<String> = (((page_num - 1) * page_size)..((page_num) * page_size))
        .map(|post_id| post_id.to_string())
        .collect();
    serde_wasm_bindgen::to_value(&keys).unwrap()
}
