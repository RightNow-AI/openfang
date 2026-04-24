//! Moonlock deepscan integration.
//!
//! Shells out to the user's Moonlock CLI for malware-grade scanning of
//! quarantined content. Operator configures the binary path via the
//! `OPENFANG_MOONLOCK_PATH` env var or supplies it explicitly to
//! [`MoonlockDeepscanner::with_binary`]. If the binary isn't present at scan
//! time, the scanner returns [`Verdict::ScanFailed`] with a `binary_missing`
//! finding — fail-closed per the Phase 5 contract.
//!
//! Expected CLI contract (we keep the parser permissive so minor schema
//! drift doesn't break us):
//!
//! ```text
//! moonlock scan --deep --json <dir>
//!   → stdout: {"verdict": "safe"|"suspicious"|"malicious",
//!              "findings": ["rule-1", "rule-2"]}
//!   → exit 0
//! ```
//!
//! If the JSON has neither key the scanner returns ScanFailed with the raw
//! line in the findings for operator review.

use super::{ContentScanner, ScanOutcome, Verdict};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::debug;

/// Hard timeout on a single Moonlock invocation. Larger fetches take longer
/// to deep-scan; 30s is generous for the typical OpenFang snippet but not
/// so long that a wedged binary blocks the agent loop indefinitely.
pub const SCAN_TIMEOUT: Duration = Duration::from_secs(30);

/// Default args passed after the subcommand. Operators with a different
/// CLI shape can override the whole binary path.
const DEFAULT_ARGS: &[&str] = &["scan", "--deep", "--json"];

pub struct MoonlockDeepscanner {
    name: &'static str,
    binary: Option<PathBuf>,
    extra_args: Vec<String>,
    timeout: Duration,
}

impl Default for MoonlockDeepscanner {
    fn default() -> Self {
        Self::new()
    }
}

impl MoonlockDeepscanner {
    /// Construct using `OPENFANG_MOONLOCK_PATH` if set, else `which moonlock`.
    /// If neither resolves, the scanner is still constructible — every call
    /// to `scan` will return `ScanFailed` with a clear binary-missing reason.
    pub fn new() -> Self {
        let binary = std::env::var("OPENFANG_MOONLOCK_PATH")
            .ok()
            .map(PathBuf::from)
            .or_else(|| which_in_path("moonlock"));
        Self {
            name: "moonlock",
            binary,
            extra_args: Vec::new(),
            timeout: SCAN_TIMEOUT,
        }
    }

    /// Override the binary path (test/operator escape hatch).
    pub fn with_binary(mut self, path: impl Into<PathBuf>) -> Self {
        self.binary = Some(path.into());
        self
    }

    /// Override timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Append extra args between the subcommand and the directory argument.
    /// Supplied verbatim — operator's responsibility to keep them safe.
    pub fn with_extra_args(mut self, args: Vec<String>) -> Self {
        self.extra_args = args;
        self
    }
}

#[async_trait]
impl ContentScanner for MoonlockDeepscanner {
    fn name(&self) -> &str {
        self.name
    }

    async fn scan(&self, dir: &Path) -> ScanOutcome {
        let Some(binary) = &self.binary else {
            return ScanOutcome::scan_failed(
                self.name,
                "binary_missing: set OPENFANG_MOONLOCK_PATH or install `moonlock` on PATH",
            );
        };

        let mut cmd = tokio::process::Command::new(binary);
        for a in DEFAULT_ARGS {
            cmd.arg(a);
        }
        for a in &self.extra_args {
            cmd.arg(a);
        }
        cmd.arg(dir);

        let fut = cmd.output();
        let output = match tokio::time::timeout(self.timeout, fut).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return ScanOutcome::scan_failed(
                    self.name,
                    format!("spawn_failed: {e}"),
                );
            }
            Err(_) => {
                return ScanOutcome::scan_failed(
                    self.name,
                    format!("timeout after {}s", self.timeout.as_secs()),
                );
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            debug!(stderr = %stderr, "moonlock exited non-zero");
            return ScanOutcome::scan_failed(
                self.name,
                format!(
                    "exit_status_{}: {}",
                    output.status.code().unwrap_or(-1),
                    stderr.lines().next().unwrap_or("").trim()
                ),
            );
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_moonlock_output(&stdout, self.name)
    }
}

/// Pure parser separated from process plumbing so unit tests can exercise
/// every branch without spawning a subprocess.
pub fn parse_moonlock_output(stdout: &str, scanner_name: &str) -> ScanOutcome {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return ScanOutcome::scan_failed(scanner_name, "empty_stdout");
    }

    let value: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(e) => {
            return ScanOutcome {
                scanner: scanner_name.to_string(),
                verdict: Verdict::ScanFailed,
                findings: vec![format!("parse_error: {e}")],
                raw: None,
            };
        }
    };

    let verdict_str = value
        .get("verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let verdict = match verdict_str {
        "safe" | "clean" | "ok" => Verdict::Safe,
        "suspicious" | "warn" => Verdict::Suspicious,
        "malicious" | "infected" | "malware" => Verdict::Malicious,
        "" => {
            return ScanOutcome {
                scanner: scanner_name.to_string(),
                verdict: Verdict::ScanFailed,
                findings: vec!["missing_verdict_field".to_string()],
                raw: Some(value),
            };
        }
        other => {
            return ScanOutcome {
                scanner: scanner_name.to_string(),
                verdict: Verdict::ScanFailed,
                findings: vec![format!("unknown_verdict:{other}")],
                raw: Some(value),
            };
        }
    };

    let findings = value
        .get("findings")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    ScanOutcome {
        scanner: scanner_name.to_string(),
        verdict,
        findings,
        raw: Some(value),
    }
}

