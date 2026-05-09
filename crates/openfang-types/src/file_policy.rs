//! Unified filesystem access policy (ANAI-40).
//!
//! A single `[file_policy]` block gates all filesystem access regardless of
//! vector — MCP tool, shell builtin, redirected output, `tee`, etc. The agent
//! workspace stays the implicit read+write root; this policy extends it with
//! explicit allow/prompt/deny path globs.
//!
//! ## Two-stage design
//!
//! - [`FilePolicy`] is the serde-derived configuration shape that lives in
//!   `config.toml` and `agent.toml`. It stores raw glob strings.
//! - [`CompiledFilePolicy`] is the runtime form. Build once at config load via
//!   [`FilePolicy::compile`]; evaluate many times via
//!   [`CompiledFilePolicy::evaluate`].
//!
//! Glob compilation is fallible (a typo in the user's TOML shouldn't panic at
//! request time), so the `compile` step is explicit. The agent workspace is
//! also bound at compile time — passing it on every `evaluate` call would
//! invite a wrong value at one site silently shifting policy.

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Default tier applied to paths that match no explicit pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultTier {
    /// Hard-blocked. Strictest, and the recommended default for most agents.
    #[default]
    Deny,
    /// Approval required (read or write).
    Prompt,
    /// Read allowed silently; write requires approval.
    ReadOnly,
}

/// Filesystem operation being attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOp {
    Read,
    Write,
}

/// Outcome of evaluating a path against [`CompiledFilePolicy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileDecision {
    Allow,
    Prompt { reason: String },
    Deny { reason: String },
}

/// Serde-derived configuration shape. See module docs.
///
/// Schema (in TOML):
/// ```toml
/// [file_policy]
/// read_paths   = ["~/Documents/GitHub/Repos/openfang/**"]
/// write_paths  = ["~/.openfang/scratch/**"]
/// prompt_paths = ["~/.ssh/**"]
/// deny_paths   = ["~/.aws/credentials"]
/// default      = "deny"   # "deny" | "prompt" | "read_only"
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilePolicy {
    /// Read-only: agent can read but not write.
    pub read_paths: Vec<String>,
    /// Read+write: full access.
    pub write_paths: Vec<String>,
    /// Any access (read or write) requires approval.
    pub prompt_paths: Vec<String>,
    /// Hard-blocked even with approval.
    pub deny_paths: Vec<String>,
    /// Tier applied to paths that match no pattern.
    ///
    /// `None` means "inherit from base / use the compile-time fallback (`Deny`)".
    /// `Some(_)` overrides explicitly. Modeled as `Option` so per-agent overlays
    /// can choose to inherit (omit the field) vs override (set the field) —
    /// without that, an agent could not re-assert a stricter `deny` over a
    /// laxer base, since `Default::default()` and `Some(Deny)` would be
    /// indistinguishable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<DefaultTier>,
}

impl FilePolicy {
    /// Compile the configured glob patterns into runtime form, binding the
    /// agent workspace at the same time.
    ///
    /// Tilde expansion (`~/...`) is applied to each pattern before glob
    /// compilation. Path separators are normalized to forward slashes so
    /// patterns work uniformly on Unix and Windows. Returns an error if any
    /// pattern fails to parse.
    ///
    /// `workspace` should be an absolute, canonicalized path. Anything under
    /// it gets implicit read+write access (precedence step 5 in
    /// [`CompiledFilePolicy::evaluate`]).
    ///
    /// If `default` is `None`, the compiled policy uses [`DefaultTier::Deny`].
    pub fn compile(&self, workspace: PathBuf) -> Result<CompiledFilePolicy, FilePolicyError> {
        Ok(CompiledFilePolicy {
            deny: build_set(&self.deny_paths).map_err(|e| e.in_field("deny_paths"))?,
            prompt: build_set(&self.prompt_paths).map_err(|e| e.in_field("prompt_paths"))?,
            write: build_set(&self.write_paths).map_err(|e| e.in_field("write_paths"))?,
            read: build_set(&self.read_paths).map_err(|e| e.in_field("read_paths"))?,
            default: self.default.unwrap_or(DefaultTier::Deny),
            workspace,
        })
    }

