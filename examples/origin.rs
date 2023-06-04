use anyhow::Result;
use axum::http::StatusCode;
use axum::{routing::post, Router};
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
        .route("/", post(success_handler))
        .route("/failure", post(failure_handler));

    let addr = "0.0.0.0:8080";
    tracing::info!("origin listening on {}", addr);
    axum::Server::bind(&addr.parse()?)
        .serve(origin.into_make_service())
        .await?;

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
