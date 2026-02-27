# Dimension 12 — Error Handling & Production Safety

**Auditor:** Copilot  
**Date:** 2025-07-18  
**Scope:** All crates — `unwrap()`, `expect()`, `panic!()` in non-test code; error type definitions; mutex poisoning; swallowed errors; error propagation patterns

---

## Summary

OpenFang demonstrates **mature error handling** across most of the codebase. Every major
crate defines a dedicated error type via `thiserror`, the `?` operator is used pervasively
for error propagation, and mutex poisoning is handled gracefully via
`unwrap_or_else(|e| e.into_inner())` in almost all production code paths. The `openfang-memory`
crate is a standout — it converts every `lock()` failure to `OpenFangError::Internal` or
`OpenFangError::Memory` via `map_err`, which is exemplary.

However, there are several areas where `expect()` or `unwrap()` calls in non-test code
paths could cause production panics, and a few crates lack their own error types. The
retry loop contains one `expect()` that is technically reachable, and the graceful shutdown
coordinator has two bare `.unwrap()` calls on mutex locks.

| Severity | Count |
|----------|-------|
| 🔴 High | 2 |
| 🟡 Medium | 5 |
| 🟢 Low / Informational | 4 |

---

## unwrap() / expect() / panic! Counts Per Crate (Non-Test Source Code Only)

| Crate | `unwrap()` | `expect()` | `panic!()` | Notes |
|-------|-----------|-----------|-----------|-------|
| openfang-api | 22 | 2 | 0 | `expect` on signal handlers (Linux only) |
| openfang-channels | ~90 | 1 | 0 | Mostly in channel adapters; HMAC `expect` is safe |
| openfang-cli | 30 | 4 | 0 | HTTP client builder, TUI draw, runtime init |
| openfang-desktop | 0 | 5 | 0 | Tauri setup — all `expect()` |
| openfang-extensions | 16 | 0 | 3 | `panic!` in bundled extension parsing |
| openfang-hands | 18 | 0 | 1 | `panic!` in bundled hand parsing |
| openfang-kernel | ~45 | 0 | 0 | Strong — uses `unwrap_or_else` for poisoning |
| openfang-memory | ~50 | 0 | 0 | **Exemplary** — all `lock()` → `map_err` |
| openfang-migrate | 137 | 0 | 0 | Migration tool — acceptable for offline use |
| openfang-runtime | ~55 | 3 | 0 | retry.rs, link_understanding.rs, graceful_shutdown |
| openfang-skills | 24 | 1 | 0 | marketplace HTTP client `expect` |
| openfang-types | ~40 | 0 | 0 | Mostly in serde/config deserialization |
| openfang-wire | 27 | 1 | 0 | HMAC `expect` is safe |

> Counts exclude `#[cfg(test)]` modules and `tests/` directories where `unwrap()`/`expect()`/`panic!()` are standard practice.

---

## Findings

### EH1 — `expect()` in retry loop is technically reachable 🔴

**File:** `crates\openfang-runtime\src\retry.rs:199`

```rust
// Should not be reachable, but handle gracefully.
RetryOutcome::Exhausted {
    last_error: last_error.expect("at least one attempt should have been made"),
    attempts: max,
}
```

The comment says "handle gracefully" but then calls `expect()`, which panics. While the
code is designed to be unreachable (the `for` loop should always return before this point),
defensive programming demands this path not panic — especially in a retry/fallback loop
that is central to LLM API resilience. If `max` is somehow 0 (e.g., from a
misconfiguration), `last_error` will be `None` and this will panic.

**Impact:** Production panic in the agent loop's retry path — the most critical hot path.

**Recommendation:** Replace with:
```rust
last_error: last_error.unwrap_or_else(|| LlmError::Other("retry loop completed with no attempts".into())),
```

---

### EH2 — `graceful_shutdown` has bare `.unwrap()` on mutex locks 🔴

**File:** `crates\openfang-runtime\src\graceful_shutdown.rs:165-166, 194-195, 215-216`

```rust
let elapsed = self.started_at.lock().unwrap()  // line 166
    .map(|s| s.elapsed().as_millis() as u64)
    .unwrap_or(0);
```

