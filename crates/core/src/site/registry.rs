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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Minimal SiteCore stub for testing the registry
    struct StubSiteCore {
        name: String,
        url: String,
    }

    impl SiteCore for StubSiteCore {
        fn name(&self) -> &str {
            &self.name
        }
        fn base_url(&self) -> &str {
            &self.url
        }
        fn capabilities(&self) -> HashSet<SiteCapability> {
            HashSet::new()
        }
    }

    fn make_handle(name: &str) -> AdapterHandle {
        AdapterHandle {
            core: Arc::new(StubSiteCore {
                name: name.to_string(),
                url: format!("https://{}.example.com", name),
            }),
            reseed: None,
            repost: None,
            user_info: None,
            search: None,
            rate_limiter: Arc::new(SiteRateLimiter::new(1000, 1)),
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = SiteRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn default_registry_is_empty() {
        let reg = SiteRegistry::default();
        assert!(reg.is_empty());
    }

    #[test]
    fn register_and_get_adapter() {
        let mut reg = SiteRegistry::new();
        let id = SiteId(1);
        reg.register(id, make_handle("hdsky"));

        let handle = reg.get(&id).unwrap();
        assert_eq!(handle.core.name(), "hdsky");
        assert_eq!(reg.len(), 1);
        assert!(!reg.is_empty());
    }

    #[test]
    fn get_returns_none_for_missing_id() {
        let reg = SiteRegistry::new();
        assert!(reg.get(&SiteId(999)).is_none());
    }

    #[test]
    fn remove_returns_handle_and_decrements_len() {
        let mut reg = SiteRegistry::new();
        let id = SiteId(1);
        reg.register(id, make_handle("site1"));
        assert_eq!(reg.len(), 1);

        let removed = reg.remove(&id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().core.name(), "site1");
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn remove_returns_none_for_missing_id() {
        let mut reg = SiteRegistry::new();
        assert!(reg.remove(&SiteId(42)).is_none());
    }

    #[test]
    fn list_ids_returns_all_registered_ids() {
        let mut reg = SiteRegistry::new();
        reg.register(SiteId(1), make_handle("a"));
        reg.register(SiteId(2), make_handle("b"));
        reg.register(SiteId(3), make_handle("c"));

        let mut ids = reg.list_ids();
        ids.sort_by_key(|id| id.0);
        assert_eq!(ids, vec![SiteId(1), SiteId(2), SiteId(3)]);
    }

    #[test]
    fn get_all_returns_all_entries() {
        let mut reg = SiteRegistry::new();
        reg.register(SiteId(10), make_handle("x"));
        reg.register(SiteId(20), make_handle("y"));

        let all = reg.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn register_overwrites_existing_id() {
        let mut reg = SiteRegistry::new();
        let id = SiteId(1);
        reg.register(id, make_handle("old"));
        reg.register(id, make_handle("new"));

        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get(&id).unwrap().core.name(), "new");
    }
}
