use std::net::SocketAddr;

use axum::{
    extract::State,
    http::{header, StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use bpaf::{construct, long, Parser};
use rust_embed::RustEmbed;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod error;

#[derive(RustEmbed)]
#[folder = "static/"]
struct Assets;

#[derive(Clone, Debug)]
struct Config {
    api_url: String,
    api_secret: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ui=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let api_url = long("api-url")
        .help("URL of the Soldr Management API")
        .argument::<String>("API_URL");

    let api_secret = long("api-secret")
        .help("Soldr Management API secret key")
        .argument::<String>("API_SECRET");

    let parser = construct!(Config {
        api_url,
        api_secret
    });
    let ui_parser = parser.to_options().descr("Soldr UI");
    let config: Config = ui_parser.run();

    let app = Router::new()
        .route("/hello", get(|| async { "Hello, World!" }))
        .fallback(static_handler)
        .layer(TraceLayer::new_for_http())
        .with_state(config);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8888));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn static_handler(State(config): State<Config>, uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() || path == "index.html" {
        return index_html(config).await;
    }

    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();

            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => {
            if path.contains('.') {
                return not_found().await;
            }

            index_html(config).await
        }
    }
}

async fn index_html(config: Config) -> Response {
    match Assets::get("index.html") {
        Some(content) => {
            let script_tag = format!(
                r#"<script type="module">window.config = {{ apiUrl: "{}", apiSecret: "{}" }};</script>"#,
                config.api_url, config.api_secret
            );

            let html = String::from_utf8(content.data.into_owned()).unwrap();

            let html = html.replace("<!-- __SOLDR_UI_CONFIG__ -->", script_tag.as_str());
            Html(html).into_response()
        }
        None => not_found().await,
    }
}

async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "404").into_response()
}
