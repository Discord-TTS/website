use std::{net::SocketAddr, path::PathBuf, io::ErrorKind};

use axum::{response::{IntoResponse, Response}, body::{Bytes, Full}, extract::Path, Json, http};
use http::{HeaderMap, StatusCode};
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

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Build Response Error: {0}")]
    BuildResponse(#[from] http::Error),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        tracing::error!("{self}");

        StatusCode::INTERNAL_SERVER_ERROR
            .into_response()
    }
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

fn render_page(template_name: &str) -> String {
    TERA.render(template_name, &context())
        .expect("Template missing!")
}

fn build_html_response(content: String) -> Result<http::Response<String>, Error> {
    Response::builder()
        .header(http::header::CONTENT_TYPE, "text/html")
        .body(content).map_err(Error::BuildResponse)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let app = axum::Router::new()
        .route("/", axum::routing::get(index))
        .route("/tos", axum::routing::get(tos))
        .route("/premium", axum::routing::get(premium))
        .route("/privacy", axum::routing::get(privacy))
        .route("/index.html", axum::routing::get(index))
        .route("/static/*path", axum::routing::get(static_req))
        .route("/update_stats", axum::routing::post(update_stats));

    axum::Server::bind(&CONFIG.bind_addr.map_or_else(|| "0.0.0.0:8080".parse(), Ok)?)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async {drop(tokio::signal::ctrl_c().await)})
        .await?;
    
    Ok(())
}


async fn index() -> impl IntoResponse {
    build_html_response(render_page("index.html"))
}

async fn premium() -> impl IntoResponse {
    build_html_response(render_page("premium.html"))
}

async fn tos() -> impl IntoResponse {
    build_html_response(render_page("tos.html"))
}

async fn privacy() -> impl IntoResponse {
    build_html_response(render_page("privacy.html"))
}

async fn update_stats(Json(stats): Json<TemplateContext>, headers: HeaderMap) -> impl IntoResponse {
    let auth_header = headers.get(http::header::AUTHORIZATION).map(|v| v.to_str().ok());

    if auth_header.flatten() != Some(&CONFIG.stats_key) {
        return StatusCode::UNAUTHORIZED;
    };

    *TEMPLATE_CONTEXT.write() = stats;
    StatusCode::NO_CONTENT
}

async fn static_req(Path(mut path): Path<String>) -> Result<impl IntoResponse, Error> {
    if path.starts_with('/') {
        path.remove(0);
    }

    let static_path = {
        let mut base_path = PathBuf::from("./static");
        base_path.push(&path);
        base_path
    };

    let file_contents = match tokio::fs::read(&static_path).await {
        Ok(contents) => Full::new(Bytes::from(contents)),
        Err(not_found) if not_found.kind() == ErrorKind::NotFound => {
            tracing::info!("Static file not found: {path}");

            return Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(Bytes::new()))
                .map_err(Error::BuildResponse)
        },
        Err(err) => return Err(Error::Io(err))
    };

    Response::builder().body(file_contents).map_err(Error::BuildResponse)
}
