//! Runtime drift detection and safe auto-remediation.
//!
//! Runs on daemon boot and on a periodic interval (see `[watchdog]` in config.toml).
//! Only applies fixes that are safe without user confirmation: autospawn sync,
//! optionally stopping non-allowlisted agents (only when `[watchdog] kill_disallowed = true`),
//! clearing stuck LLM runs.
//!
//! Allowlist and model checks are report-only by default — they never kill agents or
//! block the dashboard.

use crate::kernel::OMTAEKernel;
use chrono::Utc;
use omtae_types::agent::{AgentEntry, AgentManifest, AgentState, ScheduleMode};
use omtae_types::config::{DefaultModelConfig, WatchdogConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

/// One drift finding from a check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftFinding {
    pub check: String,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub remediated: bool,
}

/// Result of a drift scan (GET `/api/system/drift` or periodic task).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriftReport {
    pub ok: bool,
    pub checked_at: String,
    pub findings: Vec<DriftFinding>,
    pub fixes_applied: Vec<String>,
    /// Echo of `[watchdog] manual_allowlist` so the dashboard can hide expected on-demand agents.
    #[serde(default)]
    pub manual_allowlist: Vec<String>,
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Normalize agent names for allowlist comparisons (trim + lowercase).
fn normalize_agent_name(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Build a set of `[watchdog] manual_allowlist` entries (on-demand agents, never drift).
fn manual_allowlist_set(names: &[String]) -> HashSet<String> {
    names.iter().map(|s| normalize_agent_name(s)).collect()
}

fn is_manual_allowlisted(agent_name: &str, manual: &HashSet<String>) -> bool {
    manual.contains(&normalize_agent_name(agent_name))
}

/// Resolve manifest "default" placeholders the same way the agents API does.
fn resolve_agent_model<'a>(entry: &'a AgentEntry, dm: &'a DefaultModelConfig) -> (&'a str, &'a str) {
    let provider =
        if entry.manifest.model.provider.is_empty() || entry.manifest.model.provider == "default" {
            dm.provider.as_str()
        } else {
            entry.manifest.model.provider.as_str()
        };
    let model = if entry.manifest.model.model.is_empty() || entry.manifest.model.model == "default" {
        dm.model.as_str()
    } else {
        entry.manifest.model.model.as_str()
    };
    (provider, model)
}

/// Drop remediated rows; `ok` is true when no warn/error findings remain.
fn finalize_drift_report(
    findings: Vec<DriftFinding>,
    fixes: Vec<String>,
    manual_allowlist: Vec<String>,
) -> DriftReport {
    let findings: Vec<_> = findings
        .into_iter()
        .filter(|f| !f.remediated)
        .collect();
    let ok = findings
        .iter()
        .all(|f| f.severity == "info" || f.severity == "debug");
    DriftReport {
        ok,
        checked_at: Utc::now().to_rfc3339(),
        findings,
        fixes_applied: fixes,
        manual_allowlist,
    }
}

impl OMTAEKernel {
    /// Scan runtime state for drift from config; optionally apply safe fixes.
    pub fn run_drift_check(&self, remediate: bool) -> DriftReport {
        let cfg = &self.config.watchdog;
        let manual_allowlist = cfg.manual_allowlist.clone();
        let mut findings = Vec::new();
        let mut fixes = Vec::new();

        self.clean_stale_running_task_timestamps();

        if cfg.checks.iter().any(|c| c == "autospawn" || c == "allowlist") {
            let kill_disallowed = remediate && cfg.kill_disallowed;
            self.check_autospawn(remediate, kill_disallowed, &mut findings, &mut fixes);
        }

        if cfg.checks.iter().any(|c| c == "stuck_run") {
            self.check_stuck_runs(cfg.stuck_run_secs, remediate, &mut findings, &mut fixes);
        }

        if cfg.checks.iter().any(|c| c == "continuous") {
            self.check_continuous_stuck(cfg.stuck_run_secs, remediate, &mut findings, &mut fixes);
        }

        if cfg.checks.iter().any(|c| c == "model") {
            let patterns = self.expected_model_patterns(cfg);
            if !patterns.is_empty() {
                self.check_model_drift(&patterns, &mut findings);
            }
        }

        finalize_drift_report(findings, fixes, manual_allowlist)
    }

    /// Remove timestamps left behind when a run task finished without clearing the map.
    fn clean_stale_running_task_timestamps(&self) {
        let stale: Vec<_> = self
            .running_task_started
            .iter()
            .filter_map(|e| {
                let id = *e.key();
                if self.running_tasks.contains_key(&id) {
                    None
                } else {
                    Some(id)
                }
            })
            .collect();
        for id in stale {
            self.running_task_started.remove(&id);
        }
    }

