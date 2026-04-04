/// In-memory LRU cache for search results.
/// Avoids re-running BM25 + vector search for repeated identical queries.
/// This is the "Rust cache" payoff — sub-microsecond hits for warm queries.

use crate::types::SearchResult;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};

const DEFAULT_CAPACITY: usize = 512;

#[derive(Clone)]
pub struct SearchCache {
    inner: Arc<RwLock<CacheInner>>,
}

struct CacheInner {
    map: HashMap<String, Vec<SearchResult>>,
    order: VecDeque<String>,
    capacity: usize,
    hits: u64,
    misses: u64,
}

impl SearchCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(CacheInner {
                map: HashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
                capacity,
                hits: 0,
                misses: 0,
            })),
        }
    }

    pub fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    pub fn make_key(query: &str, mode: &str, collection: Option<&str>, limit: usize) -> String {
        format!("{}|{}|{}|{}", mode, query, collection.unwrap_or("*"), limit)
    }

    pub fn get(&self, key: &str) -> Option<Vec<SearchResult>> {
        let mut inner = self.inner.write().ok()?;
        if inner.map.contains_key(key) {
            inner.hits += 1;
            inner.map.get(key).cloned()
        } else {
            inner.misses += 1;
            None
        }
    }

    pub fn set(&self, key: String, results: Vec<SearchResult>) {
        if let Ok(mut inner) = self.inner.write() {
            // Remove existing key from order queue to avoid duplicates
            if inner.map.contains_key(&key) {
                inner.order.retain(|k| k != &key);
            }
            if inner.map.len() >= inner.capacity {
                if let Some(oldest) = inner.order.pop_front() {
                    inner.map.remove(&oldest);
                }
            }
            inner.order.push_back(key.clone());
            inner.map.insert(key, results);
        }
    }

    /// Invalidate all cached results (called after update/embed)
    pub fn invalidate_all(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.map.clear();
            inner.order.clear();
        }
    }

    pub fn stats(&self) -> (u64, u64, usize) {
        let inner = self.inner.read().unwrap();
        (inner.hits, inner.misses, inner.map.len())
    }
}

// Global cache instance — shared across CLI invocations via a Unix socket
// For the simple CLI use case, we just use it per-process (still saves
// re-embedding the query on identical back-to-back calls from OpenClaw)
use std::sync::OnceLock;
static GLOBAL_CACHE: OnceLock<SearchCache> = OnceLock::new();

pub fn global_cache() -> &'static SearchCache {
    GLOBAL_CACHE.get_or_init(|| {
        let cap: usize = std::env::var("HORCRUX_CACHE_SIZE")
            .or_else(|_| std::env::var("HOARD_CACHE_SIZE")) // backward compat
            .or_else(|_| std::env::var("CLAW_CACHE_SIZE"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_CAPACITY);
        SearchCache::new(cap)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_result(path: &str, score: f32) -> SearchResult {
        SearchResult {
            path: path.into(),
            docid: "abc123".into(),
            score,
            snippet: "test snippet".into(),
            title: "Test Title".into(),
            context: None,
            line: None,
        }
    }

    #[test]
    fn test_cache_key_generation() {
        let key1 = SearchCache::make_key("query", "search", Some("coll"), 10);
        let key2 = SearchCache::make_key("query", "search", Some("coll"), 10);
        let key3 = SearchCache::make_key("query", "search", None, 10);
        
        assert_eq!(key1, key2, "Same parameters should produce same key");
        assert_ne!(key1, key3, "Different collection should produce different key");
    }

    #[test]
    fn test_cache_basic_operations() {
        let cache = SearchCache::new(10);
        let key = "test_key";
        let results = vec![
            create_test_result("path1", 0.9),
            create_test_result("path2", 0.8),
        ];
        
        // Initially empty
        assert!(cache.get(key).is_none());
        
        // Insert
        cache.set(key.into(), results.clone());
        
        // Retrieve
        let cached = cache.get(key).unwrap();
        assert_eq!(cached.len(), 2);
        assert_eq!(cached[0].path, "path1");
        assert_eq!(cached[1].score, 0.8);
    }

    #[test]
    fn test_cache_update_existing() {
        let cache = SearchCache::new(10);
        let key = "test_key";
        
        let results1 = vec![create_test_result("path1", 0.9)];
        let results2 = vec![create_test_result("path2", 0.95)];
        
        cache.set(key.into(), results1);
        cache.set(key.into(), results2);
        
        let cached = cache.get(key).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].path, "path2");
    }

    #[test]
    fn test_cache_eviction() {
        let cache = SearchCache::new(2); // Small capacity
        
        cache.set("key1".into(), vec![create_test_result("p1", 0.9)]);
        cache.set("key2".into(), vec![create_test_result("p2", 0.8)]);
        cache.set("key3".into(), vec![create_test_result("p3", 0.7)]);
        
        // First key should be evicted
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_some());
        assert!(cache.get("key3").is_some());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = SearchCache::new(10);
        
        cache.set("key1".into(), vec![create_test_result("p1", 0.9)]);
        cache.set("key2".into(), vec![create_test_result("p2", 0.8)]);
        
        cache.invalidate_all();
        
        assert!(cache.get("key1").is_none());
        assert!(cache.get("key2").is_none());
    }

    #[test]
    fn test_cache_stats() {
        let cache = SearchCache::new(10);
        
        // Initially empty
        let (hits, misses, size) = cache.stats();
        assert_eq!(hits, 0);
        assert_eq!(misses, 0);
        assert_eq!(size, 0);
        
        // Add item and miss
        cache.set("key".into(), vec![]);
        let _ = cache.get("key"); // hit
        let _ = cache.get("missing"); // miss
        
        let (hits, misses, size) = cache.stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
        assert_eq!(size, 1);
    }

    #[test]
    fn test_cache_thread_safety() {
        use std::thread;
        
        let cache = SearchCache::new(100);
        let mut handles = vec![];
        
        // Spawn multiple threads
        for i in 0..10 {
            let cache_clone = cache.clone();
            let handle = thread::spawn(move || {
                let key = format!("thread_{}", i);
                let results = vec![create_test_result(&format!("path{}", i), 0.9)];
                cache_clone.set(key, results);
                
                // Try to read back
                let key = format!("thread_{}", i);
                cache_clone.get(&key)
            });
            handles.push(handle);
        }
        
        // Wait for all threads
        for handle in handles {
            let result = handle.join().unwrap();
            assert!(result.is_some());
        }
    }
}