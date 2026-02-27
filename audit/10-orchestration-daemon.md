# Dimension 10 — Orchestration & Daemon Patterns

**Auditor:** Copilot  
**Date:** 2025-07-15  
**Scope:** Scheduler, triggers, supervisor, daemon lifecycle, signal handling, graceful shutdown, concurrency policies, testability

---

## Summary

OpenFang has a **well-structured orchestration layer** with clear separation between scheduling concerns and execution. The codebase features four distinct orchestration subsystems: an `AgentScheduler` (resource quota tracking), a `CronScheduler` (time-based job scheduling), a `TriggerEngine` (event-driven activation), and a `BackgroundExecutor` (autonomous agent loops). A `Supervisor` provides process-level shutdown coordination via `tokio::sync::watch`. Signal handling is cross-platform (SIGINT/SIGTERM on Unix, Ctrl+C on Windows). Concurrency is bounded by a global semaphore (`MAX_CONCURRENT_BG_LLM = 5`) and per-agent busy flags prevent overlap. Tests cover scheduling logic, trigger evaluation, and background executor behavior including skip-if-busy semantics.

The main gaps are: (1) the kernel `shutdown()` does not drain in-flight invocations — it suspends agents immediately without waiting for active tasks to complete, (2) the `CronScheduler.due_jobs()` calls `Utc::now()` internally, making it impossible to test time-dependent scheduling without real clock progression, (3) the `UsageTracker` in `scheduler.rs` uses `Instant::now()` directly, also preventing deterministic testing, and (4) the shutdown sequence does not emit lifecycle events to the event bus.

**Overall Grade: B+** — Strong architectural separation and concurrency safety, with gaps in graceful draining, testability of time-dependent code, and shutdown telemetry.

---

## Findings

### O1 — Kernel Shutdown Does Not Drain In-Flight Invocations
**Severity:** High  
**Location:** `crates/openfang-kernel/src/kernel.rs:3415-3455`

The `shutdown()` method immediately sets all agents to `Suspended` state and signals the supervisor, but does **not** wait for in-flight `send_message` or cron job executions to complete. The `running_tasks` DashMap tracks `AbortHandle` instances, but these are never awaited — they are simply abandoned when shutdown occurs.

The `BackgroundExecutor` loops do check `shutdown.changed()` via `tokio::select!`, which cleanly exits the scheduling loops. However, any LLM call that is mid-flight when shutdown fires will be silently dropped, potentially losing partial work.

The server-side shutdown in `server.rs` calls `kernel.shutdown()` *after* the HTTP server's graceful shutdown completes (line 737), so HTTP-originated tasks may have already been aborted by the server's connection draining, but background-originated tasks (cron, continuous loops) have no drain period.

**Recommendation:** Add a drain phase to `shutdown()`: after signaling the supervisor, iterate over `running_tasks` and await each `AbortHandle` with a timeout (e.g., 10 seconds). This gives in-flight LLM calls a chance to finish before the process exits. Consider persisting incomplete cron job state for retry on next boot.

---

### O2 — CronScheduler Uses `Utc::now()` Internally, Preventing Deterministic Testing
**Severity:** Medium  
**Location:** `crates/openfang-kernel/src/cron.rs:225-235`, `crates/openfang-kernel/src/cron.rs:300-329`

`due_jobs()` calls `Utc::now()` internally to determine which jobs are due. The `compute_next_run()` function also calls `Utc::now()` for `Every` schedules. This makes it impossible to write time-sensitive tests without real clock progression or manipulating internal state via `jobs.get_mut()` (which the tests already do as a workaround — see line 549-551).

The existing tests work around this by directly mutating `next_run` fields, but this couples tests to internal representation and prevents testing the full scheduling lifecycle.

**Recommendation:** Inject a clock abstraction (e.g., `trait Clock { fn now(&self) -> DateTime<Utc>; }`) into `CronScheduler` and `compute_next_run()`. Provide a `RealClock` for production and a `FakeClock` for tests. This follows the testable scheduler pattern and is a minimal change to the API surface.

---

### O3 — UsageTracker Window Reset Uses `Instant::now()` Directly
**Severity:** Low  
**Location:** `crates/openfang-kernel/src/scheduler.rs:18-19, 34-39`

`UsageTracker` stores `window_start: Instant` and checks elapsed time via `Instant::now()`. This means the hourly window reset cannot be tested without waiting a real hour (or wrapping `Instant`). The existing tests only verify basic counting, not window rollover behavior.

This is low severity because the quota logic is straightforward (compare, reset), but the untested path could hide edge cases (e.g., race conditions when multiple concurrent `check_quota` calls trigger reset simultaneously).

**Recommendation:** Extract time queries behind a trait or accept an `Instant` parameter. For simpler fix, add a `reset_window()` test helper behind `#[cfg(test)]`.

---

### O4 — Shutdown Does Not Emit Lifecycle Events to EventBus
**Severity:** Medium  
**Location:** `crates/openfang-kernel/src/kernel.rs:3415-3455`

The `shutdown()` method suspends agents and logs info messages, but does not publish `LifecycleEvent::Terminated` or `SystemEvent::KernelStopping` events to the event bus. This means:

1. Proactive agents subscribed to lifecycle events will not be notified of peers terminating during shutdown.
2. External monitoring systems polling the event history will miss the shutdown sequence.
3. The `SystemEvent::KernelStopping` variant exists in the types crate but is never published.