    fn expected_model_patterns(&self, cfg: &WatchdogConfig) -> Vec<String> {
        if !cfg.expected_model_substrings.is_empty() {
            return cfg
                .expected_model_substrings
                .iter()
                .map(|s| s.to_lowercase())
                .collect();
        }
        let dm = &self.config.default_model;
        let mut patterns = Vec::new();
        if !dm.provider.is_empty() && dm.provider != "default" {
            patterns.push(dm.provider.to_lowercase());
        }
        if !dm.model.is_empty() && dm.model != "default" {
            patterns.push(dm.model.to_lowercase());
            for token in dm.model.to_lowercase().split(['-', '_', '/', '.']) {
                if token.len() >= 3 {
                    patterns.push(token.to_string());
                }
            }
        }
        patterns.sort();
        patterns.dedup();
        patterns
    }
}

impl OMTAEKernel {
    fn check_autospawn(
        &self,
        remediate: bool,
        kill_disallowed: bool,
        findings: &mut Vec<DriftFinding>,
        fixes: &mut Vec<String>,
    ) {
        let allowlist: Vec<String> = self.config.agents.autospawn.clone();
        if allowlist.is_empty() {
            return;
        }

        let allow: HashSet<String> = allowlist.iter().map(|s| normalize_agent_name(s)).collect();
        let manual = manual_allowlist_set(&self.config.watchdog.manual_allowlist);
        let running: Vec<_> = self.registry.list();

        for entry in &running {
            if is_manual_allowlisted(&entry.name, &manual) {
                continue;
            }
            if !allow.contains(&normalize_agent_name(&entry.name)) {
                findings.push(DriftFinding {
                    check: "allowlist".into(),
                    severity: "warn".into(),
                    message: format!(
                        "Agent '{}' is running but not in [agents] autospawn (report-only; add to autospawn or manual_allowlist)",
                        entry.name
                    ),
                    remediated: false,
                });
                if kill_disallowed && !is_manual_allowlisted(&entry.name, &manual) {
                    if self.kill_agent(entry.id).is_ok() {
                        fixes.push(format!("Stopped non-allowlisted agent '{}'", entry.name));
                        if let Some(last) = findings.last_mut() {
                            last.remediated = true;
                        }
                    }
                }
            }
        }

        let agents_dir = self.config.home_dir.join("agents");
        if !agents_dir.is_dir() {
            return;
        }

        for name in &allowlist {
            if self.registry.find_by_name(name).is_some() {
                continue;
            }
            let toml_path = agents_dir.join(name).join("agent.toml");
            if !toml_path.exists() {
                findings.push(DriftFinding {
                    check: "autospawn".into(),
                    severity: "warn".into(),
                    message: format!(
                        "Autospawn agent '{}' is allowlisted but has no ~/.omtae/agents/{}/agent.toml",
                        name, name
                    ),
                    remediated: false,
                });
                continue;
            }

            findings.push(DriftFinding {
                check: "autospawn".into(),
                severity: "warn".into(),
                message: format!("Autospawn agent '{}' is allowlisted but not running", name),
                remediated: false,
            });

            if !remediate {
                continue;
            }

            let toml_str = match std::fs::read_to_string(&toml_path) {
                Ok(s) => s,
                Err(e) => {
                    warn!(agent = %name, "Drift autospawn: read failed: {e}");
                    continue;
                }
            };
            let mut manifest: AgentManifest = match toml::from_str(&toml_str) {
                Ok(m) => m,
                Err(e) => {
                    warn!(agent = %name, "Drift autospawn: invalid manifest: {e}");
                    continue;
                }
            };
            if manifest.name.is_empty() {
                manifest.name = name.clone();
            }
            match self.spawn_agent(manifest) {
                Ok(id) => {
                    fixes.push(format!("Spawned missing autospawn agent '{}' ({})", name, id));
                    if let Some(last) = findings.last_mut() {
                        last.remediated = true;
                    }
                    info!(agent = %name, "Drift guard spawned autospawn agent");
                }
                Err(e) => {
                    warn!(agent = %name, "Drift autospawn spawn failed: {e}");
                }
            }
        }
    }

    fn check_stuck_runs(
        &self,
        stuck_secs: u64,
        remediate: bool,
        findings: &mut Vec<DriftFinding>,
        fixes: &mut Vec<String>,
    ) {
        let now = now_epoch_secs();
        let stuck_ids: Vec<_> = self
            .running_task_started
            .iter()
            .filter_map(|e| {
                let agent_id = *e.key();
                if !self.running_tasks.contains_key(&agent_id) {
                    return None;
                }
                let started = *e.value();
                if now.saturating_sub(started) >= stuck_secs {
                    Some(agent_id)
                } else {
                    None
                }
            })
            .collect();

        for agent_id in stuck_ids {
            let name = self
                .registry
                .get(agent_id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| agent_id.to_string());
            findings.push(DriftFinding {
                check: "stuck_run".into(),
                severity: "error".into(),
                message: format!(
                    "Agent '{}' LLM run exceeded {}s while inferencing",
                    name, stuck_secs
                ),
                remediated: false,
            });
            if remediate {
                if self.stop_agent_run(agent_id).unwrap_or(false) {
                    fixes.push(format!("Cancelled stuck run for '{}'", name));
                    if let Some(last) = findings.last_mut() {
                        last.remediated = true;
                    }
                }
            }
        }
    }

