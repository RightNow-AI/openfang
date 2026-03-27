# OpenFang Deployment

This guide documents the deployment paths that are actually supported by the assets in this repository.

## Deployment Matrix

| Mode | Use it for | Source of truth |
|------|------------|-----------------|
| Local source build | Development, debugging, and the fastest local iteration loop | `cargo`, `openfang-cli`, local config |
| CLI install scripts | Host installs when release artifacts exist | `scripts/install.sh`, `scripts/install.ps1` |
| Docker / Compose | Local services and containerized hosts | `Dockerfile`, `docker-compose.yml` |
| Linux systemd | Server deployment | `deploy/openfang.service` |

CI now includes a `deploy-lint` job that runs `systemd-analyze verify deploy/openfang.service`, `docker compose config`, and `promtool check` against `deploy/openfang-alerts.yml`/`deploy/prometheus-scrape.yml`, plus a provider canary stage in release flow.

## Common Requirements

- one working inference path: a remote LLM provider or a reachable local model endpoint
- writable OpenFang home directory
- a machine API key (`OPENFANG_API_KEY`) configured in the real process environment before exposing the service outside loopback

Local models are optional. If you plan to use Groq, OpenAI, Gemini, or another remote provider, not having a pre-pulled Ollama model is not a deployment blocker.

Default runtime paths:

- config: `~/.openfang/config.toml`
- env files: `~/.openfang/.env` and `~/.openfang/secrets.env`
- default sqlite state: `~/.openfang/data/openfang.db`
- optional service env file: `/etc/openfang/env` (systemd deployments)

`OPENFANG_HOME` changes that root.
`data_dir` changes the default data root, and `[memory].sqlite_path` can move the runtime database to another path under `OPENFANG_HOME`.

At runtime, OpenFang resolves credentials from the vault, `secrets.env`, `.env`, and process environment variables. If you save provider keys through the dashboard, preserve `secrets.env` as part of the deployment state.
Operational scripts in this repository (`preflight-openfang.sh`, `backup-openfang.sh`, `restore-openfang.sh`) also support an external env file path via `OPENFANG_ENV_FILE`. If unset, they only auto-detect `/etc/openfang/env` when it matches the current `OPENFANG_HOME` (or the default systemd home `/var/lib/openfang`).
Do not put `OPENFANG_LISTEN` or `OPENFANG_API_KEY` into `~/.openfang/.env` or `~/.openfang/secrets.env`; the daemon only honors those two overrides from the actual process environment (or an external env file that the supervisor exports into the process).

## Default Choice Order For This Fork

Pick the narrowest deployment path that matches the job:

| Need | Use this first | Notes |
|------|----------------|-------|
| Same-machine full-stack development or maintenance | `scripts/local-stack.sh start` from the repo root | Default maintainer baseline for this fork's integrated OpenFang + `shipinbot` flow |
| OpenFang-only local work | `target/debug/openfang start` | Fastest Rust iteration loop when you do not need the media service |
| Integrated container validation or containerized host deployment | repo-root `docker-compose.yml` | Full stack: `openfang` + `media-pipeline-service` + optional `telegram-bot-api` |
| shipinbot service-only container deployment | `projects/shipinbot/docker-compose.yml` | Only the media service; does not boot OpenFang |
| Linux host deployment | `deploy/openfang.service` | systemd-managed daemon install |

The two Compose files are intentionally different:

- repo-root `docker-compose.yml` is the integrated full-stack topology for this fork
- `projects/shipinbot/docker-compose.yml` is a service-only topology owned by the `shipinbot` project

If you are editing both Rust and Python paths on the same machine, do not default
to Docker just because Compose exists. Use `scripts/local-stack.sh start` unless
you are intentionally validating container topology or release parity.

## 1. Local Source Build

This is the most reliable path for maintainers.

