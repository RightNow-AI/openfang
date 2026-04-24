//! External memory backends — Obsidian vault, Mempalace MCP, and friends.
//!
//! The native `MemorySubstrate` (SQLite + semantic + KG) is always present.
//! External backends augment it with read-union (reads merge results from
//! every registered backend) and write-fanout (writes propagate to every
//! registered backend) semantics, with per-backend error isolation.
//!
//! Each backend declares a [`Criticality`]:
//!
//! * [`Criticality::Critical`] — a failed write is treated as a write-fanout
//!   failure (surfaces up). Boot-time health failures should fail the daemon
//!   boot (enforced by callers, not this module).
//! * [`Criticality::Degraded`] — a failed write is logged and dropped.
//!   Health reports it as degraded.
//! * [`Criticality::Optional`] — a failed write is logged at debug level.
//!   Health reports it as degraded but noise is kept low.
//!
//! Read paths are always best-effort: a backend that fails search contributes
//! zero results, never an error to the caller.

use async_trait::async_trait;
use openfang_types::memory::MemoryFragment;
use tracing::{debug, warn};

/// How catastrophic a backend's failure is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criticality {
    /// Write failures surface as errors. Boot-time health failures should
    /// block daemon startup (enforced by callers).
    Critical,
    /// Write failures are logged at warn and dropped. Reports as degraded on
    /// health.
    Degraded,
    /// Write failures are logged at debug. Reports as degraded on health.
    Optional,
}

/// A backend's self-reported health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackendHealth {
    /// Reachable and serving requests.
    Ok,
    /// Reachable but in a reduced-capability state (e.g. slow, partial data).
    Degraded(String),
    /// Unreachable or returning errors.
    Failed(String),
}

/// Provenance tag carried with every external write so backends can persist
/// it as frontmatter / metadata for later auditability.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// Short label — "scrape", "obsidian_import", "user_input", "agent_reflection", …
    pub source: String,
    /// True if the content came from an untrusted channel (web scrape, inbound
    /// channel message, etc.). Backends should mark these prominently.
    pub untrusted: bool,
    /// URL the content originated from, if any.
    pub source_url: Option<String>,
    /// Free-form scanner verdicts attached by the triage pipeline (Phase 5).
    pub scan_results: Option<serde_json::Value>,
}

/// Trait implemented by external memory backends (Obsidian, Mempalace, …).
#[async_trait]
pub trait ExternalMemoryBackend: Send + Sync {
    /// Stable short name used in logs and health output.
    fn name(&self) -> &str;

    /// How the registry should treat this backend's failures.
    fn criticality(&self) -> Criticality;

    /// Full-text / semantic search. Errors are mapped to "no results" by the
    /// registry — the returned Err is for logging only, never surfaced to the
    /// Memory-trait caller.
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryFragment>, String>;

    /// Persist a fragment with provenance metadata.
    async fn write(
        &self,
        fragment: &MemoryFragment,
        provenance: &Provenance,
    ) -> Result<(), String>;

    /// Self-reported health. Must be cheap — called during `/api/health`
    /// polling.
    async fn health(&self) -> BackendHealth;
}

/// Registry of external backends. Owned by the kernel alongside the native
/// `MemorySubstrate`.
#[derive(Default)]
pub struct ExternalBackends {
    backends: Vec<Box<dyn ExternalMemoryBackend>>,
}