    /// Field-by-field merge: this policy (the overlay) wins per field if
    /// non-empty / `Some`, otherwise the field is inherited from `base`.
    ///
    /// Used to layer a per-agent `[file_policy]` block over the global one.
    /// Vectors are treated as "set" if non-empty; an empty vector inherits.
    /// The `default` tier is `Option<DefaultTier>` precisely so an agent can
    /// either inherit (omit the field) or override (set the field, including
    /// re-asserting the same value as the global default).
    ///
    /// Instance method (rather than a free function `merged(base, overlay)`)
    /// so the type system catches argument flips: `overlay.layered_over(&base)`
    /// reads correctly; `base.layered_over(&overlay)` would silently invert
    /// merge direction with a free function.
    pub fn layered_over(&self, base: &Self) -> Self {
        fn pick<T: Clone>(base: &[T], overlay: &[T]) -> Vec<T> {
            if overlay.is_empty() {
                base.to_vec()
            } else {
                overlay.to_vec()
            }
        }
        Self {
            read_paths: pick(&base.read_paths, &self.read_paths),
            write_paths: pick(&base.write_paths, &self.write_paths),
            prompt_paths: pick(&base.prompt_paths, &self.prompt_paths),
            deny_paths: pick(&base.deny_paths, &self.deny_paths),
            default: self.default.or(base.default),
        }
    }
}

/// Runtime form of [`FilePolicy`]. Build via [`FilePolicy::compile`].
#[derive(Debug, Clone)]
pub struct CompiledFilePolicy {
    deny: GlobSet,
    prompt: GlobSet,
    write: GlobSet,
    read: GlobSet,
    default: DefaultTier,
    workspace: PathBuf,
}

impl CompiledFilePolicy {
    /// Evaluate a single path against the policy.
    ///
    /// Precedence (most-specific wins; ties broken `deny > prompt > write > read`):
    /// 1. `deny_paths` — always wins.
    /// 2. `prompt_paths`.
    /// 3. `write_paths`.
    /// 4. `read_paths`:
    ///    - `op = Read` → `Allow`.
    ///    - `op = Write` → fall through to **default tier** (NOT workspace).
    ///      Otherwise a `read_paths` entry inside the workspace would silently
    ///      permit writes via the workspace fallback.
    /// 5. Workspace root → implicit `Allow` for both read and write.
    /// 6. Otherwise → default tier.
    ///
    /// `path` should be absolute and canonicalized by the caller. The
    /// evaluator does not touch the filesystem.
    pub fn evaluate(&self, path: &Path, op: FileOp) -> FileDecision {
        debug_assert!(
            path.is_absolute(),
            "CompiledFilePolicy::evaluate requires an absolute path; got {:?}",
            path
        );

        let path_str = path.to_string_lossy();

        // 1. Deny — absolute block.
        if self.deny.is_match(path) {
            return FileDecision::Deny {
                reason: format!("path `{}` matches deny_paths", path_str),
            };
        }

        // 2. Prompt — approval required for any op.
        if self.prompt.is_match(path) {
            return FileDecision::Prompt {
                reason: format!("path `{}` matches prompt_paths ({op:?})", path_str),
            };
        }

        // 3. Write — full access.
        if self.write.is_match(path) {
            return FileDecision::Allow;
        }

        // 4. Read — read allowed; write skips workspace fallback and goes to
        //    default tier so a read-only path inside the workspace stays
        //    write-protected.
        if self.read.is_match(path) {
            match op {
                FileOp::Read => return FileDecision::Allow,
                FileOp::Write => return self.default_decision(&path_str, op),
            }
        }

        // 5. Workspace implicit read+write.
        if path.starts_with(&self.workspace) {
            return FileDecision::Allow;
        }

        // 6. Default tier.
        self.default_decision(&path_str, op)
    }

    fn default_decision(&self, path_str: &str, op: FileOp) -> FileDecision {
        match (self.default, op) {
            (DefaultTier::Deny, _) => FileDecision::Deny {
                reason: format!("path `{}` falls under default = deny", path_str),
            },
            (DefaultTier::Prompt, _) => FileDecision::Prompt {
                reason: format!("path `{}` falls under default = prompt ({op:?})", path_str),
            },
            (DefaultTier::ReadOnly, FileOp::Read) => FileDecision::Allow,
            (DefaultTier::ReadOnly, FileOp::Write) => FileDecision::Deny {
                reason: format!(
                    "path `{}` falls under default = read_only; write denied",
                    path_str
                ),
            },
        }
    }

