//! L1 Cache: High-performance in-process cache backed by Moka.
//!
//! This layer provides sub-millisecond access to the hottest data for a single
//! Maestro instance. It uses the TinyLFU eviction policy and supports both
//! time-to-live (TTL) and time-to-idle (TTI) expiration.

use moka::future::Cache;
use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use tracing::{debug, trace};

/// Configuration for a single L1 cache partition.
#[derive(Debug, Clone)]
pub struct L1Config {
    /// Maximum number of entries in this cache partition.
    pub max_capacity: u64,
    /// Time-to-live: entries expire this long after insertion.
    pub ttl: Duration,
    /// Time-to-idle: entries expire this long after last access.
    /// If `None`, only TTL is used.
    pub tti: Option<Duration>,
}

impl Default for L1Config {
    fn default() -> Self {
        Self {
            max_capacity: 10_000,
            ttl: Duration::from_secs(60),
            tti: None,
        }
    }
}

/// L1 in-process cache using Moka.
///
/// Values are stored as JSON-encoded `Vec<u8>` to allow heterogeneous types
/// in a single cache instance. The serialization overhead is minimal compared
/// to the latency savings from avoiding L2/L3 lookups.
#[derive(Clone)]
pub struct L1Cache {
    inner: Cache<String, Vec<u8>>,
    name: String,
}

impl L1Cache {
    /// Create a new L1 cache with the given configuration.
    pub fn new(name: impl Into<String>, config: &L1Config) -> Self {
        let name = name.into();
        let mut builder = Cache::builder()
            .max_capacity(config.max_capacity)
            .time_to_live(config.ttl);

        if let Some(tti) = config.tti {
            builder = builder.time_to_idle(tti);
        }

        let cache_name = name.clone();
        let inner = builder
            .eviction_listener(move |key: Arc<String>, _value, cause| {
                trace!(cache = %cache_name, key = %key, cause = ?cause, "L1 eviction");
            })
            .name(&name)
            .build();

        debug!(cache = %name, max_capacity = config.max_capacity, ttl_secs = config.ttl.as_secs(), "L1 cache initialized");

        Self { inner, name }
    }

    /// Get a value from the cache, deserializing from JSON.
    ///
    /// Returns `None` on cache miss or deserialization failure.
    pub async fn get<V: DeserializeOwned>(&self, key: &str) -> Option<V> {
        let bytes = self.inner.get(key).await?;
        match serde_json::from_slice(&bytes) {
            Ok(value) => {
                trace!(cache = %self.name, key = %key, "L1 hit");
                Some(value)
            }
            Err(e) => {
                debug!(cache = %self.name, key = %key, error = %e, "L1 deserialization error, treating as miss");
                // Invalidate corrupted entry
                self.inner.invalidate(key).await;
                None
            }
        }
    }

    /// Insert a value into the cache, serializing to JSON.
    pub async fn insert<V: Serialize>(&self, key: &str, value: &V) {
        match serde_json::to_vec(value) {
            Ok(bytes) => {
                self.inner.insert(key.to_string(), bytes).await;
                trace!(cache = %self.name, key = %key, "L1 insert");
            }
            Err(e) => {
                debug!(cache = %self.name, key = %key, error = %e, "L1 serialization error, skipping insert");
            }
        }
    }

    /// Invalidate (remove) a single key from the cache.
    pub async fn invalidate(&self, key: &str) {
        self.inner.invalidate(key).await;
        trace!(cache = %self.name, key = %key, "L1 invalidate");
    }

    /// Invalidate all entries in this cache partition.
    pub async fn invalidate_all(&self) {
        self.inner.invalidate_all();
        self.inner.run_pending_tasks().await;
        debug!(cache = %self.name, "L1 invalidate_all");
    }

    /// Get the current number of entries in the cache.
    pub fn entry_count(&self) -> u64 {
        self.inner.entry_count()
    }

    /// Get the name of this cache partition.
    pub fn name(&self) -> &str {
        &self.name
    }
}

// We need the Arc import for the eviction listener closure.
use std::sync::Arc;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_l1_basic_get_set() {
        let config = L1Config {
            max_capacity: 100,
            ttl: Duration::from_secs(60),
            tti: None,
        };
        let cache = L1Cache::new("test", &config);

        // Miss
        let result: Option<String> = cache.get("key1").await;
        assert!(result.is_none());

        // Insert and hit
        cache.insert("key1", &"hello".to_string()).await;
        let result: Option<String> = cache.get("key1").await;
        assert_eq!(result, Some("hello".to_string()));
    }

    #[tokio::test]
    async fn test_l1_invalidate() {
        let config = L1Config::default();
        let cache = L1Cache::new("test_inv", &config);

        cache.insert("key1", &42i64).await;
        assert_eq!(cache.get::<i64>("key1").await, Some(42));

        cache.invalidate("key1").await;
        assert_eq!(cache.get::<i64>("key1").await, None);
    }

    #[tokio::test]
    async fn test_l1_invalidate_all() {
        let config = L1Config::default();
        let cache = L1Cache::new("test_all", &config);

        cache.insert("a", &1i64).await;
        cache.insert("b", &2i64).await;
        cache.insert("c", &3i64).await;

        cache.invalidate_all().await;

        assert_eq!(cache.get::<i64>("a").await, None);
        assert_eq!(cache.get::<i64>("b").await, None);
        assert_eq!(cache.get::<i64>("c").await, None);
    }

    #[tokio::test]
    async fn test_l1_json_complex_type() {
        use serde::{Deserialize, Serialize};

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct TestStruct {
            name: String,
            value: i64,
        }

        let config = L1Config::default();
        let cache = L1Cache::new("test_complex", &config);

        let obj = TestStruct {
            name: "test".to_string(),
            value: 42,
        };
        cache.insert("obj1", &obj).await;

        let result: Option<TestStruct> = cache.get("obj1").await;
        assert_eq!(result, Some(obj));
    }

    #[tokio::test]
    async fn test_l1_overwrite() {
        let config = L1Config::default();
        let cache = L1Cache::new("test_overwrite", &config);

        cache.insert("key", &"first".to_string()).await;
        assert_eq!(cache.get::<String>("key").await, Some("first".to_string()));

        cache.insert("key", &"second".to_string()).await;
        assert_eq!(cache.get::<String>("key").await, Some("second".to_string()));
    }
}
