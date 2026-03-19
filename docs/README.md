# OpenFang Documentation

This documentation set is maintained against the code in this repository. When docs and code disagree, treat the code as the source of truth and update the docs.

## Maintainer Reading Order

If you are taking over the project, read these in order:

1. [Architecture](architecture.md)
2. [Core Modules](core-modules.md)
3. [Deployment](deployment.md)
4. [Configuration](configuration.md)
5. [Operations Runbook](operations-runbook.md)
6. [Troubleshooting](troubleshooting.md)

Then move to the detailed references for your area.

## Documentation Map

| Guide | Use it for |
|-------|------------|
| [Getting Started](getting-started.md) | First local boot of the daemon |
| [Architecture](architecture.md) | System layers, boot flow, data flow, ownership boundaries |
| [Core Modules](core-modules.md) | Which crate/file owns which behavior |
| [Deployment](deployment.md) | Local, container, and server deployment paths |
| [Configuration](configuration.md) | Config layout, precedence, secrets, hot reload boundaries |
| [Operations Runbook](operations-runbook.md) | Health checks, backup, restore, upgrade, rollback |
| `scripts/preflight-openfang.sh` | Runtime and deployment preflight for production cutovers |
| `deploy/prometheus-scrape.yml` | Starter Prometheus scrape job with Bearer auth for `/api/metrics` |
| `deploy/openfang-alerts.yml` | Starter Prometheus alert rules for readiness and stability signals |
| [Release Runbook](release-runbook.md) | Step-by-step release publishing and post-release verification |
| [Troubleshooting](troubleshooting.md) | Symptom-based fault isolation |
| [Health Check Guide](health-check-guide.md) | Quick health diagnostics and automated checks |
| [Telegram @Mention Troubleshooting](telegram-mention-troubleshooting.md) | Telegram group @mention issues and UTF-16 bug fixes |
| [CLI Reference](cli-reference.md) | Command surface and examples |
| [API Reference](api-reference.md) | HTTP routes, payloads, and responses |
| [Channel Adapters](channel-adapters.md) | Channel-specific setup and routing |
| [Providers](providers.md) | LLM provider support and model routing |
| [Workflows](workflows.md) | Workflow and trigger model |
| [Skill Development](skill-development.md) | Bundled and custom skill authoring |
| [MCP & A2A](mcp-a2a.md) | External tool and agent-to-agent integration |
| [Desktop](desktop.md) | Tauri embedding model |
| [Security](security.md) | Security model and hardening notes |
| [Production Checklist](production-checklist.md) | Release-engineering checklist, not the primary deployment guide |

## Source of Truth

Use these files when verifying facts:

| Area | Source |
|------|--------|
| Workspace members | `Cargo.toml` |
| Kernel config schema | `crates/openfang-types/src/config.rs` |
| Config loading and include behavior | `crates/openfang-kernel/src/config.rs` |
| Boot sequence and runtime assembly | `crates/openfang-kernel/src/kernel.rs` |
| HTTP routes and router wiring | `crates/openfang-api/src/server.rs`, `crates/openfang-api/src/routes.rs` |
| CLI behavior | `crates/openfang-cli/src/main.rs` |
| Deployment assets | `Dockerfile`, `docker-compose.yml`, `deploy/openfang.service` |
| Example config | `openfang.toml.example`, `.env.example` |

## Project Boundaries

This repository has two documentation spaces:

- `docs/` covers the OpenFang platform in this fork.
- `projects/shipinbot/docs/` covers the shipinbot project, workflow, and production procedures.

Keep project-specific operational procedures in the shipinbot docs instead of mixing them into core platform guides.
