<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# JavaScript SDK — Official OpenFang REST Client

## Purpose

Official JavaScript/TypeScript client library for the OpenFang Agent OS REST API. Provides type-safe async methods for agent management, messaging, memory, workflows, and all kernel operations.

## Key Files

| File | Purpose |
|------|---------|
| `package.json` | NPM metadata — `@openfang/sdk` package, Node 18+ requirement |
| `index.js` | Main client class — `OpenFang` constructor, resource accessors |
| `index.d.ts` | TypeScript type definitions — full type coverage for all resources |

## For AI Agents

**When to read:** Understand the JavaScript SDK API, how to interact with OpenFang from Node.js/TypeScript, or extending the SDK.

**Key classes:**
- `OpenFang` — main client (takes baseUrl, optional headers)
- `AgentResource` — agent lifecycle (create, list, message, stream)
- `SessionResource` — session management
- `WorkflowResource` — workflow orchestration
- `SkillResource` — skill management
- `ChannelResource` — channel configuration
- `ToolResource` — tool management
- `ModelResource` — model info
- `ProviderResource` — LLM provider config
- `MemoryResource` — agent memory access
- `TriggerResource` — trigger management
- `ScheduleResource` — cron/schedule management

**Usage example:**
```javascript
const { OpenFang } = require("@openfang/sdk");
const client = new OpenFang("http://localhost:3000");

const agent = await client.agents.create({ template: "assistant" });
const reply = await client.agents.message(agent.id, "Hello!");

// Streaming
for await (const event of client.agents.stream(agent.id, "Tell me a joke")) {
  process.stdout.write(event.delta || "");
}
```

**Architecture note:** Lightweight wrapper over REST API — all state lives in the OpenFang kernel, client is stateless.
