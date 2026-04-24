//! Triage pipeline — security scanners that classify quarantined content.
//!
//! This module owns the [`ContentScanner`] trait and the verdict types it
//! emits. Two concrete impls land alongside in this commit:
//!
//! * [`heuristic::HeuristicScanner`] — cheap regex pass detecting obvious
//!   injection / credential-exfil / SSRF patterns.
//! * [`moonlock::MoonlockDeepscanner`] — shells out to the user's Moonlock CLI
//!   and parses its JSON verdict.
//!
//! Both impls are **fail-closed**: any internal error returns
//! [`Verdict::ScanFailed`], which the Phase 5.3 cyber-agent classifier and
//! Phase 5.4 pinboard treat as a route-to-pinboard signal — never as
//! permission-to-release.

pub mod classifier;
pub mod heuristic;
pub mod moonlock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Verdict emitted by a [`ContentScanner`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nothing concerning. Content can flow downstream.
    Safe,
    /// Concerning but not definitively malicious. Triage routes to pinboard.
    Suspicious,
    /// Active threat. Quarantine permanently — never re-feed.
    Malicious,
    /// Scanner couldn't run or produced unparseable output. Treated as
    /// suspicious by the pipeline (fail-closed).
    ScanFailed,
}

impl Verdict {
    /// True if the verdict permits downstream release of the content.
    pub fn is_safe(&self) -> bool {
        matches!(self, Verdict::Safe)
    }

    /// Combine two verdicts, taking the more conservative one. Order is
    /// `Malicious > ScanFailed > Suspicious > Safe`. Used to fold a chain
    /// of scanner outcomes into a single decision.
    pub fn worst_of(a: &Self, b: &Self) -> Self {
        let rank = |v: &Self| match v {
            Verdict::Malicious => 3,
            Verdict::ScanFailed => 2,
            Verdict::Suspicious => 1,
            Verdict::Safe => 0,
        };
        if rank(a) >= rank(b) {
            a.clone()
        } else {
            b.clone()
        }
    }
}

/// Outcome of a single scanner run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanOutcome {
    /// Scanner identifier (matches `ContentScanner::name`).
    pub scanner: String,
    /// Aggregate verdict.
    pub verdict: Verdict,
    /// Human-readable hit list (rule names, indicator names, file paths
    /// that tripped a rule).
    pub findings: Vec<String>,
    /// Raw scanner output (when available — e.g. parsed Moonlock JSON).
    /// Persisted alongside the quarantined content for the pinboard surface.
    #[serde(default)]
    pub raw: Option<serde_json::Value>,
}

impl ScanOutcome {
    /// Helper: a clean Safe outcome with no findings.
    pub fn safe(scanner: impl Into<String>) -> Self {
        Self {
            scanner: scanner.into(),
            verdict: Verdict::Safe,
            findings: Vec::new(),
            raw: None,
        }
    }

    /// Helper: a fail-closed outcome.
    pub fn scan_failed(scanner: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            scanner: scanner.into(),
            verdict: Verdict::ScanFailed,
            findings: vec![reason.into()],
            raw: None,
        }
    }
}

/// Trait every triage scanner implements.
#[async_trait]
pub trait ContentScanner: Send + Sync {
    /// Stable short name for logs and outcome attribution.
    fn name(&self) -> &str;

    /// Scan a quarantined-content directory. Implementations must NOT panic
    /// on malformed input — return `Verdict::ScanFailed` instead.
    async fn scan(&self, dir: &Path) -> ScanOutcome;
}

/// Run a chain of scanners against the same dir, sequentially, and fold the
/// verdicts via [`Verdict::worst_of`]. Returns one outcome per scanner plus
/// the aggregate verdict.
pub async fn scan_chain(
    scanners: &[Box<dyn ContentScanner>],
    dir: &Path,
) -> (Vec<ScanOutcome>, Verdict) {
    let mut outcomes = Vec::with_capacity(scanners.len());
    let mut overall = Verdict::Safe;
    for s in scanners {
        let o = s.scan(dir).await;
        overall = Verdict::worst_of(&overall, &o.verdict);
        outcomes.push(o);
    }
    (outcomes, overall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdict_worst_of_orders_correctly() {
        let s = Verdict::Safe;
        let q = Verdict::Suspicious;
        let f = Verdict::ScanFailed;
        let m = Verdict::Malicious;

        assert_eq!(Verdict::worst_of(&s, &q), Verdict::Suspicious);
        assert_eq!(Verdict::worst_of(&q, &f), Verdict::ScanFailed);
        assert_eq!(Verdict::worst_of(&f, &m), Verdict::Malicious);
        assert_eq!(Verdict::worst_of(&m, &s), Verdict::Malicious);
        // ScanFailed must win over Suspicious — explicit fail-closed contract.
        assert_eq!(Verdict::worst_of(&q, &f), Verdict::ScanFailed);
        assert_eq!(Verdict::worst_of(&f, &q), Verdict::ScanFailed);
    }

    #[test]
    fn verdict_is_safe_only_for_safe() {
        assert!(Verdict::Safe.is_safe());
        assert!(!Verdict::Suspicious.is_safe());
        assert!(!Verdict::ScanFailed.is_safe());
        assert!(!Verdict::Malicious.is_safe());
    }

    #[test]
    fn scan_outcome_helpers() {
        let safe = ScanOutcome::safe("h");
        assert_eq!(safe.scanner, "h");
        assert_eq!(safe.verdict, Verdict::Safe);
        assert!(safe.findings.is_empty());

        let failed = ScanOutcome::scan_failed("m", "binary missing");
        assert_eq!(failed.verdict, Verdict::ScanFailed);
        assert_eq!(failed.findings, vec!["binary missing"]);
    }
}