    /// Test/diagnostic accessor.
    #[doc(hidden)]
    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FilePolicyError {
    #[error("invalid glob in `{field}`: pattern `{pattern}`: {source}")]
    InvalidGlob {
        field: &'static str,
        pattern: String,
        #[source]
        source: globset::Error,
    },
    #[error("failed to build globset for `{field}`: {source}")]
    GlobSetBuild {
        field: &'static str,
        #[source]
        source: globset::Error,
    },
}

enum GlobBuildError {
    Pattern {
        pattern: String,
        source: globset::Error,
    },
    Build(globset::Error),
}

impl GlobBuildError {
    fn in_field(self, field: &'static str) -> FilePolicyError {
        match self {
            GlobBuildError::Pattern { pattern, source } => FilePolicyError::InvalidGlob {
                field,
                pattern,
                source,
            },
            GlobBuildError::Build(source) => FilePolicyError::GlobSetBuild { field, source },
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_set(patterns: &[String]) -> Result<GlobSet, GlobBuildError> {
    let mut builder = GlobSetBuilder::new();
    for raw in patterns {
        let expanded = expand_tilde(raw);
        match Glob::new(&expanded) {
            Ok(g) => {
                builder.add(g);
            }
            Err(e) => {
                return Err(GlobBuildError::Pattern {
                    pattern: raw.clone(),
                    source: e,
                });
            }
        }
    }
    builder.build().map_err(GlobBuildError::Build)
}

/// Expand a leading `~` or `~/` to the user's home directory.
///
/// Lone `~` and `~/...` are expanded; `~user/...` is left untouched (we don't
/// resolve other users' homes). Returns the input unchanged if `dirs::home_dir`
/// returns `None`.
///
/// Output is normalized to forward slashes and the home portion is escaped for
/// glob meta characters — a `$HOME` containing `[`, `?`, `*`, `{`, etc. would
/// otherwise turn into wildcards inside the resulting pattern. Globset already
/// matches `/` and `\` interchangeably on Windows, but consistent forward
/// slashes also make compiled patterns easier to debug.
pub fn expand_tilde(input: &str) -> String {
    fn home_as_pattern() -> Option<String> {
        let home = dirs::home_dir()?;
        let home_str = home.to_string_lossy().replace('\\', "/");
        Some(globset::escape(&home_str))
    }

    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = home_as_pattern() {
            return format!("{}/{}", home, rest);
        }
    } else if input == "~" {
        if let Some(home) = home_as_pattern() {
            return home;
        }
    }
    input.to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ws() -> PathBuf {
        PathBuf::from("/tmp/openfang-test-ws")
    }

    fn policy(toml_src: &str) -> CompiledFilePolicy {
        let p: FilePolicy = toml::from_str(toml_src).expect("parse");
        p.compile(ws()).expect("compile")
    }

    #[test]
    fn workspace_implicit_allow() {
        let p = policy("");
        let inside = ws().join("notes.md");
        assert_eq!(p.evaluate(&inside, FileOp::Read), FileDecision::Allow);
        assert_eq!(p.evaluate(&inside, FileOp::Write), FileDecision::Allow);
    }

    #[test]
    fn default_deny_outside_workspace() {
        let p = policy("");
        let outside = PathBuf::from("/etc/hosts");
        assert!(matches!(
            p.evaluate(&outside, FileOp::Read),
            FileDecision::Deny { .. }
        ));
    }

    #[test]
    fn deny_overrides_everything() {
        let p = policy(
            r#"
            read_paths = ["/secrets/**"]
            write_paths = ["/secrets/**"]
            prompt_paths = ["/secrets/**"]
            deny_paths = ["/secrets/**"]
        "#,
        );
        assert!(matches!(
            p.evaluate(&PathBuf::from("/secrets/key.pem"), FileOp::Read),
            FileDecision::Deny { .. }
        ));
    }

    #[test]
    fn prompt_beats_write_and_read() {
        let p = policy(
            r#"
            write_paths = ["/etc/**"]
            prompt_paths = ["/etc/passwd"]
        "#,
        );
        assert!(matches!(
            p.evaluate(&PathBuf::from("/etc/passwd"), FileOp::Read),
            FileDecision::Prompt { .. }
        ));
        // Sibling falls through to write.
        assert_eq!(
            p.evaluate(&PathBuf::from("/etc/hostname"), FileOp::Write),
            FileDecision::Allow
        );
    }

    #[test]
    fn read_only_blocks_write_via_default_tier() {
        let p = policy(
            r#"
            read_paths = ["/data/**"]
        "#,
        );
        assert_eq!(
            p.evaluate(&PathBuf::from("/data/x.txt"), FileOp::Read),
            FileDecision::Allow
        );
        // Write to a read-only matched path falls to default = deny.
        assert!(matches!(
            p.evaluate(&PathBuf::from("/data/x.txt"), FileOp::Write),
            FileDecision::Deny { .. }
        ));
    }

    /// Regression for the must-fix from code review: a `read_paths` entry
    /// inside the workspace must NOT silently permit writes via the workspace
    /// fallback. Step 4 short-circuits to default tier on Write.
    #[test]
    fn read_paths_inside_workspace_blocks_writes() {
        let p = policy(
            r#"
            read_paths = ["/tmp/openfang-test-ws/secrets/**"]
        "#,
        );
        let target = ws().join("secrets/key.pem");
        assert_eq!(p.evaluate(&target, FileOp::Read), FileDecision::Allow);
        assert!(
            matches!(
                p.evaluate(&target, FileOp::Write),
                FileDecision::Deny { .. }
            ),
            "read-only path inside workspace must reject writes"
        );
    }

    /// Tier-precedence pinning: when a path matches both `read_paths` and
    /// `write_paths`, `write` wins (full access).
    #[test]
    fn write_beats_read_on_overlap() {
        let p = policy(
            r#"
            read_paths  = ["/data/**"]
            write_paths = ["/data/scratch/**"]
        "#,
        );
        assert_eq!(
            p.evaluate(&PathBuf::from("/data/scratch/x.txt"), FileOp::Write),
            FileDecision::Allow
        );
    }

    #[test]
    fn default_prompt_tier() {
        let p = policy(r#"default = "prompt""#);
        assert!(matches!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Read),
            FileDecision::Prompt { .. }
        ));
    }

    #[test]
    fn default_read_only_tier() {
        let p = policy(r#"default = "read_only""#);
        assert_eq!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Read),
            FileDecision::Allow
        );
        assert!(matches!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Write),
            FileDecision::Deny { .. }
        ));
    }

    #[test]
    fn tilde_expansion_in_patterns() {
        let home = dirs::home_dir().expect("home");
        let p = policy(
            r#"
            read_paths = ["~/Documents/**"]
        "#,
        );
        let target = home.join("Documents/report.txt");
        assert_eq!(p.evaluate(&target, FileOp::Read), FileDecision::Allow);
    }

    #[test]
    fn invalid_glob_rejected() {
        let p: FilePolicy = toml::from_str(r#"deny_paths = ["[unclosed"]"#).unwrap();
        let err = p.compile(ws()).expect_err("should fail");
        match err {
            FilePolicyError::InvalidGlob { field, .. } => assert_eq!(field, "deny_paths"),
            other => panic!("expected InvalidGlob, got {other:?}"),
        }
    }

    #[test]
    fn layered_over_takes_overlay_when_set() {
        let base = FilePolicy {
            read_paths: vec!["/base/**".into()],
            default: Some(DefaultTier::Deny),
            ..Default::default()
        };
        let overlay = FilePolicy {
            read_paths: vec!["/overlay/**".into()],
            default: Some(DefaultTier::Prompt),
            ..Default::default()
        };
        let merged = overlay.layered_over(&base);
        assert_eq!(merged.read_paths, vec!["/overlay/**".to_string()]);
        assert_eq!(merged.default, Some(DefaultTier::Prompt));
    }

    #[test]
    fn layered_over_inherits_when_overlay_empty() {
        let base = FilePolicy {
            read_paths: vec!["/base/**".into()],
            write_paths: vec!["/scratch/**".into()],
            default: Some(DefaultTier::Prompt),
            ..Default::default()
        };
        let overlay = FilePolicy::default();
        let merged = overlay.layered_over(&base);
        assert_eq!(merged.read_paths, base.read_paths);
        assert_eq!(merged.write_paths, base.write_paths);
        assert_eq!(merged.default, Some(DefaultTier::Prompt));
    }

    /// Regression: an overlay must be able to downgrade a base default
    /// (e.g. base = `Prompt`, overlay = `Some(Deny)` → `Deny`). With the old
    /// `default: DefaultTier` field this was impossible because `Deny` was
    /// indistinguishable from "field not set".
    #[test]
    fn layered_over_can_downgrade_default() {
        let base = FilePolicy {
            default: Some(DefaultTier::Prompt),
            ..Default::default()
        };
        let overlay = FilePolicy {
            default: Some(DefaultTier::Deny),
            ..Default::default()
        };
        let merged = overlay.layered_over(&base);
        assert_eq!(merged.default, Some(DefaultTier::Deny));
    }

    #[test]
    fn workspace_root_not_substring_matched() {
        // /tmp/openfang-test-ws-other should NOT match /tmp/openfang-test-ws.
        let p = policy("");
        let sibling = PathBuf::from("/tmp/openfang-test-ws-other/file.txt");
        assert!(matches!(
            p.evaluate(&sibling, FileOp::Read),
            FileDecision::Deny { .. }
        ));
    }
}
