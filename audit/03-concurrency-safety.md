# Dimension 3 — Concurrency & Safety

**Auditor:** Copilot  
**Date:** 2025-07-15  
**Scope:** All crates using `Mutex`, `RwLock`, `DashMap`, `Arc`, `AtomicBool`, `Semaphore`, `block_on`, `spawn_blocking`

---

## Summary

OpenFang demonstrates **strong concurrency hygiene overall**. The codebase consistently
uses `tokio::sync` primitives for async locks, `DashMap` for concurrent maps,
`tokio::task::spawn_blocking` for SQLite I/O, and `Semaphore` for concurrency bounds.
Mutex-poisoning is handled gracefully in most production paths via
`unwrap_or_else(|e| e.into_inner())`. However, several findings warrant attention — most
notably `block_on()` inside WASM host functions (which risks deadlock if the tokio runtime
is single-threaded), `std::sync::Mutex` locks acquired in async route handlers (blocking
the executor), and inconsistent poisoning strategies.

| Severity | Count |
|----------|-------|
| 🔴 High | 2 |
| 🟡 Medium | 5 |
| 🟢 Low / Informational | 4 |

---

## Findings

### C1 — `block_on()` inside WASM host functions (sync context bootstrapped from async) 🔴

**Files:** `crates\openfang-runtime\src\host_functions.rs:293, 464, 484`

The WASM sandbox runs on `spawn_blocking` threads (good), but host functions call
`state.tokio_handle.block_on(async { ... })` to re-enter the tokio runtime. This is
architecturally correct when the blocking thread is *outside* the async executor, but it
creates a **deadlock risk** if:

- The tokio runtime is `current_thread` (single-threaded) — `block_on` from a
  `spawn_blocking` thread will park waiting for a worker that is itself blocked.
- The `Handle` is exhausted (all workers busy with `spawn_blocking` tasks calling
  `block_on`).

Additionally, `host_shell_exec` (line 354) uses `std::process::Command::output()` which
is blocking I/O. While this runs inside `spawn_blocking`, if agent concurrency is high
this can saturate the blocking thread pool.

**Recommendation:** Validate that the runtime is always `multi_thread`. Consider adding
a guard: `assert!(handle.runtime_flavor() == RuntimeFlavor::MultiThread)` at sandbox
init. For `host_shell_exec`, consider capping concurrent shell execs with a semaphore.

---

### C2 — `std::sync::Mutex` used in async handler paths (executor blocking) 🔴

**Files:**
- `crates\openfang-kernel\src\kernel.rs:80` — `mcp_tools: std::sync::Mutex<Vec<ToolDefinition>>`
- `crates\openfang-kernel\src\kernel.rs:84` — `a2a_external_agents: std::sync::Mutex<...>`
- `crates\openfang-kernel\src\kernel.rs:113` — `bindings: std::sync::Mutex<...>`
- `crates\openfang-kernel\src\kernel.rs:72` — `model_catalog: std::sync::RwLock<...>`
- `crates\openfang-kernel\src\kernel.rs:74` — `skill_registry: std::sync::RwLock<...>`
- `crates\openfang-kernel\src\kernel.rs:101` — `extension_registry: std::sync::RwLock<...>`

These `std::sync` locks are acquired directly in async route handlers
(`crates\openfang-api\src\routes.rs:4013, 5237, 5628, 8639` etc.) and in async kernel
methods. While the critical sections are short (read-clone-drop), they block the tokio
worker thread for the duration of the lock acquisition. Under contention (e.g., MCP tool
refresh happening concurrently with API requests), this can stall other tasks on that
worker.

**Recommendation:** Migrate these to `tokio::sync::RwLock` or use `spawn_blocking` for
the lock-acquire-clone pattern. Alternatively, replace with `DashMap` or `arc_swap::ArcSwap`
for read-heavy / write-rare patterns.

---

### C3 — Inconsistent mutex-poisoning handling 🟡

**Files (`.lock().unwrap()` — will panic on poison):**
- `crates\openfang-memory\src\consolidation.rs:77, 91` (test code)
- `crates\openfang-runtime\src\audit.rs:251` (test code)
- `crates\openfang-runtime\src\hooks.rs:127, 132` (test code)
- `crates\openfang-runtime\src\graceful_shutdown.rs:166, 195, 216` (**production**)
- `crates\openfang-channels\src\bridge.rs:943, 947` (test mock)
- `crates\openfang-channels\tests\bridge_integration_test.rs` (test code)

