//! # maestro-cache
//!
//! Multi-tier caching layer for the Maestro agent platform.
//!
//! This crate provides a transparent caching wrapper (`CachingMemory`) around
//! the `MemorySubstrate` (L3), adding:
//!
//! - **L1 (Moka):** Sub-millisecond in-process cache using the TinyLFU eviction
//!   policy. Ideal for hot data on a single instance.
//! - **L2 (Redis):** Optional distributed cache for shared state across multiple
//!   Maestro instances. Feature-gated behind `redis-cache`.
//!
//! ## Architecture
//!
//! ```text
//! Read:  L1 (Moka) → L2 (Redis) → L3 (SurrealDB)
//! Write: L3 (SurrealDB) → invalidate L2 → invalidate L1
//! ```
//!
//! ## Usage
//!
//! ```rust,no_run
//! use maestro_cache::{CachingMemory, CacheConfig};
//! use openfang_memory::MemorySubstrate;
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let l3 = Arc::new(MemorySubstrate::connect_in_memory().await?);
//! let config = CacheConfig::default();
//! let memory = CachingMemory::new(l3, config).await;
//! // Use `memory` wherever you'd use `MemorySubstrate`
//! # Ok(())
//! # }
//! ```

pub mod l1;
pub mod l2;
pub mod caching_memory;

// Re-export the main types at crate root for convenience.
pub use caching_memory::{CachingMemory, CacheConfig, CacheStats};
pub use l1::{L1Cache, L1Config};
pub use l2::{L2Cache, L2Config};
