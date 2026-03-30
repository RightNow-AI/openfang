<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# deploy

## Purpose
Production deployment configurations — systemd unit file for Linux daemon operation with hardened security and resource limits.

## Key Files
| File | Description |
|------|-------------|
| `openfang.service` | systemd unit file — Type=simple, security hardening, resource limits, env file support |

## For AI Agents

### Working In This Directory
- The systemd unit expects binary at `/usr/local/bin/openfang` and config at `/etc/openfang/env`.
- Working directory is `/var/lib/openfang` — all persistent data goes there.
- Security hardening is strict: `ProtectSystem=strict`, `NoNewPrivileges=true`, sandboxed `/tmp`, kernel module protection.
- Resource limits: 65536 open files, 4096 processes — adjust if agents require more.
- On-failure restart with 5-second backoff prevents rapid restart loops.
- When changing paths or security settings, verify they work on actual Linux before deploying.

<!-- MANUAL: -->
