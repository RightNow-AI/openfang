//! Boot-warm health gating primitive.
//!
//! Today `/api/health` returns `200 OK` as soon as Axum binds, even if
//! mempalace is unreachable, the Obsidian vault hasn't been opened, the skill
//! registry is still loading, or the model catalog hasn't downloaded. Phase 6
//! of the hardening plan changes that to a three-state response:
//!
//! ```text
//! warming   → 503 with `pending: ["mempalace", "obsidian", …]`
//! degraded  → 200 with `degraded: ["obsidian: vault path missing", …]`
//! ok        → 200 with no warnings
//! ```
//!
//! This commit lands the **primitive**: a `BootWarmRegistry` that subsystems
//! report into during their `warm()` phase, plus a deterministic computation
//! of the aggregate state. Wiring into `/api/health` and into the kernel's
//! actual subsystem startup paths is follow-up plumbing — splitting it out
//! keeps the blast radius bounded and the contract testable in isolation.
//!
//! Contract:
//!
//! 1. Each subsystem registers itself at boot with a name + criticality.
//!    Initial status is `Pending`.
//! 2. As warming progresses, the kernel calls `mark_ok` / `mark_degraded` /
//!    `mark_failed` per subsystem.
//! 3. A subsystem with `Critical` criticality blocks the aggregate state at
//!    `Warming` until it reports Ok or Failed. Failed → aggregate is
//!    `Failed` (boot-blocking).
//! 4. A subsystem with `NonCritical` criticality can stay Pending forever
//!    without blocking — after the warm-deadline expires, it's auto-marked
//!    Degraded with a "warm timeout" reason. The deadline check is the
//!    caller's responsibility (`tick_deadline()`).
//! 5. The aggregate state is computed lazily (`snapshot()`); registry state
//!    is the source of truth.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::{Duration, Instant};

/// How blocking a subsystem's failure is.
///
/// Mirrors [`crate::USER_AGENT`]-level constants by keeping the variant set
/// tiny and stable. Note that Phase 4's [`openfang_memory::external::Criticality`]
/// has a third "Optional" tier for memory backends; the boot-warm registry
/// rolls Optional and Degraded together since for boot purposes both behave
/// the same — non-critical, never block aggregate health.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    /// Failure → aggregate `Failed`. Kernel boot should refuse.
    Critical,
    /// Failure / timeout → aggregate `Degraded`. Boot continues.
    NonCritical,
}

/// Per-subsystem status reported into the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reason")]
pub enum SubsystemStatus {
    /// Subsystem hasn't reported yet. Aggregate state stays `Warming` while
    /// any Critical subsystem is still Pending.
    Pending,
    /// Subsystem warmed successfully.
    Ok,
    /// Subsystem warmed with reduced capability (slow, partial, fallback).
    Degraded(String),
    /// Subsystem failed to warm. Critical → aggregate `Failed`.
    Failed(String),
}

impl SubsystemStatus {
    pub fn is_terminal(&self) -> bool {
        !matches!(self, SubsystemStatus::Pending)
    }
}

/// Snapshot of the aggregate boot-warm state at one point in time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregateState {
    /// At least one Critical subsystem is still Pending.
    Warming,
    /// All Critical subsystems are Ok; at least one (Critical or
    /// non-critical) is Degraded or Failed. NonCritical Failed counts as
    /// Degraded for the aggregate.
    Degraded,
    /// At least one Critical subsystem is Failed.
    Failed,
    /// Every reported subsystem is Ok.
    Ok,
}

impl AggregateState {
    /// HTTP status code recommended for `/api/health` when this state is
    /// observed. Lifecycle plumbing can override but the default mapping
    /// matches what the plan specifies.
    pub fn http_status(&self) -> u16 {
        match self {
            AggregateState::Warming => 503,
            AggregateState::Failed => 503,
            AggregateState::Degraded => 200,
            AggregateState::Ok => 200,
        }
    }
}

/// One entry in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubsystemEntry {
    pub name: String,
    pub criticality: Criticality,
    pub status: SubsystemStatus,
}

#[derive(Debug)]
struct InternalEntry {
    criticality: Criticality,
    status: SubsystemStatus,
    registered_at: Instant,
}

