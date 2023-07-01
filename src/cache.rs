use crate::db::{list_origins, Origin};
use parking_lot::RwLock;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AppError;

#[derive(Debug)]
pub struct OriginCache {
    origins: Arc<RwLock<HashMap<String, Origin>>>,
    pool: Arc<SqlitePool>,
}

impl OriginCache {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        OriginCache {
            origins: Arc::new(RwLock::new(HashMap::new())),
            pool,
        }
    }

    pub async fn refresh(&self) -> Result<(), AppError> {
        // Fetch the latest origin data from the database using the provided SqlitePool
        let new_origins = list_origins(&self.pool).await?;

        // Create a new HashMap to store the updated origin data
        let mut map = HashMap::new();

        // Iterate over the fetched origins and insert them into the map
        for origin in new_origins {
            map.insert(origin.domain.clone(), origin);
        }

        // Update the cache by acquiring a write lock and replacing the HashMap
        *self.origins.write() = map;
        Ok(())
    }

    pub async fn get(&self, domain: &str) -> Option<Origin> {
        tracing::info!("Get called on cache for domain: {}", domain);
        let origins = self.origins.read();

        // Look up domain in the cache and clone if found
        let result = origins.get(domain).cloned();

        // Mostly for development, but also useful if you want to see how often the cache is hit
        if result.is_some() {
            tracing::info!("Found origin in cache");
        } else {
            tracing::info!("Origin not found in cache");
        }

        // Return the result if found, otherwise None
        result
    }
}