```bash
cargo build --workspace --lib
cargo run -p openfang-cli -- init
GROQ_API_KEY=your-key cargo run -p openfang-cli -- start
```

Useful follow-up commands:

```bash
cargo run -p openfang-cli -- doctor
cargo run -p openfang-cli -- status
cargo run -p openfang-cli -- health
```

Use this mode when you are changing code, routes, config fields, or runtime wiring.

### Fastest Local Iteration

On a developer Mac, the fastest way to run the latest local code is usually a
debug CLI binary plus the normal daemon process:

```bash
cargo build -p openfang-cli
target/debug/openfang start
```

For repeated edits, restart with:

```bash
target/debug/openfang stop
cargo build -p openfang-cli
target/debug/openfang start
```

Why this is usually faster than Docker for local work:

- `docker build` or `docker compose up --build` still recompiles the Rust code
- the local debug binary skips the slowest release-optimization and image-build steps
- the daemon still uses the same runtime home, API, and local filesystem layout as production-style host installs

Prefer the debug daemon when the goal is "run the newest local code as quickly
as possible". Switch back to `target/release/openfang` for final validation,
install packaging, or release-like smoke tests.

### Default Maintainer Baseline For This Fork

If you are working on this fork's integrated OpenFang + `shipinbot` stack on
the same machine, the default local deployment should be two host processes from
the current checkout:

```bash
cargo build -p openfang-cli
target/debug/openfang start

cd projects/shipinbot
./scripts/start_media_web.sh
```

Why this is the default local baseline:

- it is the shortest rebuild/restart loop for Rust and Python changes together
- it keeps one clear source of truth: the current checkout under `projects/shipinbot/`
- it avoids path drift from a second local runtime copy such as `~/shipinbot-runtime`
- it avoids the rebuild cost of `docker compose up --build` during normal local work

Use Docker / Compose only when you are intentionally validating the container
topology. Use release binaries or systemd only when you need release-parity or
host-install verification.

For hand activation or reconciliation, `python3 projects/shipinbot/scripts/sync_openfang_local_hands.py --force`
is a maintenance tool, not the default way to boot local services. Do not use
it as a substitute for starting the host media service from the current repo.

## 2. CLI Install Scripts

The repository includes install scripts:

- `scripts/install.sh`
- `scripts/install.ps1`

These scripts are bootstrap helpers for release artifacts. They are not a replacement for source builds during development, and their success depends on published binaries being available.

For repository maintenance, prefer source build or an internally managed release package.

## 3. Docker and Docker Compose

Container assets exist in:

- `Dockerfile`
- `docker-compose.yml`

The current Compose file is intended to be built locally:

```bash
docker compose up --build
```

This is useful for containerized hosts or isolation, but it is not the fastest
iteration path for the newest local code on a maintainer Mac. If the goal is
"edit code, rebuild, restart, test" on the same machine, prefer the local debug
CLI + daemon flow above.

### Service-only shipinbot Compose

This repository also contains `projects/shipinbot/docker-compose.yml`.
That file is not a second copy of the integrated stack. It only starts
`media-pipeline-service` and is meant for service-only deployment or isolated
validation of the shipinbot runtime.

Use the service-only Compose file when all of the following are true:

- you only want the media service container
- OpenFang is managed separately on that machine
- you do not need the integrated OpenFang + hand bootstrap container topology

If you need the full stack in containers, stay at the repo root and use the
repo-root `docker-compose.yml`.

### Integrated shipinfabu Stack

This fork now ships one Compose topology that keeps the `shipinfabu` production
chain honest instead of pretending the bridge is "just another script":

- `openfang` — the daemon plus the bundled Python runtime needed by
  `/app/scripts/openfang_clean_publish_bridge.py`
- `media-pipeline-service` — the actual shipinbot execution backend
- `telegram-bot-api` — optional but recommended when `use_local_api = true`

Two shared paths are the contract:

- `/app/data/ingest` — shared between `openfang` and `media-pipeline-service`
  so staged source files and Telegram intake batches are visible to both sides
