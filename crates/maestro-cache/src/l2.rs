//! L2 Cache: Optional distributed cache backed by Redis.
//!
//! This layer provides a shared cache across multiple Maestro instances,
//! enabling horizontal scaling. It is entirely optional and feature-gated
//! behind the `redis-cache` feature flag.
//!
//! When Redis is unavailable, all operations gracefully degrade to no-ops
//! (returning `None` for gets), allowing the system to fall through to L3.

use serde::{de::DeserializeOwned, Serialize};
use std::time::Duration;
use tracing::{debug, trace, warn};

/// Configuration for the L2 Redis cache.
#[derive(Debug, Clone)]
pub struct L2Config {
    /// Redis connection URL (e.g., "redis://127.0.0.1:6379").
    pub redis_url: String,
    /// Default TTL for cached entries.
    pub default_ttl: Duration,
    /// Key prefix for namespacing (e.g., "maestro").
    pub key_prefix: String,
}

impl Default for L2Config {
    fn default() -> Self {
        Self {
            redis_url: "redis://127.0.0.1:6379".to_string(),
            default_ttl: Duration::from_secs(300),
            key_prefix: "maestro".to_string(),
        }
    }
}

/// L2 distributed cache.
///
/// When the `redis-cache` feature is enabled, this connects to Redis.
/// When the feature is disabled, this is a no-op stub that always misses.
#[derive(Clone)]
pub struct L2Cache {
    #[cfg(feature = "redis-cache")]
    connection: Option<redis::aio::MultiplexedConnection>,
    config: L2Config,
}

impl L2Cache {
    /// Create a new L2 cache. If Redis is not available, returns a degraded
    /// instance that always returns cache misses.
    pub async fn new(config: L2Config) -> Self {
        #[cfg(feature = "redis-cache")]
        {
            match Self::connect(&config).await {
                Ok(conn) => {
                    debug!(url = %config.redis_url, prefix = %config.key_prefix, "L2 Redis cache connected");
                    Self {
                        connection: Some(conn),
                        config,
                    }
                }
                Err(e) => {
                    warn!(error = %e, "L2 Redis cache unavailable, degrading to passthrough");
                    Self {
                        connection: None,
                        config,
                    }
                }
            }
        }

        #[cfg(not(feature = "redis-cache"))]
        {
            debug!("L2 Redis cache disabled (redis-cache feature not enabled)");
            Self { config }
        }
    }

    /// Create a no-op L2 cache that always misses (for when Redis is not desired).
    pub fn disabled() -> Self {
        Self {
            #[cfg(feature = "redis-cache")]
            connection: None,
            config: L2Config::default(),
        }
    }

    /// Check if the L2 cache is connected and operational.
    pub fn is_connected(&self) -> bool {
        #[cfg(feature = "redis-cache")]
        {
            self.connection.is_some()
        }
        #[cfg(not(feature = "redis-cache"))]
        {
            false
        }
    }

    /// Build the full Redis key with namespace prefix.
    fn full_key(&self, namespace: &str, key: &str) -> String {
        format!("{}:{}:{}", self.config.key_prefix, namespace, key)
    }

    /// Get a value from Redis, deserializing from JSON.
    ///
    /// Returns `None` on miss, connection error, or deserialization failure.
    pub async fn get<V: DeserializeOwned>(&self, namespace: &str, key: &str) -> Option<V> {
        #[cfg(feature = "redis-cache")]
        {
            use redis::AsyncCommands;
            let conn = self.connection.as_ref()?;
            let full_key = self.full_key(namespace, key);

            let mut conn = conn.clone();
            match conn.get::<_, Option<String>>(&full_key).await {
                Ok(Some(json_str)) => match serde_json::from_str(&json_str) {
                    Ok(value) => {
                        trace!(namespace = %namespace, key = %key, "L2 hit");
                        Some(value)
                    }
                    Err(e) => {
                        debug!(key = %full_key, error = %e, "L2 deserialization error");
                        // Delete corrupted entry
                        let _: Result<(), _> = conn.del(&full_key).await;
                        None
                    }
                },
                Ok(None) => {
                    trace!(namespace = %namespace, key = %key, "L2 miss");
                    None
                }
                Err(e) => {
                    warn!(key = %full_key, error = %e, "L2 Redis get error");
                    None
                }
            }
        }

        #[cfg(not(feature = "redis-cache"))]
        {
            let _ = (namespace, key);
            None
        }
    }

