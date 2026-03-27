# OpenFang Health Check Guide

This is the shortest safe operator path for checking whether an OpenFang node is healthy enough to stay in service.

For the canonical day-2 runbook, deeper recovery steps, and backup/restore procedures, use [operations-runbook.md](operations-runbook.md). For deployment-specific setup, use [deployment.md](deployment.md).

## 1. Fast Path

Run the smallest checks first:

```bash
curl -s http://127.0.0.1:4200/api/health
```

If auth is enabled, follow with:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/health/detail
```

Expected:

- `/api/health` returns `{"status":"ok", ...}` for liveness.
- `/api/health/detail` returns `status = "ok"` and `readiness.ready = true` when the node is ready to serve traffic.

Then run the host-side preflight:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

If you only want file-level validation without a live daemon:

```bash
scripts/preflight-openfang.sh --offline
```

## 2. Process Checks

Use the command form that matches how the daemon is installed:

```bash
openfang status
openfang doctor
```

Or, for a local source build:

```bash
target/release/openfang status
target/release/openfang doctor
```

Do not inspect secrets with `ps eww`, `env`, or shell history dumps. If a provider or channel auth check is needed, prefer `/api/health/detail`, `/api/providers`, or the config files/runbook paths that already redact secret values.

## 3. Logs

There is no universal daemon log file by default.

Use the platform log source that matches the deployment:

### systemd

```bash
sudo systemctl status openfang
sudo journalctl -u openfang -n 200 --no-pager
sudo journalctl -u openfang -f
```

### Docker / Compose

```bash
docker compose ps
docker compose logs --tail=200 openfang
docker compose logs -f openfang
```

### Local foreground run

Check the terminal where `openfang start` is running.

## 4. API Smoke

Once liveness is confirmed, check the protected operational surfaces:

```bash
curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/status

curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/metrics | head

curl -s -H "Authorization: Bearer $OPENFANG_API_KEY" \
  http://127.0.0.1:4200/api/audit/verify
```

For a bundled smoke check:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
```

For systemd hosts, `OPENFANG_ENV_FILE=/etc/openfang/env scripts/smoke-openfang.sh` reuses the installed machine-auth source.

For the stateful agent lifecycle and budget path:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/live-api-smoke-openfang.sh
```

For a real provider-backed canary:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" \
OPENFANG_CANARY_PROVIDER=groq \
OPENFANG_CANARY_MODEL=llama-3.3-70b-versatile \
OPENFANG_CANARY_API_KEY_ENV=GROQ_API_KEY \
scripts/provider-canary-openfang.sh
```

Those three scripts also honor `OPENFANG_ENV_FILE=/etc/openfang/env` for machine auth and listen-address resolution when you are validating a systemd install.

## 5. Restart Safely

Prefer graceful stop/restart paths:

### systemd

```bash
sudo systemctl restart openfang
```

### Docker / Compose

```bash
docker compose restart openfang
```

### Local process

```bash
target/release/openfang stop
target/release/openfang start
```

Avoid `kill -9` except as a last resort after graceful shutdown has failed and you have already captured enough logs for diagnosis.

## 6. Before and After Recovery

Before invasive changes, take a backup:

```bash
scripts/backup-openfang.sh
```

If you restore or replace runtime state, validate again:

```bash
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/smoke-openfang.sh
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/live-api-smoke-openfang.sh
OPENFANG_API_KEY="$OPENFANG_API_KEY" scripts/preflight-openfang.sh
```

## 7. Escalation

Escalate from this quick guide to the full runbook when:

- `/api/health/detail` is degraded
- preflight fails
- audit verification fails
- provider canary fails
- restart does not recover the node

Use these next:

- [operations-runbook.md](operations-runbook.md)
- [deployment.md](deployment.md)
- [release-runbook.md](release-runbook.md)
- [troubleshooting.md](troubleshooting.md)
- [../deploy/openfang-alerts.yml](../deploy/openfang-alerts.yml)
