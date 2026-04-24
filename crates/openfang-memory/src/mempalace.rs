//! Mempalace MCP backend for external memory.
//!
//! Mempalace is the user's cross-project memory/knowledge-graph system,
//! exposed as an MCP server (see `~/Library/Mobile Documents/com~apple~CloudDocs/
//! mempalace/INTEGRATION_PLAN.md`). For OpenFang it's a **required** / **critical**
//! backend — the daemon fails boot if it is unreachable.
//!
//! # Architecture
//!
//! The backend itself lives here in `openfang-memory` (a leaf crate). The
//! actual MCP wire is abstracted behind the [`MempalaceClient`] trait so the
//! rmcp-based client implementation can live in a higher-level crate (kernel
//! or runtime) that already depends on rmcp. This keeps `openfang-memory`
//! free of an MCP protocol dependency and makes the backend trivially
//! mockable in tests.
//!
//! # Boot-time contract
//!
//! Callers (the kernel, in Phase 6 boot-warm) MUST:
//! 1. Construct a concrete `MempalaceClient` at boot.
//! 2. Call `.health()` and verify the result is `BackendHealth::Ok`.
//! 3. If the verdict is not `Ok`, refuse to finish boot and log a message
//!    pointing at `~/Library/Mobile Documents/com~apple~CloudDocs/mempalace/
//!    INTEGRATION_PLAN.md` so the operator knows exactly what to install.
//!
//! The backend's `criticality` defaults to [`Criticality::Critical`] so the
//! registry's `write_fanout` surfaces write failures even if the backend was
//! (incorrectly) registered without boot-verification.

use crate::external::{BackendHealth, Criticality, ExternalMemoryBackend, Provenance};
use async_trait::async_trait;
use openfang_types::memory::MemoryFragment;
use std::sync::Arc;

/// Default wing tag used when a caller doesn't specify one. The kernel may
/// override this per deployment (e.g. `"wing_openfang"`).
pub const DEFAULT_WING: &str = "wing_openfang";

/// Actionable error message emitted when the backend is unreachable at boot.
/// Callers should surface this verbatim so the operator has a clear next step.
pub const MEMPALACE_UNREACHABLE_REMEDIATION: &str =
    "Mempalace MCP is required and unreachable. Install and start the \
     mempalace server per \
     ~/Library/Mobile Documents/com~apple~CloudDocs/mempalace/INTEGRATION_PLAN.md, \
     then retry boot.";

/// Client trait that the Mempalace backend calls. Implementations wire this
/// to an rmcp client (stdio or http) in a higher-level crate.
#[async_trait]
pub trait MempalaceClient: Send + Sync {
    /// Quick health probe — maps to `mempalace_status` on the MCP server.
    /// Must be cheap (called from `health()` + on boot).
    async fn status(&self) -> Result<(), String>;

    /// Search within a wing. Maps to `mempalace_search`.
    async fn search(
        &self,
        query: &str,
        limit: usize,
        wing: &str,
    ) -> Result<Vec<MemoryFragment>, String>;

    /// Append a fragment to the knowledge graph. Maps to `mempalace_kg_add`.
    async fn kg_add(
        &self,
        fragment: &MemoryFragment,
        provenance: &Provenance,
        wing: &str,
    ) -> Result<(), String>;
}

/// Mempalace backend — implements [`ExternalMemoryBackend`] over any
/// [`MempalaceClient`].
pub struct MempalaceBackend {
    name: &'static str,
    criticality: Criticality,
    client: Arc<dyn MempalaceClient>,
    default_wing: String,
}

impl MempalaceBackend {
    /// Build a new backend. Defaults to [`Criticality::Critical`] and
    /// [`DEFAULT_WING`].
    pub fn new(client: Arc<dyn MempalaceClient>) -> Self {
        Self {
            name: "mempalace",
            criticality: Criticality::Critical,
            client,
            default_wing: DEFAULT_WING.to_string(),
        }
    }

    /// Override the default wing. Typical call:
    /// `MempalaceBackend::new(client).with_wing("wing_openfang")`.
    pub fn with_wing(mut self, wing: impl Into<String>) -> Self {
        self.default_wing = wing.into();
        self
    }

    /// Downgrade criticality. Discouraged for production — the plan's
    /// contract is that Mempalace is required. Provided for tests and for
    /// operators who explicitly disable the strict contract.
    pub fn with_criticality(mut self, criticality: Criticality) -> Self {
        self.criticality = criticality;
        self
    }

    /// Blocking boot-verification helper. Returns `Ok(())` if the server is
    /// reachable, or a formatted error containing the remediation message.
    /// Kernel's boot-warm path calls this; if it returns `Err`, boot fails.
    pub async fn verify_boot(&self) -> Result<(), String> {
        self.client
            .status()
            .await
            .map_err(|e| format!("{MEMPALACE_UNREACHABLE_REMEDIATION} (underlying error: {e})"))
    }
}