/// Process-wide registry. Construct once at boot, share via `Arc` to anything
/// that warms (kernel subsystems) or reads (the `/api/health` route).
pub struct BootWarmRegistry {
    inner: RwLock<BTreeMap<String, InternalEntry>>,
    /// Soft deadline for non-critical subsystems. After this elapses,
    /// `tick_deadline()` flips Pending NonCritical entries to a "warm
    /// timeout" Degraded.
    warm_deadline: Duration,
}

impl Default for BootWarmRegistry {
    fn default() -> Self {
        Self::new(Duration::from_secs(30))
    }
}

impl BootWarmRegistry {
    /// New empty registry with the given soft deadline for non-critical
    /// subsystems.
    pub fn new(warm_deadline: Duration) -> Self {
        Self {
            inner: RwLock::new(BTreeMap::new()),
            warm_deadline,
        }
    }

    /// Register a subsystem at boot. Initial status is `Pending`. Returns
    /// `true` if newly registered, `false` if a same-named entry already
    /// existed (in which case the criticality is left as-is — a warning at
    /// trace level may be appropriate but registries shouldn't fight at runtime).
    pub fn register(&self, name: &str, criticality: Criticality) -> bool {
        let mut g = self
            .inner
            .write()
            .expect("BootWarmRegistry inner lock poisoned");
        if g.contains_key(name) {
            return false;
        }
        g.insert(
            name.to_string(),
            InternalEntry {
                criticality,
                status: SubsystemStatus::Pending,
                registered_at: Instant::now(),
            },
        );
        true
    }

    pub fn mark_ok(&self, name: &str) -> bool {
        self.set_status(name, SubsystemStatus::Ok)
    }
    pub fn mark_degraded(&self, name: &str, reason: impl Into<String>) -> bool {
        self.set_status(name, SubsystemStatus::Degraded(reason.into()))
    }
    pub fn mark_failed(&self, name: &str, reason: impl Into<String>) -> bool {
        self.set_status(name, SubsystemStatus::Failed(reason.into()))
    }

    /// Auto-degrade every Pending NonCritical subsystem whose registration
    /// is older than `warm_deadline`. Returns the count of entries flipped.
    /// Critical subsystems are NEVER auto-flipped — they must complete or
    /// fail explicitly.
    pub fn tick_deadline(&self) -> usize {
        let now = Instant::now();
        let mut g = self
            .inner
            .write()
            .expect("BootWarmRegistry inner lock poisoned");
        let mut flipped = 0;
        for (_, entry) in g.iter_mut() {
            if entry.criticality == Criticality::NonCritical
                && matches!(entry.status, SubsystemStatus::Pending)
                && now.duration_since(entry.registered_at) >= self.warm_deadline
            {
                entry.status = SubsystemStatus::Degraded(format!(
                    "warm timeout after {}s",
                    self.warm_deadline.as_secs()
                ));
                flipped += 1;
            }
        }
        flipped
    }

    /// Read-only snapshot of every subsystem (sorted by name).
    pub fn entries(&self) -> Vec<SubsystemEntry> {
        let g = self
            .inner
            .read()
            .expect("BootWarmRegistry inner lock poisoned");
        g.iter()
            .map(|(name, entry)| SubsystemEntry {
                name: name.clone(),
                criticality: entry.criticality,
                status: entry.status.clone(),
            })
            .collect()
    }

    /// Compute the aggregate state.
    ///
    /// Rules (in priority order):
    /// 1. Any Critical Failed → `Failed`.
    /// 2. Any Critical Pending → `Warming`.
    /// 3. All entries Ok → `Ok`.
    /// 4. Otherwise → `Degraded`.
    pub fn aggregate(&self) -> AggregateState {
        let g = self
            .inner
            .read()
            .expect("BootWarmRegistry inner lock poisoned");
        let entries: Vec<&InternalEntry> = g.values().collect();
        if entries.is_empty() {
            return AggregateState::Ok;
        }
        for e in &entries {
            if e.criticality == Criticality::Critical
                && matches!(e.status, SubsystemStatus::Failed(_))
            {
                return AggregateState::Failed;
            }
        }
        for e in &entries {
            if e.criticality == Criticality::Critical
                && matches!(e.status, SubsystemStatus::Pending)
            {
                return AggregateState::Warming;
            }
        }
        let all_ok = entries.iter().all(|e| matches!(e.status, SubsystemStatus::Ok));
        if all_ok {
            AggregateState::Ok
        } else {
            AggregateState::Degraded
        }
    }

