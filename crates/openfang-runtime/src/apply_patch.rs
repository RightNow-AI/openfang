//! Multi-hunk diff-based file patching.
//!
//! Implements a structured patch format similar to unified diffs, allowing
//! targeted edits without full file overwrites. Supports adding, updating
//! (including move/rename), and deleting files with multi-hunk precision.
//!
//! Patch format:
//! ```text
//! *** Begin Patch
//! *** Add File: path/to/new.rs
//! +line1
//! +line2
//! *** Update File: path/to/existing.rs
//! @@ context_before @@
//!  unchanged_line
//! -old_line
//! +new_line
//!  unchanged_line
//! *** Delete File: path/to/old.rs
//! *** End Patch
//! ```

use std::path::{Path, PathBuf};
use tracing::warn;

/// A single operation in a patch.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchOp {
    /// Add a new file with the given content.
    AddFile { path: String, content: String },
    /// Update an existing file, optionally moving/renaming it.
    UpdateFile {
        path: String,
        move_to: Option<String>,
        hunks: Vec<Hunk>,
    },
    /// Delete an existing file.
    DeleteFile { path: String },
}

/// A single hunk within a file update — describes one contiguous change region.
#[derive(Debug, Clone, PartialEq)]
pub struct Hunk {
    /// Lines of unchanged context before the change (for anchoring).
    pub context_before: Vec<String>,
    /// Old lines to be removed (without `-` prefix).
    pub old_lines: Vec<String>,
    /// New lines to be inserted (without `+` prefix).
    pub new_lines: Vec<String>,
    /// Lines of unchanged context after the change (for anchoring).
    pub context_after: Vec<String>,
}

/// Result of applying a patch.
#[derive(Debug, Default)]
pub struct PatchResult {
    /// Number of files added.
    pub files_added: u32,
    /// Number of files updated.
    pub files_updated: u32,
    /// Number of files deleted.
    pub files_deleted: u32,
    /// Number of files moved/renamed.
    pub files_moved: u32,
    /// Errors encountered during application.
    pub errors: Vec<String>,
}

