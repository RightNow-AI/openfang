# Troubleshooting

This guide focuses on failures that are common in the current repository and deployment assets.

## 1. Quick Triage Order

Start with these commands before guessing:

```bash
openfang doctor
openfang status
curl -s http://127.0.0.1:4200/api/health
```

If auth is enabled:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

Then inspect deployment-specific logs:

- local foreground process stderr
- `docker compose logs`
- `journalctl -u openfang`

## 2. Daemon Will Not Start

### Symptoms

- `openfang start` exits early
- the API never appears on port `4200`
- `openfang status` cannot find a daemon

### Checks

```bash
openfang doctor
openfang config show
RUST_LOG=debug openfang start
```

### Common causes

- malformed `config.toml`
- no reachable provider credentials
- bind/auth conflict
- another process already using the port

## 3. Refusing to Expose the API on 0.0.0.0

### Symptom

Startup fails with a refusal to expose the API off-loopback.

### Cause

The server validates that non-loopback bind addresses must have auth enabled, and it rejects obvious placeholder API keys on public listeners.

### Fix

Either:

```bash
export OPENFANG_API_KEY="$(openssl rand -hex 32)"
```

or bind locally:

```toml
api_listen = "127.0.0.1:4200"
```

## 4. CLI Says a Daemon Exists but It Is Not Reachable

### Symptom

- `openfang status` points to an old daemon
- commands fail even though no real daemon is serving traffic

### Cause

`~/.openfang/daemon.json` is stale.

### Fix

```bash
openfang doctor --repair
```

Or remove `daemon.json` manually after confirming the daemon is dead.

## 5. `/api/health/detail` Returns 401 or 403

### Cause

`/api/health/detail` is a protected operational endpoint when auth is enabled.

### Fix

Use:

```bash
curl -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

For anonymous liveness checks, use `/api/health` instead.

## 6. Channel Config Is Ignored

### Symptom

You added channel settings, but the adapter does not start.

### Common cause

Using the wrong TOML shape, for example:

```toml
[telegram]
```

instead of:

```toml
[channels.telegram]
```

### Fix

Move channel config under `channels.<name>`.

## 7. Wrong Field Used for HTTP Bind

### Symptom

You changed `[network]` and expected the HTTP API to move.

### Cause

`[network]` config is for OFP peer networking. The HTTP API uses top-level `api_listen`.

### Fix

Set:

```toml
api_listen = "127.0.0.1:4200"
```

and leave `[network].listen_addresses` for peer networking only.

## 8. Docker Container Is Running but the Host Cannot Reach It

### Checks

```bash
docker compose ps
docker compose logs --tail=200 openfang
```

### Common causes

- `OPENFANG_LISTEN` is still loopback inside the container
- missing provider key
- server refused non-loopback bind without auth

### Fix

Use:

```bash
OPENFANG_LISTEN=0.0.0.0:4200
OPENFANG_API_KEY=$(openssl rand -hex 32)
```

in Compose or the container environment.

## 9. Config Changes Did Not Apply Live

### Cause

OpenFang has config reload support, but not every field reloads hot.

### Fix

Try:

```bash
curl -X POST -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/config/reload
```

Restart the daemon after changing:

- API listen or auth behavior
- network settings
- memory backend settings
- home/data directory settings
- vault settings

## 10. `openfang logs` Does Not Show Daemon Output

### Cause

`openfang logs` tails `~/.openfang/tui.log`, which is only meaningful for TUI logging.

### Fix

Use the real runtime log surface for your deployment:

- foreground stderr for local dev
- `docker compose logs`
- `journalctl -u openfang`

Treat `/api/logs/stream` as an audit stream, not as daemon stderr.

## 11. Telegram Bot Does Not Respond

### Checks

```bash
echo "$TELEGRAM_BOT_TOKEN"
openfang doctor
```

Verify:

- `[channels.telegram]` exists
- `bot_token_env` points to the right env var
- allowed-user filters are not blocking you
- logs show the Telegram adapter actually starting

## 12. Telegram Large File Downloads Still Fail

### Checks

- `download_enabled = true`
- `use_local_api = true` when relying on Local Bot API
- `api_url` or `local_api_port` is correct
- `telegram_api_id` and `telegram_api_hash_env` are present when auto-starting Local Bot API

See [telegram-large-files.md](telegram-large-files.md) for the full Local Bot API path.

## 13. Provider Auth Fails Even Though Config Looks Correct

### Cause

The config usually stores env-var names, not secret values.

### Fix

If config says:

```toml
api_key_env = "GROQ_API_KEY"
```

then the process environment or `~/.openfang/.env` must actually contain `GROQ_API_KEY`.

Use:

```bash
openfang doctor
openfang config show
```

to confirm both config and secret presence.

## 14. Permission Errors Under `~/.openfang`

### Symptoms

- database cannot open
- workspaces cannot be created
- `daemon.json` cannot be written

### Fix

Verify that the runtime user owns the runtime home:

```bash
ls -la ~/.openfang
```

For server installs, also verify `/var/lib/openfang` ownership and the systemd `User=` setting.

## 15. Route Compiles but Endpoint Is Missing

### Cause

The handler exists, but the router was not updated.

### Fix

For API changes, always verify both:

- `crates/openfang-api/src/routes.rs`
- `crates/openfang-api/src/server.rs`

Then run a live daemon and hit the endpoint directly.

## 16. What to Inspect First

| Area | File |
| --- | --- |
| Config schema | `crates/openfang-types/src/config.rs` |
| Config loading and include behavior | `crates/openfang-kernel/src/config.rs` |
| Config reload | `crates/openfang-kernel/src/config_reload.rs` |
| Boot sequence | `crates/openfang-kernel/src/kernel.rs` |
| Router assembly | `crates/openfang-api/src/server.rs` |
| Runtime request handling | `crates/openfang-runtime/` |
| Channel ingress | `crates/openfang-channels/` |

## 17. Escalation Rule

If a problem crosses OpenFang and `projects/shipinbot/`, treat it as a contract issue, not a one-sided bug. Check both sides before changing schema, batch files, or runtime assumptions.
