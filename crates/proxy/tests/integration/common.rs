use std::sync::Once;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use soldr::config::{Config, Database, Management, Proxy, Tls};

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
        database: Database {
            url: "sqlite::memory:".to_string(),
        },
        management: Management {
            listen: "0.0.0.0:3443".to_string(),
            secret: "a secret with minimum length of 32 characters".to_string(),
        },
        proxy: Proxy {
            listen: "0.0.0.0:3000".to_string(),
        },
        tls: Tls {
            enable: false,
            cert_path: None,
            key_path: None,
        },
    }
}
