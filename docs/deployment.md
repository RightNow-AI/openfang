# OpenFang Deployment

This guide documents the deployment paths that are actually supported by the assets in this repository.

## Deployment Matrix

| Mode | Use it for | Source of truth |
|------|------------|-----------------|
| Local source build | Development and debugging | `cargo`, `openfang-cli`, local config |
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
- env file: `~/.openfang/.env`
- data: `~/.openfang/data/openfang.db`

`OPENFANG_HOME` changes that root.

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
  http://127.0.0.1:4200/api/health
```

If auth is disabled and you are testing from inside the container, omit the header.

For a broader post-deploy check, run:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
```

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
sudo install -m 0644 deploy/openfang.service /etc/systemd/system/openfang.service
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
- preserve `Authorization` headers
- terminate TLS at the proxy
- rate-limit and IP-filter at the proxy where appropriate
- keep API auth enabled even behind the proxy

## Runtime Env Files

Do not confuse the repository root `.env.example` with the runtime env file.

- `.env.example` is a reference template in the repository
- `~/.openfang/.env` is what CLI and kernel logic actually load at runtime

For container deployments, Compose may also load a project-level `.env`, but that is separate from the daemon's own `OPENFANG_HOME` runtime files.

## Post-Deploy Smoke Checklist

After any deployment change:

1. `openfang status` or `/api/status` shows the expected listen address.
2. `/api/health` responds.
3. `/api/health/detail` responds when authenticated.
4. one provider is configured and reachable.
5. state persists across restart.
6. logs are visible in the chosen execution environment.