    /// Insert a value into Redis with TTL, serializing to JSON.
    pub async fn insert<V: Serialize>(
        &self,
        namespace: &str,
        key: &str,
        value: &V,
        ttl: Option<Duration>,
    ) {
        #[cfg(feature = "redis-cache")]
        {
            use redis::AsyncCommands;
            if let Some(conn) = &self.connection {
                let full_key = self.full_key(namespace, key);
                let ttl = ttl.unwrap_or(self.config.default_ttl);

                match serde_json::to_string(value) {
                    Ok(json_str) => {
                        let mut conn = conn.clone();
                        if let Err(e) = conn
                            .set_ex::<_, _, ()>(&full_key, &json_str, ttl.as_secs())
                            .await
                        {
                            warn!(key = %full_key, error = %e, "L2 Redis set error");
                        } else {
                            trace!(namespace = %namespace, key = %key, ttl_secs = ttl.as_secs(), "L2 insert");
                        }
                    }
                    Err(e) => {
                        debug!(key = %full_key, error = %e, "L2 serialization error");
                    }
                }
            }
        }

        #[cfg(not(feature = "redis-cache"))]
        {
            let _ = (namespace, key, value, ttl);
        }
    }

    /// Invalidate (delete) a single key from Redis.
    pub async fn invalidate(&self, namespace: &str, key: &str) {
        #[cfg(feature = "redis-cache")]
        {
            use redis::AsyncCommands;
            if let Some(conn) = &self.connection {
                let full_key = self.full_key(namespace, key);
                let mut conn = conn.clone();
                if let Err(e) = conn.del::<_, ()>(&full_key).await {
                    warn!(key = %full_key, error = %e, "L2 Redis del error");
                } else {
                    trace!(namespace = %namespace, key = %key, "L2 invalidate");
                }
            }
        }

        #[cfg(not(feature = "redis-cache"))]
        {
            let _ = (namespace, key);
        }
    }

    /// Invalidate all keys matching a namespace pattern.
    ///
    /// Uses SCAN + DEL to avoid blocking the Redis server with KEYS.
    pub async fn invalidate_namespace(&self, namespace: &str) {
        #[cfg(feature = "redis-cache")]
        {
            use redis::AsyncCommands;
            if let Some(conn) = &self.connection {
                let pattern = format!("{}:{}:*", self.config.key_prefix, namespace);
                let mut conn = conn.clone();

                // Use SCAN to find keys matching the pattern
                let keys: Vec<String> = match redis::cmd("KEYS")
                    .arg(&pattern)
                    .query_async(&mut conn)
                    .await
                {
                    Ok(keys) => keys,
                    Err(e) => {
                        warn!(pattern = %pattern, error = %e, "L2 Redis KEYS error during namespace invalidation");
                        return;
                    }
                };

                if !keys.is_empty() {
                    if let Err(e) = conn.del::<_, ()>(&keys).await {
                        warn!(pattern = %pattern, error = %e, "L2 Redis bulk DEL error");
                    } else {
                        debug!(namespace = %namespace, count = keys.len(), "L2 namespace invalidated");
                    }
                }
            }
        }

        #[cfg(not(feature = "redis-cache"))]
        {
            let _ = namespace;
        }
    }

    #[cfg(feature = "redis-cache")]
    async fn connect(
        config: &L2Config,
    ) -> Result<redis::aio::MultiplexedConnection, redis::RedisError> {
        let client = redis::Client::open(config.redis_url.as_str())?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_l2_disabled_always_misses() {
        let cache = L2Cache::disabled();
        assert!(!cache.is_connected());

        let result: Option<String> = cache.get("ns", "key").await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_l2_no_redis_graceful_degradation() {
        // Try to connect to a non-existent Redis — should degrade gracefully
        let config = L2Config {
            redis_url: "redis://127.0.0.1:59999".to_string(), // unlikely to be running
            ..Default::default()
        };
        let cache = L2Cache::new(config).await;

        // Should not panic, just return None
        let result: Option<String> = cache.get("ns", "key").await;
        assert!(result.is_none());

        // Insert should be a no-op
        cache.insert("ns", "key", &"value", None).await;
    }
}