The event bus infrastructure is fully capable of emitting these events — the `EventPayload::Lifecycle(LifecycleEvent::Terminated { ... })` and `EventPayload::System(SystemEvent::KernelStopping)` variants are defined and used elsewhere for pattern matching.

**Recommendation:** At the start of `shutdown()`, publish a `SystemEvent::KernelStopping` event. For each agent being suspended, publish a `LifecycleEvent::Suspended` event. This enables observability and allows agents to persist state on shutdown.

---

### O5 — No Maximum Concurrency Limit on Cron Job Execution
**Severity:** Medium  
**Location:** `crates/openfang-kernel/src/kernel.rs:3090-3180`

The cron tick loop fires all due jobs sequentially within a single tick, but each `AgentTurn` job calls `kernel.send_message()` which spawns LLM work. If many jobs become due simultaneously (e.g., after a long outage), all will fire in the same tick with no concurrency cap. Unlike the `BackgroundExecutor` which has a global `Semaphore(5)`, the cron executor has no concurrency bound.

The existing `MAX_CONCURRENT_BG_LLM` semaphore only covers continuous/periodic background loops, not cron-triggered agent turns. A burst of 50 due cron jobs could overwhelm the LLM provider with simultaneous requests.

**Recommendation:** Apply the same `llm_semaphore` (or a separate cron-specific semaphore) to cron `AgentTurn` executions. Alternatively, limit the number of due jobs processed per tick and defer the rest to the next cycle.

---

### O6 — Trigger Engine Has No Rate Limiting or Debouncing
**Severity:** Low  
**Location:** `crates/openfang-kernel/src/triggers.rs:175-209`

The `TriggerEngine::evaluate()` method fires all matching triggers for every event with no debounce or rate limit. A high-frequency event source (e.g., rapid memory updates) could cause a cascade of agent invocations. The `max_fires` field provides a total cap, but no time-based throttling (e.g., "at most once per 5 minutes").

In practice, the `BackgroundExecutor`'s skip-if-busy guard and the global LLM semaphore provide downstream protection. But triggers with `TriggerPattern::All` or `TriggerPattern::MemoryUpdate` on a chatty bus could generate excessive wakeup attempts.

**Recommendation:** Add an optional `min_interval_secs` field to `Trigger` that prevents re-firing within the cooldown period. Track `last_fired_at: Option<DateTime<Utc>>` and check it in `evaluate()`.

---

### O7 — Background Executor `stop_agent()` Aborts Without Drain
**Severity:** Low  
**Location:** `crates/openfang-kernel/src/background.rs:189-194`

`stop_agent()` calls `handle.abort()` which immediately cancels the tokio task. If the agent's background loop is mid-LLM-call (holding the semaphore permit), the abort will drop the permit (which is correct for the semaphore), but the in-flight `send_message` spawned via the inner `tokio::spawn` will be orphaned — it continues running as a detached task.

The inner task pattern (line 111-115) spawns a watcher task that drops the permit after the message completes. Aborting the outer loop task does not abort this inner watcher or the actual LLM call, so the work finishes but the busy flag is never cleared (though it no longer matters since the loop has exited).

**Recommendation:** Track the inner JoinHandle and abort it along with the outer loop. Alternatively, pass a `CancellationToken` to the send_message closure for cooperative cancellation.

---

## Strengths

### S1 — Clean Separation of Scheduling Concerns
The codebase maintains four distinct scheduling subsystems with clear responsibilities: `AgentScheduler` (quotas), `CronScheduler` (time-based), `TriggerEngine` (event-driven), `BackgroundExecutor` (autonomous loops). The `kernel.rs` composition layer wires them together without blurring boundaries.

### S2 — Cross-Platform Signal Handling
The `shutdown_signal()` function in `server.rs` uses `#[cfg(unix)]` / `#[cfg(not(unix))]` to handle SIGINT+SIGTERM on Unix and Ctrl+C on Windows. An API-triggered shutdown path (`/api/shutdown`) provides a platform-independent alternative. The CLI's `install_ctrlc_handler()` handles Windows console events via `SetConsoleCtrlHandler`.

### S3 — Concurrency Safety via Semaphore + Busy Flag
Background loops use both a global `Semaphore(5)` for LLM call concurrency and per-agent `AtomicBool` busy flags to prevent overlapping ticks. The `compare_exchange` skip-if-busy pattern (background.rs:85-91) is correct and well-tested (see `test_skip_if_busy`).

### S4 — Supervisor Watch Channel Pattern
The `Supervisor` uses `tokio::sync::watch` for shutdown signaling — a clean pub/sub pattern that allows any number of subscribers to receive the shutdown signal. All background loops select on both their timer and the shutdown channel, ensuring prompt exit.

### S5 — CronScheduler Auto-Disable on Repeated Failures
Jobs that fail `MAX_CONSECUTIVE_ERRORS` (5) times are automatically disabled with a warning log. This prevents a broken job from consuming resources indefinitely. Re-enabling a job resets the error counter and recomputes `next_run`.

### S6 — Comprehensive Test Coverage for Orchestration Primitives
The trigger engine, cron scheduler, supervisor, and background executor all have thorough unit tests including edge cases (max fires, quota exceeded, busy skip, one-shot removal, persist/load round-trip, auto-disable). The `BackgroundExecutor` tests use real tokio runtime with short intervals for integration-level coverage.

### S7 — Config Hot-Reload Infrastructure
The `config_reload.rs` module provides a structured `ReloadPlan` that categorizes config changes into restart-required, hot-reloadable, and no-op buckets. The daemon polls for config file changes every 30 seconds and applies hot actions without restart.