impl PatchResult {
    /// Returns true if no errors occurred.
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Summary string for tool output.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if self.files_added > 0 {
            parts.push(format!("{} added", self.files_added));
        }
        if self.files_updated > 0 {
            parts.push(format!("{} updated", self.files_updated));
        }
        if self.files_deleted > 0 {
            parts.push(format!("{} deleted", self.files_deleted));
        }
        if self.files_moved > 0 {
            parts.push(format!("{} moved", self.files_moved));
        }
        if !self.errors.is_empty() {
            parts.push(format!("{} errors", self.errors.len()));
        }
        if parts.is_empty() {
            "No changes applied".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Parse a patch string into a list of `PatchOp`s.
///
/// Expects the format delimited by `*** Begin Patch` and `*** End Patch`.
/// Within that block, each file operation starts with `*** Add File:`,
/// `*** Update File:`, or `*** Delete File:`.
pub fn parse_patch(input: &str) -> Result<Vec<PatchOp>, String> {
    let lines: Vec<&str> = input.lines().collect();
    let mut ops = Vec::new();

    // Find begin/end markers
    let begin = lines
        .iter()
        .position(|l| l.trim() == "*** Begin Patch")
        .ok_or("Missing '*** Begin Patch' marker")?;
    let end = lines
        .iter()
        .rposition(|l| l.trim() == "*** End Patch")
        .ok_or("Missing '*** End Patch' marker")?;

    if end <= begin {
        return Err("'*** End Patch' must come after '*** Begin Patch'".to_string());
    }

    let body = &lines[begin + 1..end];
    let mut i = 0;

    while i < body.len() {
        let line = body[i].trim();

        if line.starts_with("*** Add File:") {
            let path = line
                .strip_prefix("*** Add File:")
                .unwrap()
                .trim()
                .to_string();
            if path.is_empty() {
                return Err("Empty path in '*** Add File:'".to_string());
            }
            i += 1;

            // Collect content lines (prefixed with +)
            let mut content_lines = Vec::new();
            while i < body.len() && !body[i].trim().starts_with("***") {
                let l = body[i];
                if let Some(stripped) = l.strip_prefix('+') {
                    content_lines.push(stripped.to_string());
                } else if !l.trim().is_empty() {
                    return Err(format!(
                        "Expected '+' prefix in Add File content, got: {}",
                        l
                    ));
                }
                i += 1;
            }
            ops.push(PatchOp::AddFile {
                path,
                content: content_lines.join("\n"),
            });
        } else if line.starts_with("*** Update File:") {
            let rest = line.strip_prefix("*** Update File:").unwrap().trim();
            // Check for move syntax: "old_path -> new_path"
            let (path, move_to) = if let Some((old, new)) = rest.split_once("->") {
                (old.trim().to_string(), Some(new.trim().to_string()))
            } else {
                (rest.to_string(), None)
            };
            if path.is_empty() {
                return Err("Empty path in '*** Update File:'".to_string());
            }
            i += 1;

            // Parse hunks
            let mut hunks = Vec::new();
            while i < body.len() && !body[i].trim().starts_with("***") {
                let l = body[i].trim();
                if l.starts_with("@@") {
                    i += 1;
                    // Parse hunk body
                    let mut context_before = Vec::new();
                    let mut old_lines = Vec::new();
                    let mut new_lines = Vec::new();
                    let mut context_after = Vec::new();
                    let mut in_change = false;
                    let mut past_change = false;

                    while i < body.len()
                        && !body[i].trim().starts_with("@@")
                        && !body[i].trim().starts_with("***")
                    {
                        let hl = body[i];
                        if let Some(stripped) = hl.strip_prefix('-') {
                            in_change = true;
                            past_change = false;
                            old_lines.push(stripped.to_string());
                        } else if let Some(stripped) = hl.strip_prefix('+') {
                            in_change = true;
                            past_change = false;
                            new_lines.push(stripped.to_string());
                        } else if let Some(stripped) = hl.strip_prefix(' ') {
                            if in_change || past_change {
                                past_change = true;
                                in_change = false;
                                context_after.push(stripped.to_string());
                            } else {
                                context_before.push(stripped.to_string());
                            }
                        } else if hl.trim().is_empty() {
                            // Blank line counts as context
                            if in_change || past_change {
                                past_change = true;
                                in_change = false;
                                context_after.push(String::new());
                            } else {
                                context_before.push(String::new());
                            }
                        } else {
                            // Unrecognized line, treat as context
                            if in_change || past_change {
                                past_change = true;
                                in_change = false;
                                context_after.push(hl.to_string());
                            } else {
                                context_before.push(hl.to_string());
                            }
                        }
                        i += 1;
                    }

                    hunks.push(Hunk {
                        context_before,
                        old_lines,
                        new_lines,
                        context_after,
                    });
                } else {
                    i += 1;
                }
            }

            if hunks.is_empty() {
                return Err(format!("Update File '{}' has no hunks", path));
            }

            ops.push(PatchOp::UpdateFile {
                path,
                move_to,
                hunks,
            });
        } else if line.starts_with("*** Delete File:") {
            let path = line
                .strip_prefix("*** Delete File:")
                .unwrap()
                .trim()
                .to_string();
            if path.is_empty() {
                return Err("Empty path in '*** Delete File:'".to_string());
            }
            i += 1;
            ops.push(PatchOp::DeleteFile { path });
        } else if line.is_empty() {
            i += 1;
        } else {
            return Err(format!("Unexpected line in patch: {}", line));
        }
    }

    if ops.is_empty() {
        return Err("Patch contains no operations".to_string());
    }

    Ok(ops)
}

/// Resolve a patch path through workspace confinement.
fn resolve_patch_path(raw: &str, workspace_root: &Path) -> Result<PathBuf, String> {
    crate::workspace_sandbox::resolve_sandbox_path(raw, workspace_root)
}

/// Apply parsed patch operations against the filesystem.
///
/// All file paths are confined to `workspace_root` via sandbox resolution.
pub async fn apply_patch(ops: &[PatchOp], workspace_root: &Path) -> PatchResult {
    let mut result = PatchResult::default();

    for op in ops {
        match op {
            PatchOp::AddFile { path, content } => match resolve_patch_path(path, workspace_root) {
                Ok(resolved) => {
                    if let Some(parent) = resolved.parent() {
                        if let Err(e) = tokio::fs::create_dir_all(parent).await {
                            result.errors.push(format!("mkdir {}: {}", path, e));
                            continue;
                        }
                    }
                    match tokio::fs::write(&resolved, content).await {
                        Ok(()) => result.files_added += 1,
                        Err(e) => result.errors.push(format!("write {}: {}", path, e)),
                    }
                }
                Err(e) => result.errors.push(format!("{}: {}", path, e)),
            },

            PatchOp::UpdateFile {
                path,
                move_to,
                hunks,
            } => {
                let resolved = match resolve_patch_path(path, workspace_root) {
                    Ok(r) => r,
                    Err(e) => {
                        result.errors.push(format!("{}: {}", path, e));
                        continue;
                    }
                };

                // Read existing content
                let original = match tokio::fs::read_to_string(&resolved).await {
                    Ok(c) => c,
                    Err(e) => {
                        result.errors.push(format!("read {}: {}", path, e));
                        continue;
                    }
                };

                // Apply hunks sequentially
                match apply_hunks(&original, hunks) {
                    Ok(patched) => {
                        // Determine target path (move or in-place)
                        let target = if let Some(new_path) = move_to {
                            match resolve_patch_path(new_path, workspace_root) {
                                Ok(t) => {
                                    result.files_moved += 1;
                                    t
                                }
                                Err(e) => {
                                    result.errors.push(format!("{}: {}", new_path, e));
                                    continue;
                                }
                            }
                        } else {
                            resolved.clone()
                        };

                        if let Some(parent) = target.parent() {
                            let _ = tokio::fs::create_dir_all(parent).await;
                        }

                        match tokio::fs::write(&target, patched).await {
                            Ok(()) => {
                                result.files_updated += 1;
                                // If moved, delete original
                                if move_to.is_some() && target != resolved {
                                    let _ = tokio::fs::remove_file(&resolved).await;
                                }
                            }
                            Err(e) => {
                                result.errors.push(format!("write {}: {}", path, e));
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("patch {}: {}", path, e));
                    }
                }
            }

            PatchOp::DeleteFile { path } => match resolve_patch_path(path, workspace_root) {
                Ok(resolved) => match tokio::fs::remove_file(&resolved).await {
                    Ok(()) => result.files_deleted += 1,
                    Err(e) => {
                        result.errors.push(format!("delete {}: {}", path, e));
                    }
                },
                Err(e) => result.errors.push(format!("{}: {}", path, e)),
            },
        }
    }

    result
}

/// Apply a sequence of hunks to file content.
///
/// Each hunk's `context_before` + `old_lines` are searched for in the content.
/// When found, `old_lines` are replaced with `new_lines`. Includes fuzzy
/// whitespace fallback on mismatch.
fn apply_hunks(content: &str, hunks: &[Hunk]) -> Result<String, String> {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    // Track if original file ended with newline
    let trailing_newline = content.ends_with('\n');

    for (hunk_idx, hunk) in hunks.iter().enumerate() {
        let anchor: Vec<&str> = hunk
            .context_before
            .iter()
            .chain(hunk.old_lines.iter())
            .map(|s| s.as_str())
            .collect();

        if anchor.is_empty() && hunk.old_lines.is_empty() {
            // Pure insertion hunk — append new lines at end
            lines.extend(hunk.new_lines.iter().cloned());
            continue;
        }

        // Find the anchor in the file
        let pos = find_anchor(&lines, &anchor)
            .or_else(|| find_anchor_fuzzy(&lines, &anchor))
            .ok_or_else(|| {
                format!(
                    "Hunk {} failed: could not find context/old lines in file",
                    hunk_idx + 1
                )
            })?;

        // Replace: remove context_before + old_lines, insert context_before + new_lines
        let remove_count = hunk.context_before.len() + hunk.old_lines.len();
        let mut replacement: Vec<String> = hunk.context_before.clone();
        replacement.extend(hunk.new_lines.iter().cloned());

        lines.splice(pos..pos + remove_count, replacement);
    }

    let mut result = lines.join("\n");
    if trailing_newline && !result.ends_with('\n') {
        result.push('\n');
    }
    Ok(result)
}

/// Find an exact match for the anchor lines in the file.
fn find_anchor(file_lines: &[String], anchor: &[&str]) -> Option<usize> {
    if anchor.is_empty() {
        return Some(file_lines.len());
    }
    if anchor.len() > file_lines.len() {
        return None;
    }

    'outer: for start in 0..=file_lines.len() - anchor.len() {
        for (j, expected) in anchor.iter().enumerate() {
            if file_lines[start + j] != *expected {
                continue 'outer;
            }
        }
        return Some(start);
    }
    None
}

/// Fuzzy anchor matching — trims trailing whitespace before comparing.
fn find_anchor_fuzzy(file_lines: &[String], anchor: &[&str]) -> Option<usize> {
    if anchor.is_empty() {
        return Some(file_lines.len());
    }
    if anchor.len() > file_lines.len() {
        return None;
    }

    'outer: for start in 0..=file_lines.len() - anchor.len() {
        for (j, expected) in anchor.iter().enumerate() {
            if file_lines[start + j].trim_end() != expected.trim_end() {
                continue 'outer;
            }
        }
        warn!(
            "Patch hunk matched with fuzzy whitespace at line {}",
            start + 1
        );
        return Some(start);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_add_file() {
        let patch = "\
*** Begin Patch
*** Add File: src/new.rs
+fn main() {
+    println!(\"hello\");
+}
*** End Patch";
        let ops = parse_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            PatchOp::AddFile { path, content } => {
                assert_eq!(path, "src/new.rs");
                assert!(content.contains("fn main()"));
            }
            _ => panic!("Expected AddFile"),
        }
    }

    #[test]
    fn test_parse_update_file() {
        let patch = "\
*** Begin Patch
*** Update File: src/lib.rs
@@ hunk 1 @@
 fn existing() {
-    old_code();
+    new_code();
 }
*** End Patch";
        let ops = parse_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            PatchOp::UpdateFile {
                path,
                hunks,
                move_to,
            } => {
                assert_eq!(path, "src/lib.rs");
                assert!(move_to.is_none());
                assert_eq!(hunks.len(), 1);
                assert_eq!(hunks[0].context_before, vec!["fn existing() {"]);
                assert_eq!(hunks[0].old_lines, vec!["    old_code();"]);
                assert_eq!(hunks[0].new_lines, vec!["    new_code();"]);
                assert_eq!(hunks[0].context_after, vec!["}"]);
            }
            _ => panic!("Expected UpdateFile"),
        }
    }

    #[test]
    fn test_parse_delete_file() {
        let patch = "\
*** Begin Patch
*** Delete File: src/old.rs
*** End Patch";
        let ops = parse_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            PatchOp::DeleteFile { path } => assert_eq!(path, "src/old.rs"),
            _ => panic!("Expected DeleteFile"),
        }
    }