The `ShutdownCoordinator` uses `std::sync::Mutex` for `started_at` and calls `.unwrap()`
directly (without the `unwrap_or_else(|e| e.into_inner())` pattern used everywhere else).
If this mutex is poisoned — which is more likely during shutdown since threads may be
panicking — this call will panic during the very process meant to handle panics gracefully.

The `phase_log` mutex in the same struct correctly uses `unwrap_or_else(|e| e.into_inner())`
on lines 178-179, making the inconsistency more conspicuous.

**Impact:** Panic during graceful shutdown — the worst possible time for an unhandled panic.

**Recommendation:** Change all `started_at.lock().unwrap()` calls to
`started_at.lock().unwrap_or_else(|e| e.into_inner())` to match the pattern used for
`phase_log` in the same file.

---

### EH3 — Three crates lack dedicated error types 🟡

**Crates missing `thiserror` error enums:**
- `openfang-channels` — uses `String` or re-exports from `openfang-types`
- `openfang-api` — uses `String` errors or axum's error types directly
- `openfang-desktop` — uses `expect()` exclusively; no Result-based error handling

Most crates define proper error types (`KernelError`, `LlmError`, `OpenFangError`,
`WireError`, `HandError`, `ExtensionError`, `SkillError`, `MigrateError`, `EmbeddingError`,
`SandboxError`, `PythonError`). The three missing crates rely on `String` or framework
error types, which reduces error composability and makes match-based error handling
impossible upstream.

**Recommendation:** Add a `ChannelError` enum to `openfang-channels` and an `ApiError`
enum to `openfang-api`. `openfang-desktop` is acceptable as a thin UI shell.

---

### EH4 — `expect()` on HTTP client construction in CLI and skills 🟡

**Files:**
- `crates\openfang-cli\src\mcp.rs:143` — `.expect("Failed to build HTTP client")`
- `crates\openfang-cli\src\main.rs:974` — `.expect("Failed to build HTTP client")`
- `crates\openfang-skills\src\marketplace.rs:42` — `.expect("Failed to build HTTP client")`

`reqwest::Client::builder().build()` can fail if TLS backend initialization fails (e.g.,
missing system certificates, rustls issues). Using `expect()` here causes a panic rather
than a graceful error message.

**Recommendation:** Return `Result` from these functions or use a fallback:
```rust
.build().unwrap_or_else(|_| reqwest::Client::new())
```
This pattern is already used in `crates\openfang-cli\src\tui\event.rs:523`.

---

### EH5 — `expect()` on Tauri/Desktop initialization 🟡

**Files:**
- `crates\openfang-desktop\src\lib.rs:44` — `.expect("Failed to start OpenFang server")`
- `crates\openfang-desktop\src\lib.rs:115` — `.expect("Invalid server URL")`
- `crates\openfang-desktop\src\lib.rs:201` — `.expect("Failed to build Tauri application")`
- `crates\openfang-desktop\src\server.rs:71,102,104` — `.expect()` on runtime/listener

All initialization in the desktop crate uses `expect()`. While these are fail-fast paths
where recovery is impossible, the error messages should be more actionable (e.g., include
the port number, the URL that failed to parse, or the system error).

**Recommendation:** Enhance messages: `expect(&format!("Failed to bind to port {port}"))`
or switch to `anyhow::Context` for richer error reporting.

---

### EH6 — `panic!` in bundled extension/hand registration 🟡

**Files:**
- `crates\openfang-extensions\src\bundled.rs:81,139,147`
- `crates\openfang-hands\src\bundled.rs:208`

```rust
.unwrap_or_else(|e| panic!("Failed to parse '{}': {}", id, e));
```

Bundled extensions and hands use `panic!` if their hardcoded TOML fails to parse. While
this should never happen with correct code, it makes the daemon crash on startup with a
panic backtrace instead of a structured error.

**Recommendation:** Convert to `tracing::error!` + skip the faulty extension, or propagate
the error via `Result` from the registration function.

---

### EH7 — Swallowed errors in TUI daemon client calls 🟡

**File:** `crates\openfang-cli\src\tui\event.rs` (multiple locations: 242, 525, 538, 581, etc.)

