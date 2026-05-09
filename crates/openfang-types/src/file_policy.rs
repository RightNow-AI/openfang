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
//! request time), so the `compile` step is explicit.

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
    /// Tier applied to paths that match no pattern. Defaults to `Deny`.
    pub default: DefaultTier,
}

impl FilePolicy {
    /// Compile the configured glob patterns into runtime form.
    ///
    /// Tilde expansion (`~/...`) is applied to each pattern before glob
    /// compilation. Returns an error if any pattern fails to parse.
    pub fn compile(&self) -> Result<CompiledFilePolicy, FilePolicyError> {
        Ok(CompiledFilePolicy {
            deny: build_set(&self.deny_paths).map_err(|e| e.in_field("deny_paths"))?,
            prompt: build_set(&self.prompt_paths).map_err(|e| e.in_field("prompt_paths"))?,
            write: build_set(&self.write_paths).map_err(|e| e.in_field("write_paths"))?,
            read: build_set(&self.read_paths).map_err(|e| e.in_field("read_paths"))?,
            default: self.default,
        })
    }

    /// Field-by-field merge: `override` wins per field if non-empty / non-default.
    ///
    /// Used to layer a per-agent `[file_policy]` block over the global one. A
    /// field is treated as "set" if its vector is non-empty; an absent vector
    /// inherits from the base. The `default` tier is overridden iff the
    /// override side is non-default — this means an agent cannot explicitly
    /// re-assert `default = "deny"` to override a base `prompt`, but that's
    /// intentional: per-agent toml is "extend the base", not "replace".
    ///
    /// (If we later need replace-semantics, prefer an explicit `inherit = false`
    /// flag in the schema rather than special-casing values.)
    pub fn merged_over(base: &Self, overlay: &Self) -> Self {
        fn pick<T: Clone>(base: &[T], overlay: &[T]) -> Vec<T> {
            if overlay.is_empty() {
                base.to_vec()
            } else {
                overlay.to_vec()
            }
        }
        Self {
            read_paths: pick(&base.read_paths, &overlay.read_paths),
            write_paths: pick(&base.write_paths, &overlay.write_paths),
            prompt_paths: pick(&base.prompt_paths, &overlay.prompt_paths),
            deny_paths: pick(&base.deny_paths, &overlay.deny_paths),
            default: if overlay.default == DefaultTier::default() {
                base.default
            } else {
                overlay.default
            },
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
}

impl CompiledFilePolicy {
    /// Evaluate a single path against the policy.
    ///
    /// Precedence (most-specific wins; ties broken `deny > prompt > write > read`):
    /// 1. `deny_paths` — always wins.
    /// 2. `prompt_paths`.
    /// 3. `write_paths`.
    /// 4. `read_paths` (read allowed; write → `default`).
    /// 5. Workspace root → implicit `write`.
    /// 6. Otherwise → `default`.
    ///
    /// `path` and `workspace` should both be absolute and canonicalized by the
    /// caller where possible. The evaluator does not touch the filesystem.
    pub fn evaluate(&self, path: &Path, op: FileOp, workspace: &Path) -> FileDecision {
        let path_str = path.to_string_lossy();

        // 1. Deny — absolute block.
        if self.deny.is_match(path.as_os_str()) || self.deny.is_match(&*path_str) {
            return FileDecision::Deny {
                reason: format!("path `{}` matches deny_paths", path_str),
            };
        }

        // 2. Prompt — approval required for any op.
        if self.prompt.is_match(path.as_os_str()) || self.prompt.is_match(&*path_str) {
            return FileDecision::Prompt {
                reason: format!("path `{}` matches prompt_paths ({op:?})", path_str),
            };
        }

        // 3. Write — full access.
        if self.write.is_match(path.as_os_str()) || self.write.is_match(&*path_str) {
            return FileDecision::Allow;
        }

        // 4. Read — read allowed, write falls through to default tier.
        let read_match =
            self.read.is_match(path.as_os_str()) || self.read.is_match(&*path_str);
        if read_match && op == FileOp::Read {
            return FileDecision::Allow;
        }

        // 5. Workspace implicit read+write.
        if path_starts_with(path, workspace) {
            return FileDecision::Allow;
        }

        // 6. Default tier.
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
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum FilePolicyError {
    #[error("invalid glob in `{field}`: pattern `{pattern}`: {source}")]
    InvalidGlob {
        field: &'static str,
        pattern: String,
        #[source]
        source: globset::Error,
    },
}

struct GlobBuildError {
    pattern: String,
    source: globset::Error,
}

impl GlobBuildError {
    fn in_field(self, field: &'static str) -> FilePolicyError {
        FilePolicyError::InvalidGlob {
            field,
            pattern: self.pattern,
            source: self.source,
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
                return Err(GlobBuildError {
                    pattern: raw.clone(),
                    source: e,
                });
            }
        }
    }
    builder.build().map_err(|e| GlobBuildError {
        pattern: String::new(),
        source: e,
    })
}

/// Expand a leading `~` or `~/` to the user's home directory.
///
/// Lone `~` and `~/...` are expanded; `~user/...` is left untouched (we don't
/// resolve other users' homes). Returns the input unchanged if `dirs::home_dir`
/// returns `None`.
pub fn expand_tilde(input: &str) -> String {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), rest);
        }
    } else if input == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.display().to_string();
        }
    }
    input.to_string()
}

