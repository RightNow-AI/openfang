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
//!   `find` (root arg, when no destructive primary).
//! - **Write:** `cp` (dst), `mv` (dst), `rm`, `mkdir`, `tee` (target file),
//!   `touch`, `sed -i` (in-place input files), `dd of=`, `find -delete` /
//!   `find -fprint*` / `find -fls` (the find roots become Write; the
//!   explicit `-fprint{,0,f} TARGET` and `-fls TARGET` files become Write).
//!   `cp` / `mv` source paths are tagged Read.
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
//! ## Prerequisite: `exec_policy = Allowlist`
//!
//! `file_policy` enforcement on the shell vector is only meaningful when
//! `exec_policy.mode = "allowlist"`. In `Full` mode, an agent can rename or
//! shadow binaries (`alias mycat=cat`, copy `/bin/cat` to `mycat`, write a
//! tiny script named `myrm`, etc.) and the extractor — which keys off
//! `argv[0]` basename — will fall through to `Unknown`. Treat the shell-vector
//! gate as **best-effort** under `Full`; the MCP `file_*` tools remain fully
//! gated regardless of `exec_policy`. For strong enforcement on the shell
//! vector, agents should run with `exec_policy.mode = "allowlist"` and a
//! curated `allowed_commands` list.
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

        // Write: rm/mkdir/touch touch every positional path destructively.
        "rm" | "mkdir" | "touch" => Extraction::Known(positional_paths(rest, FileOp::Write)),

        // cp / mv: positionals are [src.., dst]. Sources Read, dst Write.
        "cp" | "mv" => Extraction::Known(extract_cp_mv(rest)),

        // tee: writes to each positional file (and reads stdin, no FS).
        // `tee -a FILE` still writes.
        "tee" => Extraction::Known(positional_paths(rest, FileOp::Write)),

        // sed: `sed [-i[SUFFIX]] SCRIPT FILE...`. With `-i` (in-place edit),
        // the input files are Write targets; without, they're Read. The first
        // positional is normally the script, not a file — but flagging it as
        // Read/Write is harmless if it's not a real path (it won't match
        // any policy rule that targets actual files).
        "sed" => Extraction::Known(extract_sed(rest)),

        // dd: `dd if=SRC of=DST [bs=...] [count=...]`. Key=value form, not
        // positional. `if=` is Read, `of=` is Write.
        "dd" => Extraction::Known(extract_dd(rest)),

        // find: roots up to first primary; some primaries are destructive
        // (`-delete`) or have file-output sinks (`-fprint`, `-fprint0`,
        // `-fprintf`, `-fls`).
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
            // `-` as a bare token = stdin/stdout, not a path. Skip — but
            // only while options are still being parsed. Post-`--`, `-` is
            // a literal filename per POSIX.
            if tok == "-" {
                continue;
            }
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

/// `sed [-i[SUFFIX]] [-n] [-e SCRIPT|-f FILE] [SCRIPT] FILE...`.
///
/// We only need to know two things:
///   - Is `-i` (in-place) set? → input files become Write.
///   - Otherwise, input files are Read.
/// We don't reliably know which positional is the script vs a file (depends
/// on whether `-e`/`-f` was used), so we evaluate all positionals at the
/// chosen op. False positives on a script-string are harmless: the policy
/// won't match it as a path.
fn extract_sed(argv: &[String]) -> Vec<(PathBuf, FileOp)> {
    let in_place = argv.iter().any(|t| {
        // GNU/BSD: `-i`, `-i.bak`, `-i ''`, `--in-place`, `--in-place=.bak`
        t == "-i"
            || t.starts_with("-i") && t.len() > 1 && !t.starts_with("--")
            || t == "--in-place"
            || t.starts_with("--in-place=")
    });
    let op = if in_place { FileOp::Write } else { FileOp::Read };
    positional_paths(argv, op)
}

/// `dd if=SRC of=DST [other=...]`. Key=value form; `if=` is Read, `of=` is Write.
/// Anything else is ignored.
fn extract_dd(argv: &[String]) -> Vec<(PathBuf, FileOp)> {
    let mut out = Vec::new();
    for tok in argv {
        if let Some(rest) = tok.strip_prefix("if=") {
            if !rest.is_empty() {
                out.push((PathBuf::from(rest), FileOp::Read));
            }
        } else if let Some(rest) = tok.strip_prefix("of=") {
            if !rest.is_empty() {
                out.push((PathBuf::from(rest), FileOp::Write));
            }
        }
    }
    out
}

/// `find [PATH...] [PRIMARY...]`.
///
/// Roots are the positionals up to the first primary (any token starting
/// with `-`). Default path is `.` if none given. Then we scan the entire
/// argv for destructive / file-output primaries:
///
///   - `-delete` → mark every root as Write.
///   - `-fprint TARGET`, `-fprint0 TARGET`, `-fprintf TARGET FMT`, `-fls TARGET`
///     → emit (TARGET, Write). The next token is the file argument.
fn extract_find(argv: &[String]) -> Vec<(PathBuf, FileOp)> {
    // 1. Collect roots.
    let mut roots: Vec<PathBuf> = Vec::new();
    for tok in argv {
        if tok.starts_with('-') {
            break;
        }
        roots.push(PathBuf::from(tok));
    }
    if roots.is_empty() {
        roots.push(PathBuf::from("."));
    }

    // 2. Scan for destructive / sink primaries.
    let mut roots_are_writes = false;
    let mut sinks: Vec<PathBuf> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let tok = &argv[i];
        match tok.as_str() {
            "-delete" => {
                roots_are_writes = true;
            }
            "-fprint" | "-fprint0" | "-fls" => {
                if let Some(target) = argv.get(i + 1) {
                    sinks.push(PathBuf::from(target));
                    i += 1;
                }
            }
            "-fprintf" => {
                // `-fprintf FILE FORMAT`: the FILE is the first arg.
                if let Some(target) = argv.get(i + 1) {
                    sinks.push(PathBuf::from(target));
                    i += 2; // skip FILE and FORMAT
                }
            }
            _ => {}
        }
        i += 1;
    }

    let root_op = if roots_are_writes {
        FileOp::Write
    } else {
        FileOp::Read
    };
    let mut out: Vec<(PathBuf, FileOp)> = roots.into_iter().map(|p| (p, root_op)).collect();
    out.extend(sinks.into_iter().map(|p| (p, FileOp::Write)));
    out
}

