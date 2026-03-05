# OpenFang Documentation

Welcome to the OpenFang documentation. OpenFang is the open-source Agent Operating System -- 14 Rust crates, 40 channels, 60 skills, 20 LLM providers, 76 API endpoints, and 16 security systems in a single binary.

---

## Getting Started

| Guide | Description |
|-------|-------------|
| [Getting Started](getting-started.md) | Installation, first agent, first chat session |
| [Configuration](configuration.md) | Complete `config.toml` reference with every field |
| [CLI Reference](cli-reference.md) | Every command and subcommand with examples |
| [Troubleshooting](troubleshooting.md) | Common issues, FAQ, diagnostics |

## Core Concepts

| Guide | Description |
|-------|-------------|
| [Architecture](architecture.md) | 12-crate structure, kernel boot, agent lifecycle, memory substrate |
| [Agent Templates](agent-templates.md) | 30 pre-built agents across 4 performance tiers |
| [Workflows](workflows.md) | Multi-agent pipelines with branching, fan-out, loops, and triggers |
| [Security](security.md) | 16 defense-in-depth security systems |

## Integrations

| Guide | Description |
|-------|-------------|
| [Channel Adapters](channel-adapters.md) | 40 messaging channels -- setup, configuration, custom adapters |
| [LLM Providers](providers.md) | 20 providers, 51 models, 23 aliases -- setup and model routing |
| [Skills](skill-development.md) | 60 bundled skills, custom skill development, FangHub marketplace |
| [MCP & A2A](mcp-a2a.md) | Model Context Protocol and Agent-to-Agent protocol integration |
| [Integration Platform Roadmap](integration-platform-roadmap.md) | Connector architecture and rollout plan for YouTube, Facebook, Blender, DaVinci Resolve, and Houdini |

## Reference

| Guide | Description |
|-------|-------------|
| [API Reference](api-reference.md) | All 76 REST/WS/SSE endpoints with request/response examples |
| [Desktop App](desktop.md) | Tauri 2.0 native app -- build, features, architecture |

## Release & Operations

| Guide | Description |
|-------|-------------|
| [Production Checklist](production-checklist.md) | Every step before tagging v0.1.0 -- signing keys, secrets, verification |

## Additional Resources

| Resource | Description |
|----------|-------------|
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Development setup, code style, PR guidelines |
| [MIGRATION.md](../MIGRATION.md) | Migrating from OpenClaw, LangChain, or AutoGPT |
| [SECURITY.md](../SECURITY.md) | Security policy and vulnerability reporting |
| [CHANGELOG.md](../CHANGELOG.md) | Release notes and version history |

---

## Quick Reference

### Start in 30 Seconds

```bash
export GROQ_API_KEY="your-key"
openfang init && openfang start
# Open http://127.0.0.1:4200
```

### Key Numbers

| Metric | Count |
|--------|-------|
| Crates | 14 |
| Agent templates | 30 |
| Messaging channels | 40 |
| Bundled skills | 60 |
| Built-in tools | 38 |
| LLM providers | 20 |
| Models in catalog | 51 |
| Model aliases | 23 |
| API endpoints | 76 |
| Security systems | 16 |
| Tests | 967 |

### Important Paths

| Path | Description |
|------|-------------|
| `~/.openfang/config.toml` | Main configuration file |
| `~/.openfang/data/openfang.db` | SQLite database |
| `~/.openfang/skills/` | Installed skills |
| `~/.openfang/daemon.json` | Daemon PID and port info |
| `agents/` | Agent template manifests |

### Key Environment Variables

| Variable | Provider |
|----------|----------|
| `ANTHROPIC_API_KEY` | Anthropic (Claude) |
| `OPENAI_API_KEY` | OpenAI (GPT-4o) |
| `GEMINI_API_KEY` | Google Gemini |
| `GROQ_API_KEY` | Groq (fast Llama/Mixtral) |
| `DEEPSEEK_API_KEY` | DeepSeek |
| `XAI_API_KEY` | xAI (Grok) |

Only one provider key is needed to get started. Groq offers a free tier.