/// Minimal `which` implementation — splits PATH, joins with the binary name,
/// returns the first executable hit.
fn which_in_path(binary: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_safe_verdict() {
        let r = parse_moonlock_output(
            r#"{"verdict":"safe","findings":[]}"#,
            "moonlock",
        );
        assert_eq!(r.verdict, Verdict::Safe);
        assert!(r.findings.is_empty());
        assert!(r.raw.is_some());
    }

    #[test]
    fn parse_suspicious_with_findings() {
        let r = parse_moonlock_output(
            r#"{"verdict":"suspicious","findings":["yara/eicar","heur/script"]}"#,
            "moonlock",
        );
        assert_eq!(r.verdict, Verdict::Suspicious);
        assert_eq!(r.findings, vec!["yara/eicar", "heur/script"]);
    }

    #[test]
    fn parse_malicious_alias() {
        for alias in &["malicious", "infected", "malware"] {
            let payload = format!(r#"{{"verdict":"{alias}","findings":["x"]}}"#);
            let r = parse_moonlock_output(&payload, "moonlock");
            assert_eq!(r.verdict, Verdict::Malicious, "alias {alias} did not map to Malicious");
        }
    }

    #[test]
    fn parse_safe_aliases() {
        for alias in &["safe", "clean", "ok"] {
            let payload = format!(r#"{{"verdict":"{alias}"}}"#);
            let r = parse_moonlock_output(&payload, "moonlock");
            assert_eq!(r.verdict, Verdict::Safe);
        }
    }

    #[test]
    fn parse_empty_stdout_fails_closed() {
        let r = parse_moonlock_output("", "moonlock");
        assert_eq!(r.verdict, Verdict::ScanFailed);
        assert!(r.findings.iter().any(|f| f.contains("empty_stdout")));
    }

    #[test]
    fn parse_invalid_json_fails_closed() {
        let r = parse_moonlock_output("not json at all", "moonlock");
        assert_eq!(r.verdict, Verdict::ScanFailed);
        assert!(r.findings.iter().any(|f| f.contains("parse_error")));
    }

    #[test]
    fn parse_missing_verdict_field_fails_closed() {
        let r = parse_moonlock_output(r#"{"findings":["x"]}"#, "moonlock");
        assert_eq!(r.verdict, Verdict::ScanFailed);
        assert!(r.findings.iter().any(|f| f.contains("missing_verdict")));
        assert!(r.raw.is_some());
    }

    #[test]
    fn parse_unknown_verdict_fails_closed() {
        let r =
            parse_moonlock_output(r#"{"verdict":"weird","findings":[]}"#, "moonlock");
        assert_eq!(r.verdict, Verdict::ScanFailed);
        assert!(r.findings.iter().any(|f| f.contains("unknown_verdict:weird")));
    }

    #[test]
    fn parse_findings_filters_non_strings() {
        let r = parse_moonlock_output(
            r#"{"verdict":"suspicious","findings":["a", 42, null, "b"]}"#,
            "moonlock",
        );
        assert_eq!(r.verdict, Verdict::Suspicious);
        assert_eq!(r.findings, vec!["a", "b"]);
    }

    #[tokio::test]
    async fn scan_returns_binary_missing_when_no_binary() {
        // Construct without env var or which-resolution.
        let s = MoonlockDeepscanner {
            name: "moonlock",
            binary: None,
            extra_args: Vec::new(),
            timeout: SCAN_TIMEOUT,
        };
        let r = s.scan(Path::new("/tmp")).await;
        assert_eq!(r.verdict, Verdict::ScanFailed);
        assert!(r
            .findings
            .iter()
            .any(|f| f.contains("binary_missing")));
    }

    #[tokio::test]
    async fn scan_returns_spawn_failed_when_binary_does_not_exist() {
        let s = MoonlockDeepscanner::default()
            .with_binary("/no/such/moonlock/binary/anywhere");
        let r = s.scan(Path::new("/tmp")).await;
        assert_eq!(r.verdict, Verdict::ScanFailed);
        // Either spawn_failed (file not found) or exit_status_* — both are
        // fail-closed and acceptable. We just confirm Verdict is ScanFailed.
        assert!(!r.findings.is_empty());
    }

    #[test]
    fn which_in_path_handles_no_path_env() {
        // We can't easily nuke PATH for a test process, but at least confirm
        // the function returns Option without panicking on a clearly-fake
        // binary name.
        let r = which_in_path("nonexistent-binary-xyz-987zzz");
        assert!(r.is_none());
    }
}