- `/var/lib/telegram-bot-api` — shared between `openfang` and
  `telegram-bot-api` so `file://` paths returned by Local Bot API are not
  container-private lies

`docker-compose.yml` now mounts both paths consistently and enables
`OPENFANG_BOOTSTRAP_SHIPINBOT=1` by default so the OpenFang container can
upsert and activate the external `shipinfabu` hand on boot with container-safe
defaults such as:

- `media_api_base_url = http://media-pipeline-service:8000`
- `bridge_script_path = /app/scripts/openfang_clean_publish_bridge.py`
- `local_source_staging_dir = /app/data/ingest`
- `local_media_intake_dir = /app/data/ingest`

That bootstrap only fixes the hand/runtime side. You still must point your
Telegram channel config at the same stack:

```toml
[channels.telegram]
default_agent = "shipinfabu-hand"
use_local_api = true
auto_start_local_api = false
api_url = "http://telegram-bot-api:8081"
```

If you leave `auto_start_local_api = true` while also running the Compose
`telegram-bot-api` service, you are back to running two different endpoints and
deserve the confusion that follows.

### Important Container Networking Note

The repository Compose file now does two things by default:

- publishes only to host loopback: `127.0.0.1:4200:4200`
- binds inside the container on `0.0.0.0:4200`

This keeps local developer access working while avoiding accidental LAN exposure.

You must provide a real API key before booting the container. The daemon rejects obvious placeholder values such as `change-me` or `replace-me` on non-loopback listeners.

### Optional Compose Environment File

```bash
cat > .env <<'EOF'
OPENFANG_LISTEN=0.0.0.0:4200
OPENFANG_API_KEY=<paste-a-random-hex-string-here>
OPENFANG_STRICT_PRODUCTION=1
GROQ_API_KEY=replace-me
EOF

docker compose up --build -d
```

This repo-root `.env` file is optional convenience, not a hard requirement. You can also export the same variables in your shell before running Compose.

### Volume Layout

The container uses `/data` as `OPENFANG_HOME`. Persist that path with a volume so config, state, agents, skills, and database survive restarts.

A first boot with an empty `/data` volume is expected to succeed without running `openfang init` ahead of time. That gives you a live daemon, but not necessarily a usable inference path.

For real agent responses, you still need at least one working provider configuration or reachable local model endpoint.

### Smoke Test

```bash
curl -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

If auth is disabled and you are testing from inside the container, omit the header.
Treat `/api/health` as liveness only. For deploy validation and orchestrator health checks, prefer `/api/health/detail` and require `status = "ok"`.
The container image and Compose healthcheck now resolves auth the same way the daemon does: `OPENFANG_API_KEY` from the real process environment wins, otherwise `api_key` from `config.toml` is used. The repository Compose file also sets `OPENFANG_STRICT_PRODUCTION=1`, so a missing machine credential causes the container probe to fail closed instead of silently downgrading from readiness to liveness.
For production automation, treat a machine API key as mandatory for readiness probes, Prometheus scrapes, and operator scripts; dashboard auth alone is not enough for full protected-path validation.
Healthcheck address resolution now follows `OPENFANG_BASE_URL` first; when that is unset it derives the probe URL from `OPENFANG_LISTEN` so non-default listen ports do not flap to unhealthy.

For a broader post-deploy check, run:

```bash
OPENFANG_STRICT_PRODUCTION=1 OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
OPENFANG_STRICT_PRODUCTION=1 OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

