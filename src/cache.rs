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

    pub fn get(&self, domain: &str) -> Option<Origin> {
        tracing::info!("Got called on cache for domain: {}", domain);
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
