# API Surfaces

This page is the top-level routing guide for choosing the right API contract before you dive into the full [API Reference](api-reference.md).

---

## Start Here

- Read [API Reference](api-reference.md) for the complete endpoint inventory.
- Read [Integration Contract](integration-contract.md) for the stable app-facing boundary.
- Read [Providers And Models](providers-and-models.md) if provider choice is driving the API decision.

## Choose By Contract

### Managed Agent API

Use this when you need durable agents, memory, tools, and specialist manifests.

Primary routes:

- `GET /api/agents`
- `POST /api/agents`
- `POST /api/agents/{id}/message`
- `POST /api/agents/{id}/message/stream`
- `GET /api/agents/{id}/session`

Best fit:

- product-specific agents
- dashboard backends
- stateful sessions and operator workflows

### Workflow And Trigger API

Use this when the main job is orchestration, branching, schedules, or automation.

Primary routes:

- `GET /api/workflows`
- `POST /api/workflows`
- `POST /api/workflows/{id}/run`
- `GET /api/triggers`
- `POST /api/triggers`

Best fit:

- automation backends
- orchestrated multi-agent processes
- repeatable internal operations

### Command And Product Surface APIs

Use this when your frontend maps to product workflows such as client operations or mode-specific execution.

Current route families include:

- command-center routes such as `POST /clients` and `POST /wizard/generate-plan`
- mode routes such as `POST /modes/{mode}/records` and `POST /modes/{mode}/generate-plan`

Best fit:

- product dashboards
- business-mode UIs
- operator-facing shells

### Model And Provider API

Use this when your integration needs to inspect models, manage provider keys, or test provider connectivity.

Primary routes:

- `GET /api/models`
- `GET /api/providers`
- `POST /api/providers/{name}/key`
- `POST /api/providers/{name}/test`

Best fit:

- setup flows
- admin consoles
- environment validation

### Streaming API

Use this when the product needs partial output, progress streaming, or live event delivery.

Primary routes:

- `POST /api/agents/{id}/message/stream`
- SSE event types documented in [API Reference](api-reference.md)
- WebSocket protocol documented in [API Reference](api-reference.md)

Best fit:

- live dashboards
- streaming chat
- progress UIs

### OpenAI-Compatible API

Use this when you need the fastest compatibility path for existing OpenAI-style clients.

Primary routes:

- `POST /v1/chat/completions`
- `GET /v1/models`

Best fit:

- thin chat products
- rapid migrations
- client libraries that already assume the OpenAI contract

### MCP And A2A Protocol APIs

Use this when the system should interoperate with tool ecosystems or external agent runtimes.

Primary routes:

- `POST /mcp`
- `GET /api/mcp/servers`
- `GET /.well-known/agent.json`
- `GET /a2a/agents`
- `POST /a2a/tasks/send`

Best fit:

- IDE and tool interoperability
- multi-runtime agent systems
- protocol-driven integrations

## Choose By Product Need

- Choose managed agents for most full-product integrations.
- Choose workflows when orchestration is the core job.
- Choose product-surface APIs when you are building around business modes.
- Choose model and provider routes for setup and admin tooling.
- Choose streaming or WebSocket surfaces when latency and live updates matter.
- Choose OpenAI-compatible access when compatibility matters more than platform-specific features.
- Choose MCP or A2A when integration is protocol-first.

## What The Full Reference Covers

The full [API Reference](api-reference.md) adds:

- authentication behavior
- request and response examples
- system, security, audit, and usage endpoints
- error formats
- endpoint-by-endpoint details across the whole daemon

## Next Step

After choosing an API surface, use [Integrations](integrations.md) to place it inside the right backend boundary and use [Providers And Models](providers-and-models.md) if model or routing decisions still need to be made.