**Files (`.unwrap_or_else(|e| e.into_inner())` — graceful recovery):**
- `crates\openfang-wire\src\registry.rs` (all lock sites)
- `crates\openfang-runtime\src\a2a.rs` (all lock sites)
- `crates\openfang-runtime\src\audit.rs:112, 113, 142, 179, 184, 197` (production)
- `crates\openfang-kernel\src\kernel.rs:2704, 2712`
- `crates\openfang-runtime\src\drivers\copilot.rs:55, 61`
- `crates\openfang-runtime\src\graceful_shutdown.rs:138, 179, 207`
- `crates\openfang-channels\src\router.rs` (all lock sites)

The `ShutdownCoordinator` is split: `advance_phase` and `status` use graceful recovery
but `started_at.lock().unwrap()` (line 166) and `is_timeout_exceeded` (line 216) will
**panic** on poison. During shutdown — where a panic in another thread is most likely —
this could cascade.

**Recommendation:** Standardize on `unwrap_or_else(|e| e.into_inner())` for all
production `std::sync::Mutex` locks. The existing pattern in `audit.rs` and `a2a.rs` is
the correct model.

---

### C4 — Memory substrate: `std::sync::Mutex<Connection>` blocking in sync callers 🟡

**Files:**
- `crates\openfang-memory\src\substrate.rs:29` — `conn: Arc<Mutex<Connection>>`
- All memory stores: `semantic.rs`, `structured.rs`, `session.rs`, `knowledge.rs`,
  `usage.rs`, `consolidation.rs`

The `MemorySubstrate` wraps a `std::sync::Mutex<rusqlite::Connection>`. The async `Memory`
trait impl correctly uses `spawn_blocking` (substrate.rs:362–643). However, many **sync**
methods on the substrate (e.g., `save_agent`, `load_agent`, `remove_agent`, `get_session`)
are called directly from async kernel methods like `kill_agent()`,
`send_to_agent_internal()`, and `spawn_agent()` **without** `spawn_blocking`.

This means the `std::sync::Mutex` lock and the SQLite I/O block a tokio worker thread.
SQLite queries are typically fast (<1ms with WAL mode and `busy_timeout=5000`), but under
contention or with the 5-second busy timeout, this could stall the async runtime.

**Recommendation:** Wrap all synchronous memory calls in async kernel methods with
`spawn_blocking`, or provide async wrappers (as already done for `recall_with_embedding_async`,
`remember_with_embedding_async`, `task_post`, etc.) and migrate callers.

---

### C5 — Config hot-reload poll loop uses `std::fs::metadata` without `spawn_blocking` 🟡

**File:** `crates\openfang-api\src\server.rs:648-669`

The config watcher spawns an async task that calls `std::fs::metadata()` (blocking
filesystem I/O) every 30 seconds without `spawn_blocking`. While this is fast on most
filesystems, it technically blocks the tokio worker. The subsequent `reload_config()` call
also acquires multiple `std::sync::Mutex` locks (bindings, registries).

**Recommendation:** Wrap the metadata check and reload in `spawn_blocking`, or use
`tokio::fs::metadata`.

---

### C6 — AtomicBool busy flag: TOCTOU window in background loops 🟡

**File:** `crates\openfang-kernel\src\background.rs:85-114`

The continuous/periodic background loops use `AtomicBool` with `compare_exchange` to
implement a "skip if busy" pattern:

```rust
if busy.compare_exchange(false, true, SeqCst, SeqCst).is_err() {
    continue; // skip tick
}
// ... spawn work, then clear busy in a watcher task
```

This is **correct** for the single-producer use case (one timer loop per agent). The
`compare_exchange` is atomic and prevents double-dispatch. The `SeqCst` ordering is
stronger than necessary (`AcqRel` would suffice) but not harmful.

However, there is a minor TOCTOU concern: if the watcher task that clears `busy` panics
(line 112-115), the flag is never reset, and the background agent is permanently stalled.
The semaphore permit would also leak.

**Recommendation:** Use a `Drop` guard or `scopeguard` to ensure `busy` is cleared and
the permit is dropped even on panic. Consider `Relaxed`→`Acquire` / `Release` ordering
to reduce overhead.

---

### C7 — Audit log SSE: polling loop instead of event-driven 🟡

**File:** `crates\openfang-api\src\routes.rs:3858-3859`

The audit log SSE endpoint polls every 1 second:

```rust
loop {
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let entries = state.kernel.audit_log.recent(200);
    // ... filter and send
}
```

This is a busy-wait with a 1-second sleep — wasting work when idle and adding up to 1
second of latency when entries arrive. A `tokio::sync::broadcast` or `watch` channel
from the audit log would be more efficient and responsive.

**Recommendation:** Add a `tokio::sync::Notify` or `broadcast` channel to `AuditLog`
that wakes SSE consumers when new entries are appended.