    #[test]
    fn test_parse_move_file() {
        let patch = "\
*** Begin Patch
*** Update File: old/path.rs -> new/path.rs
@@ hunk @@
 keep_this
-remove_this
+add_this
*** End Patch";
        let ops = parse_patch(patch).unwrap();
        assert_eq!(ops.len(), 1);
        match &ops[0] {
            PatchOp::UpdateFile { path, move_to, .. } => {
                assert_eq!(path, "old/path.rs");
                assert_eq!(move_to.as_deref(), Some("new/path.rs"));
            }
            _ => panic!("Expected UpdateFile"),
        }
    }

    #[test]
    fn test_parse_multi_op() {
        let patch = "\
*** Begin Patch
*** Add File: a.txt
+hello
*** Delete File: b.txt
*** Update File: c.txt
@@ hunk @@
-old
+new
*** End Patch";
        let ops = parse_patch(patch).unwrap();
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], PatchOp::AddFile { .. }));
        assert!(matches!(&ops[1], PatchOp::DeleteFile { .. }));
        assert!(matches!(&ops[2], PatchOp::UpdateFile { .. }));
    }

    #[test]
    fn test_parse_missing_begin() {
        let patch = "*** Add File: a.txt\n+hello\n*** End Patch";
        assert!(parse_patch(patch).is_err());
    }

    #[test]
    fn test_parse_missing_end() {
        let patch = "*** Begin Patch\n*** Add File: a.txt\n+hello";
        assert!(parse_patch(patch).is_err());
    }

    #[test]
    fn test_parse_empty_patch() {
        let patch = "*** Begin Patch\n*** End Patch";
        assert!(parse_patch(patch).is_err());
    }

    #[test]
    fn test_apply_hunks_simple() {
        let content = "line1\nline2\nline3\n";
        let hunks = vec![Hunk {
            context_before: vec!["line1".to_string()],
            old_lines: vec!["line2".to_string()],
            new_lines: vec!["replaced".to_string()],
            context_after: vec![],
        }];
        let result = apply_hunks(content, &hunks).unwrap();
        assert!(result.contains("replaced"));
        assert!(!result.contains("line2"));
        assert!(result.contains("line1"));
        assert!(result.contains("line3"));
    }

    #[test]
    fn test_apply_hunks_multi_hunk() {
        let content = "a\nb\nc\nd\ne\n";
        let hunks = vec![
            Hunk {
                context_before: vec!["a".to_string()],
                old_lines: vec!["b".to_string()],
                new_lines: vec!["B".to_string()],
                context_after: vec![],
            },
            Hunk {
                context_before: vec!["c".to_string()],
                old_lines: vec!["d".to_string()],
                new_lines: vec!["D".to_string(), "D2".to_string()],
                context_after: vec![],
            },
        ];
        let result = apply_hunks(content, &hunks).unwrap();
        assert!(result.contains("B"));
        assert!(result.contains("D\nD2"));
        assert!(!result.contains("\nb\n"));
        assert!(!result.contains("\nd\n"));
    }

    #[test]
    fn test_apply_hunks_context_mismatch() {
        let content = "alpha\nbeta\ngamma\n";
        let hunks = vec![Hunk {
            context_before: vec!["nonexistent".to_string()],
            old_lines: vec!["also_nonexistent".to_string()],
            new_lines: vec!["new".to_string()],
            context_after: vec![],
        }];
        assert!(apply_hunks(content, &hunks).is_err());
    }

    #[test]
    fn test_apply_hunks_fuzzy_whitespace() {
        let content = "line1  \nline2\t\nline3\n";
        let hunks = vec![Hunk {
            context_before: vec!["line1".to_string()],
            old_lines: vec!["line2".to_string()],
            new_lines: vec!["replaced".to_string()],
            context_after: vec![],
        }];
        let result = apply_hunks(content, &hunks).unwrap();
        assert!(result.contains("replaced"));
    }

    #[test]
    fn test_apply_hunks_preserves_unchanged() {
        let content = "header\nkeep1\nkeep2\nold_line\nkeep3\nfooter\n";
        let hunks = vec![Hunk {
            context_before: vec!["keep2".to_string()],
            old_lines: vec!["old_line".to_string()],
            new_lines: vec!["new_line".to_string()],
            context_after: vec![],
        }];
        let result = apply_hunks(content, &hunks).unwrap();
        assert!(result.contains("header"));
        assert!(result.contains("keep1"));
        assert!(result.contains("keep2"));
        assert!(result.contains("new_line"));
        assert!(result.contains("keep3"));
        assert!(result.contains("footer"));
        assert!(!result.contains("old_line"));
    }

    #[test]
    fn test_find_anchor_exact() {
        let lines: Vec<String> = vec!["a", "b", "c", "d"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(find_anchor(&lines, &["b", "c"]), Some(1));
    }

    #[test]
    fn test_find_anchor_not_found() {
        let lines: Vec<String> = vec!["a", "b", "c"].into_iter().map(String::from).collect();
        assert_eq!(find_anchor(&lines, &["x", "y"]), None);
    }

    #[test]
    fn test_find_anchor_fuzzy() {
        let lines: Vec<String> = vec!["a  ", "b\t", "c"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(find_anchor_fuzzy(&lines, &["a", "b"]), Some(0));
    }

    #[tokio::test]
    async fn test_apply_patch_integration() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        // Write a file to update
        tokio::fs::write(dir.join("existing.txt"), "line1\nline2\nline3\n")
            .await
            .unwrap();

        let ops = vec![
            PatchOp::AddFile {
                path: "new.txt".to_string(),
                content: "hello world".to_string(),
            },
            PatchOp::UpdateFile {
                path: "existing.txt".to_string(),
                move_to: None,
                hunks: vec![Hunk {
                    context_before: vec!["line1".to_string()],
                    old_lines: vec!["line2".to_string()],
                    new_lines: vec!["replaced".to_string()],
                    context_after: vec![],
                }],
            },
        ];

        let result = apply_patch(&ops, dir).await;
        assert!(result.is_ok());
        assert_eq!(result.files_added, 1);
        assert_eq!(result.files_updated, 1);

        // Verify files
        let new_content = tokio::fs::read_to_string(dir.join("new.txt"))
            .await
            .unwrap();
        assert_eq!(new_content, "hello world");

        let updated = tokio::fs::read_to_string(dir.join("existing.txt"))
            .await
            .unwrap();
        assert!(updated.contains("replaced"));
        assert!(!updated.contains("line2"));
    }

    #[tokio::test]
    async fn test_apply_patch_delete() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();

        tokio::fs::write(dir.join("doomed.txt"), "goodbye")
            .await
            .unwrap();

        let ops = vec![PatchOp::DeleteFile {
            path: "doomed.txt".to_string(),
        }];

        let result = apply_patch(&ops, dir).await;
        assert!(result.is_ok());
        assert_eq!(result.files_deleted, 1);
        assert!(!dir.join("doomed.txt").exists());
    }
}
