use crate::db::Origin;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::AppError;

#[derive(Debug)]
pub struct OriginCache(pub(crate) Arc<OriginCacheInner>);

impl OriginCache {
    pub fn new() -> Self {
        let inner = OriginCacheInner::new();
        Self(Arc::new(inner))
    }

    pub fn refresh(&self, new_origins: Vec<Origin>) -> Result<(), AppError> {
        self.0.refresh(new_origins)
    }

    pub fn get(&self, domain: &str) -> Option<Origin> {
        self.0.get(domain)
    }
}

impl Clone for OriginCache {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
pub struct OriginCacheInner {
    origins: Arc<RwLock<HashMap<String, Origin>>>,
}

impl OriginCacheInner {
    pub fn new() -> Self {
        Self {
            origins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn refresh(&self, new_origins: Vec<Origin>) -> Result<(), AppError> {
        // Iterate over the fetched origins and insert them into the map
        let map = new_origins
            .into_iter()
            .map(|origin| (origin.domain.clone(), origin))
            .collect();

        // Update the cache by acquiring a write lock and replacing the HashMap
        *self.origins.write() = map;
        Ok(())
    }

    pub fn get(&self, domain: &str) -> Option<Origin> {
        tracing::info!("Got called on cache for domain: {}", domain);
        // Look up domain in the cache and clone if found
        let result = {
            let origins = self.origins.read();

            origins.get(domain).cloned()
        };

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
