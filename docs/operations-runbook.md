# Operations Runbook

This is the maintainer runbook for day-2 operation of the OpenFang daemon in this repository.

## 1. Operational Invariants

- default API bind: `127.0.0.1:4200`
- runtime home: `~/.openfang` unless `OPENFANG_HOME` overrides it
- main persisted state includes: `config.toml`, `.env`, `secrets.env`, `vault.enc`, `custom_models.json`, `integrations.toml`, the configured runtime sqlite file (default `data/openfang.db`), `agents/`, `skills/`, `workspaces/`, `workflows/`, `hand_state.json`, `cron_jobs.json`
- systemd deployments often also use `/etc/openfang/env` as an external env source; scripts honor `OPENFANG_ENV_FILE` explicitly and only auto-detect `/etc/openfang/env` when it matches the current `OPENFANG_HOME`
- `daemon.json` is a discovery file for the CLI, not a restore asset
- if the API is bound off-loopback, auth must be enabled with `OPENFANG_API_KEY` or dashboard auth

## 2. Start and Stop

### Command Matrix

Use the command form that matches how the daemon is installed on that host:

| Environment | Start / Stop / Status / Doctor |
| --- | --- |
| source checkout, debug binary built locally | `target/debug/openfang ...` |
| source checkout, release binary built locally | `target/release/openfang ...` |
| source checkout, no built binary | `cargo run -p openfang-cli -- ...` |
| installed host / package / release artifact | `openfang ...` |

The examples below use `openfang ...` for the installed-path form and `target/release/openfang ...` for the local release-binary form. For the fastest local iteration loop, you can substitute `target/debug/openfang ...`. On a source checkout without a built binary, swap in `cargo run -p openfang-cli -- ...`.

### Local source process

Start:

```bash
target/release/openfang start
```

Stop:

```bash
target/release/openfang stop
```

### Local debug process

For repeated edit/build/restart cycles on the same machine:

```bash
target/debug/openfang start
target/debug/openfang stop
target/debug/openfang status
target/debug/openfang doctor
```

Recommended maintainer loop:

```bash
target/debug/openfang stop
cargo build -p openfang-cli
target/debug/openfang start
```

Use the debug daemon when iteration speed matters more than release parity. Use
the release binary for final validation, packaging, or install verification.

### Docker / Compose

```bash
docker compose ps
docker compose logs --tail=200 openfang
docker compose restart openfang
docker compose down
```

### systemd

```bash
sudo systemctl status openfang
sudo journalctl -u openfang -n 200 --no-pager
sudo journalctl -u openfang -f
sudo systemctl restart openfang
```

The shipped systemd unit now performs a two-stage fail-closed gate. Pre-start (`ExecStartPre`) always verifies that the binary exists, `config.toml` is readable, and the runtime home/data directories are writable, then runs `preflight-openfang.sh --offline` to catch malformed config/state before boot. Post-start (`ExecStartPost`) retries live readiness verification with `preflight-openfang.sh` (no `--offline`) for up to 30 attempts with a 2-second interval, so strict production mode fails if the node never reaches authenticated ready status. In strict production mode, `/usr/local/lib/openfang/preflight-openfang.sh` is mandatory for both stages: if the helper is missing, the unit fails instead of silently skipping deeper validation.

### API shutdown

```bash
curl -X POST -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/shutdown
```

## 3. Health Check Matrix

| Surface | Purpose | Auth |
| --- | --- | --- |
| `openfang doctor` | broad local diagnostics and config checks | no |
| `openfang status` | daemon discovery and runtime status | no when local |
| `openfang health` | quick CLI health probe | no when local |
| `/api/health` | liveness probe | no |
| `/api/health/detail` | readiness probe plus detailed health, config, and restore warnings | yes when auth is enabled |
| `/api/status` | daemon status, model, agent summary | yes when auth is enabled |
| `/api/metrics` | Prometheus metrics | yes when auth is enabled |
| `/api/audit/verify` | audit-chain integrity check | yes when auth is enabled |
| `/api/integrations/health` | extension and integration health | yes when auth is enabled |
| `/api/mcp/servers` | MCP connection state | yes when auth is enabled |

