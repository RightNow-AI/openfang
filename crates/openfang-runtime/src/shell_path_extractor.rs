//! Shell command → filesystem path extractor (ANAI-40 step 4).
//!
//! `file_policy` (ANAI-40) needs to gate filesystem access from the shell
//! vector the same way it gates the MCP `file_*` tools. We can't trivially
//! know which files an arbitrary `bash -c "..."` will touch, but we *can*
//! recognize the small set of common file-touching commands and pull the
//! paths out of their argv. That's what this module does.
//!
//! ## Scope
//!
//! Per the ANAI-40 brief, v1 covers:
//!
//! - **Read-only:** `cat`, `rg`, `grep`, `head`, `tail`, `less`, `wc`, `ls`,
//!   `find` (root arg).
//! - **Write:** `cp` (dst), `mv` (dst), `rm`, `mkdir`, `tee` (target file),
//!   plus `cp` / `mv` source paths as Read.
//!
//! Shell metacharacters (`>`, `>>`, `|`, `<`, `&`, `;`, `$()`, backticks,
//! brace expansion, newlines, etc.) are already rejected upstream by
//! [`subprocess_sandbox::contains_shell_metacharacters`], so this module
//! does not need to handle redirect operators.
//!
//! Anything not in the table → we return `Unknown`. The caller decides what
//! to do (current policy: fall through to `exec_policy`; `file_policy` does
//! not gate unrecognized commands).
//!
//! ## Design notes
//!
//! - Pure function: input is parsed argv, output is a list of `(path, op)`
//!   pairs. No I/O. No globbing. No canonicalization. The caller is
//!   responsible for resolving paths against the agent CWD before evaluating.
//! - We accept the *split argv*, not the raw command string. The caller has
//!   already run `shlex::split` (or equivalent) — duplicating that here would
//!   risk a different parse than the one used to actually spawn the process.
//! - Best-effort flag handling: we only need to skip option-style tokens so
//!   we don't mistake `-rf` for a path. We don't validate flags or values.
//!   For commands where `-X` takes a value (e.g. `find -name PATTERN`),
//!   `find`'s special primary-vs-path positional rules are handled inline.

use openfang_types::file_policy::FileOp;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Outcome of running the extractor over a parsed argv.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Extraction {
    /// We recognize this command and extracted zero or more (path, op) pairs.
    /// Empty vec is legitimate — e.g. `ls` with no args lists CWD, which the
    /// caller can map to its own CWD evaluation if it cares.
    Known(Vec<(PathBuf, FileOp)>),
    /// We don't recognize the base command. `file_policy` does not gate it.
    Unknown,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Extract filesystem paths from a parsed shell argv.
///
/// `argv[0]` is treated as the command (basename only — `/usr/bin/cat` and
/// `cat` are equivalent). Returns [`Extraction::Unknown`] for empty argv or
/// unrecognized commands.
pub fn extract(argv: &[String]) -> Extraction {
    let Some(first) = argv.first() else {
        return Extraction::Unknown;
    };

    // Take basename of argv[0]: `/usr/bin/rg` → `rg`.
    let cmd = std::path::Path::new(first)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(first.as_str());

    let rest = &argv[1..];

    match cmd {
        // Read-only: positional args (post-flag) are paths read from.
        "cat" | "rg" | "grep" | "head" | "tail" | "less" | "wc" | "ls" => {
            Extraction::Known(positional_paths(rest, FileOp::Read))
        }

        // Write: rm/mkdir touch every positional path destructively.
        "rm" | "mkdir" => Extraction::Known(positional_paths(rest, FileOp::Write)),

        // cp / mv: positionals are [src.., dst]. Sources Read, dst Write.
        "cp" | "mv" => Extraction::Known(extract_cp_mv(rest)),

        // tee: writes to each positional file (and reads stdin, no FS).
        // `tee -a FILE` still writes.
        "tee" => Extraction::Known(positional_paths(rest, FileOp::Write)),

        // find: first positional (or args until first primary `-name`/`-type`/...)
        // is the root being read.
        "find" => Extraction::Known(extract_find(rest)),

        _ => Extraction::Unknown,
    }
}

// ---------------------------------------------------------------------------
// Per-command helpers
// ---------------------------------------------------------------------------

/// Strip option-like tokens and return remaining positionals tagged with `op`.
///
/// Stops flag-skipping after a bare `--` token (POSIX end-of-options marker).
/// Tokens starting with `-` (other than `-` itself, which means stdin/stdout)
/// are treated as flags. We do NOT parse flag values — for the v1 command
/// list, none of the read-only commands have flags whose values are paths
/// (e.g. `grep -f FILE` would be a false positive, but `grep -f` is rare and
/// the worst case is an over-evaluation that the user can resolve via policy).
fn positional_paths(argv: &[String], op: FileOp) -> Vec<(PathBuf, FileOp)> {
    let mut out = Vec::new();
    let mut end_of_options = false;
    for tok in argv {
        if !end_of_options {
            if tok == "--" {
                end_of_options = true;
                continue;
            }
            if is_flag(tok) {
                continue;
            }
        }
        // `-` as a bare token = stdin/stdout, not a path. Skip.
        if tok == "-" {
            continue;
        }
        out.push((PathBuf::from(tok), op));
    }
    out
}

/// `cp [flags] SRC... DST` and `mv [flags] SRC... DST`.
///
/// Treats every positional except the last as `Read`, last as `Write`.
/// Single-positional case (rare for cp/mv but technically possible with
/// `-t DIR`) is treated as `Read` to be safe — better to over-prompt than
/// under-prompt on a write.
fn extract_cp_mv(argv: &[String]) -> Vec<(PathBuf, FileOp)> {
    let positionals: Vec<&String> = strip_flags(argv).collect();
    let n = positionals.len();
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![(PathBuf::from(positionals[0]), FileOp::Read)];
    }
    let mut out = Vec::with_capacity(n);
    for src in &positionals[..n - 1] {
        out.push((PathBuf::from(*src), FileOp::Read));
    }
    out.push((PathBuf::from(positionals[n - 1]), FileOp::Write));
    out
}

