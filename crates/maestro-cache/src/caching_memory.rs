//! CachingMemory: A transparent wrapper around `MemorySubstrate` that
//! adds L1 (Moka) and L2 (Redis) caching tiers.
//!
//! This struct implements the `Memory` trait and also exposes all
//! `MemorySubstrate`-specific methods via delegation, so it can be
//! used as a drop-in replacement in the kernel.

use crate::l1::{L1Cache, L1Config};
use crate::l2::{L2Cache, L2Config};

use async_trait::async_trait;
use openfang_memory::MemorySubstrate;
use openfang_types::agent::{AgentEntry, AgentId, SessionId};
use openfang_types::error::OpenFangResult;
use openfang_types::memory::*;
use openfang_types::session::Session;
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info};

/// Configuration for the entire caching layer.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Whether caching is enabled at all.
    pub enabled: bool,
    /// L1 (Moka) configuration for KV store operations.
    pub l1_kv: L1Config,
    /// L1 (Moka) configuration for session caching.
    pub l1_sessions: L1Config,
    /// L1 (Moka) configuration for agent config caching.
    pub l1_agents: L1Config,
    /// L2 (Redis) configuration. If `None`, L2 is disabled.
    pub l2: Option<L2Config>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            l1_kv: L1Config {
                max_capacity: 10_000,
                ttl: Duration::from_secs(60),
                tti: None,
            },
            l1_sessions: L1Config {
                max_capacity: 1_000,
                ttl: Duration::from_secs(120),
                tti: Some(Duration::from_secs(60)),
            },
            l1_agents: L1Config {
                max_capacity: 100,
                ttl: Duration::from_secs(300),
                tti: None,
            },
            l2: None, // Redis disabled by default
        }
    }
}

/// The main caching wrapper around `MemorySubstrate`.
///
/// Provides transparent L1/L2 caching for read-heavy operations while
/// delegating all writes to the L3 backend and invalidating caches.
pub struct CachingMemory {
    /// L3 backend: the source of truth.
    l3: Arc<MemorySubstrate>,
    /// L1 cache for KV store operations (get/set/delete).
    l1_kv: L1Cache,
    /// L1 cache for session data.
    l1_sessions: L1Cache,
    /// L1 cache for agent configurations.
    l1_agents: L1Cache,
    /// L2 distributed cache (may be disabled).
    l2: L2Cache,
    /// Whether caching is active.
    enabled: bool,
}

impl CachingMemory {
    /// Create a new CachingMemory wrapping the given SurrealDB backend.
    pub async fn new(l3: Arc<MemorySubstrate>, config: CacheConfig) -> Self {
        let l2 = if let Some(l2_config) = &config.l2 {
            L2Cache::new(l2_config.clone()).await
        } else {
            L2Cache::disabled()
        };

        let l1_kv = L1Cache::new("kv", &config.l1_kv);
        let l1_sessions = L1Cache::new("sessions", &config.l1_sessions);
        let l1_agents = L1Cache::new("agents", &config.l1_agents);

        info!(
            enabled = config.enabled,
            l2_connected = l2.is_connected(),
            l1_kv_capacity = config.l1_kv.max_capacity,
            l1_sessions_capacity = config.l1_sessions.max_capacity,
            l1_agents_capacity = config.l1_agents.max_capacity,
            "CachingMemory initialized"
        );

        Self {
            l3,
            l1_kv,
            l1_sessions,
            l1_agents,
            l2,
            enabled: config.enabled,
        }
    }

    /// Create a CachingMemory with caching disabled (pure passthrough to L3).
    pub fn passthrough(l3: Arc<MemorySubstrate>) -> Self {
        Self {
            l3,
            l1_kv: L1Cache::new("kv_disabled", &L1Config { max_capacity: 0, ..Default::default() }),
            l1_sessions: L1Cache::new("sessions_disabled", &L1Config { max_capacity: 0, ..Default::default() }),
            l1_agents: L1Cache::new("agents_disabled", &L1Config { max_capacity: 0, ..Default::default() }),
            l2: L2Cache::disabled(),
            enabled: false,
        }
    }