fn path_starts_with(path: &Path, root: &Path) -> bool {
    // Component-wise so `/foo` does not match `/foobar`.
    let path_buf: PathBuf = path.into();
    let root_buf: PathBuf = root.into();
    path_buf.starts_with(&root_buf)
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
        p.compile().expect("compile")
    }

    #[test]
    fn workspace_implicit_allow() {
        let p = policy("");
        let inside = ws().join("notes.md");
        assert_eq!(
            p.evaluate(&inside, FileOp::Read, &ws()),
            FileDecision::Allow
        );
        assert_eq!(
            p.evaluate(&inside, FileOp::Write, &ws()),
            FileDecision::Allow
        );
    }

    #[test]
    fn default_deny_outside_workspace() {
        let p = policy("");
        let outside = PathBuf::from("/etc/hosts");
        assert!(matches!(
            p.evaluate(&outside, FileOp::Read, &ws()),
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
            p.evaluate(
                &PathBuf::from("/secrets/key.pem"),
                FileOp::Read,
                &ws()
            ),
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
            p.evaluate(
                &PathBuf::from("/etc/passwd"),
                FileOp::Read,
                &ws()
            ),
            FileDecision::Prompt { .. }
        ));
        // Sibling falls through to write.
        assert_eq!(
            p.evaluate(&PathBuf::from("/etc/hostname"), FileOp::Write, &ws()),
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
            p.evaluate(&PathBuf::from("/data/x.txt"), FileOp::Read, &ws()),
            FileDecision::Allow
        );
        // Write to a read-only matched path falls to default = deny.
        assert!(matches!(
            p.evaluate(&PathBuf::from("/data/x.txt"), FileOp::Write, &ws()),
            FileDecision::Deny { .. }
        ));
    }

    #[test]
    fn default_prompt_tier() {
        let p = policy(r#"default = "prompt""#);
        assert!(matches!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Read, &ws()),
            FileDecision::Prompt { .. }
        ));
    }

    #[test]
    fn default_read_only_tier() {
        let p = policy(r#"default = "read_only""#);
        assert_eq!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Read, &ws()),
            FileDecision::Allow
        );
        assert!(matches!(
            p.evaluate(&PathBuf::from("/random/path"), FileOp::Write, &ws()),
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
        assert_eq!(
            p.evaluate(&target, FileOp::Read, &ws()),
            FileDecision::Allow
        );
    }

    #[test]
    fn invalid_glob_rejected() {
        let p: FilePolicy = toml::from_str(r#"deny_paths = ["[unclosed"]"#).unwrap();
        let err = p.compile().expect_err("should fail");
        match err {
            FilePolicyError::InvalidGlob { field, .. } => assert_eq!(field, "deny_paths"),
        }
    }

    #[test]
    fn merged_over_takes_overlay_when_set() {
        let base = FilePolicy {
            read_paths: vec!["/base/**".into()],
            default: DefaultTier::Deny,
            ..Default::default()
        };
        let overlay = FilePolicy {
            read_paths: vec!["/overlay/**".into()],
            default: DefaultTier::Prompt,
            ..Default::default()
        };
        let merged = FilePolicy::merged_over(&base, &overlay);
        assert_eq!(merged.read_paths, vec!["/overlay/**".to_string()]);
        assert_eq!(merged.default, DefaultTier::Prompt);
    }

    #[test]
    fn merged_over_inherits_when_overlay_empty() {
        let base = FilePolicy {
            read_paths: vec!["/base/**".into()],
            write_paths: vec!["/scratch/**".into()],
            default: DefaultTier::Prompt,
            ..Default::default()
        };
        let overlay = FilePolicy::default();
        let merged = FilePolicy::merged_over(&base, &overlay);
        assert_eq!(merged.read_paths, base.read_paths);
        assert_eq!(merged.write_paths, base.write_paths);
        assert_eq!(merged.default, DefaultTier::Prompt);
    }

    #[test]
    fn workspace_root_not_substring_matched() {
        // /tmp/openfang-test-ws-other should NOT match /tmp/openfang-test-ws.
        let p = policy("");
        let sibling = PathBuf::from("/tmp/openfang-test-ws-other/file.txt");
        assert!(matches!(
            p.evaluate(&sibling, FileOp::Read, &ws()),
            FileDecision::Deny { .. }
        ));
    }
}
