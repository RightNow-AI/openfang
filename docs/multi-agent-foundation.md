# Multi-Agent Foundation Configuration Guide

This guide explains how admins and end users enable and tune OpenFang multi-agent workflows safely.

## Scope

The foundation covers:

- Workflow orchestration (`plan/review/dispatch/worker` patterns)
- Review reject-and-return loops
- Parallel fan-out/fan-in aggregation
- Retry/block escalation behavior
- Traceable audit events and observability metrics

## Admin Setup

### 1) Define the workflow with safe defaults

Create workflows through `POST /api/workflows` using conservative execution controls:

- Set per-step `timeout_secs` (avoid unbounded runtime).
- Prefer `error_mode: "fail"` by default.
- Use `error_mode: { "retry": { "max_retries": N } }` only for idempotent steps.
- Add a `review` step for high-impact outputs.
- Use `StepMode::Review` reject-return settings to enforce quality gates before dispatch.

### 2) Register role-specific agents

Use distinct agents for planning, review, dispatch, and workers. Keep capabilities minimal per role:

- Planner: analysis/planning only.
- Reviewer: validation/rejection decisions.
- Dispatcher: final packaging/routing.
- Worker: bounded task execution.

This reduces blast radius and makes audit trails clearer.

### 3) Enforce approval and permission boundaries

For sensitive tools/actions:

- Require explicit approval before execution.
- Deny unauthorized actions by default.
- Keep allowlists narrow and role-specific.

### 4) Enable observability and auditing

Use these endpoints for runtime governance:

- `GET /api/workflows/{id}/runs`: run list with `trace_id`.
- `GET /api/workflows/traces/{trace_id}/events`: decision/dispatch/execution/review event stream.
- `GET /api/workflows/metrics`: workflow success/failure/retry/reject/resume metrics.
- `GET /api/metrics`: Prometheus metrics (includes workflow observability gauges).

## User Operation

### 1) Run a workflow

Execute:

- `POST /api/workflows/{id}/run`

Response includes:

- `run_id`
- `trace_id`
- `status`
- `output`

### 1.5) Run in shadow against the current production path

When OpenFang is still the candidate path, keep the production output authoritative and pass it into the workflow run request:

- `POST /api/workflows/{id}/run` with `shadow.enabled = true`
- Include `shadow.production_output` from the current production path (for example OpenClaw)
- OpenFang runs the workflow, stores the shadow comparison on the run, and returns:
  - `output`: the production output
  - `shadow_output`: the OpenFang output
  - `shadow.matches` / `shadow.normalized_matches` / `shadow.first_mismatch_index`

This keeps rollout safe while making output drift visible before promotion.

### 1.6) Prepare a fast rollback before promotion

Use rollout controls to keep the stable path explicit and make rollback a one-call operation:

- `GET /api/workflows/{id}/rollout` inspects the current primary path, stable path, shadow flag, and rollback checklist.
- `PUT /api/workflows/{id}/rollout` updates rollout intent, for example promoting `primary_path` to `openfang` while keeping `stable_path` as `production`.
- `POST /api/workflows/{id}/rollback` immediately switches traffic back to the stable path, disables shadow by default, and returns a recorded checklist plus rollback duration.

Recommended rollback checklist:

1. Freeze the candidate path.
2. Switch the primary path back to the last stable route.
3. Disable shadow traffic until the incident is understood.
4. Verify SLI/SLO signals and recent traces.
5. Capture operator notes and incident follow-up.

### 2) Track progress and outcomes

After execution:

- Query `GET /api/workflows/{id}/runs` for lifecycle state.
- Use `trace_id` with `GET /api/workflows/traces/{trace_id}/events` to inspect:
  - Why review rejected/approved
  - Which dispatch happened
  - Which execution steps retried or failed

### 3) Interpret review/retry behavior

- Reject-return loops: review can send work back to planning with explicit feedback.
- Retry escalation: repeated failures can promote a run from `FAILED` behavior to `BLOCKED` semantics (depending on error mode policy).

## Safe Tuning Checklist

Before widening scale or risk:

1. Keep `max_retries` low (start with `1` or `2`).
2. Keep `timeout_secs` explicit on every step.
3. Require review for externally visible or high-risk actions.
4. Verify `reject_rate` and `retry_rate` from `/api/workflows/metrics` before increasing concurrency.
5. Investigate any high `resume_time_ms` before production promotion.
6. Use `trace_id` event logs for incident review and rollback decisions.

## Minimal Rollout Plan

1. Start with one workflow and one user path.
2. Monitor `success_rate`, `failure_rate`, `retry_rate`, and `reject_rate`.
3. Fix noisy steps (high retries/rejects) before enabling broader traffic.
4. Expand gradually to more task types and channels.

This staged rollout keeps multi-agent orchestration observable, recoverable, and safe.