    /// Get a reference to the underlying L3 backend.
    pub fn l3(&self) -> &Arc<MemorySubstrate> {
        &self.l3
    }

    /// Get cache statistics for monitoring.
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            enabled: self.enabled,
            l1_kv_entries: self.l1_kv.entry_count(),
            l1_sessions_entries: self.l1_sessions.entry_count(),
            l1_agents_entries: self.l1_agents.entry_count(),
            l2_connected: self.l2.is_connected(),
        }
    }

    /// Invalidate all caches (L1 + L2 for a given namespace).
    pub async fn invalidate_all(&self) {
        self.l1_kv.invalidate_all().await;
        self.l1_sessions.invalidate_all().await;
        self.l1_agents.invalidate_all().await;
        self.l2.invalidate_namespace("kv").await;
        self.l2.invalidate_namespace("sessions").await;
        self.l2.invalidate_namespace("agents").await;
        debug!("All caches invalidated");
    }

    // ── Key helpers ──────────────────────────────────────────────────

    fn kv_cache_key(agent_id: AgentId, key: &str) -> String {
        format!("{}:{}", agent_id, key)
    }

    fn session_cache_key(session_id: SessionId) -> String {
        format!("{}", session_id)
    }

    fn agent_cache_key(agent_id: AgentId) -> String {
        format!("{}", agent_id)
    }

    // ── Delegated MemorySubstrate-specific methods ────────────
    // These methods are NOT on the Memory trait but are used by the kernel.
    // They delegate directly to L3, with caching where appropriate.

    /// Save an agent entry (write-through + invalidate).
    pub async fn save_agent(&self, entry: &AgentEntry) -> OpenFangResult<()> {
        self.l3.save_agent(entry).await?;
        if self.enabled {
            let key = Self::agent_cache_key(entry.id);
            self.l1_agents.invalidate(&key).await;
            self.l2.invalidate("agents", &key).await;
        }
        Ok(())
    }

    /// Load all agents (cached).
    pub async fn load_all_agents(&self) -> OpenFangResult<Vec<AgentEntry>> {
        if self.enabled {
            // Cache the full agent list under a special key
            if let Some(agents) = self.l1_agents.get::<Vec<AgentEntry>>("__all__").await {
                return Ok(agents);
            }
            if let Some(agents) = self.l2.get::<Vec<AgentEntry>>("agents", "__all__").await {
                self.l1_agents.insert("__all__", &agents).await;
                return Ok(agents);
            }
        }
        let agents = self.l3.load_all_agents().await?;
        if self.enabled {
            self.l1_agents.insert("__all__", &agents).await;
            self.l2.insert("agents", "__all__", &agents, None).await;
        }
        Ok(agents)
    }

    /// Remove an agent (write-through + invalidate).
    pub async fn remove_agent(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.l3.remove_agent(agent_id).await?;
        if self.enabled {
            let key = Self::agent_cache_key(agent_id);
            self.l1_agents.invalidate(&key).await;
            self.l1_agents.invalidate("__all__").await;
            self.l2.invalidate("agents", &key).await;
            self.l2.invalidate("agents", "__all__").await;
        }
        Ok(())
    }

    /// Get a session by ID (cached).
    pub async fn get_session(&self, session_id: SessionId) -> OpenFangResult<Option<Session>> {
        let cache_key = Self::session_cache_key(session_id);
        if self.enabled {
            if let Some(session) = self.l1_sessions.get::<Session>(&cache_key).await {
                return Ok(Some(session));
            }
            if let Some(session) = self.l2.get::<Session>("sessions", &cache_key).await {
                self.l1_sessions.insert(&cache_key, &session).await;
                return Ok(Some(session));
            }
        }
        let session = self.l3.get_session(session_id).await?;
        if self.enabled {
            if let Some(ref s) = session {
                self.l1_sessions.insert(&cache_key, s).await;
                self.l2.insert("sessions", &cache_key, s, None).await;
            }
        }
        Ok(session)
    }

    /// Create a session (write-through + invalidate).
    pub async fn create_session(&self, agent_id: AgentId) -> OpenFangResult<Session> {
        let session = self.l3.create_session(agent_id).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session.id);
            self.l1_sessions.insert(&cache_key, &session).await;
            self.l2.insert("sessions", &cache_key, &session, None).await;
        }
        Ok(session)
    }

    /// Create a session with a label (write-through + cache).
    pub async fn create_session_with_label(
        &self,
        agent_id: AgentId,
        label: Option<&str>,
    ) -> OpenFangResult<Session> {
        let session = self.l3.create_session_with_label(agent_id, label).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session.id);
            self.l1_sessions.insert(&cache_key, &session).await;
            self.l2.insert("sessions", &cache_key, &session, None).await;
        }
        Ok(session)
    }

    /// Save a session (write-through + invalidate).
    pub async fn save_session(&self, session: &Session) -> OpenFangResult<()> {
        self.l3.save_session(session).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session.id);
            self.l1_sessions.invalidate(&cache_key).await;
            self.l2.invalidate("sessions", &cache_key).await;
        }
        Ok(())
    }

    /// Delete a session (write-through + invalidate).
    pub async fn delete_session(&self, session_id: SessionId) -> OpenFangResult<()> {
        self.l3.delete_session(session_id).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session_id);
            self.l1_sessions.invalidate(&cache_key).await;
            self.l2.invalidate("sessions", &cache_key).await;
        }
        Ok(())
    }

    /// Delete all sessions for an agent (write-through + invalidate namespace).
    pub async fn delete_agent_sessions(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.l3.delete_agent_sessions(agent_id).await?;
        if self.enabled {
            // We can't know which session IDs to invalidate, so clear all sessions
            self.l1_sessions.invalidate_all().await;
            self.l2.invalidate_namespace("sessions").await;
        }
        Ok(())
    }

    /// Delete the canonical session for an agent.
    pub async fn delete_canonical_session(&self, agent_id: AgentId) -> OpenFangResult<()> {
        self.l3.delete_canonical_session(agent_id).await?;
        Ok(())
    }

    /// List sessions for an agent (not cached — returns metadata, not full sessions).
    pub async fn list_agent_sessions(
        &self,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<JsonValue>> {
        self.l3.list_agent_sessions(agent_id).await
    }

    /// List all sessions (not cached — admin operation).
    pub async fn list_sessions(&self) -> OpenFangResult<Vec<JsonValue>> {
        self.l3.list_sessions().await
    }

    /// Set a session label (write-through + invalidate).
    pub async fn set_session_label(
        &self,
        session_id: SessionId,
        label: Option<String>,
    ) -> OpenFangResult<()> {
        self.l3.set_session_label(session_id, label).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session_id);
            self.l1_sessions.invalidate(&cache_key).await;
            self.l2.invalidate("sessions", &cache_key).await;
        }
        Ok(())
    }

    /// Find a session by label (not cached — infrequent lookup).
    pub async fn find_session_by_label(
        &self,
        agent_id: AgentId,
        label: &str,
    ) -> OpenFangResult<Option<Session>> {
        self.l3.find_session_by_label(agent_id, label).await
    }

    /// Structured get (cached via KV cache).
    pub async fn structured_get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<Option<JsonValue>> {
        let cache_key = Self::kv_cache_key(agent_id, key);
        if self.enabled {
            if let Some(value) = self.l1_kv.get::<JsonValue>(&cache_key).await {
                return Ok(Some(value));
            }
            if let Some(value) = self.l2.get::<JsonValue>("kv", &cache_key).await {
                self.l1_kv.insert(&cache_key, &value).await;
                return Ok(Some(value));
            }
        }
        let value = self.l3.structured_get(agent_id, key).await?;
        if self.enabled {
            if let Some(ref v) = value {
                self.l1_kv.insert(&cache_key, v).await;
                self.l2.insert("kv", &cache_key, v, None).await;
            }
        }
        Ok(value)
    }

    /// Structured set (write-through + invalidate).
    pub async fn structured_set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: JsonValue,
    ) -> OpenFangResult<()> {
        self.l3.structured_set(agent_id, key, value).await?;
        if self.enabled {
            let cache_key = Self::kv_cache_key(agent_id, key);
            self.l1_kv.invalidate(&cache_key).await;
            self.l2.invalidate("kv", &cache_key).await;
        }
        Ok(())
    }

    /// Structured delete (write-through + invalidate).
    pub async fn structured_delete(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<()> {
        self.l3.structured_delete(agent_id, key).await?;
        if self.enabled {
            let cache_key = Self::kv_cache_key(agent_id, key);
            self.l1_kv.invalidate(&cache_key).await;
            self.l2.invalidate("kv", &cache_key).await;
        }
        Ok(())
    }

    /// List KV pairs (not cached — admin/debug operation).
    pub async fn list_kv(
        &self,
        agent_id: AgentId,
    ) -> OpenFangResult<Vec<(String, JsonValue)>> {
        self.l3.list_kv(agent_id).await
    }

    /// Get canonical context (not cached — dynamic, context-dependent).
    pub async fn canonical_context(
        &self,
        agent_id: AgentId,
        limit: Option<usize>,
    ) -> OpenFangResult<Vec<openfang_types::message::Message>> {
        self.l3.canonical_context(agent_id, limit).await
    }

    /// Append to canonical session (write-through, no cache).
    pub async fn append_canonical(
        &self,
        agent_id: AgentId,
        messages: &[openfang_types::message::Message],
        limit: Option<usize>,
    ) -> OpenFangResult<()> {
        self.l3.append_canonical(agent_id, messages, limit).await
    }

    /// Write JSONL mirror (sync, passthrough).
    pub fn write_jsonl_mirror(
        &self,
        session: &Session,
        path: &Path,
    ) -> OpenFangResult<()> {
        self.l3.write_jsonl_mirror(session, path)
    }

    /// Store LLM summary (write-through, no cache).
    pub async fn store_llm_summary(
        &self,
        agent_id: AgentId,
        summary: &str,
        kept_messages: Vec<openfang_types::message::Message>,
    ) -> OpenFangResult<()> {
        self.l3.store_llm_summary(agent_id, summary, kept_messages).await
    }

    /// Load paired devices (not cached — infrequent).
    pub async fn load_paired_devices(&self) -> OpenFangResult<Vec<JsonValue>> {
        self.l3.load_paired_devices().await
    }

    /// Save a paired device (write-through).
    pub async fn save_paired_device(&self, device: JsonValue) -> OpenFangResult<()> {
        self.l3.save_paired_device(device).await
    }

    /// Remove a paired device (write-through).
    pub async fn remove_paired_device(&self, device_id: &str) -> OpenFangResult<()> {
        self.l3.remove_paired_device(device_id).await
    }

    /// Post a task (write-through).
    pub async fn task_post(
        &self,
        title: &str,
        description: &str,
        assigned_to: Option<&str>,
        created_by: Option<&str>,
    ) -> OpenFangResult<String> {
        self.l3.task_post(title, description, assigned_to, created_by).await
    }

    /// Claim a task (write-through).
    pub async fn task_claim(&self, agent_id: &str) -> OpenFangResult<Option<JsonValue>> {
        self.l3.task_claim(agent_id).await
    }

    /// Complete a task (write-through).
    pub async fn task_complete(&self, task_id: &str, result: &str) -> OpenFangResult<()> {
        self.l3.task_complete(task_id, result).await
    }

    /// List tasks (not cached — real-time).
    pub async fn task_list(&self, status: Option<&str>) -> OpenFangResult<Vec<JsonValue>> {
        self.l3.task_list(status).await
    }

    /// Get the usage store reference.
    pub fn usage(&self) -> &openfang_memory::SurrealUsageStore {
        self.l3.usage()
    }
}

