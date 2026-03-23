# LegendClaw Documentation

This documentation index is organized around how people actually approach the product: getting it running, understanding the platform, using workflow surfaces, connecting integrations, and contributing changes.

The workspace still uses `openfang-*` crate names internally. The product-facing surface is moving toward the LegendClaw shape described in the root README.

---

## Start Here

- Install and run the system: [Getting Started](getting-started.md)
- Configure the runtime: [Configuration](configuration.md)
- Debug setup issues: [Troubleshooting](troubleshooting.md)
- Understand commands and flags: [CLI Reference](cli-reference.md)

## Core Platform

- [Architecture](architecture.md): kernel, runtime, memory, API, channels, desktop, and build layers
- [Workflows](workflows.md): multi-agent pipelines, branching, approvals, and execution flow
- [Security](security.md): defense-in-depth model and runtime protections
- [API Reference](api-reference.md): app-facing REST, SSE, and WebSocket routes

## Hands And Agents

- [Agent Templates](agent-templates.md): built-in agent manifests and authoring patterns
- [Skill Development](skill-development.md): skill authoring, structure, and runtime behavior
- [Skill State Contract](skill-state-contract.md): state expectations for skill execution

## Business And Product Surfaces

- [Business Modes](business-modes.md): entry point for Command Center, Agency, Growth, School, and Chief of Staff product surfaces
- [Command Center](command-center.md): shared operating shell for intake, planning, approvals, and results
- [Agency Mode](agency-mode.md): service delivery surface for scoped client work
- [Growth Mode](growth-mode.md): campaign and acquisition surface for creative and optimization loops
- [School Mode](school-mode.md): program and cohort surface for education operations
- [Chief Of Staff Mode](chief-of-staff-mode.md): planning and follow-through surface for structured operator support
- [Workflows](workflows.md): execution model behind business-mode and orchestration flows
- [Personal Chief of Staff v1](personal-chief-of-staff-v1.md): product-shaping reference for structured agent operations
- [Launch Roadmap](launch-roadmap.md): current direction, priorities, and sequencing

## Channels And Integrations

- [Channels](channels.md): top-level guide to channel families, selection, and deployment pattern
- [Channel Adapters](channel-adapters.md): messaging adapters, setup, and custom adapter extension points
- [Integrations](integrations.md): top-level guide to app, SDK, MCP, A2A, and gateway contracts
- [API Surfaces](api-surfaces.md): top-level routing guide for REST, streaming, OpenAI-compatible, and protocol APIs
- [Providers And Models](providers-and-models.md): top-level routing guide for choosing provider strategy and model access
- [Providers](providers.md): LLM provider setup and model routing
- [MCP & A2A](mcp-a2a.md): integration patterns for external agent and tool systems
- [Integration Contract](integration-contract.md): stable app-facing contract for SDKs, gateways, auth, and base URLs

## Operations

- [Production Checklist](production-checklist.md): release and deployment readiness checklist
- [Desktop](desktop.md): native desktop app notes and build context
- [Configuration](configuration.md): runtime configuration reference
- [Troubleshooting](troubleshooting.md): common failure modes and recovery paths

## Additional References

- [CONTRIBUTING.md](../CONTRIBUTING.md): contribution paths, testing requirements, and PR expectations
- [MIGRATION.md](../MIGRATION.md): migration notes from adjacent ecosystems
- [SECURITY.md](../SECURITY.md): security policy and reporting process
- [CHANGELOG.md](../CHANGELOG.md): release history and notable changes

---

## Quick Reference

### Minimal Bring-Up

```bash
export GROQ_API_KEY="your-key"
cargo build --workspace --lib
target/release/openfang.exe start
```

Then open the backend or dashboard:

- `http://127.0.0.1:50051/api/health`
- `http://localhost:3002`

### Useful Paths

- `agents/`: agent manifest definitions
- `crates/`: Rust workspace crates
- `sdk/javascript/examples/nextjs-app-router/`: primary web frontend
- `packages/whatsapp-gateway/`: WhatsApp web gateway package
- `docs/`: product and development documentation

### Common Environment Variables

- `GROQ_API_KEY`: fast low-friction provider for local validation
- `OPENAI_API_KEY`: OpenAI-compatible runtime access
- `ANTHROPIC_API_KEY`: Claude provider access
- `GEMINI_API_KEY`: Gemini provider access
- `OPENFANG_DASHBOARD_URL`: override dashboard redirect target
- `OPENFANG_LEGACY_UI`: force legacy UI behavior when needed
