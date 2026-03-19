# OpenFang Deployment

This guide documents the deployment paths that are actually supported by the assets in this repository.

## Deployment Matrix

| Mode | Use it for | Source of truth |
|------|------------|-----------------|
| Local source build | Development, debugging, and the fastest local iteration loop | `cargo`, `openfang-cli`, local config |
| CLI install scripts | Host installs when release artifacts exist | `scripts/install.sh`, `scripts/install.ps1` |
| Docker / Compose | Local services and containerized hosts | `Dockerfile`, `docker-compose.yml` |
| Linux systemd | Server deployment | `deploy/openfang.service` |

## Common Requirements

- one working inference path: a remote LLM provider or a reachable local model endpoint
- writable OpenFang home directory
- a plan for API authentication before exposing the service outside loopback

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
The container image and Compose healthcheck use `/api/health/detail` when `OPENFANG_API_KEY` is available to the probe. If a deployment relies on dashboard auth only, the baked-in probe falls back to `/api/health` because it cannot reuse a login session cookie.
For production automation, keep a machine API key available for readiness probes, Prometheus scrapes, and operator scripts; dashboard auth alone is not enough for full protected-path validation.
Healthcheck address resolution now follows `OPENFANG_BASE_URL` first; when that is unset it derives the probe URL from `OPENFANG_LISTEN` so non-default listen ports do not flap to unhealthy.

For a broader post-deploy check, run:

```bash
OPENFANG_STRICT_PRODUCTION=1 OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
OPENFANG_STRICT_PRODUCTION=1 OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

These operational scripts are repository artifacts and are typically run from the host (or CI runner) that has this repo checked out. Do not assume they exist inside the runtime container image.
`scripts/preflight-openfang.sh` is strict by default and fails if the live API is unreachable or if `/api/health/detail` reports a degraded node. Use `--offline` (or `OPENFANG_PREFLIGHT_OFFLINE=1`) only for intentional file-only checks.

If this node is supposed to serve real provider-backed responses, also run a real canary:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" \
OPENFANG_CANARY_PROVIDER=groq \
OPENFANG_CANARY_MODEL=llama-3.3-70b-versatile \
OPENFANG_CANARY_API_KEY_ENV=GROQ_API_KEY \
scripts/provider-canary-openfang.sh
```

The canary now requires per-agent token usage to increase after the LLM round-trip. Spend counters are still checked for non-regression, but they may remain unchanged for free or local provider paths.
For automation and cutovers, prefer `OPENFANG_STRICT_PRODUCTION=1` so smoke/preflight fail closed when protected operational checks cannot authenticate.

If you scrape Prometheus, wire alerts from `deploy/openfang-alerts.yml` into the same environment. The readiness metrics (`openfang_readiness_ready`, `openfang_database_ok`, `openfang_default_provider_auth_missing`, `openfang_config_warnings`, `openfang_restore_warnings`) are designed to match the daemon's own `/api/health/detail` interpretation, including secrets resolved from `vault.enc`, `secrets.env`, and `.env`.
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
sudo useradd --system --home /var/lib/openfang --shell /usr/sbin/nologin openfang
sudo install -d -o openfang -g openfang /var/lib/openfang
sudo install -d /etc/openfang
sudo install -d /usr/local/lib/openfang
sudo install -m 0644 deploy/openfang.service /etc/systemd/system/openfang.service
sudo install -m 0755 scripts/preflight-openfang.sh /usr/local/lib/openfang/preflight-openfang.sh
sudo install -m 0640 -o root -g openfang /dev/null /etc/openfang/env
```

Create `/etc/openfang/env` with at least:

```bash
OPENFANG_HOME=/var/lib/openfang
OPENFANG_API_KEY=<generate-a-real-random-key>
GROQ_API_KEY=replace-me
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
The unit also performs a minimal pre-start gate: it requires a readable `config.toml`, writable runtime directories, and, when `/usr/local/lib/openfang/preflight-openfang.sh` is installed, it runs `preflight-openfang.sh --offline` before starting the daemon.

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
