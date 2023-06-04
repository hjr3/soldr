use std::sync::Once;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
