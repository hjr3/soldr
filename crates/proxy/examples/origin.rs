use anyhow::Result;
use axum::http::StatusCode;
use axum::{routing::any, Router};
use tokio::net::TcpListener;
use tokio::time::{sleep, Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "origin=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let origin = Router::new()
        .route("/", any(success_handler))
        .route("/failure", any(failure_handler))
        .route("/timeout", any(timeout_handler));

    let addr = "0.0.0.0:8080";
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("origin listening on {}", addr);
    axum::serve(listener, origin).await?;

    Ok(())
}

async fn success_handler() -> impl axum::response::IntoResponse {
    "Hello, World!"
}

async fn failure_handler() -> impl axum::response::IntoResponse {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        "unexpected error".to_string(),
    )
}

async fn timeout_handler() -> impl axum::response::IntoResponse {
    sleep(Duration::from_secs(6)).await;
    "We shouldn't see this"
}
