# Integrations

Integrations are the ways LegendClaw connects to application backends, SDKs, external tool systems, and other agent frameworks. This page is the top-level entry point for deciding which contract to use before you wire code against the platform.

---

## Start Here

- Read [Integration Contract](integration-contract.md) for the stable application-facing boundary.
- Read [API Surfaces](api-surfaces.md) to choose the right route family or protocol first.
- Read [Providers And Models](providers-and-models.md) if model access is part of the integration decision.
- Read [MCP & A2A](mcp-a2a.md) for protocol-level interoperability with tools and external agents.

## Integration Paths

### App Backend Integration

Use this when you are building a product backend that should own auth, tenancy, rate limits, request logging, and agent lifecycle management.

Best fit:

- SaaS backends
- internal tools
- mobile backends
- server-side web applications

Primary references:

- [Integration Contract](integration-contract.md)
- [API Surfaces](api-surfaces.md)

### OpenAI-Compatible Integration

Use this when you need the fastest path to integrate existing chat clients, SDKs, or prompt tooling that already speaks the OpenAI chat contract.

Best fit:

- thin chat products
- quick compatibility layers
- migration paths from OpenAI-style clients

Primary references:

- [Integration Contract](integration-contract.md)
- [API Surfaces](api-surfaces.md)

### Managed Agent Integration

Use this when you need durable agents with memory, tools, streaming, and role-specific manifests instead of stateless chat completions.

Best fit:

- product-specific agent backends
- long-running or stateful sessions
- specialist templates with tool access

Primary references:

- [Agent Templates](agent-templates.md)
- [API Surfaces](api-surfaces.md)
- [Security](security.md)

### MCP Integration

Use MCP when you want LegendClaw to either consume external tool servers or expose its own capabilities to MCP clients such as IDEs and desktop agent tools.

Best fit:

- IDE integrations
- tool federation
- shared tool ecosystems across agent systems

Primary references:

- [MCP & A2A](mcp-a2a.md)
- [Configuration](configuration.md)

### A2A Integration

Use A2A when the system should exchange tasks with external agent runtimes instead of only calling tools.

Best fit:

- agent-to-agent delegation
- multi-runtime ecosystems
- task handoff between services

Primary references:

- [MCP & A2A](mcp-a2a.md)
- [Integration Contract](integration-contract.md)

### Gateway And Channel Bridge Integration

Use this when your own service should act as a trusted adapter between end-user surfaces and LegendClaw.

Best fit:

- custom web apps
- WhatsApp or messaging gateways
- internal proxies and policy layers

Primary references:

- [Integration Contract](integration-contract.md)
- [Channels](channels.md)

## Sharper Routing Guides

- [API Surfaces](api-surfaces.md): choose between managed agents, workflows, streaming, OpenAI-compatible, provider admin, and protocol endpoints
- [Providers And Models](providers-and-models.md): choose the provider strategy before reading the full provider catalog

## Recommended Architecture

Prefer a backend-owned integration shape:

```text
Frontend or channel
  -> your backend or gateway
      -> LegendClaw
          -> providers, tools, memory, workflows
```

This keeps the trust boundary out of the browser and centralizes auth, audit, tenant mapping, and fallback behavior.

## How To Choose An Integration Path

- Choose app backend integration for most production products.
- Choose OpenAI-compatible access for fast compatibility or migration.
- Choose managed agents when you need durable identity, tools, and memory.
- Choose MCP when the main job is tool interoperability.
- Choose A2A when the main job is task exchange with external agents.
- Choose gateway integration when your own service must own the policy layer.

## Validation Checklist

- Pick the API contract before writing frontend code.
- Keep provider keys and tenant logic in your backend.
- Prefer agent templates over hardcoded prompt assembly.
- Validate streaming, auth, and failure behavior early.
- Check [Production Checklist](production-checklist.md) before external rollout.

## Next Step

After choosing an integration path, use [Business Modes](business-modes.md) to choose the operator surface, [API Surfaces](api-surfaces.md) to lock the route family, and [Channels](channels.md) to decide where users or systems will interact with it.