#[async_trait]
impl ExternalMemoryBackend for MempalaceBackend {
    fn name(&self) -> &str {
        self.name
    }

    fn criticality(&self) -> Criticality {
        self.criticality
    }

    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryFragment>, String> {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        self.client.search(query, limit, &self.default_wing).await
    }

    async fn write(
        &self,
        fragment: &MemoryFragment,
        provenance: &Provenance,
    ) -> Result<(), String> {
        self.client
            .kg_add(fragment, provenance, &self.default_wing)
            .await
    }

    async fn health(&self) -> BackendHealth {
        match self.client.status().await {
            Ok(()) => BackendHealth::Ok,
            Err(e) => BackendHealth::Failed(format!("mempalace unreachable: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::agent::AgentId;
    use openfang_types::memory::{MemoryId, MemorySource};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    /// Records every call the backend makes for assertions.
    struct StubClient {
        status_ok: Mutex<bool>,
        status_err_msg: Mutex<String>,
        search_result: Mutex<Result<Vec<MemoryFragment>, String>>,
        write_result: Mutex<Result<(), String>>,
        searches: AtomicUsize,
        writes: AtomicUsize,
        last_search: Mutex<Option<(String, usize, String)>>,
        last_write: Mutex<Option<(MemoryFragment, Provenance, String)>>,
    }

    impl StubClient {
        fn new_ok() -> Self {
            Self {
                status_ok: Mutex::new(true),
                status_err_msg: Mutex::new(String::new()),
                search_result: Mutex::new(Ok(Vec::new())),
                write_result: Mutex::new(Ok(())),
                searches: AtomicUsize::new(0),
                writes: AtomicUsize::new(0),
                last_search: Mutex::new(None),
                last_write: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl MempalaceClient for StubClient {
        async fn status(&self) -> Result<(), String> {
            if *self.status_ok.lock().unwrap() {
                Ok(())
            } else {
                Err(self.status_err_msg.lock().unwrap().clone())
            }
        }
        async fn search(
            &self,
            query: &str,
            limit: usize,
            wing: &str,
        ) -> Result<Vec<MemoryFragment>, String> {
            self.searches.fetch_add(1, Ordering::SeqCst);
            *self.last_search.lock().unwrap() =
                Some((query.to_string(), limit, wing.to_string()));
            self.search_result.lock().unwrap().clone()
        }
        async fn kg_add(
            &self,
            fragment: &MemoryFragment,
            provenance: &Provenance,
            wing: &str,
        ) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            *self.last_write.lock().unwrap() =
                Some((fragment.clone(), provenance.clone(), wing.to_string()));
            self.write_result.lock().unwrap().clone()
        }
    }

    fn sample_fragment(content: &str) -> MemoryFragment {
        MemoryFragment {
            id: MemoryId::new(),
            agent_id: AgentId::new(),
            content: content.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            source: MemorySource::Conversation,
            confidence: 0.9,
            created_at: chrono::Utc::now(),
            accessed_at: chrono::Utc::now(),
            access_count: 0,
            scope: "default".to_string(),
        }
    }

    #[tokio::test]
    async fn default_criticality_is_critical() {
        let b = MempalaceBackend::new(Arc::new(StubClient::new_ok()));
        assert_eq!(b.criticality(), Criticality::Critical);
    }

    #[tokio::test]
    async fn with_criticality_overrides_default() {
        let b = MempalaceBackend::new(Arc::new(StubClient::new_ok()))
            .with_criticality(Criticality::Degraded);
        assert_eq!(b.criticality(), Criticality::Degraded);
    }

    #[tokio::test]
    async fn default_wing_is_wing_openfang() {
        let client = Arc::new(StubClient::new_ok());
        let b = MempalaceBackend::new(client.clone());
        b.search("anything", 5).await.unwrap();
        let (_, _, wing) = client.last_search.lock().unwrap().clone().unwrap();
        assert_eq!(wing, DEFAULT_WING);
    }

    #[tokio::test]
    async fn with_wing_overrides_default() {
        let client = Arc::new(StubClient::new_ok());
        let b = MempalaceBackend::new(client.clone()).with_wing("wing_custom");
        b.search("q", 1).await.unwrap();
        let (_, _, wing) = client.last_search.lock().unwrap().clone().unwrap();
        assert_eq!(wing, "wing_custom");
    }

    #[tokio::test]
    async fn health_ok_when_client_status_ok() {
        let b = MempalaceBackend::new(Arc::new(StubClient::new_ok()));
        assert_eq!(b.health().await, BackendHealth::Ok);
    }

    #[tokio::test]
    async fn health_failed_when_client_status_errs() {
        let client = StubClient::new_ok();
        *client.status_ok.lock().unwrap() = false;
        *client.status_err_msg.lock().unwrap() = "mcp closed".into();
        let b = MempalaceBackend::new(Arc::new(client));
        match b.health().await {
            BackendHealth::Failed(m) => {
                assert!(m.contains("mempalace unreachable"));
                assert!(m.contains("mcp closed"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn verify_boot_passes_when_client_ok() {
        let b = MempalaceBackend::new(Arc::new(StubClient::new_ok()));
        assert!(b.verify_boot().await.is_ok());
    }

    #[tokio::test]
    async fn verify_boot_includes_remediation_message() {
        let client = StubClient::new_ok();
        *client.status_ok.lock().unwrap() = false;
        *client.status_err_msg.lock().unwrap() = "spawn failed".into();
        let b = MempalaceBackend::new(Arc::new(client));
        let err = b.verify_boot().await.unwrap_err();
        assert!(err.contains("INTEGRATION_PLAN.md"));
        assert!(err.contains("spawn failed"));
    }

    #[tokio::test]
    async fn search_short_circuits_on_empty_query() {
        let client = Arc::new(StubClient::new_ok());
        let b = MempalaceBackend::new(client.clone());
        assert!(b.search("", 10).await.unwrap().is_empty());
        assert!(b.search("   ", 10).await.unwrap().is_empty());
        // Client was never hit.
        assert_eq!(client.searches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn search_short_circuits_on_zero_limit() {
        let client = Arc::new(StubClient::new_ok());
        let b = MempalaceBackend::new(client.clone());
        assert!(b.search("q", 0).await.unwrap().is_empty());
        assert_eq!(client.searches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn search_forwards_results_from_client() {
        let frag = sample_fragment("from-mempalace");
        let client = StubClient::new_ok();
        *client.search_result.lock().unwrap() = Ok(vec![frag.clone()]);
        let client = Arc::new(client);
        let b = MempalaceBackend::new(client.clone());
        let got = b.search("x", 5).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].content, "from-mempalace");
        let (q, lim, _wing) = client.last_search.lock().unwrap().clone().unwrap();
        assert_eq!(q, "x");
        assert_eq!(lim, 5);
    }

    #[tokio::test]
    async fn search_surfaces_client_error() {
        let client = StubClient::new_ok();
        *client.search_result.lock().unwrap() = Err("oops".into());
        let b = MempalaceBackend::new(Arc::new(client));
        let err = b.search("x", 1).await.unwrap_err();
        assert_eq!(err, "oops");
    }

    #[tokio::test]
    async fn write_forwards_to_kg_add_with_wing() {
        let client = Arc::new(StubClient::new_ok());
        let b = MempalaceBackend::new(client.clone()).with_wing("wing_test");
        let frag = sample_fragment("hello");
        let prov = Provenance {
            source: "scrape".into(),
            untrusted: true,
            source_url: Some("https://ex.com".into()),
            scan_results: None,
        };
        b.write(&frag, &prov).await.unwrap();
        assert_eq!(client.writes.load(Ordering::SeqCst), 1);
        let (got_frag, got_prov, wing) = client.last_write.lock().unwrap().clone().unwrap();
        assert_eq!(got_frag.content, "hello");
        assert_eq!(got_prov.source, "scrape");
        assert!(got_prov.untrusted);
        assert_eq!(wing, "wing_test");
    }

    #[tokio::test]
    async fn write_surfaces_client_error_as_critical_for_registry() {
        // When this backend is wrapped in ExternalBackends and the client
        // errors, write_fanout MUST surface the error (criticality=Critical).
        use crate::external::ExternalBackends;
        let client = StubClient::new_ok();
        *client.write_result.lock().unwrap() = Err("mcp write timeout".into());
        let backend = MempalaceBackend::new(Arc::new(client));
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(backend));

        let err = reg
            .write_fanout(&sample_fragment("x"), &Provenance::default())
            .await
            .unwrap_err();
        assert!(err.contains("mempalace"));
        assert!(err.contains("mcp write timeout"));
    }

    #[tokio::test]
    async fn registry_aggregate_health_fails_when_mempalace_critical_fails() {
        use crate::external::ExternalBackends;
        let client = StubClient::new_ok();
        *client.status_ok.lock().unwrap() = false;
        *client.status_err_msg.lock().unwrap() = "not running".into();
        let backend = MempalaceBackend::new(Arc::new(client));
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(backend));

        match reg.aggregate_health().await {
            BackendHealth::Failed(m) => {
                assert!(m.contains("mempalace"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
