use std::net::SocketAddr;

use axum::{response::IntoResponse, Json, http::HeaderMap};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use tera::{Tera, Context};


#[derive(serde::Deserialize)]
struct Config {
    /// Sent by the bot instance while updating the
    /// current statistics (user_count, guild_count)
    stats_key: String,
    /// Address to bind to, by default localhost:8080
    bind_addr: Option<SocketAddr>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug)]
struct TemplateContext {
    message_count: u64,
    guild_count: u32,
    user_count: u64,
}


static CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(&std::fs::read_to_string("config.toml").unwrap()).unwrap()
});

static TERA: Lazy<tera::Tera> = Lazy::new(|| {
    Tera::new("templates/**.html").unwrap()
});

static TEMPLATE_CONTEXT: RwLock<TemplateContext> = RwLock::new(TemplateContext {
    message_count: 0,
    guild_count: 0,
    user_count: 0
});


fn context() -> Context {
    Context::from_serialize(*TEMPLATE_CONTEXT.read()).unwrap()
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = axum::Router::new()
        .route("/", axum::routing::get(index))
        .route("/index.html", axum::routing::get(index))
        .route("/update_stats", axum::routing::post(update_stats));

    axum::Server::bind(&CONFIG.bind_addr.map_or_else(|| "0.0.0.0:8080".parse(), Ok)?)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {drop(tokio::signal::ctrl_c().await)})
        .await?;
    
    Ok(())
}


async fn update_stats(Json(stats): Json<TemplateContext>, headers: HeaderMap) -> impl IntoResponse {
    let auth_header = headers.get(http::header::AUTHORIZATION).map(|v| v.to_str().ok());

    if auth_header.flatten() != Some(&CONFIG.stats_key) {
        return http::StatusCode::UNAUTHORIZED;
    };

    *TEMPLATE_CONTEXT.write() = stats;
    http::StatusCode::NO_CONTENT
}

async fn index() -> impl IntoResponse {
    let content = TERA.render("index.html", &context())
        .expect("index.html template missing!");

    axum::response::Response::builder()
        .header(http::header::CONTENT_TYPE, "text/html")
        .body(content).unwrap()
}
