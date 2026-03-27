# Production Readiness

This document is the current production-ready assessment for the OpenFang platform in this repository.
It is intentionally narrow: it focuses on whether the existing product can be deployed and operated safely, not on future roadmap work or large refactors.

Assessment date: 2026-03-27

## Current Status

Current recommendation: conditionally ready for production cutover.

Meaning:

- the codebase now meets the repository's compile, test, lint, health, preflight, and backup expectations
- the most immediate production stability gaps found in this review have been patched
- final cutover should still wait for target-environment verification of the real provider canary, Linux systemd validation, Prometheus alert delivery checks, and rollback drill evidence

## What Is Now Covered

### Functional closure and correctness

- runtime default-model overrides now apply consistently to hand activation and runtime-facing API surfaces
- agent session lifecycle updates now persist the active `session_id`, so create/switch/reset/clear operations survive daemon restart instead of snapping back to stale sessions
- global budget limits are now enforced on the live message path rather than only exposed in read APIs
- global budget updates now reject negative values instead of silently converting them into unlimited budgets
- upload serving now sniffs on-disk content types instead of defaulting unknown files to `image/png`
- agent config, identity, model, skill, tool, and MCP updates now fail closed when SQLite persistence fails instead of returning success with runtime-only drift
- cloned agents now persist copied identity state instead of dropping it on daemon restart
- health/readiness already fails closed for broken usage-store access, degraded restore state, and missing provider auth where applicable

### Stability and failure handling

- agent spawn and kill now fail closed on persistence errors instead of leaving runtime-only or database-only ghosts behind
- agent metadata/config mutation paths now roll back in-memory changes when persistence fails, so restarts cannot silently resurrect stale state after a "successful" API write
- config hot reload now surfaces follow-up warnings instead of silently appearing fully applied
- budget endpoints and channel/websocket budget readouts now return explicit errors when metering storage is unavailable
- per-agent budget persistence rolls back in-memory state on save failure
- metering write failures now emit explicit warnings instead of disappearing silently
- strict production health/preflight paths fail closed when authenticated readiness cannot be checked

### Security baseline

- public binds still require explicit auth
- systemd strict mode now treats `/usr/local/lib/openfang/preflight-openfang.sh` as mandatory, so deeper validation cannot be bypassed by omission
- preflight enforces tighter sensitive-file permission checks, including config include files

### Observability

- readiness metrics now include `openfang_usage_store_ok`
- Prometheus alerting now includes a dedicated usage-store outage alert
- request correlation and detailed readiness diagnostics are already exposed through `/api/health/detail`, `/api/metrics`, and request-scoped logging

### Backup, restore, and rollback

- backups now record `openfang_binary`, `openfang_version`, `openfang_binary_sha256`, and `openfang_git_sha` in `BACKUP.txt`, with Git checkout fallback when the binary version string does not carry a commit
- backup metadata hashing now falls back cleanly on macOS and other environments without `sha256sum`
- restore now preserves the staged rollback tree after a successful file restore so operators can still revert quickly while post-restore smoke and preflight are running
- this makes restore evidence materially better for rollback and binary/state matching

### Deployment and release hygiene

- CI now validates shipped deployment artifacts beyond the Rust workspace:
  - provider canary validation in release flow
  - release preflight syntax-checks the shipped ops scripts and lints the systemd/Prometheus artifacts
  - `docker compose config`
  - `systemd-analyze verify deploy/openfang.service`
  - `promtool check` for the bundled Prometheus artifacts
  - stateful `scripts/live-api-smoke-openfang.sh` validation in daemon smoke and release provider-canary flows
  - scheduled-backup deployment assets (`deploy/openfang-backup.service`, `deploy/openfang-backup.timer`)
- Linux host installs now have a single repo-owned installer entrypoint (`scripts/install-systemd-openfang.sh`) that stages the binary, ops helpers, systemd units, backup timer assets, and baseline env/config templates together instead of relying on a manual copy checklist

## Evidence Handling

Keep one-off execution evidence in CI logs, release artifacts, or an operator review record instead of hard-coding machine-specific command output into this document.
For each new production-readiness review, capture at least:

- the three Rust quality gates (`cargo build --workspace --lib`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`)
- release-binary or container boot evidence
- authenticated `/api/health/detail` and `/api/metrics` checks
- `scripts/smoke-openfang.sh`, `scripts/live-api-smoke-openfang.sh`, and strict `scripts/preflight-openfang.sh` output
- backup evidence showing the resulting `BACKUP.txt` metadata
- any target-environment `provider-canary`, `systemd-analyze`, and `promtool` results

This file intentionally records the stable conclusion and the remaining cutover gates, not the ephemeral details of one workstation run.

## Remaining Required Checks Before Real Production Cutover

These are not theoretical nice-to-haves. They should be treated as cutover gates because they depend on target-environment tooling or secrets that were not available in this review environment.

1. Run one real provider canary against the target deployment.
   Use `scripts/provider-canary-openfang.sh` with the same provider, model, API key source, and auth mode the production node will use. This review environment did not have `GROQ_API_KEY`, so no real provider-backed LLM round-trip was executed here.

2. Confirm CI/release canary wiring is actually backed by live secrets.
   Provider canary evidence is only meaningful when the same provider/model/key path used by production is available in the execution environment.

3. Validate the systemd unit on a Linux host or CI runner with `systemd-analyze`.
   This review updated the unit and CI wiring, but the local review machine did not have `systemd-analyze` installed.

4. Validate Prometheus artifacts with `promtool` where that binary is available.
   The rules and scrape config are wired for linting in CI, but lint alone is insufficient.

5. Confirm alert delivery, not just alert rules.
   `deploy/openfang-alerts.yml` can be syntactically valid while notification routing is still broken in the real monitoring stack.

6. Run one rollback drill using a fresh backup plus the binary/image version recorded in `BACKUP.txt`.
   The metadata is now present and restore keeps the rollback tree around for post-restore validation; the final confidence gain comes from proving restore speed, operator familiarity, and the point at which you safely delete that rollback copy.

7. Confirm production machine API key contract.
   `OPENFANG_API_KEY` must be present in the real supervisor environment so `/api/health/detail`, `/api/metrics`, and operator scripts can authenticate in strict production mode.

8. Confirm scheduled backups are enabled on production hosts.
   Install and enable `deploy/openfang-backup.timer` and verify it is active before cutover.

## Residual Risks

The highest remaining risks are operational, not architectural:

- real provider paths can still regress if no canary is run with live secrets
- Linux service behavior still needs one environment with `systemd-analyze` and actual unit installation
- Prometheus alert delivery still depends on your external monitoring system, not just the repo files
- operational recovery still depends on proving scheduled backups and rollback drill execution
- cross-platform runtime smoke remains lighter on macOS and Windows than on Linux

## Recommended Cutover Sequence

1. Freeze the release candidate branch or commit.
2. Run the three Rust quality gates.
3. Build the release binary or image that will actually be deployed.
4. Run offline preflight against the target runtime home.
5. Start the daemon and require `/api/health/detail` to return `status = "ok"`.
6. Run the real provider canary.
7. Confirm Prometheus scraping plus alert delivery.
8. Take and archive a fresh backup.
9. Proceed with cutover.
10. If rollback is needed, restore the matching state snapshot and deploy the binary/image version recorded in `BACKUP.txt`.

## Not In Scope For This Review

These may still be worth doing later, but they were intentionally not treated as production blockers for this pass:

- large architectural refactors
- major API redesign
- new product features unrelated to safety, stability, observability, or operability
- broad platform rework outside the current OpenFang design