Basic liveness:

```bash
curl -s http://127.0.0.1:4200/api/health
```

Detailed health:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

Interpretation:

- `/api/health` answers "is the process alive enough for a liveness probe?"
- `/api/health/detail` answers "is the node ready to serve traffic?" and now returns HTTP `503` when boot-time config warnings, restore warnings, unhealthy autonomous/crashed agents, missing default-provider auth, explicit embedding-provider failures, or shutdown-in-progress make the node not ready

Common smoke checks:

```bash
curl -s http://127.0.0.1:4200/api/health
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/status
target/release/openfang status
target/release/openfang doctor
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

`scripts/preflight-openfang.sh` resolves `config.toml` includes plus runtime helper files (`.env` / `secrets.env`) for provider/channel credentials, and it resolves daemon runtime overrides (`OPENFANG_LISTEN`, `OPENFANG_API_KEY`) from the same sources the daemon actually uses: the real process environment plus `OPENFANG_ENV_FILE` when present. It honors `OPENFANG_ENV_FILE`; if unset and `/etc/openfang/env` exists, that file is auto-detected only when it matches the current `OPENFANG_HOME`. When both runtime files define the same provider key, `secrets.env` wins so API/dashboard-managed secrets survive restart. Keep `/etc/openfang/env` readable by the `openfang` service user (for example `0640 root:openfang`), otherwise the unit's `ExecStartPre` preflight and host-side operator scripts will fail before the daemon starts. Strict mode accepts that external env baseline while still requiring owner-only permissions for the rest of the sensitive runtime files. If `.env` or `secrets.env` contains `OPENFANG_LISTEN` or `OPENFANG_API_KEY`, preflight now warns because the daemon ignores those override keys there. When preflight can resolve an API key, it now treats failures on protected operational endpoints as blocking instead of advisory, and it requires `/api/health/detail` to report `status = "ok"` rather than merely returning HTTP 200.
For production hosts, keep a machine API key available even when dashboard auth is enabled, otherwise protected-path checks remain advisory instead of fully enforceable. Always run the stateful smoke script defined in this repo as part of cutover verification so you prove the agent lifecycle path that customers exercise.
`scripts/smoke-openfang.sh` now also validates the public dashboard shell (`/`) and confirms `/api/metrics` still exposes the operational metric families used by the bundled alerts and runbooks, so a broken SPA bundle or alert drift fails during smoke instead of after cutover.
Use `OPENFANG_STRICT_PRODUCTION=1` in deployment automation so missing machine auth becomes a hard failure instead of a warning. The shipped systemd unit enables this by default in both pre-start and post-start gates, so host installs fail closed on config/state drift before boot and on live authenticated readiness after boot (same contract as Docker/Compose health gating).
Add `OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/live-api-smoke-openfang.sh` to your post-cutover checklist so the agent spawn/budget/kill workflow runs against each target environment before you declare it ready.
It is strict by default: if `/api/health` is unreachable, preflight fails. Use `--offline` (or `OPENFANG_PREFLIGHT_OFFLINE=1`) only when you intentionally want file-only checks without a live daemon.
In strict production mode, preflight still fails if `config.toml`, included config files, `.env`, `secrets.env`, or `vault.enc` are readable by group/other users. For the external env file, the supported exception is a service-readable baseline such as `0640 root:openfang`; anything broader remains a rollout blocker.
These scripts are expected to run from a host/runner checkout of this repository; they are not guaranteed to be present inside runtime container images.

## 4. Logging and Observability

Use `RUST_LOG` to increase detail:

```bash
RUST_LOG=info target/release/openfang start
RUST_LOG=debug target/release/openfang start
RUST_LOG=openfang=debug target/release/openfang start
```

Important logging realities:

- normal daemon logs go to stderr, Docker logs, or systemd journal
- there is no universal daemon log file by default
- `openfang logs` reads `~/.openfang/tui.log`, which only exists for TUI-driven logging
- `/api/logs/stream` is an audit-log SSE stream, not a full daemon stderr stream
- API requests now run inside a request-scoped tracing span; use the shared `x-request-id` response header and `request_id` log field together when correlating a failing request across logs

Prometheus and alerting:

- `/api/metrics` is the scrape surface for production monitoring
- the metrics set now includes `openfang_readiness_ready`, `openfang_database_ok`, `openfang_usage_store_ok`, `openfang_shutdown_requested`, `openfang_default_provider_auth_missing`, `openfang_config_warnings`, `openfang_restore_warnings`, and `openfang_agent_runtime_issues`
- `openfang_usage_store_ok` now feeds the `OpenFangUsageStoreUnavailable` alert (see `deploy/openfang-alerts.yml`), so any usage/monitoring database outage raises a critical notification.
- readiness-related warnings are resolved against the active credential chain (vault, `secrets.env`, `.env`, process env), so vault-backed or runtime-file-backed secrets do not create false degraded alerts
- sample scrape config lives in `deploy/prometheus-scrape.yml`
- sample Prometheus alert rules live in `deploy/openfang-alerts.yml`
- set `OPENFANG_LOG_FORMAT=json` when you want machine-parseable daemon logs in journald or container stderr; otherwise the daemon keeps the normal human-readable text format

Fast triage order:

1. `/api/health`
2. `/api/health/detail`
3. `openfang status`
4. deployment-specific logs
5. `openfang doctor`

## 5. Config Changes and Reload

The daemon has both automatic and manual reload paths:

- config watcher polling at runtime
- `POST /api/config/reload` for explicit reload

Operational caveat:
- the automatic watcher now tracks the root `config.toml` plus resolved `include = [...]` files
- if a config edit changes boot-time wiring or reload reports pending follow-up actions, restart the daemon instead of assuming it is live

Manual reload:

```bash
curl -X POST -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/config/reload
```

Remember the reload boundary:

- hot-reloadable: channels, skills, usage footer, web, browser, approval, cron, webhook, extensions, MCP, A2A, fallback providers, provider URLs, default model
- restart required: listen address, API auth key, network, memory, home/data directories, vault

Today only a subset auto-applies immediately without follow-up:

- `approval`
- `max_cron_jobs`
- `provider_urls`
- `default_model`

When config changes affect boot-time wiring, restart the daemon instead of trusting hot reload.

## 6. Required Validation After Code Changes

### Build and static checks

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

### Live validation

Use a real daemon after changes to routes, config, runtime wiring, or integrations:

```bash
cargo build --release -p openfang-cli
target/release/openfang start
curl -s http://127.0.0.1:4200/api/health
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/status
```

If startup fails after a config edit, treat that as expected until the config parses and deserializes cleanly again. The daemon no longer silently falls back to defaults when `config.toml` or an included config file is malformed.

If a provider key is available, also verify one real agent message round-trip and any side effects the feature is supposed to create.

For a repeatable provider canary, use:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" \
OPENFANG_CANARY_PROVIDER=groq \
OPENFANG_CANARY_MODEL=llama-3.3-70b-versatile \
OPENFANG_CANARY_API_KEY_ENV=GROQ_API_KEY \
scripts/provider-canary-openfang.sh
```