/// `find [PATH...] [PRIMARY...]`.
///
/// Paths are the positionals up to the first primary (any token starting
/// with `-`). Default path is `.` if none given. We do not descend into
/// `-name`/`-path` patterns — those are matchers, not roots.
fn extract_find(argv: &[String]) -> Vec<(PathBuf, FileOp)> {
    let mut roots = Vec::new();
    for tok in argv {
        if tok.starts_with('-') {
            break;
        }
        roots.push(PathBuf::from(tok));
    }
    if roots.is_empty() {
        roots.push(PathBuf::from("."));
    }
    roots.into_iter().map(|p| (p, FileOp::Read)).collect()
}

// ---------------------------------------------------------------------------
// Token classification
// ---------------------------------------------------------------------------

/// True if `tok` looks like a flag: starts with `-` and is not the bare `-`
/// (which conventionally means stdin/stdout).
fn is_flag(tok: &str) -> bool {
    tok.len() > 1 && tok.starts_with('-')
}

/// Iterator over positionals only, honoring `--` end-of-options.
fn strip_flags(argv: &[String]) -> impl Iterator<Item = &String> {
    let mut end = false;
    argv.iter().filter(move |tok| {
        if end {
            return tok.as_str() != "-"; // `-` is stdin, not a path.
        }
        if *tok == "--" {
            end = true;
            return false;
        }
        if is_flag(tok) {
            return false;
        }
        tok.as_str() != "-"
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    fn paths(e: Extraction) -> Vec<(PathBuf, FileOp)> {
        match e {
            Extraction::Known(v) => v,
            Extraction::Unknown => panic!("expected Known, got Unknown"),
        }
    }

    #[test]
    fn empty_argv_is_unknown() {
        assert_eq!(extract(&[]), Extraction::Unknown);
    }

    #[test]
    fn unknown_command_returns_unknown() {
        assert_eq!(
            extract(&argv(&["whatever_tool", "/etc/hosts"])),
            Extraction::Unknown
        );
    }

    #[test]
    fn cat_extracts_each_positional_as_read() {
        let got = paths(extract(&argv(&["cat", "/etc/hosts", "/tmp/notes.txt"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/etc/hosts"), FileOp::Read),
                (PathBuf::from("/tmp/notes.txt"), FileOp::Read),
            ]
        );
    }

    #[test]
    fn cat_skips_flags() {
        let got = paths(extract(&argv(&["cat", "-n", "/etc/hosts"])));
        assert_eq!(got, vec![(PathBuf::from("/etc/hosts"), FileOp::Read)]);
    }

    #[test]
    fn double_dash_ends_options() {
        // After `--`, even `-foo` is a path.
        let got = paths(extract(&argv(&["cat", "--", "-weird-filename"])));
        assert_eq!(
            got,
            vec![(PathBuf::from("-weird-filename"), FileOp::Read)]
        );
    }

    #[test]
    fn bare_dash_is_not_a_path() {
        // `cat -` reads stdin; not a filesystem path.
        let got = paths(extract(&argv(&["cat", "-", "/etc/hosts"])));
        assert_eq!(got, vec![(PathBuf::from("/etc/hosts"), FileOp::Read)]);
    }

    #[test]
    fn rg_with_pattern_and_path() {
        // First positional is the pattern, second is the path. The extractor
        // can't distinguish them — so it tags both as Read. That's a known
        // false positive; policy should pass for non-paths (the pattern
        // won't resolve to anything restricted).
        let got = paths(extract(&argv(&["rg", "needle", "src/"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("needle"), FileOp::Read),
                (PathBuf::from("src/"), FileOp::Read),
            ]
        );
    }

    #[test]
    fn rm_treats_positionals_as_writes() {
        let got = paths(extract(&argv(&["rm", "-rf", "/tmp/foo", "/tmp/bar"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/tmp/foo"), FileOp::Write),
                (PathBuf::from("/tmp/bar"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn mkdir_is_write() {
        let got = paths(extract(&argv(&["mkdir", "-p", "/tmp/new/dir"])));
        assert_eq!(got, vec![(PathBuf::from("/tmp/new/dir"), FileOp::Write)]);
    }

    #[test]
    fn cp_marks_sources_read_dest_write() {
        let got = paths(extract(&argv(&[
            "cp", "-r", "/src/a", "/src/b", "/dst/dir",
        ])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/src/a"), FileOp::Read),
                (PathBuf::from("/src/b"), FileOp::Read),
                (PathBuf::from("/dst/dir"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn mv_single_positional_treated_as_read() {
        // Pathological case — better to over-prompt as Read than to silently
        // mark a single positional as Write and let dst-path semantics drift.
        let got = paths(extract(&argv(&["mv", "only-arg"])));
        assert_eq!(got, vec![(PathBuf::from("only-arg"), FileOp::Read)]);
    }

    #[test]
    fn tee_writes_to_each_file() {
        let got = paths(extract(&argv(&["tee", "-a", "/var/log/a", "/var/log/b"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/var/log/a"), FileOp::Write),
                (PathBuf::from("/var/log/b"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn find_uses_first_positional_as_root() {
        let got = paths(extract(&argv(&[
            "find", "/etc", "-name", "*.conf", "-type", "f",
        ])));
        assert_eq!(got, vec![(PathBuf::from("/etc"), FileOp::Read)]);
    }

    #[test]
    fn find_multiple_roots() {
        let got = paths(extract(&argv(&["find", "/etc", "/var", "-name", "x"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/etc"), FileOp::Read),
                (PathBuf::from("/var"), FileOp::Read),
            ]
        );
    }

    #[test]
    fn find_with_no_root_defaults_to_dot() {
        let got = paths(extract(&argv(&["find", "-name", "x"])));
        assert_eq!(got, vec![(PathBuf::from("."), FileOp::Read)]);
    }

    #[test]
    fn ls_with_only_flags_yields_no_paths() {
        // `ls -la` lists CWD; the extractor returns no explicit paths and
        // the caller can decide whether to evaluate CWD itself.
        let got = paths(extract(&argv(&["ls", "-la"])));
        assert_eq!(got, vec![]);
    }

    #[test]
    fn basename_normalization() {
        // `/usr/bin/cat` should be treated identically to `cat`.
        let got = paths(extract(&argv(&["/usr/bin/cat", "/etc/hosts"])));
        assert_eq!(got, vec![(PathBuf::from("/etc/hosts"), FileOp::Read)]);
    }
}