impl ExternalBackends {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            backends: Vec::new(),
        }
    }

    /// Register a backend. Order matters for deterministic read-union output
    /// — results are appended in registration order.
    pub fn push(&mut self, backend: Box<dyn ExternalMemoryBackend>) {
        debug!(name = backend.name(), "external memory backend registered");
        self.backends.push(backend);
    }

    /// Number of registered backends.
    pub fn len(&self) -> usize {
        self.backends.len()
    }

    /// True if no backends registered.
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Read-union across all backends. A backend that errors contributes zero
    /// results and a warn-level log entry.
    pub async fn search_union(&self, query: &str, limit: usize) -> Vec<MemoryFragment> {
        let mut all = Vec::new();
        for backend in &self.backends {
            match backend.search(query, limit).await {
                Ok(mut results) => {
                    all.append(&mut results);
                }
                Err(e) => {
                    warn!(
                        backend = backend.name(),
                        error = %e,
                        "external backend search failed; contributing no results"
                    );
                }
            }
        }
        all
    }

    /// Write-fanout. Returns `Err` only when a [`Criticality::Critical`]
    /// backend failed; non-critical failures are logged and dropped.
    pub async fn write_fanout(
        &self,
        fragment: &MemoryFragment,
        provenance: &Provenance,
    ) -> Result<(), String> {
        let mut critical_errors: Vec<String> = Vec::new();
        for backend in &self.backends {
            match backend.write(fragment, provenance).await {
                Ok(()) => {}
                Err(e) => match backend.criticality() {
                    Criticality::Critical => {
                        critical_errors.push(format!("{}: {e}", backend.name()));
                    }
                    Criticality::Degraded => {
                        warn!(
                            backend = backend.name(),
                            error = %e,
                            "degraded external backend write failed; dropping"
                        );
                    }
                    Criticality::Optional => {
                        debug!(
                            backend = backend.name(),
                            error = %e,
                            "optional external backend write failed; dropping"
                        );
                    }
                },
            }
        }
        if critical_errors.is_empty() {
            Ok(())
        } else {
            Err(critical_errors.join("; "))
        }
    }

    /// Per-backend health snapshot.
    pub async fn health_summary(&self) -> Vec<BackendHealthEntry> {
        let mut out = Vec::with_capacity(self.backends.len());
        for backend in &self.backends {
            let health = backend.health().await;
            out.push(BackendHealthEntry {
                name: backend.name().to_string(),
                criticality: backend.criticality(),
                health,
            });
        }
        out
    }

    /// Aggregate verdict for top-level `/api/health`:
    /// - `Ok` iff every backend is `Ok`.
    /// - `Failed` iff any *critical* backend is `Failed`.
    /// - `Degraded` otherwise (any non-critical backend is non-Ok).
    pub async fn aggregate_health(&self) -> BackendHealth {
        let entries = self.health_summary().await;
        let mut has_non_ok = false;
        for e in &entries {
            match &e.health {
                BackendHealth::Ok => {}
                BackendHealth::Failed(reason) if e.criticality == Criticality::Critical => {
                    return BackendHealth::Failed(format!("{}: {reason}", e.name));
                }
                _ => has_non_ok = true,
            }
        }
        if has_non_ok {
            BackendHealth::Degraded(format!(
                "{} non-ok external backend(s)",
                entries
                    .iter()
                    .filter(|e| !matches!(e.health, BackendHealth::Ok))
                    .count()
            ))
        } else {
            BackendHealth::Ok
        }
    }
}

/// Entry in [`ExternalBackends::health_summary`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendHealthEntry {
    pub name: String,
    pub criticality: Criticality,
    pub health: BackendHealth,
}

#[cfg(test)]
mod tests {
    use super::*;
    use openfang_types::agent::AgentId;
    use openfang_types::memory::{MemoryId, MemorySource};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct StubBackend {
        name_: &'static str,
        criticality_: Criticality,
        search_result: Result<Vec<MemoryFragment>, String>,
        write_result: Result<(), String>,
        health_result: BackendHealth,
        writes: Arc<AtomicUsize>,
    }

    impl StubBackend {
        fn new(
            name_: &'static str,
            criticality_: Criticality,
            search_result: Result<Vec<MemoryFragment>, String>,
            write_result: Result<(), String>,
            health_result: BackendHealth,
        ) -> Self {
            Self {
                name_,
                criticality_,
                search_result,
                write_result,
                health_result,
                writes: Arc::new(AtomicUsize::new(0)),
            }
        }
    }

    #[async_trait]
    impl ExternalMemoryBackend for StubBackend {
        fn name(&self) -> &str {
            self.name_
        }
        fn criticality(&self) -> Criticality {
            self.criticality_
        }
        async fn search(&self, _query: &str, _limit: usize) -> Result<Vec<MemoryFragment>, String> {
            self.search_result.clone()
        }
        async fn write(
            &self,
            _fragment: &MemoryFragment,
            _provenance: &Provenance,
        ) -> Result<(), String> {
            self.writes.fetch_add(1, Ordering::SeqCst);
            self.write_result.clone()
        }
        async fn health(&self) -> BackendHealth {
            self.health_result.clone()
        }
    }

    fn sample_fragment(content: &str) -> MemoryFragment {
        MemoryFragment {
            id: MemoryId::new(),
            agent_id: AgentId::new(),
            content: content.to_string(),
            embedding: None,
            metadata: HashMap::new(),
            source: MemorySource::UserProvided,
            confidence: 0.9,
            created_at: chrono::Utc::now(),
            accessed_at: chrono::Utc::now(),
            access_count: 0,
            scope: "test".to_string(),
        }
    }