Canary pass criteria now include strict per-agent token growth after the message round-trip. Spend counters are still checked for non-regression, but may legitimately stay flat for free/local provider paths.

## 7. Backup

Back up the runtime home before upgrades or invasive changes.

```bash
scripts/backup-openfang.sh
```

The backup script creates a consistent SQLite snapshot instead of relying on a raw `cp` of a live WAL-mode database.
It now refuses to back up a running daemon unless you explicitly set `OPENFANG_ALLOW_LIVE_BACKUP=1`.
It also preserves `config.toml` include dependencies under `OPENFANG_HOME` so split-config deployments restore with the same effective config tree.
When `OPENFANG_ENV_FILE` is set (or a matching `/etc/openfang/env` is auto-detected), backup also captures that external env file as `external-env.env` in the backup directory and records its source path in `BACKUP.txt`.
`BACKUP.txt` now also records `openfang_binary`, `openfang_version`, `openfang_binary_sha256`, and `openfang_git_sha`, so rollback runs know exactly which binary (and build) the snapshot was created for. The backup script fingerprints `OPENFANG_BINARY_PATH` when provided, otherwise it falls back to `openfang` on `PATH` or a repo-local `target/release|debug/openfang` binary when you run it from a source checkout.

For hot systems, stop the service first when possible. If you must back up online, set `OPENFANG_ALLOW_LIVE_BACKUP=1` and understand that the database snapshot is consistent but concurrently changing directories can still reflect a live point in time.

