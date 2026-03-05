# Integration Platform Roadmap

OpenFang already has strong integration primitives (`openfang-extensions`, MCP, tool runner, channel adapters). This roadmap defines how to scale from "many adapters" to a structured integration platform for diverse external apps such as YouTube, Facebook, DaVinci Resolve, Houdini, and Blender.

---

## Goals

- Expand supported application integrations without increasing architecture coupling.
- Standardize connector behavior for auth, action execution, retries, rate limits, and auditing.
- Enable production-safe automation for both cloud APIs and local creative tools.
- Reduce integration delivery time from weeks to days with reusable contracts and test kits.

---

## Target Architecture

### Module Ownership

- `openfang-types`
  - Own connector contracts and versioned schemas.
  - Define integration event envelope and compatibility rules.
- `openfang-extensions`
  - Own integration registry, auth lifecycle (API key/OAuth), health, and install state.
  - Provide connector metadata and capability declarations.
- `openfang-runtime`
  - Own tool execution policy engine (quota/timeout/retry/audit).
  - Execute connector actions through a single policy pipeline.
- `openfang-kernel`
  - Own orchestration and governance (capabilities, approvals, budgets).
  - Route integration events into workflows/triggers.
- `openfang-api`
  - Expose versioned integration APIs (`/api/v1/integrations/*`).
  - Enforce stream backpressure policy for integration-heavy SSE/WS usage.
- `openfang-memory`
  - Persist connector credentials metadata, sync state, event cursors, and idempotency keys.
- `openfang-channels`
  - Remain focused on messaging channels, not generic app integrations.

---

## Connector Classes

1. Cloud API Connectors
- Example: YouTube, Facebook.
- Mode: OAuth/API key + HTTP APIs + webhooks/polling.

2. Desktop App Connectors
- Example: DaVinci Resolve, Blender, Houdini.
- Mode: local bridge (Python/plugin) + secure local IPC.

3. Automation Bridge Connectors
- Example: browser/CLI/script workflows for systems with weak APIs.
- Mode: controlled automation under strict approval and sandbox policy.

---

## Unified Connector Contract (v1)

Each connector must define:

- `id`: stable connector slug.
- `version`: connector schema version.
- `auth`: `api_key | oauth2 | local_bridge`.
- `capabilities`: list of actions/events.
- `actions`: typed input/output schemas for executable operations.
- `events`: webhook/polling event schemas with cursor strategy.
- `rate_limits`: default per-provider throttling profile.
- `retry_policy`: retryable error classes and max attempts.
- `timeouts`: connect/read/overall action deadlines.
- `audit_tags`: security and cost tags attached to each execution.
- `approval_policy`: high-risk action gates (`publish`, `delete`, `pay`, `render_farm`).

Event envelope for kernel/runtime:

```json
{
  "schema": "openfang.integration.event",
  "version": "1.0",
  "connector_id": "youtube",
  "event_type": "video.published",
  "occurred_at": "2026-03-03T10:00:00Z",
  "payload": {}
}
```

---

## Delivery Process for New Integrations

1. Problem framing (1-pager)
- Goal, user flow, modules touched, KPI targets.

2. RFC + contract design
- `openfang-types` schemas first.
- Backward compatibility and migration notes required.

3. Vertical slice implementation
- `extensions -> runtime policy -> kernel orchestration -> api -> ui/sdk`.

4. Test gates
- Unit tests (connector logic).
- Integration tests (mock provider/local bridge).
- Contract tests (API/dashboard/SDK).
- Load smoke (stream and concurrent actions).

5. Safe rollout
- Feature flag + staged rollout.
- Runtime kill switch per connector.

---

## Wave Roadmap (First 90 Days)

### Wave 1 (Days 1-30): Platform Base + Social APIs

- Build connector contract v1 and shared integration API surface.
- Ship YouTube connector MVP.
- Ship Facebook connector MVP.
- Add connector conformance test kit.

### Wave 2 (Days 31-60): Creative Desktop Bridges

- Ship Blender local bridge MVP.
- Ship DaVinci Resolve local bridge MVP.
- Add local bridge supervisor + health checks.

### Wave 3 (Days 61-90): Production Hardening + Houdini

- Ship Houdini local bridge MVP.
- Add reliability controls: idempotency, dead-letter queue, replayable failure logs.
- Add governance policies for high-cost render/publish actions.

---

## Initial Connector Specs

### YouTube Connector (MVP)

- Auth: OAuth2 (channel scope).
- Core actions:
  - `youtube.video.upload`
  - `youtube.video.update_metadata`
  - `youtube.video.list`
  - `youtube.comment.list`
  - `youtube.comment.reply`
- Events:
  - `youtube.video.published`
  - `youtube.comment.created`
- Approval gates:
  - publish/update metadata (optional per org policy).
- KPI:
  - upload success rate >= 99%
  - p95 action latency <= 3s (excluding large file transfer)

### Facebook Connector (MVP)

- Auth: Meta Graph API (page token + app setup).
- Core actions:
  - `facebook.page.post_create`
  - `facebook.page.post_delete`
  - `facebook.page.comment_reply`
  - `facebook.page.insights.get`
- Events:
  - `facebook.page.message_received`
  - `facebook.page.comment_created`
- Approval gates:
  - post delete and bulk publish.
- KPI:
  - publish success rate >= 99%
  - webhook processing failure <= 0.5%

### Blender Connector (MVP)

- Auth: local bridge trust + optional local token.
- Core actions:
  - `blender.scene.open`
  - `blender.scene.render`
  - `blender.asset.import`
  - `blender.project.save`
- Events:
  - `blender.render.completed`
  - `blender.render.failed`
- Approval gates:
  - long render jobs, external file writes.
- KPI:
  - render task completion >= 98%
  - bridge crash recovery <= 10s

### DaVinci Resolve Connector (MVP)

- Auth: local bridge to Resolve scripting API.
- Core actions:
  - `resolve.project.open`
  - `resolve.timeline.render`
  - `resolve.media.import`
  - `resolve.export.start`
- Events:
  - `resolve.render.completed`
  - `resolve.export.failed`
- Approval gates:
  - export to external destinations.
- KPI:
  - export completion >= 98%
  - action timeout violation <= 1%

### Houdini Connector (MVP)

- Auth: local bridge using Python API/hython worker.
- Core actions:
  - `houdini.hip.open`
  - `houdini.node.cook`
  - `houdini.cache.generate`
  - `houdini.render.submit`
- Events:
  - `houdini.cook.completed`
  - `houdini.render.failed`
- Approval gates:
  - farm submit and high-cost simulations.
- KPI:
  - job success >= 97%
  - deterministic replay coverage for failed jobs >= 90%

---

## Optimization Track (Run in Parallel)

- Runtime:
  - Centralize connector action execution in one policy engine.
  - Deterministic replay for incident forensics.
- API:
  - `/api/v1` contracts + stream session caps + timeout policy.
- Kernel:
  - Versioned internal events to reduce feature side effects.
- Memory:
  - Cursor/index optimization for high-volume connector events.

---

## Success Metrics

- Time-to-integrate a new connector reduced by at least 40%.
- Connector production incident rate below 1.0% per 10k actions.
- No P0 security incidents from integration pathways.
- 100% of connector actions audited with policy metadata.
