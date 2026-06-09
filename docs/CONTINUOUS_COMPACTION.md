# Continuous Compaction with Contextual Hand Summaries

OpenFang's default compaction is reactive: when a session crosses a size or token
threshold, the oldest turns are summarised into a single block and the recent
tail is kept verbatim. **Continuous compaction** adds a second, proactive flavour
on top of that — one that fires on a cadence (every N exchanges) or after a long
wall-clock gap, and that can optionally pull in fresh context from other agents
("hands") at the same time.

This document describes what the feature does, how to enable it, the LLM/tool
budget operators should expect, and how it interacts with the structured-memory
stack (PRs #1224–#1227).

---

## What it does

Continuous compaction has two triggers and four moving parts.

### Triggers

1. **Cadence trigger** — every `continuous_interval` user exchanges (default
   disabled), after the session grows past `keep_recent` messages, a compaction
   pass fires. Both streaming and non-streaming send paths use the same
   trigger.

2. **Gap trigger** — when a new inbound channel message arrives more than
   `gap_secs` seconds after the previous one for the same `(agent, user)` pair,
   a compaction pass fires *before* the new message is dispatched, so the
   refreshed context is part of the session the LLM reads on the very next
   turn.

### Moving parts

1. **Compaction itself** — the standard `compact_agent_session` pass:
   summarise older turns, keep the recent tail verbatim. Nothing new here.

2. **Context-source queries** — for each entry in
   `[[compaction.context_sources]]`, OpenFang spawns a parallel task that
   sends a time-bounded prompt to the named hand (e.g. `calendar-hand`,
   `mail-hand`). Bounded by `(from_ts, to_ts)`:
   - `from_ts` defaults to `now - gap_max_lookback_secs` on the first
     compaction; subsequent compactions use the previous compaction's
     timestamp, so the second pass asks the calendar-hand only about the
     window since the previous pass — not the entire conversation history.
   - `to_ts` is `now`.

3. **Token cap** — the combined hand summaries are truncated to
   `context_token_cap` tokens before injection (default 2000). A clear
   `…[truncated]` marker is appended so operators can see the cap engaged.

4. **Context injection** — the truncated payload is appended to the live
   session as `Message::context_injection(…)`, which carries
   `MessageSource::ContextInjection`. The LLM sees it on the next turn; the
   structured-memory dreamer skips it (so calendar/mail summaries do not
   bleed into long-term user memory).

---

## Configuration

```toml
[compaction]
continuous_interval = 5           # 0 disables continuous compaction (default)
keep_recent = 6                   # messages kept verbatim after compaction
gap_secs = 900                    # 0 disables gap-triggered refresh (default)
gap_max_lookback_secs = 86400     # cap on the gap query window (24h)
context_token_cap = 2000          # max combined hand-summary tokens injected

[[compaction.context_sources]]
hand = "calendar-hand"
prompt = "Summarize events from the last few hours and any upcoming in the next 24 hours."

[[compaction.context_sources]]
hand = "mail-hand"
prompt = "Summarize unread or notable emails."
```

### Opt-in by default

Both triggers default to off: `continuous_interval = 0` (cadence) and
`gap_secs = 0` (gap). With no `[compaction]` block in config, continuous
compaction is fully disabled — the channel bridge skips the pre-dispatch
gap probe entirely (no lock acquisition, no session read), and the
standard message-count and token-budget compaction paths still run
unchanged. The feature kicks in only when at least one of
`continuous_interval`, `gap_secs`, or `[[compaction.context_sources]]` is
set, and the cadence trigger additionally requires at least one context
source to do useful work.

### Naming: `[compaction] gap_secs` vs `[sessions] gap_secs`

OpenFang has two `gap_secs` knobs — they look similar and they're easy to
confuse, but they drive independent subsystems:

| Knob | Default | Drives |
|------|---------|--------|
| `[sessions] gap_secs` | 300s (5 min) | Dream lifecycle loop — when an idle session is consolidated into structured memory. |
| `[compaction] gap_secs` | 0 (disabled) | Pre-dispatch compaction + context refresh on the next inbound message. Set explicitly (e.g. `900` = 15 min) to enable. |

They measure the same wall-clock dimension (inactivity) but trigger different
work. Keep them separate.

---

## Guardrails — what the kernel does *not* do

### Activity gate

If a session contains only `[AUTONOMOUS TICK]` / `[SCHEDULED TICK]` heartbeats
and previously-injected `ContextInjection` messages, the compaction trigger is
short-circuited via [`has_real_user_activity`]. Without this gate, a quiet
heartbeat-only week would still hammer the calendar-hand every hour. The same
predicate gates the dream lifecycle loop in PR #1226 — wired in here for
symmetry.

### Per-(agent, user) lock with `try_lock`

When a compaction is already in flight for the same `(agent, user)` pair,
concurrent callers `try_lock` and skip rather than queue. This mirrors the
`agent_dream_locks` pattern from PR #1226: never queue work that the next
trigger will re-evaluate anyway.

### Failure tolerance for context sources

Each context source runs in its own `tokio::spawn` with a 30-second timeout.
Failures, timeouts, and empty responses are logged and skipped — one broken
hand never tanks the whole refresh.

### Token cap

The combined summary payload is truncated to `context_token_cap` tokens
(default 2000). The truncated suffix is replaced with `…[truncated]` so the
cap is visible in both the injected prompt and the operator-facing logs.

---

## Budget — what to expect

For every compaction trigger, in the worst case:

- **1 summarisation LLM call** — the existing compaction pass.
- **N hand calls** — one per `[[compaction.context_sources]]` entry. Each
  hand call is itself a full agent loop (LLM + any tools the hand uses), so
  costs depend on the hand's design.

A calendar-hand backed by a single Calendar API call is cheap (~1 LLM
roundtrip + 1 API call). A mail-hand that fetches and ranks the last 50
messages is more expensive. Plan accordingly.

Token cost of the injected payload itself is capped at `context_token_cap` —
that's the *amount the LLM sees on every subsequent turn until the next
compaction*, so keep this tight (2 000 tokens is the default; higher numbers
buy more context but cost on every turn).

---

## Latency considerations

The gap trigger runs **synchronously before** the user's channel dispatch
— the injected context must be in the session when the LLM reads it for
the first response after the gap. Worst-case added latency on the user's
first message after a long gap:

- Compaction summarisation: 1–3 seconds (1 LLM call).
- Context source queries: up to 30 seconds (per-source timeout, run in
  parallel — total ≈ slowest source, not sum).
- Token-cap truncation + session injection: negligible.

The cadence trigger fires asynchronously via `tokio::spawn` and never
blocks user dispatch. Only the gap trigger sits in front of `send_message`,
because the whole point of the gap refresh is to have the injected context
visible to the LLM on the very next turn.

**Operators should configure conservatively:**

- Keep `[[compaction.context_sources]]` short (1–3 hands max).
- Pick fast hands (`calendar-hand`, `mail-hand` are typical — a single API
  call each).
- Avoid slow agents in `context_sources` — they will time out and delay
  the user's first response after the gap.
- If your channel has a strict response-time SLA, consider disabling the
  gap trigger (`gap_secs = 0`) and relying only on the cadence trigger
  (`continuous_interval = N`), which runs asynchronously after each turn.

**Cadence-trigger and dashboard render:** The cadence trigger fires via
`tokio::spawn` AFTER the agent loop's `send_message` save returns. A
request to `GET /api/agents/{id}/session` between the save and the spawn
completing can race — the injected `[Context refresh — ts]` message may
not yet appear in the rendered session, even though the kernel has logged
"Context injected into session". The injection is durable in SQLite within
~tens of milliseconds of the log line; subsequent dashboard refreshes will
see it. The gap trigger has no such race because it injects before
`send_message` returns.

---

## Cross-PR integration

This feature builds on three pieces that landed earlier in the memory stack:

| PR | Piece | How continuous compaction uses it |
|----|-------|----------------------------------|
| #1224 | `Session::user_id`, persistent default user, `MessageSource` enum | All per-(agent, user) trackers are keyed on `session.user_id`. Injection uses `Message::context_injection` which carries `MessageSource::ContextInjection`. |
| #1225 | `extract_structured` filters `ContextInjection` | Calendar/mail summaries do not enter the structured-extraction pipeline. |
| #1226 | `has_real_user_activity`, `agent_dream_locks` | Same predicate gates the compaction trigger. The compaction lock copies the `try_lock`-and-skip pattern from the dream lock. |

---

## Debugging

Look for these log lines:

| Line | Meaning |
|------|---------|
| `Continuous compaction triggered` | The cadence gate fired and the background task started. |
| `Session gap detected, running compaction + context refresh` | The gap gate fired before dispatching a new inbound message. |
| `Compaction context source responded` | A hand returned a non-empty summary. |
| `Compaction context source failed` / `timed out` / `panicked` | One source failed — the others continue. |
| `Continuous compaction: skipping — previous compaction still in flight` | `try_lock` failed because a previous compaction is still running. |
| `Continuous compaction: skipping — no real user activity` | The activity gate short-circuited the trigger. |
| `Context injected into session (ContextInjection tag)` | The injected message is now part of the session for the next turn. |