These operational scripts are repository artifacts and are typically run from the host (or CI runner) that has this repo checked out. Do not assume they exist inside the runtime container image.
`scripts/smoke-openfang.sh` now fails if `/api/metrics` is reachable but missing the operational metric families that the bundled Prometheus rules and runbooks depend on.
`scripts/preflight-openfang.sh` is strict by default and fails if the live API is unreachable or if `/api/health/detail` reports a degraded node. Use `--offline` (or `OPENFANG_PREFLIGHT_OFFLINE=1`) only for intentional file-only checks.
For systemd hosts, these three scripts also honor `OPENFANG_ENV_FILE=/etc/openfang/env` for API-key and listen-address resolution, so you can validate the installed service without manually re-exporting the machine key.

If this node is supposed to serve real provider-backed responses, also run a real canary and the stateful live smoke:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" \
OPENFANG_CANARY_PROVIDER=groq \
OPENFANG_CANARY_MODEL=llama-3.3-70b-versatile \
OPENFANG_CANARY_API_KEY_ENV=GROQ_API_KEY \
scripts/provider-canary-openfang.sh
```

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/live-api-smoke-openfang.sh
```

The canary now requires per-agent token usage to increase after the LLM round-trip. Spend counters are still checked for non-regression, but they may remain unchanged for free or local provider paths.
For automation and cutovers, prefer `OPENFANG_STRICT_PRODUCTION=1` so smoke/preflight fail closed when protected operational checks cannot authenticate.

If you scrape Prometheus, wire alerts from `deploy/openfang-alerts.yml` into the same environment. The readiness metrics (`openfang_readiness_ready`, `openfang_database_ok`, `openfang_usage_store_ok`, `openfang_default_provider_auth_missing`, `openfang_config_warnings`, `openfang_restore_warnings`, `openfang_agent_runtime_issues`) are designed to match the daemon's own `/api/health/detail` interpretation, including secrets resolved from `vault.enc`, `secrets.env`, and `.env`.
Use `deploy/prometheus-scrape.yml` as the starter scrape job when you want a working Prometheus example that already includes Bearer auth for `/api/metrics`.
The sample alert rules now include both `absent(openfang_info)` and `up{job="openfang"} == 0`; update the `job` matcher if your Prometheus scrape config uses a different label.

## 4. Linux Server with systemd

The repository ships a systemd unit template at `deploy/openfang.service`.

It assumes:

- binary: `/usr/local/bin/openfang`
- user/group: `openfang`
- home/state directory: `/var/lib/openfang`
- env file: `/etc/openfang/env`

### Recommended Layout

```bash
sudo scripts/install-systemd-openfang.sh --binary target/release/openfang
```

That installer creates the `openfang` service account when needed and stages:

- `/usr/local/bin/openfang`
- `/usr/local/lib/openfang/{backup,preflight,restore,smoke,live-api-smoke,provider-canary,healthcheck}`
- `/etc/systemd/system/openfang.service`
- `/etc/systemd/system/openfang-backup.{service,timer}`
- `/etc/openfang/env` from `deploy/openfang.env.example` if missing
- `/var/lib/openfang/config.toml` from `openfang.toml.example` if missing

Then edit `/etc/openfang/env` with at least:

```bash
OPENFANG_HOME=/var/lib/openfang
OPENFANG_API_KEY=<generate-a-real-random-key>
ANTHROPIC_API_KEY=replace-me
```

Generate the API key with something like:

```bash
openssl rand -hex 32
```

