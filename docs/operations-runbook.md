# Operations Runbook

This is the maintainer runbook for day-2 operation of the OpenFang daemon in this repository.

## 1. Operational Invariants

- default API bind: `127.0.0.1:4200`
- runtime home: `~/.openfang` unless `OPENFANG_HOME` overrides it
- main persisted state includes: `config.toml`, `.env`, `secrets.env`, `vault.enc`, `custom_models.json`, `integrations.toml`, `data/`, `agents/`, `skills/`, `workspaces/`, `workflows/`, `hand_state.json`, `cron_jobs.json`
- systemd deployments often also use `/etc/openfang/env` as an external env source; scripts auto-detect it (or honor `OPENFANG_ENV_FILE`)
- `daemon.json` is a discovery file for the CLI, not a restore asset
- if the API is bound off-loopback, auth must be enabled with `OPENFANG_API_KEY` or dashboard auth

## 2. Start and Stop

### Command Matrix

Use the command form that matches how the daemon is installed on that host:

| Environment | Start / Stop / Status / Doctor |
| --- | --- |
| source checkout, release binary built locally | `target/release/openfang ...` |
| source checkout, no built binary | `cargo run -p openfang-cli -- ...` |
| installed host / package / release artifact | `openfang ...` |

The examples below use `openfang ...` for the installed-path form and `target/release/openfang ...` for the local release-binary form. On a source checkout without an installed binary, swap in `cargo run -p openfang-cli -- ...`.

### Local source process

Start:

```bash
target/release/openfang start
```

Stop:

```bash
target/release/openfang stop
```

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
| `/api/health/detail` | readiness probe plus detailed health and config warnings | yes when auth is enabled |
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
- `/api/health/detail` answers "is the node ready to serve traffic?" and now degrades when boot-time config warnings, missing default-provider auth, shutdown-in-progress, or supervisor panics are present

Common smoke checks:

```bash
curl -s http://127.0.0.1:4200/api/health
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/status
target/release/openfang status
target/release/openfang doctor
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

`scripts/preflight-openfang.sh` resolves `config.toml` includes plus runtime `.env` / `secrets.env` / external env file / process env overrides. It honors `OPENFANG_ENV_FILE`; if unset and `/etc/openfang/env` exists, that file is auto-detected. When both runtime files define the same key, `secrets.env` wins so API/dashboard-managed secrets survive restart. When preflight can resolve an API key, it now treats failures on protected operational endpoints as blocking instead of advisory, and it requires `/api/health/detail` to report `status = "ok"` rather than merely returning HTTP 200.
It is strict by default: if `/api/health` is unreachable, preflight fails. Use `--offline` (or `OPENFANG_PREFLIGHT_OFFLINE=1`) only when you intentionally want file-only checks without a live daemon.
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
- the metrics set now includes `openfang_readiness_ready`, `openfang_database_ok`, `openfang_shutdown_requested`, `openfang_default_provider_auth_missing`, and `openfang_config_warnings`
- readiness-related warnings are resolved against the active credential chain (vault, `secrets.env`, `.env`, process env), so vault-backed or runtime-file-backed secrets do not create false degraded alerts
- sample Prometheus alert rules live in `deploy/openfang-alerts.yml`

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
When `OPENFANG_ENV_FILE` is set (or `/etc/openfang/env` is auto-detected), backup also captures that external env file as `external-env.env` in the backup directory and records its source path in `BACKUP.txt`.

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
If `external-env.env` is present in the backup, restore writes it back to `OPENFANG_ENV_FILE` (or auto-detected `/etc/openfang/env`, or the manifest-recorded source path) when possible.

After restore:

1. start the daemon
2. let it recreate `daemon.json`
3. rerun health checks
4. run `scripts/smoke-openfang.sh`
5. run `scripts/preflight-openfang.sh`
6. if provider-backed traffic matters, run `scripts/provider-canary-openfang.sh`
7. verify state-dependent features such as agents, hands, workflows, or channels

## 9. Upgrade and Rollback

### Upgrade

1. back up runtime state
2. deploy the new binary or image
3. restart the service
4. run `scripts/smoke-openfang.sh`
5. verify provider, channel, and workflow paths touched by the change

### Rollback

1. stop the service
2. restore the previous binary or image
3. restore the latest known-good backup if needed
4. restart
5. rerun health checks and `scripts/smoke-openfang.sh`

## 10. High-Value Inspection Targets

Check these first when the daemon behaves unexpectedly:

| File or area | Why |
| --- | --- |
| `~/.openfang/config.toml` | config parse or schema mismatch |
| `~/.openfang/.env` | missing provider or channel secrets |
| `/etc/openfang/env` | systemd env drift (API/auth/provider keys, OPENFANG_HOME) |
| `~/.openfang/data/` | persistence and permission failures |
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
