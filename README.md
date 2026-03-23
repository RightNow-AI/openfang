# LegendClaw

Agent Operating System for Autonomous Workflows, Secure Tools, and Real-World Automation.

Built on the OpenFang workspace and evolving into a sharper product surface for business workflows, hands, channels, and long-running agent operations.

Badges: Rust, Next.js 15, Rust + Axum backend, MIT license.

---

## One-Line Positioning

LegendClaw is an open-source Rust agent operating system for people who want agents to do work, not just chat. It combines long-running workflows, secure tool execution, built-in hands, business mode wizards, multi-channel adapters, and a full dashboard in one system.

## Who It Is For

- Operators who want autonomous agents running on schedules with guardrails
- Product and growth teams that need structured workflows instead of prompt-by-prompt chat
- Developers who want a real runtime, APIs, channels, memory, and extension points
- Contributors who want to extend an agent platform at the kernel, runtime, API, UI, or integration layer

## Why It Is Different

- It is an agent operating system, not a thin orchestration wrapper
- It ships a Rust workspace with runtime, memory, API, channels, CLI, desktop, and migration layers
- It supports durable workflows, approvals, routing, and secure execution boundaries
- It includes opinionated product surfaces such as Hands, Command Center, and business mode flows

## Quick Start

### Prerequisites

- Rust toolchain via `rustup`
- Node.js 18+
- At least one LLM provider key such as `GROQ_API_KEY`

### 1. Build the backend

```bash
cargo build --workspace --lib
cargo build --release -p openfang-cli
```

### 2. Start the daemon

```bash
GROQ_API_KEY=your_key_here target/release/openfang.exe start
curl http://127.0.0.1:50051/api/health
```

### 3. Start the dashboard

```bash
cd sdk/javascript/examples/nextjs-app-router
npm install
npm run dev -- --port 3002
```

Open:

- `http://localhost:3002` for the primary dashboard
- `http://127.0.0.1:50051/api/health` for backend health

The backend root can redirect to the Next.js app. See the docs for dashboard override and legacy UI behavior.

## Choose Your Path

- Evaluate the product: start with [docs/getting-started.md](docs/getting-started.md)
- Run business workflows: start with [docs/business-modes.md](docs/business-modes.md), then go to the dedicated family page that matches the job
- Explore the architecture: see [docs/architecture.md](docs/architecture.md)
- Choose a channel surface: see [docs/channels.md](docs/channels.md)
- Connect external systems: start with [docs/integrations.md](docs/integrations.md), then use [docs/api-surfaces.md](docs/api-surfaces.md) and [docs/providers-and-models.md](docs/providers-and-models.md)
- Contribute to the platform: see [CONTRIBUTING.md](CONTRIBUTING.md)

## What You Can Build With It

- Autonomous research and monitoring loops
- Lead generation and outreach operations
- Business workflow pipelines with approvals and results tracking
- Channel-native agents for chat, notifications, and support
- API-driven agent backends for other products and internal tools

## Core Product Areas

| Area | What it covers | Start here |
| ---- | -------------- | ---------- |
| Hands | Pre-built autonomous capability packages | [docs/agent-templates.md](docs/agent-templates.md) |
| Business Modes | Agency, Growth, School, and client workflow surfaces | [docs/business-modes.md](docs/business-modes.md) |
| Channels | Messaging adapters and gateway flows | [docs/channels.md](docs/channels.md) |
| Integrations | MCP, A2A, SDKs, OpenAI-compatible access | [docs/integrations.md](docs/integrations.md) |
| Security | Sandboxing, approvals, manifests, audit protections | [docs/security.md](docs/security.md) |
| Operations | Config, production checks, troubleshooting | [docs/configuration.md](docs/configuration.md) |

## Architecture At A Glance

LegendClaw runs on the OpenFang workspace and currently centers on these layers:

- `openfang-kernel`: orchestration, workflows, budgeting, RBAC, scheduler
- `openfang-runtime`: agent loop, drivers, tools, MCP, A2A, sandboxing
- `openfang-api`: REST, SSE, WebSocket, dashboard-facing routes
- `openfang-memory`: SQLite persistence, embeddings, canonical sessions
- `openfang-channels`: messaging adapters and formatting policies
- `openfang-cli`: daemon management and local execution flows
- `sdk/javascript/examples/nextjs-app-router`: primary frontend and app-facing API routes

For the full breakdown, see [docs/architecture.md](docs/architecture.md).

## Built-In Hands

Hands are pre-built autonomous capability packages that run on schedules, use tools, and respect approval gates.

| Hand | Purpose |
| ---- | ------- |
| Clip | Turn long-form video into short-form assets |
| Lead | Discover, score, and deliver prospects |
| Collector | Monitor targets and build ongoing intelligence |
| Predictor | Produce calibrated forecasts with tracked accuracy |
| Researcher | Generate cited deep research outputs |
| Twitter | Manage content workflows for X with approval control |
| Browser | Execute browser tasks behind purchase and action guardrails |

## Business Modes And Workflows

The current product surface includes richer workflow entry points than a generic chat interface:

- Command Center for client onboarding and delivery flows
- Agency mode for scoped service work
- Growth mode for campaigns, creatives, and optimization loops
- School mode for program design, enrollment, and student operations

Dedicated mode pages:

- [docs/command-center.md](docs/command-center.md)
- [docs/agency-mode.md](docs/agency-mode.md)
- [docs/growth-mode.md](docs/growth-mode.md)
- [docs/school-mode.md](docs/school-mode.md)
- [docs/chief-of-staff-mode.md](docs/chief-of-staff-mode.md)

The detailed route inventory and implementation notes belong in docs and release notes, not the landing page. This README now points people to the product shape first.

## Channels And Integrations

LegendClaw supports a large adapter surface across chat, workplace, community, privacy, and webhook environments. It also supports OpenAI-compatible APIs, MCP, and A2A integration patterns.

Start here:

- [docs/channels.md](docs/channels.md)
- [docs/api-surfaces.md](docs/api-surfaces.md)
- [docs/providers-and-models.md](docs/providers-and-models.md)
- [docs/providers.md](docs/providers.md)
- [docs/integrations.md](docs/integrations.md)
- [docs/api-reference.md](docs/api-reference.md)

## Security Model

Security is part of the product surface, not an afterthought. The platform includes sandboxing, capability gates, auditability, path protections, injection scanning, and rate limiting.

Read the full model in [docs/security.md](docs/security.md).

## Migration And Compatibility

LegendClaw is designed to work as infrastructure, not just a dashboard app. It includes migration and compatibility surfaces for adjacent ecosystems.

- [MIGRATION.md](MIGRATION.md)
- [docs/integration-contract.md](docs/integration-contract.md)
- [docs/api-reference.md](docs/api-reference.md)

## Development And Contributing

If you are contributing code, docs, examples, or integrations:

- Start with [CONTRIBUTING.md](CONTRIBUTING.md)
- Review the docs index in [docs/README.md](docs/README.md)
- Run the required checks:

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Project Status And Roadmap

LegendClaw is still pre-1.0. Core architecture is strong, but the repo surface, docs taxonomy, and example paths are still being tightened so first-time users can navigate the system without reading release notes.

Current priorities:

- sharpen the landing page and docs navigation
- keep runtime and API quality gates strict
- expand founder, client, and business workflow surfaces
- turn examples and starter paths into first-class onboarding tools

## Links

- [Documentation Index](docs/README.md)
- [Architecture](docs/architecture.md)
- [Getting Started](docs/getting-started.md)
- [Production Checklist](docs/production-checklist.md)
- [Security Policy](SECURITY.md)
- [Changelog](CHANGELOG.md)

## License

MIT.
See [LICENSE-MIT](LICENSE-MIT).