    #[tokio::test]
    async fn empty_registry_returns_empty_union_and_success_fanout() {
        let reg = ExternalBackends::new();
        assert!(reg.search_union("x", 10).await.is_empty());
        let frag = sample_fragment("hi");
        assert!(
            reg.write_fanout(&frag, &Provenance::default())
                .await
                .is_ok()
        );
        assert_eq!(reg.aggregate_health().await, BackendHealth::Ok);
    }

    #[tokio::test]
    async fn search_union_appends_in_registration_order() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![sample_fragment("from-a")]),
            Ok(()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "b",
            Criticality::Optional,
            Ok(vec![sample_fragment("from-b")]),
            Ok(()),
            BackendHealth::Ok,
        )));
        let results = reg.search_union("x", 10).await;
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "from-a");
        assert_eq!(results[1].content, "from-b");
    }

    #[tokio::test]
    async fn search_isolates_per_backend_errors() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "ok",
            Criticality::Optional,
            Ok(vec![sample_fragment("from-ok")]),
            Ok(()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "broken",
            Criticality::Optional,
            Err("boom".into()),
            Ok(()),
            BackendHealth::Ok,
        )));
        let results = reg.search_union("x", 10).await;
        // Broken backend contributes nothing; ok backend still delivers.
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "from-ok");
    }

    #[tokio::test]
    async fn write_fanout_errors_only_when_critical_backend_fails() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "degraded",
            Criticality::Degraded,
            Ok(vec![]),
            Err("slow disk".into()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "optional",
            Criticality::Optional,
            Ok(vec![]),
            Err("oops".into()),
            BackendHealth::Ok,
        )));
        // No critical failure → write_fanout succeeds overall
        let frag = sample_fragment("x");
        assert!(
            reg.write_fanout(&frag, &Provenance::default())
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn write_fanout_surfaces_critical_failure() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "ok",
            Criticality::Degraded,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "mempalace",
            Criticality::Critical,
            Ok(vec![]),
            Err("mcp down".into()),
            BackendHealth::Failed("mcp down".into()),
        )));
        let frag = sample_fragment("x");
        let err = reg
            .write_fanout(&frag, &Provenance::default())
            .await
            .unwrap_err();
        assert!(err.contains("mempalace"));
        assert!(err.contains("mcp down"));
    }

    #[tokio::test]
    async fn write_fanout_writes_to_every_backend_when_all_ok() {
        let a = StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        );
        let b = StubBackend::new(
            "b",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        );
        let a_writes = a.writes.clone();
        let b_writes = b.writes.clone();
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(a));
        reg.push(Box::new(b));

        let frag = sample_fragment("x");
        reg.write_fanout(&frag, &Provenance::default())
            .await
            .unwrap();
        assert_eq!(a_writes.load(Ordering::SeqCst), 1);
        assert_eq!(b_writes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn aggregate_health_ok_when_all_ok() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        )));
        assert_eq!(reg.aggregate_health().await, BackendHealth::Ok);
    }

    #[tokio::test]
    async fn aggregate_health_degraded_when_optional_failed() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Failed("unreachable".into()),
        )));
        match reg.aggregate_health().await {
            BackendHealth::Degraded(_) => {}
            other => panic!("expected Degraded, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn aggregate_health_failed_when_critical_failed() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "mempalace",
            Criticality::Critical,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Failed("mcp down".into()),
        )));
        match reg.aggregate_health().await {
            BackendHealth::Failed(msg) => {
                assert!(msg.contains("mempalace"));
                assert!(msg.contains("mcp down"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn health_summary_reports_each_backend() {
        let mut reg = ExternalBackends::new();
        reg.push(Box::new(StubBackend::new(
            "a",
            Criticality::Optional,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Ok,
        )));
        reg.push(Box::new(StubBackend::new(
            "b",
            Criticality::Critical,
            Ok(vec![]),
            Ok(()),
            BackendHealth::Degraded("slow".into()),
        )));
        let summary = reg.health_summary().await;
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].name, "a");
        assert_eq!(summary[0].criticality, Criticality::Optional);
        assert_eq!(summary[0].health, BackendHealth::Ok);
        assert_eq!(summary[1].name, "b");
        assert_eq!(summary[1].criticality, Criticality::Critical);
        assert_eq!(summary[1].health, BackendHealth::Degraded("slow".into()));
    }
}
