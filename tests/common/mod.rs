use std::sync::Once;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use soldr::Config;

static TRACING_INITIALIZED: Once = Once::new();

// Help function to add tracing to tests
// Note: This is safe to use for multiple tests, but since tests are run concurrently the
// output may be interleaved
#[allow(dead_code)]
pub fn enable_tracing() {
    TRACING_INITIALIZED.call_once(|| {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "soldr=trace".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    });
}

pub fn config() -> Config {
    Config {
        database_url: "sqlite::memory:".to_string(),
        management_listener: "0.0.0.0:3443".to_string(),
        ingest_listener: "0.0.0.0:3000".to_string(),
    }
}
