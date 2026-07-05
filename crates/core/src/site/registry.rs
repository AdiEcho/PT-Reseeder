use super::models::SiteId;
use super::rate_limiter::SiteRateLimiter;
use super::traits::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Holds all capability views for a registered site adapter
#[derive(Clone)]
pub struct AdapterHandle {
    pub core: Arc<dyn SiteCore>,
    pub reseed: Option<Arc<dyn ReseedCapable>>,
    pub repost: Option<Arc<dyn RepostCapable>>,
    pub user_info: Option<Arc<dyn UserInfoCapable>>,
    pub search: Option<Arc<dyn SearchCapable>>,
    pub rate_limiter: Arc<SiteRateLimiter>,
}

/// Registry of all active site adapters
#[derive(Clone)]
pub struct SiteRegistry {
    adapters: HashMap<SiteId, AdapterHandle>,
}

impl SiteRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    pub fn register(&mut self, id: SiteId, handle: AdapterHandle) {
        self.adapters.insert(id, handle);
    }

    pub fn get(&self, id: &SiteId) -> Option<&AdapterHandle> {
        self.adapters.get(id)
    }

    pub fn remove(&mut self, id: &SiteId) -> Option<AdapterHandle> {
        self.adapters.remove(id)
    }

    pub fn list_ids(&self) -> Vec<SiteId> {
        self.adapters.keys().cloned().collect()
    }

    pub fn get_all(&self) -> Vec<(&SiteId, &AdapterHandle)> {
        self.adapters.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }
}

impl Default for SiteRegistry {
    fn default() -> Self {
        Self::new()
    }
}