The TUI event module extensively uses `if let Ok(resp) = client.get(...).send()` pattern,
silently dropping network errors. While this is acceptable for a TUI that polls periodically,
no error is logged, making it impossible to diagnose connectivity issues.

**Recommendation:** Add `tracing::debug!` or `tracing::trace!` for the `Err` case:
```rust
match client.get(...).send() {
    Ok(resp) => { ... }
    Err(e) => tracing::debug!("Failed to poll daemon: {e}"),
}
```

---

### EH8 — `expect("URL regex is valid")` in link_understanding — safe but unconventional 🟢

**File:** `crates\openfang-runtime\src\link_understanding.rs:26`

```rust
.expect("URL regex is valid");
```

This compiles a regex at runtime and panics if invalid. The regex is a compile-time
constant so this is effectively safe, but using `lazy_static!` or `LazyLock` would eliminate
the (theoretical) panic path entirely and improve performance by compiling only once.

**Recommendation:** Move to `std::sync::LazyLock<Regex>` or `once_cell::sync::Lazy`.

---

### EH9 — Excellent mutex poisoning discipline across the codebase 🟢

**Files:** `crates\openfang-runtime\src\audit.rs`, `a2a.rs`, `drivers\copilot.rs`,
`crates\openfang-channels\src\router.rs`, `crates\openfang-kernel\src\approval.rs`,
`crates\openfang-wire\src\registry.rs`

The codebase uses `unwrap_or_else(|e| e.into_inner())` consistently for `std::sync::Mutex`
and `std::sync::RwLock` locks in production code. This is the correct pattern for
recovering from mutex poisoning — it acknowledges the prior panic but continues operating
with the inner data. Only two files deviate from this pattern (see EH2).

---

### EH10 — Memory crate demonstrates exemplary error propagation 🟢

**Files:** `crates\openfang-memory\src\usage.rs`, `session.rs`, `substrate.rs`,
`semantic.rs`, `knowledge.rs`, `structured.rs`

Every `lock()` call in the memory crate is wrapped with:
```rust
.lock().map_err(|e| OpenFangError::Internal(e.to_string()))?;
```

This is the gold standard for error handling with mutexes — it converts poisoning to
a typed error and propagates via `?`. All SQLite operations are similarly wrapped. This
pattern should be adopted across other crates.

---

### EH11 — `A2aClient::new()` uses `unwrap_or_default()` — good fallback 🟢

**File:** `crates\openfang-runtime\src\a2a.rs:356`

```rust
.build().unwrap_or_default(),
```

Unlike the `expect()` calls in EH4, the A2A client falls back to a default `reqwest::Client`
if the builder fails. This is the correct pattern for non-critical HTTP clients.

---

## Strengths

1. **Consistent `thiserror` adoption:** 10 of 13 crates define dedicated error enums with
   `#[derive(thiserror::Error)]`, enabling structured error matching throughout the stack.

2. **`?` operator everywhere:** The codebase consistently uses `?` for error propagation
   rather than `match` + manual returns. This is idiomatic Rust at its best.

3. **Mutex poisoning handled gracefully:** The `unwrap_or_else(|e| e.into_inner())` pattern
   is used in ~95% of `std::sync::Mutex` lock sites, preventing cascading panics.

4. **Memory crate sets the standard:** `openfang-memory` converts every fallible operation
   (mutex lock, SQLite query, serialization) into typed `OpenFangError` variants and
   propagates with `?`. This should be the template for other crates.

5. **Retry module returns `RetryOutcome` enum:** Rather than panicking on exhaustion, the
   retry system returns a typed `Exhausted` variant with the last error, allowing callers
   to decide how to handle failure (with the one exception in EH1).

6. **`panic!()` almost entirely confined to test code:** All 50+ `panic!()` calls found
   are in `#[test]` functions or test helper modules. Production code avoids `panic!()`.

7. **LLM drivers use `reqwest::Client::new()` not `expect()`:** The Anthropic, OpenAI, and
   Gemini drivers construct their HTTP clients with `Client::new()` which cannot fail,
   avoiding the `expect()` anti-pattern.