// ---------------------------------------------------------------------------
// Token classification
// ---------------------------------------------------------------------------

/// True if `tok` looks like a flag: starts with `-`, is at least two chars,
/// and is not the bare `--` end-of-options marker. (`--` is structurally
/// distinct from a flag and callers handle it explicitly.)
fn is_flag(tok: &str) -> bool {
    tok != "--" && tok.len() > 1 && tok.starts_with('-')
}

/// Iterator over positionals only, honoring `--` end-of-options.
fn strip_flags(argv: &[String]) -> impl Iterator<Item = &String> {
    let mut end = false;
    argv.iter().filter(move |tok| {
        if end {
            // Post-`--`, every token is a positional, including `-`.
            return true;
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
    fn bare_dash_is_not_a_path_pre_double_dash() {
        // `cat -` reads stdin; not a filesystem path.
        let got = paths(extract(&argv(&["cat", "-", "/etc/hosts"])));
        assert_eq!(got, vec![(PathBuf::from("/etc/hosts"), FileOp::Read)]);
    }

    #[test]
    fn bare_dash_is_filename_post_double_dash() {
        // POSIX: `cat -- -` treats `-` as a literal filename.
        let got = paths(extract(&argv(&["cat", "--", "-"])));
        assert_eq!(got, vec![(PathBuf::from("-"), FileOp::Read)]);
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
    fn touch_is_write() {
        let got = paths(extract(&argv(&["touch", "/tmp/marker"])));
        assert_eq!(got, vec![(PathBuf::from("/tmp/marker"), FileOp::Write)]);
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
    fn sed_default_is_read() {
        let got = paths(extract(&argv(&["sed", "s/a/b/", "/etc/hosts"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("s/a/b/"), FileOp::Read),
                (PathBuf::from("/etc/hosts"), FileOp::Read),
            ]
        );
    }

    #[test]
    fn sed_in_place_short_flag_is_write() {
        let got = paths(extract(&argv(&["sed", "-i", "s/a/b/", "/etc/hosts"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("s/a/b/"), FileOp::Write),
                (PathBuf::from("/etc/hosts"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn sed_in_place_with_suffix_is_write() {
        // BSD form: `-i.bak` (suffix glued to the flag).
        let got = paths(extract(&argv(&["sed", "-i.bak", "s/a/b/", "/etc/hosts"])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("s/a/b/"), FileOp::Write),
                (PathBuf::from("/etc/hosts"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn sed_in_place_long_flag_is_write() {
        let got = paths(extract(&argv(&[
            "sed",
            "--in-place=.bak",
            "s/a/b/",
            "/etc/hosts",
        ])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("s/a/b/"), FileOp::Write),
                (PathBuf::from("/etc/hosts"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn dd_if_is_read_of_is_write() {
        let got = paths(extract(&argv(&[
            "dd",
            "if=/dev/zero",
            "of=/tmp/zeros",
            "bs=1M",
            "count=10",
        ])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/dev/zero"), FileOp::Read),
                (PathBuf::from("/tmp/zeros"), FileOp::Write),
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
    fn find_delete_promotes_roots_to_write() {
        let got = paths(extract(&argv(&[
            "find", "/etc", "-name", "*.bak", "-delete",
        ])));
        assert_eq!(got, vec![(PathBuf::from("/etc"), FileOp::Write)]);
    }

    #[test]
    fn find_fprint_emits_target_as_write() {
        let got = paths(extract(&argv(&[
            "find",
            "/etc",
            "-name",
            "*.conf",
            "-fprint",
            "/tmp/out.list",
        ])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/etc"), FileOp::Read),
                (PathBuf::from("/tmp/out.list"), FileOp::Write),
            ]
        );
    }

    #[test]
    fn find_fprintf_skips_format_arg() {
        // `-fprintf FILE FORMAT` — FILE is the sink; FORMAT must not be
        // mis-treated as a `-fprint`-style arg.
        let got = paths(extract(&argv(&[
            "find",
            "/etc",
            "-fprintf",
            "/tmp/out.list",
            "%p\\n",
        ])));
        assert_eq!(
            got,
            vec![
                (PathBuf::from("/etc"), FileOp::Read),
                (PathBuf::from("/tmp/out.list"), FileOp::Write),
            ]
        );
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

    #[test]
    fn is_flag_rejects_double_dash_structurally() {
        // Reviewer-mandated: `is_flag("--")` must return false on its own,
        // not merely be saved by an earlier `tok == "--"` short-circuit.
        assert!(!is_flag("--"));
        assert!(is_flag("-rf"));
        assert!(is_flag("--long"));
        assert!(!is_flag("-"));
        assert!(!is_flag("file"));
    }
}