---

### C8 — `DashMap` used correctly throughout 🟢

**Files:**
- `crates\openfang-kernel\src\kernel.rs:76` — `running_tasks: DashMap<AgentId, AbortHandle>`
- `crates\openfang-kernel\src\capabilities.rs` — `grants: DashMap`
- `crates\openfang-runtime\src\web_cache.rs` — `entries: DashMap`
- `crates\openfang-runtime\src\browser.rs:103` — `sessions: DashMap`
- `crates\openfang-runtime\src\auth_cooldown.rs:169` — `states: DashMap`
- `crates\openfang-runtime\src\hooks.rs:39` — `handlers: DashMap`
- `crates\openfang-runtime\src\process_manager.rs:51` — `processes: DashMap`
- `crates\openfang-runtime\src\docker_sandbox.rs:276` — `entries: DashMap`
- `crates\openfang-extensions\src\health.rs:108` — `health: DashMap`
- `crates\openfang-channels\src\bridge.rs:207` — `buckets: DashMap`
- `crates\openfang-hands\src\registry.rs:43` — `instances: DashMap`

All `DashMap` usage follows best practices: no references held across `.await` points,
entries are accessed via short-lived guards or cloned out. No deadlock risk.

---

### C9 — Semaphore-based concurrency bounds well-designed 🟢

**Files:**
- `crates\openfang-runtime\src\command_lane.rs` — 3-lane semaphore system (Main=1, Cron=2, Subagent=3)
- `crates\openfang-kernel\src\background.rs:27` — `llm_semaphore` for background LLM calls
- `crates\openfang-kernel\src\auto_reply.rs:23` — `semaphore` for auto-reply concurrency
- `crates\openfang-runtime\src\media_understanding.rs:15` — `semaphore` for media processing

The `CommandQueue` correctly uses `acquire()` with proper `_permit` scoping. The
background executor uses `acquire_owned()` to transfer permits across task boundaries.
Semaphore closure is handled gracefully (break on `Err`). This is exemplary.

---

### C10 — `tokio::sync::Mutex` used correctly for async-held locks 🟢

**Files:**
- `crates\openfang-kernel\src\kernel.rs:78` — `mcp_connections: tokio::sync::Mutex<Vec<McpConnection>>`
- `crates\openfang-api\src\routes.rs:34-35` — `bridge_manager: tokio::sync::Mutex`, `channels_config: tokio::sync::RwLock`
- `crates\openfang-runtime\src\browser.rs:103` — `sessions: DashMap<String, tokio::sync::Mutex<BrowserSession>>`
- `crates\openfang-runtime\src\process_manager.rs:21-23` — `stdout_buf/stderr_buf: tokio::sync::Mutex`

These are correctly held across `.await` points (e.g., `sender.lock().await` in ws.rs,
`bridge_manager.lock().await` in routes). No mixing of `std::sync::Mutex` guards across
await boundaries was found.

---

### C11 — `block_on()` in desktop server is correct 🟢

**File:** `crates\openfang-desktop\src\server.rs:73`

The desktop server spawns a dedicated `std::thread` with its own `tokio::runtime::Builder`,
then calls `rt.block_on()` inside that thread. This is the **correct** pattern for
bridging sync thread → async runtime. No risk of nested `block_on`.

---

## Strengths

1. **Consistent `spawn_blocking` for SQLite** — The `MemorySubstrate` async trait impl
   wraps every synchronous SQLite call in `tokio::task::spawn_blocking`, preventing
   executor stalls for the async API path.

2. **Semaphore architecture** — The 3-lane `CommandQueue` with independent concurrency
   limits prevents starvation between user messages, cron jobs, and subagents. The
   background executor adds a global LLM semaphore to prevent runaway autonomous agents.

3. **Graceful mutex-poisoning recovery** — Most production code uses
   `unwrap_or_else(|e| e.into_inner())`, which is the correct Rust idiom for recovering
   from panicked threads without cascading failure.

4. **`DashMap` over `Mutex<HashMap>`** — High-traffic maps (running tasks, capabilities,
   web cache, auth cooldown) use `DashMap` for fine-grained sharded locking.

5. **No locks held across `.await`** — No instances of `std::sync::MutexGuard` held
   across `.await` points were found. All async-spanning guards use `tokio::sync` types.

6. **Proper shutdown coordination** — `watch::channel` for shutdown signals, `Notify` for
   API server shutdown, `AbortHandle` for task cancellation — all correctly wired.

7. **Channel backpressure** — WebSocket, SSE, and channel bridge systems use bounded
   `mpsc` channels with explicit capacity (e.g., 256 for SSE), preventing unbounded
   memory growth.