    /// Convenience: produce an enriched payload suitable for `/api/health`:
    /// `{state, pending: [...], degraded: [...], failed: [...]}`. Plumbing
    /// in openfang-api can `serde_json::Map`-merge this into its existing
    /// response shape.
    pub fn snapshot(&self) -> WarmSnapshot {
        let entries = self.entries();
        let mut pending = Vec::new();
        let mut degraded = Vec::new();
        let mut failed = Vec::new();
        for e in &entries {
            match &e.status {
                SubsystemStatus::Pending => pending.push(e.name.clone()),
                SubsystemStatus::Degraded(r) => degraded.push(format!("{}: {}", e.name, r)),
                SubsystemStatus::Failed(r) => failed.push(format!("{}: {}", e.name, r)),
                SubsystemStatus::Ok => {}
            }
        }
        WarmSnapshot {
            state: self.aggregate(),
            pending,
            degraded,
            failed,
            entries,
        }
    }

    fn set_status(&self, name: &str, status: SubsystemStatus) -> bool {
        let mut g = self
            .inner
            .write()
            .expect("BootWarmRegistry inner lock poisoned");
        if let Some(entry) = g.get_mut(name) {
            entry.status = status;
            true
        } else {
            false
        }
    }
}

/// Immutable snapshot consumed by the API health route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmSnapshot {
    pub state: AggregateState,
    pub pending: Vec<String>,
    pub degraded: Vec<String>,
    pub failed: Vec<String>,
    pub entries: Vec<SubsystemEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn empty_registry_is_ok() {
        let r = BootWarmRegistry::default();
        assert_eq!(r.aggregate(), AggregateState::Ok);
        assert!(r.snapshot().pending.is_empty());
    }

    #[test]
    fn warming_until_critical_pending_resolves() {
        let r = BootWarmRegistry::default();
        r.register("mempalace", Criticality::Critical);
        r.register("obsidian", Criticality::NonCritical);
        assert_eq!(r.aggregate(), AggregateState::Warming);
        let snap = r.snapshot();
        assert!(snap.pending.contains(&"mempalace".to_string()));
        assert!(snap.pending.contains(&"obsidian".to_string()));
    }

    #[test]
    fn ok_when_all_subsystems_ok() {
        let r = BootWarmRegistry::default();
        r.register("a", Criticality::Critical);
        r.register("b", Criticality::NonCritical);
        r.mark_ok("a");
        r.mark_ok("b");
        assert_eq!(r.aggregate(), AggregateState::Ok);
        assert!(r.snapshot().pending.is_empty());
    }

    #[test]
    fn degraded_when_noncritical_failed() {
        let r = BootWarmRegistry::default();
        r.register("a", Criticality::Critical);
        r.register("b", Criticality::NonCritical);
        r.mark_ok("a");
        r.mark_failed("b", "vault path missing");
        assert_eq!(r.aggregate(), AggregateState::Degraded);
        let snap = r.snapshot();
        assert!(snap.failed.iter().any(|s| s.contains("vault path missing")));
        assert!(snap.degraded.is_empty());
    }

    #[test]
    fn degraded_when_anything_degraded() {
        let r = BootWarmRegistry::default();
        r.register("a", Criticality::Critical);
        r.register("b", Criticality::NonCritical);
        r.mark_ok("a");
        r.mark_degraded("b", "slow disk");
        assert_eq!(r.aggregate(), AggregateState::Degraded);
        let snap = r.snapshot();
        assert!(snap.degraded.iter().any(|s| s.contains("slow disk")));
    }

    #[test]
    fn failed_when_critical_failed() {
        let r = BootWarmRegistry::default();
        r.register("mempalace", Criticality::Critical);
        r.mark_failed("mempalace", "mcp unreachable");
        assert_eq!(r.aggregate(), AggregateState::Failed);
        // Failed takes priority even if other subsystems are pending.
        r.register("other", Criticality::Critical);
        assert_eq!(r.aggregate(), AggregateState::Failed);
    }

    #[test]
    fn warming_priority_over_noncritical_pending() {
        let r = BootWarmRegistry::default();
        r.register("mempalace", Criticality::Critical);
        r.register("obsidian", Criticality::NonCritical);
        // Even though obsidian is also pending, the verdict is Warming, not
        // Degraded — Critical-Pending dominates.
        assert_eq!(r.aggregate(), AggregateState::Warming);
    }

    #[test]
    fn http_status_matches_state() {
        assert_eq!(AggregateState::Ok.http_status(), 200);
        assert_eq!(AggregateState::Degraded.http_status(), 200);
        assert_eq!(AggregateState::Warming.http_status(), 503);
        assert_eq!(AggregateState::Failed.http_status(), 503);
    }

    #[test]
    fn register_idempotent_with_same_name() {
        let r = BootWarmRegistry::default();
        assert!(r.register("a", Criticality::Critical));
        assert!(!r.register("a", Criticality::NonCritical));
        // Original criticality preserved.
        let snap = r.snapshot();
        assert_eq!(snap.entries.len(), 1);
        assert_eq!(snap.entries[0].criticality, Criticality::Critical);
    }

    #[test]
    fn mark_unknown_returns_false() {
        let r = BootWarmRegistry::default();
        assert!(!r.mark_ok("never_registered"));
        assert!(!r.mark_degraded("never_registered", "x"));
        assert!(!r.mark_failed("never_registered", "x"));
    }

    #[test]
    fn snapshot_categorises_per_status() {
        let r = BootWarmRegistry::default();
        r.register("a", Criticality::Critical);
        r.register("b", Criticality::Critical);
        r.register("c", Criticality::NonCritical);
        r.register("d", Criticality::NonCritical);
        r.mark_ok("a");
        // b stays Pending → blocks aggregate (Warming)
        r.mark_degraded("c", "slow");
        r.mark_failed("d", "down");

        let snap = r.snapshot();
        assert_eq!(snap.state, AggregateState::Warming);
        assert_eq!(snap.pending, vec!["b"]);
        assert!(snap.degraded.iter().any(|s| s.contains("slow")));
        assert!(snap.failed.iter().any(|s| s.contains("down")));
    }

    #[test]
    fn tick_deadline_flips_pending_noncritical_after_window() {
        let r = BootWarmRegistry::new(Duration::from_millis(50));
        r.register("a", Criticality::NonCritical);
        // Before deadline: still pending.
        assert_eq!(r.tick_deadline(), 0);
        sleep(Duration::from_millis(80));
        let flipped = r.tick_deadline();
        assert_eq!(flipped, 1);
        let snap = r.snapshot();
        assert!(snap.degraded.iter().any(|s| s.contains("warm timeout")));
    }

    #[test]
    fn tick_deadline_does_not_flip_critical() {
        let r = BootWarmRegistry::new(Duration::from_millis(20));
        r.register("mempalace", Criticality::Critical);
        sleep(Duration::from_millis(60));
        let flipped = r.tick_deadline();
        assert_eq!(flipped, 0);
        // Still Warming because Critical-Pending dominates.
        assert_eq!(r.aggregate(), AggregateState::Warming);
    }

    #[test]
    fn tick_deadline_idempotent() {
        let r = BootWarmRegistry::new(Duration::from_millis(20));
        r.register("a", Criticality::NonCritical);
        sleep(Duration::from_millis(40));
        assert_eq!(r.tick_deadline(), 1);
        // Subsequent calls don't double-flip — entry is no longer Pending.
        assert_eq!(r.tick_deadline(), 0);
    }

    #[test]
    fn entries_sorted_by_name() {
        let r = BootWarmRegistry::default();
        r.register("zeta", Criticality::Critical);
        r.register("alpha", Criticality::Critical);
        r.register("mu", Criticality::Critical);
        let names: Vec<String> = r.entries().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn subsystem_status_is_terminal() {
        assert!(!SubsystemStatus::Pending.is_terminal());
        assert!(SubsystemStatus::Ok.is_terminal());
        assert!(SubsystemStatus::Degraded("x".into()).is_terminal());
        assert!(SubsystemStatus::Failed("x".into()).is_terminal());
    }
}