Then initialize config as the service user or pre-seed `/var/lib/openfang/config.toml`.
If you enable dashboard auth in `config.toml`, set a valid Argon2id `password_hash` before first boot.
Legacy 64-character SHA-256 hex digests still work for compatibility, but do not use them for new deployments.
Keep `/etc/openfang/env` readable by the `openfang` service user as well as root. `0640 root:openfang` is the supported baseline so `ExecStartPre` preflight and host-side backup/preflight/restore scripts can read the same external env source.
The systemd unit template exports `OPENFANG_ENV_FILE=/etc/openfang/env` so operator scripts can consistently find the same external env source.
Strict preflight treats that external env file as a special case: `0600` remains valid, and `0640 root:openfang` is also accepted so the service user can read it without weakening the rest of the runtime secrets baseline.
The systemd unit template also sets `StateDirectoryMode=0700` and `LogsDirectoryMode=0700` so the runtime state and log directories do not default to world-readable permissions.
The unit now enforces a two-stage fail-closed gate in strict production mode:
- pre-start gate (`ExecStartPre`): requires a readable `config.toml`, writable runtime directories, enables `OPENFANG_STRICT_PRODUCTION=1`, and runs `/usr/local/lib/openfang/preflight-openfang.sh --offline` before boot
- post-start gate (`ExecStartPost`): retries live authenticated readiness verification for up to 30 attempts (2s interval each) using `/usr/local/lib/openfang/preflight-openfang.sh` without `--offline`
Strict mode treats the helper itself as mandatory in both stages: if `/usr/local/lib/openfang/preflight-openfang.sh` is missing while `OPENFANG_STRICT_PRODUCTION=1`, the unit fails rather than silently skipping deeper validation.

If you want the installer to reload systemd and start the host after you finish editing `/etc/openfang/env`, run:

```bash
sudo scripts/install-systemd-openfang.sh --binary target/release/openfang --enable
```

`--enable` fails closed: it runs `/usr/local/lib/openfang/preflight-openfang.sh --offline` in strict mode first and refuses to start the unit until the installed env/config baseline passes.

### Scheduled Backups (systemd)

This repository now ships minimal backup scheduling assets:

- `deploy/openfang-backup.service`
- `deploy/openfang-backup.timer`

Install and enable them on Linux hosts that use systemd:

```bash
sudo scripts/install-systemd-openfang.sh --binary target/release/openfang
sudo systemctl daemon-reload
sudo systemctl enable --now openfang-backup.timer
sudo systemctl list-timers openfang-backup.timer
```

The backup job only calls the existing `scripts/backup-openfang.sh` and reuses `OPENFANG_HOME`, `OPENFANG_ENV_FILE`, `OPENFANG_KEEP_BACKUPS`, and optional `OPENFANG_BACKUP_ROOT` from `/etc/openfang/env`. By default it writes to `${OPENFANG_HOME}/backups` and creates that directory on demand as the `openfang` service user.

### Service Management

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now openfang
sudo systemctl status openfang
sudo journalctl -u openfang -f
```

## Reverse Proxy Guidance

This repository does not ship a production Nginx, Caddy, or Traefik config. If you place OpenFang behind a reverse proxy:

- keep OpenFang bound to loopback or a private interface when possible
- do not rely on "localhost-only" mode as your auth model behind the proxy; configure `OPENFANG_API_KEY` and/or dashboard auth first
- preserve `Authorization` headers
- do not pass API keys in URL query parameters (for example `?token=...`) because reverse-proxy/access logs can leak them
- terminate TLS at the proxy
- rate-limit and IP-filter at the proxy where appropriate
- keep API auth enabled even behind the proxy

## Runtime Env Files

Do not confuse the repository root `.env.example` with the runtime env file.

- `.env.example` is a reference template in the repository
- runtime credentials can come from both `~/.openfang/.env` and `~/.openfang/secrets.env`
- when both files define the same key, `secrets.env` wins so dashboard/API writes persist across restart
- for systemd hosts, `/etc/openfang/env` can act as the external process-env source and can be explicitly targeted with `OPENFANG_ENV_FILE=/etc/openfang/env`

For container deployments, Compose may also load a project-level `.env`, but that is separate from the daemon's own `OPENFANG_HOME` runtime files.

## Post-Deploy Smoke Checklist

After any deployment change:

1. `openfang status` or `/api/status` shows the expected listen address.
2. `/api/health` responds.
3. `/api/health/detail` responds when authenticated.
4. one provider is configured and reachable.
5. state persists across restart.
6. logs are visible in the chosen execution environment.
7. `scripts/preflight-openfang.sh` passes against the target runtime home and base URL.
