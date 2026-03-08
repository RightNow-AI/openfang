# Maestro Project: Phase 5 Blueprint

**Title:** L1/L2 Caching & Shared State Layer  
**Author:** Manus AI  
**Date:** 2026-03-08

## 1. Overview

This document outlines the architecture and implementation plan for Phase 5 of the Maestro project: the introduction of a multi-layer caching system. The primary goal is to significantly reduce latency for frequent read operations and enable horizontal scaling by introducing two new caching tiers on top of the existing SurrealDB (L3) persistence layer.

*   **L1 Cache:** A high-performance, in-process cache using the `moka` crate [1]. This will provide sub-millisecond access to the hottest data for a single Maestro instance.
*   **L2 Cache:** A distributed cache using Redis [2]. This will provide a shared cache for multiple Maestro instances, ensuring consistency and reducing load on the L3 database in a scaled environment.

This implementation will follow the **cache-aside** pattern for reads and a **write-through/invalidate** pattern for writes to ensure data consistency.

## 2. Architecture

The proposed architecture introduces a new `CachingMemory` struct that will wrap the existing `SurrealMemorySubstrate`. This wrapper will manage the L1 and L2 caches, presenting a unified `Memory` trait interface to the `OpenFangKernel`.

### 2.1. Data Flow

The data flow for read and write operations will be as follows:

**Read Operation (Cache-Aside):**

```
Request
   |  
   v
[ CachingMemory ]
   |  
   +--> 1. Check L1 (Moka) Cache
   |      (Hit) -> Return Value
   |      (Miss)
   |         |
   |         v
   +--> 2. Check L2 (Redis) Cache
   |      (Hit) -> Populate L1, Return Value
   |      (Miss)
   |         |
   |         v
   +--> 3. Fetch from L3 (SurrealDB)
          (Hit) -> Populate L2 & L1, Return Value
          (Miss) -> Return None
```

**Write/Delete Operation (Write-Invalidate):**

```
Request
   |  
   v
[ CachingMemory ]
   |  
   +--> 1. Write to L3 (SurrealDB) - Source of Truth
   |         |
   |         v
   +--> 2. Invalidate L2 (Redis) Key
   |         |
   |         v
   +--> 3. Invalidate L1 (Moka) Key
   |         |
   |         v
   +--> Return Result
```

This ensures that the primary database is always up-to-date and that caches are cleared on modification, forcing subsequent reads to fetch the new data.

## 3. Implementation Details

### 3.1. New Crate: `maestro-cache`

A new crate, `maestro-cache`, will be created to house all caching logic. This keeps the implementation modular and decoupled from the kernel and memory substrate.

*   **Dependencies:**
    *   `moka = { version = "0.12", features = ["future"] }`
    *   `redis = { version = "1.0", features = ["tokio-comp", "aio"] }`
    *   `openfang-types` (for `Memory` trait)
    *   `maestro-surreal-memory` (for the L3 backend)
    *   `serde`, `serde_json`

### 3.2. Core Struct: `CachingMemory`

This struct will be the primary entry point for the caching layer.

```rust
pub struct CachingMemory {
    l1_cache: moka::future::Cache<String, Vec<u8>>,
    l2_client: Option<redis::Client>,
    l3_backend: Arc<SurrealMemorySubstrate>,
}

#[async_trait]
impl Memory for CachingMemory {
    // ... implementation of all Memory trait methods ...
}
```

### 3.3. Caching Strategy

Not all data is suitable for caching. The implementation will selectively cache the results of specific `Memory` trait methods.

| Method to Cache | Data Type | L1 TTL | L2 TTL | Max L1 Entries | Notes |
|---|---|---|---|---|---|
| `get` | KV Store | 60s | 300s | 10,000 | High-frequency, low-volatility data. |
| `load_session` | Sessions | 120s | 600s | 1,000 | Caching active sessions reduces DB load. |
| `get_agent` | Agent Configs | 300s | 1800s | 100 | Agent configurations are read often but change rarely. |
| `query_summary` | Usage Summaries | 30s | 60s | 100 | Short TTL for frequently updated usage data. |

**Methods to Exclude from Caching:**

*   `remember`, `recall`: Semantic search results are dynamic and context-dependent.
*   `save_session`, `set`, `delete`: These are write operations.
*   Task queue methods (`task_*`): Require real-time state.
*   Bulk operations (`export`, `import`).

## 4. Configuration

Caching will be configurable via the main application configuration file.

```toml
[cache]
enabled = true

[cache.l1]
max_capacity_kv = 10000
max_capacity_sessions = 1000
ttl_kv_seconds = 60
ttl_sessions_seconds = 120

[cache.l2]
enabled = true # This will be the feature flag
redis_url = "redis://127.0.0.1/"
ttl_kv_seconds = 300
ttl_sessions_seconds = 600
```

The L2 Redis cache will be entirely optional and enabled via a `redis-cache` feature flag in `maestro-cache` and `openfang-kernel`.

## 5. Integration Plan

1.  **Create `maestro-cache` crate:** Scaffold the new crate and add dependencies to its `Cargo.toml`.
2.  **Implement `CachingMemory`:** Implement the struct and the `Memory` trait, applying the cache-aside and write-invalidate logic for the targeted methods.
3.  **Update `openfang-kernel`:**
    *   Modify the `OpenFangKernel` to hold an `Arc<dyn Memory>` instead of a concrete `Arc<SurrealMemorySubstrate>`.
    *   In `OpenFangKernel::boot()`, conditionally construct the `CachingMemory` wrapper if caching is enabled in the configuration. If not, use the `SurrealMemorySubstrate` directly.
4.  **Compile and Test:** Compile the workspace incrementally (`maestro-cache` -> `openfang-kernel` -> etc.) and write unit tests for the `CachingMemory` logic.

## 6. References

[1] Moka Documentation (v0.12). *docs.rs*. Retrieved March 8, 2026, from https://docs.rs/moka/latest/moka/future/struct.Cache.html

[2] Redis-rs Repository. *GitHub*. Retrieved March 8, 2026, from https://github.com/redis-rs/redis-rs

[3] Kanishk S. (2025, April 24). Caching Patterns in Rust: From Memory to Redis Without Going Insane. *Medium*. Retrieved March 8, 2026, from https://medium.com/@kanishks772/caching-patterns-in-rust-from-memory-to-redis-without-going-insane-b12b821a332b

[4] Nawaz D. (2026, February 1). How to Implement Caching Strategies in Rust. *OneUptime Blog*. Retrieved March 8, 2026, from https://oneuptime.com/blog/post/2026-02-01-rust-caching-strategies/view
