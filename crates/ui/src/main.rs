use std::net::SocketAddr;

use axum::{extract::State, response::Html, routing::get, Router};
use bpaf::{construct, long, Parser};
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod error;

#[derive(Clone, Debug)]
struct Config {
    api_url: String,
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

    let parser = construct!(Config { api_url });
    let ui_parser = parser.to_options().descr("Soldr UI");
    let config: Config = ui_parser.run();

    let app = Router::new()
        .route("/hello", get(|| async { "Hello, World!" }))
        .nest_service("/assets", ServeDir::new("static/assets"))
        .nest_service("/vite.svg", ServeFile::new("static/vite.svg"))
        .route("/", get(serve_html))
        .route("/*path", get(serve_html))
        .layer(TraceLayer::new_for_http())
        .with_state(config);

    let addr = SocketAddr::from(([127, 0, 0, 1], 8888));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn serve_html(State(config): State<Config>) -> Html<String> {
    let html = include_str!("../static/index.html");

    let script_tag = format!(
        r#"<script type="module">window.apiUrl = "{}";</script>"#,
        config.api_url
    );
    let html = html.replace("<!-- __SOLDR_UI_CONFIG__ -->", script_tag.as_str());

    Html(html)
}
