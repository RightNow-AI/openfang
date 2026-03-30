<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# docs

## Purpose
Complete user-facing documentation for OpenFang — 40+ guides covering installation, architecture, API reference, configuration, skills, workflows, channels, providers, security, troubleshooting, and production deployment.

## Key Files
| File | Description |
|------|-------------|
| `README.md` | Index of all documentation guides |
| `getting-started.md` | Installation, first agent, first chat |
| `api-reference.md` | All 76 REST/WS/SSE endpoints with request/response examples |
| `architecture.md` | 12-crate structure, kernel boot, agent lifecycle, memory substrate |
| `configuration.md` | Complete `config.toml` reference with every field |
| `cli-reference.md` | Every command and subcommand with examples |
| `agent-templates.md` | 30 pre-built agents across 4 performance tiers |
| `workflows.md` | Multi-agent pipelines with branching, fan-out, loops, and triggers |
| `security.md` | 16 defense-in-depth security systems |
| `channel-adapters.md` | 40 messaging channels — setup, configuration, custom adapters |
| `providers.md` | 20 LLM providers, 51 models, 23 aliases — setup and model routing |
| `skill-development.md` | 60 bundled skills, custom skill development, FangHub marketplace |
| `mcp-a2a.md` | Model Context Protocol and Agent-to-Agent protocol integration |
| `production-checklist.md` | Pre-release verification, signing keys, secrets, deployment |
| `troubleshooting.md` | Common issues, FAQ, diagnostics |
| `desktop.md` | Tauri 2.0 native app — build, features, architecture |
| `launch-roadmap.md` | v0.1.0 feature roadmap and milestones |
| `VERTEX_AI_LOCAL_TESTING.md` | Testing Google Vertex AI locally |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `benchmarks/` | Performance, security, feature coverage, and architecture SVGs |

## For AI Agents

### Working In This Directory
- Keep guides concise and scannable: headers, tables, code blocks, bullet points.
- All code examples must be tested and verified to work.
- Match the existing guide style: practical, not theoretical.
- Document what the code actually does now, not what it used to do.
- API reference must include real endpoint paths, method, request/response JSON, and error codes from actual implementation.
- When adding new guides, add entries to `README.md` in the appropriate section.

<!-- MANUAL: -->
