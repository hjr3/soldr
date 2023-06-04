use anyhow::Result;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "soldr=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (ingest, mgmt) = soldr::app().await?;

    let addr = "0.0.0.0:3443";
    let addr = addr.parse()?;
    tokio::spawn(async move {
        tracing::info!("management API listening on {}", addr);
        axum::Server::bind(&addr)
            .serve(mgmt.into_make_service())
            .await
            .unwrap();
    });

    let addr = "0.0.0.0:3000";
    tracing::info!("ingest listening on {}", addr);
    axum::Server::bind(&addr.parse()?)
        .serve(ingest.into_make_service())
        .await?;

    Ok(())
}