    fn check_continuous_stuck(
        &self,
        stuck_secs: u64,
        remediate: bool,
        findings: &mut Vec<DriftFinding>,
        fixes: &mut Vec<String>,
    ) {
        let now = now_epoch_secs();
        for entry in self.registry.list() {
            let is_continuous = matches!(
                entry.manifest.schedule,
                ScheduleMode::Continuous { .. } | ScheduleMode::Periodic { .. }
            );
            if !is_continuous {
                continue;
            }
            if !self.running_tasks.contains_key(&entry.id) {
                continue;
            }
            let started = self
                .running_task_started
                .get(&entry.id)
                .map(|e| *e)
                .unwrap_or(0);
            if started == 0 || now.saturating_sub(started) < stuck_secs {
                continue;
            }
            findings.push(DriftFinding {
                check: "continuous".into(),
                severity: "warn".into(),
                message: format!(
                    "Agent '{}' on {:?} schedule has been inferencing >{}s",
                    entry.name, entry.manifest.schedule, stuck_secs
                ),
                remediated: false,
            });
            if remediate {
                let _ = self.stop_agent_run(entry.id);
                self.background.pause_agent(entry.id);
                fixes.push(format!(
                    "Stopped continuous/periodic runaway for '{}'",
                    entry.name
                ));
                if let Some(last) = findings.last_mut() {
                    last.remediated = true;
                }
            }
        }
    }

    fn check_model_drift(&self, expected_substrings: &[String], findings: &mut Vec<DriftFinding>) {
        let dm = &self.config.default_model;
        for entry in self.registry.list() {
            if entry.state != AgentState::Running {
                continue;
            }
            let (provider, model) = resolve_agent_model(&entry, dm);
            let haystack = format!("{}/{}", provider.to_lowercase(), model.to_lowercase());
            let ok = expected_substrings
                .iter()
                .any(|s| haystack.contains(s.as_str()));
            if !ok {
                findings.push(DriftFinding {
                    check: "model".into(),
                    severity: "info".into(),
                    message: format!(
                        "Agent '{}' uses {}:{} (informational; does not match expected patterns {:?})",
                        entry.name, provider, model, expected_substrings
                    ),
                    remediated: false,
                });
            }
        }
    }
}

/// Start periodic drift checks (call once from daemon boot).
pub fn spawn_drift_watchdog(kernel: Arc<OMTAEKernel>) {
    let cfg = kernel.config.watchdog.clone();
    if !cfg.enabled {
        return;
    }

    let interval = std::time::Duration::from_secs(cfg.interval_secs.max(60));

    {
        let k = kernel.clone();
        tokio::spawn(async move {
            let report = k.run_drift_check(true);
            if !report.ok {
                info!(
                    findings = report.findings.len(),
                    fixes = report.fixes_applied.len(),
                    "Boot drift check completed"
                );
            }
        });
    }

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let report = kernel.run_drift_check(true);
            if !report.ok {
                warn!(
                    findings = ?report.findings.iter().map(|f| &f.message).collect::<Vec<_>>(),
                    fixes = ?report.fixes_applied,
                    "Drift watchdog applied remediation"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_allowlist_normalizes_names() {
        let manual = manual_allowlist_set(&[" browser-hand ".into(), "Orchestrator".into()]);
        assert!(is_manual_allowlisted("browser-hand", &manual));
        assert!(is_manual_allowlisted("Orchestrator", &manual));
        assert!(!is_manual_allowlisted("coder", &manual));
    }

    #[test]
    fn finalize_drops_remediated_and_marks_ok() {
        let report = finalize_drift_report(
            vec![
                DriftFinding {
                    check: "autospawn".into(),
                    severity: "warn".into(),
                    message: "fixed".into(),
                    remediated: true,
                },
                DriftFinding {
                    check: "model".into(),
                    severity: "info".into(),
                    message: "informational".into(),
                    remediated: false,
                },
            ],
            vec!["spawned".into()],
            vec!["browser-hand".into()],
        );
        assert!(report.ok);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].severity, "info");
        assert_eq!(report.manual_allowlist, vec!["browser-hand"]);
    }

    #[test]
    fn finalize_not_ok_when_warn_remains() {
        let report = finalize_drift_report(
            vec![DriftFinding {
                check: "allowlist".into(),
                severity: "warn".into(),
                message: "still bad".into(),
                remediated: false,
            }],
            vec![],
            vec![],
        );
        assert!(!report.ok);
        assert_eq!(report.findings.len(), 1);
    }
}