// ── Memory trait implementation ──────────────────────────────────────────

#[async_trait]
impl Memory for CachingMemory {
    async fn get(
        &self,
        agent_id: AgentId,
        key: &str,
    ) -> OpenFangResult<Option<JsonValue>> {
        self.structured_get(agent_id, key).await
    }

    async fn set(
        &self,
        agent_id: AgentId,
        key: &str,
        value: JsonValue,
    ) -> OpenFangResult<()> {
        self.structured_set(agent_id, key, value).await
    }

    async fn delete(&self, agent_id: AgentId, key: &str) -> OpenFangResult<()> {
        self.structured_delete(agent_id, key).await
    }

    // Semantic operations — NOT cached (dynamic, embedding-dependent)

    async fn remember(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, JsonValue>,
    ) -> OpenFangResult<MemoryId> {
        self.l3.remember(agent_id, content, source, scope, metadata).await
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        self.l3.recall(query, limit, filter).await
    }

    async fn forget(&self, id: MemoryId) -> OpenFangResult<()> {
        self.l3.forget(id).await
    }

    // Knowledge graph — NOT cached

    async fn add_entity(&self, entity: Entity) -> OpenFangResult<String> {
        self.l3.add_entity(entity).await
    }

    async fn add_relation(&self, relation: Relation) -> OpenFangResult<String> {
        self.l3.add_relation(relation).await
    }

