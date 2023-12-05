mod cache;
mod types;

use cache::CacheKV;
use types::*;

use askama::Template;
use chrono::Local;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use worker::*;

/// If this was an actual blog site:
/// TODO: impose and enforce a character limit on title and content.
/// TODO: add a secondary post details page and only show post preview
///       on the primary page.
/// TODO: split the POST route into PUT (for creating new posts) and
///       POST for editing existing posts.
/// TODO: add an `updated_at`` field to the `Post` structure.
/// TODO: enable Markdown to HTML conversion for title and content.

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate<'a> {
    pub posts: &'a Vec<Post>,
}

#[derive(Template)]
#[template(path = "about.html")]
struct AboutTemplate {}

#[derive(Debug, Serialize)]
struct GenericResponse<T>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    status: u16,
    content: T,
}

fn generate_api_success_response<T>(content: T) -> Result<Response>
where
    T: Serialize + for<'a> Deserialize<'a>,
{
    Response::from_json(&GenericResponse {
        status: 200,
        content,
    })
}

fn generate_api_error_response(status: u16) -> Result<Response> {
    Response::error(
        StatusCode::from_u16(status)
            .unwrap()
            .canonical_reason()
            .unwrap_or(""),
        status,
    )
}

fn generate_html_response<T>(status: u16, template: T) -> Result<Response>
where
    T: Template,
{
    let body = template.render().unwrap();
    Ok(Response::from_html(body).unwrap().with_status(status))
}

/// Increment the counter for the number of posts that are contained in the KV.
/// Since posts are only ever added sequentially by increasing ID number, if this
/// value is some N, it means the keys that are in are accessed on HTML render are
/// 0, 1, 2, ..., N - 1. This makes the r/w set function very simple.
async fn increment_count(kv: &CacheKV) -> Result<()> {
    let count = get_count(kv).await?;
    let new_count: u32 = count + 1;
    kv.put("count", &new_count).await
}

/// Decrement the counter for the number of posts that are contained in the KV.
/// If the existing value is already zero, do nothing and return no error.
async fn decrement_count(kv: &CacheKV) -> Result<()> {
    let count = get_count(kv).await?;
    let new_count: u32 = if count > 0 { count - 1 } else { 0 };
    kv.put("count", &new_count).await
}

/// Gets the count of posts from the KV.
async fn get_count(kv: &CacheKV) -> Result<u32> {
    Ok(kv.get::<u32>("count").await?.unwrap_or(0))
}

/// Reset the count of posts in the KV to zero.
async fn reset_count(kv: &CacheKV) -> Result<()> {
    kv.put("count", &0).await
}

/// To add or edit an existing post, set request body to the following:
/// JSON {
///     id: u32
///     title: String(POST_TITLE_CHAR_LIMIT),
///     content: String(POST_CONTENT_CHAR_LIMIT)
/// }
async fn create_post(mut req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let kv = &CacheKV::new().await;
    let mut post = req.json::<Post>().await?;
    let post_id = post.id.to_string();
    post.created_at = Some(Local::now().format("%H:%M:%S %m-%d-%Y").to_string());
    kv.put(&post_id, &post).await?; // TODO: Currently does not check for duplicate posts (obvious bug)
    increment_count(kv).await?;

    generate_api_success_response(format!("Successfully added post #{post_id}"))
}

/// To delete an existing post, send an empty body with post id as URL parameter.
async fn delete_post(_req: Request, ctx: RouteContext<()>) -> Result<Response> {
    if let Some(id) = ctx.param("id") {
        let kv = &CacheKV::new().await;
        kv.delete(id).await?;
        decrement_count(kv).await?;
        generate_api_success_response(format!("Successfully deleted post #{id}"))
    } else {
        generate_api_error_response(400)
    }
}

/// To delete an existing post, send an empty body with post id as URL parameter.
async fn clear_kv(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let kv = &CacheKV::new().await;
    let count = get_count(kv).await?;
    for post_num in 0..count {
        kv.delete(&post_num.to_string()).await?;
    }
    reset_count(kv).await?;
    generate_api_success_response("Successfully cleared KV".to_string())
}

/// Returns an HTML page with all posts.
async fn get_posts(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let kv = &CacheKV::new().await;
    let count = get_count(kv).await?;
    let mut posts = Vec::with_capacity(count.try_into().unwrap());
    for post_num in 0..count {
        let post_opt = kv.get::<Post>(&post_num.to_string()).await.unwrap();
        if let Some(post) = post_opt {
            posts.push(post);
        }
    }
    let template = IndexTemplate { posts: &posts };
    generate_html_response(200, template)
}

/// Returns an HTML page with a simple About page.
async fn get_about(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let template = AboutTemplate {};
    generate_html_response(200, template)
}

/// Returns the read/write set of this function.
async fn get_rw_set(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let kv = &CacheKV::new().await;
    let count = get_count(kv).await?;
    let mut keys = Vec::with_capacity(count.try_into().unwrap());
    for post_num in 0..count {
        keys.push(post_num.to_string());
    }
    generate_api_success_response(keys)
}

/// Sets a dummy "value" into key "test" to verify that the cache works properly.
async fn test_kv(_req: Request, _ctx: RouteContext<()>) -> Result<Response> {
    let kv = &CacheKV::new().await;
    kv.put("test", &"value".to_string()).await?;
    let val = kv.get::<String>("test").await?;
    generate_api_success_response(val)
}

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();
    router
        .post_async("/post", create_post)
        .post_async("/clear_kv", clear_kv)
        .delete_async("/post/:id", delete_post)
        .get_async("/", get_posts)
        .get_async("/posts", get_posts)
        .get_async("/about", get_about)
        .get_async("/rw_set", get_rw_set)
        .post_async("/test_kv", test_kv)
        .run(req, env)
        .await
}
