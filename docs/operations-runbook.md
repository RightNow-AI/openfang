# Operations Runbook

This is the maintainer runbook for day-2 operation of the OpenFang daemon in this repository.

## 1. Operational Invariants

- default API bind: `127.0.0.1:4200`
- runtime home: `~/.openfang` unless `OPENFANG_HOME` overrides it
- main persisted state includes:
  - `config.toml`
  - `.env`
  - `vault.enc`
  - `data/`
  - `agents/`
  - `skills/`
  - `workspaces/`
  - `workflows/`
  - `hand_state.json`
  - `cron_jobs.json`
- `daemon.json` is a discovery file for the CLI, not a restore asset
- if the API is bound off-loopback, auth must be enabled with `OPENFANG_API_KEY` or dashboard auth

## 2. Start and Stop

### Local source process

Start:

```bash
target/release/openfang start
```

Stop:

```bash
openfang stop
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
| `/api/health/detail` | detailed health and config warnings | yes when auth is enabled |
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

Common smoke checks:

```bash
curl -s http://127.0.0.1:4200/api/health
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/status
openfang status
openfang doctor
```

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

Manual reload:

```bash
curl -X POST -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/config/reload
```

Remember the reload boundary:

- hot-reloadable: channels, skills, usage footer, web, browser, approval, cron, webhook, extensions, MCP, A2A, fallback providers, provider URLs, default model
- restart required: listen address, API auth key, network, memory, home/data directories, vault

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

If a provider key is available, also verify one real agent message round-trip and any side effects the feature is supposed to create.

## 7. Backup

Back up the runtime home before upgrades or invasive changes.

```bash
scripts/backup-openfang.sh
```

The backup script creates a consistent SQLite snapshot instead of relying on a raw `cp` of a live WAL-mode database.

For hot systems, stop the service first when possible. If you must back up online, use the script above rather than copying `openfang.db` directly.

## 8. Restore

```bash
scripts/restore-openfang.sh "$HOME/openfang-backups/openfang-<timestamp>" --yes
```

After restore:

1. start the daemon
2. let it recreate `daemon.json`
3. rerun health checks
4. run `scripts/smoke-openfang.sh`
5. verify state-dependent features such as agents, hands, workflows, or channels

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