    async fn query_graph(
        &self,
        pattern: GraphPattern,
    ) -> OpenFangResult<Vec<GraphMatch>> {
        self.l3.query_graph(pattern).await
    }

    // Session persistence

    async fn save_session(
        &self,
        session: &openfang_types::session::Session,
    ) -> OpenFangResult<()> {
        // Delegate to the inherent save_session method
        self.l3.save_session(session).await?;
        if self.enabled {
            let cache_key = Self::session_cache_key(session.id);
            self.l1_sessions.invalidate(&cache_key).await;
            self.l2.invalidate("sessions", &cache_key).await;
        }
        Ok(())
    }

    // Embedding-aware operations — NOT cached

    async fn recall_with_embedding_async(
        &self,
        query: &str,
        limit: usize,
        filter: Option<MemoryFilter>,
        query_embedding: Option<&[f32]>,
    ) -> OpenFangResult<Vec<MemoryFragment>> {
        self.l3
            .recall_with_embedding_async(query, limit, filter, query_embedding)
            .await
    }

    async fn remember_with_embedding_async(
        &self,
        agent_id: AgentId,
        content: &str,
        source: MemorySource,
        scope: &str,
        metadata: HashMap<String, JsonValue>,
        embedding: Option<&[f32]>,
    ) -> OpenFangResult<MemoryId> {
        self.l3
            .remember_with_embedding_async(agent_id, content, source, scope, metadata, embedding)
            .await
    }

    // Maintenance — NOT cached

    async fn consolidate(&self) -> OpenFangResult<ConsolidationReport> {
        // Invalidate caches after consolidation since data may have changed
        let report = self.l3.consolidate().await?;
        if self.enabled {
            self.invalidate_all().await;
        }
        Ok(report)
    }

    async fn export(&self, format: ExportFormat) -> OpenFangResult<Vec<u8>> {
        self.l3.export(format).await
    }

    async fn import(
        &self,
        data: &[u8],
        format: ExportFormat,
    ) -> OpenFangResult<ImportReport> {
        let report = self.l3.import(data, format).await?;
        if self.enabled {
            self.invalidate_all().await;
        }
        Ok(report)
    }
}

/// Cache statistics for monitoring and debugging.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheStats {
    pub enabled: bool,
    pub l1_kv_entries: u64,
    pub l1_sessions_entries: u64,
    pub l1_agents_entries: u64,
    pub l2_connected: bool,
}