Before upgrades or cutovers, also run:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

## 8. Restore

```bash
scripts/restore-openfang.sh "$HOME/openfang-backups/openfang-<timestamp>" --yes
```

The restore script now refuses to run if `daemon.json` points to a responding API. Stop the daemon first; if the file is stale, verify the API is really down and then re-run the restore.
It also refuses manifest-less backup directories by default; set `OPENFANG_ALLOW_LEGACY_RESTORE=1` only for an older backup you have already verified out of band.
It also rejects empty/malformed backup directories before deleting managed runtime state and re-hardens restored secrets/config file permissions.
It now reapplies ownership on restored files/directories by preferring the existing `OPENFANG_HOME` owner/group; override explicitly with `OPENFANG_UID` and `OPENFANG_GID` when restoring into a service-owned home such as `/var/lib/openfang`.
It restores the backed-up `config.toml` include tree alongside the root config so include-based deployments do not come back missing provider or environment fragments.
Restore now stages files into a sibling temporary home and only swaps the runtime directory into place after the staged copy is complete, reducing the chance of ending up with a half-restored runtime.
If `external-env.env` is present in the backup, restore writes it back to `OPENFANG_ENV_FILE` (or a matching auto-detected `/etc/openfang/env`). If no safe target can be resolved, or if the file cannot be restored, the restore now fails closed instead of reporting success with a warning.
On success, the script now keeps the rollback tree in a hidden sibling directory and prints that path. Do not delete it until the restored daemon has passed smoke/preflight and any provider canary you require.

After restore:

1. start the daemon
2. let it recreate `daemon.json`
3. rerun health checks
4. run `scripts/smoke-openfang.sh`
5. run `scripts/preflight-openfang.sh`
6. run `scripts/live-api-smoke-openfang.sh`
7. if provider-backed traffic matters, run `scripts/provider-canary-openfang.sh`
8. verify state-dependent features such as agents, hands, workflows, or channels
9. delete the preserved rollback tree only after those checks pass

## 9. Upgrade and Rollback

### Upgrade

1. back up runtime state
2. deploy the new binary or image
3. restart the service
4. run `scripts/smoke-openfang.sh`
5. run `scripts/live-api-smoke-openfang.sh`
6. verify provider, channel, and workflow paths touched by the change

### Rollback

1. stop the service
2. restore the previous binary or image
3. restore the latest known-good backup if needed
4. restart
5. rerun health checks, `scripts/smoke-openfang.sh`, and `scripts/live-api-smoke-openfang.sh`

## 10. High-Value Inspection Targets

Check these first when the daemon behaves unexpectedly:

| File or area | Why |
| --- | --- |
| `~/.openfang/config.toml` | config parse or schema mismatch |
| `~/.openfang/.env` | missing provider or channel secrets |
| `/etc/openfang/env` | systemd env drift (API/auth/provider keys, OPENFANG_HOME) |
| configured `data_dir` / `[memory].sqlite_path` under `OPENFANG_HOME` | persistence and permission failures |
| `crates/openfang-api/src/server.rs` | route and startup wiring |
| `crates/openfang-kernel/src/kernel.rs` | boot, reload, and background execution |
| `crates/openfang-kernel/src/config_reload.rs` | reload semantics |
| `crates/openfang-channels/` | adapter startup and routing |

## 11. Doc Sync Rules

Whenever any of the following change, update operator docs in the same patch:

- deployment assets -> `docs/deployment.md`
- runtime state paths or boot expectations -> `docs/architecture.md`, `docs/configuration.md`
- smoke checks or health semantics -> `docs/operations-runbook.md`, `docs/troubleshooting.md`
- API surfaces -> `docs/api-reference.md`
