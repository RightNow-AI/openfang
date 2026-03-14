# OpenFang Development Harness

This document describes two architectural pillars that together ensure runtime correctness and zero API drift across the full Rust ↔ TypeScript stack.

---

## Part 1 — Tool Contract & App Adapter Architecture

### Purpose

Every agent action that touches an external service (GitHub, Slack, email, calendar, Notion, etc.) is governed by a **ToolContract**.  A contract declares:

- what the tool does and which app it belongs to (`app_id`)
- its **risk tier** (`ReadOnly` → `WriteLocal` → `WriteExternal` → `Delegation`)
- its **retry policy** and **execution mode** (sync / async / fire-and-forget)
- **verification rules** that are checked against every output
- whether human approval is required before execution

All of this lives in the new `openfang-types` modules:

| File | Contents |
|---|---|
| `crates/openfang-types/src/tool_contract.rs` | `ToolContract`, `RiskTier`, `ToolPermissions`, `PreflightResult`, `ToolEventRecord`, etc. |
| `crates/openfang-types/src/app_adapter.rs` | `AppAdapter` trait, `AdapterResult`, `run_verification()`, starter pack contracts |

### ToolRegistry — `crates/openfang-kernel/src/tool_registry.rs`

`ToolRegistry` is the runtime gatekeeper for all tool execution.

```
agent request
    │
    ▼
ToolRegistry::preflight(tool_name, permissions)
    │── permission check (allowed_tools / forbidden_tools / max_risk_tier)
    │── service health check (is_service_healthy)
    │
    ▼ OK
ToolRegistry::execute(ctx, permissions)
    │── preflight (again, atomically)
    │── dispatch to registered AppAdapter (or NoopAdapter)
    │── run_verification (all VerificationRules from contract)
    │── record ToolEventRecord (audit log)
    │
    ▼
(AdapterResult, ToolEventRecord)
```

#### Key API

```rust
// Access from any route handler:
state.kernel.tool_registry.preflight("github.issue.create", &perms);
state.kernel.tool_registry.execute(&ctx, &perms);

// Permission management (per persona):
state.kernel.tool_registry.set_persona_permissions("coder", perms);
let perms = state.kernel.tool_registry.persona_permissions("coder");

// Service health:
state.kernel.tool_registry.update_health(ServiceHealth { ... });
state.kernel.tool_registry.is_service_healthy("github");
```

### Risk Tier Ordering

```
ReadOnly < WriteLocal < WriteExternal < Delegation
```

`ToolPermissions::max_risk_tier` gates which tier a persona may exercise.  
`write_external_needs_approval` and `delegation_needs_approval` add a second gate for high-risk actions even when the tier is permitted.

### Adding a New App Adapter

1. Create a struct implementing `AppAdapter` in `crates/openfang-hands/` (preferred) or a new crate.
2. Define contracts using `ToolContract` builder (see `github_contracts()` in `app_adapter.rs` for reference).
3. Register at daemon startup:
   ```rust
   kernel.tool_registry.register_adapter(Arc::new(MyAdapter::new()));
   ```
4. The registry auto-links the adapter's contracts via `app_id`.

### Starter Pack Contracts

| App | Tools |
|---|---|
| GitHub | `github.issue.create`, `github.issue.list`, `github.issue.comment`, `github.pr.comment` |
| Slack | `slack.message.send`, `slack.channel.list` |
| Calendar | `calendar.event.schedule` |
| Email | `email.draft.create` (ReadOnly), `email.message.send` (WriteExternal, approval required) |
| Notion | `notion.page.create` |

---

## Part 2 — No-Drift Rust ↔ TypeScript Contract

### The Problem

When an API response shape changes in Rust, the TypeScript frontend can silently start consuming wrong fields.  No compiler catches this.

### The Solution

The canonical source of truth is a **single OpenAPI 3.1 JSON file** generated from Rust types:

```
Rust structs + utoipa derives
         │
         ▼
  openapi.json  (committed to repo root)
         │
         ▼
  openapi-typescript
         │
         ▼
  src/types/api.ts  (committed in Next.js app)
         │
         ▼
  TypeScript compiler enforces correct usage
```

If `openapi.json` or `src/types/api.ts` is out of sync, CI fails with a clear error.

### Schema Coverage

All public API types in `crates/openfang-api/src/types.rs` derive `#[derive(ToSchema)]`.  New types **must** also derive `ToSchema` and be registered in `crates/openfang-api/src/openapi.rs`.

| Type | Endpoint |
|---|---|
| `SpawnRequest` / `SpawnResponse` | `POST /api/agents` |
| `MessageRequest` / `MessageResponse` | `POST /api/agents/{id}/message` |
| `AgentSummary` | `GET /api/agents` |
| `HealthResponse` | `GET /api/health` |
| `BudgetSnapshot` | `GET /api/budget` |
| `AgentUpdateRequest` | `PATCH /api/agents/{id}` |
| `SetModeRequest` | `PUT /api/agents/{id}/mode` |
| `SkillInstallRequest` / `SkillUninstallRequest` | `POST /api/skills/install` etc. |
| `MigrateRequest` / `MigrateScanRequest` | `POST /api/migrate` |
| `ClawHubInstallRequest` | `POST /api/clawhub/install` |
| `AgentMode` (enum) | nested in `SetModeRequest` |

### Endpoints

| Path | Description |
|---|---|
| `GET /api-doc/openapi.json` | Live OpenAPI 3.1 spec (unauthenticated) |

### Generating the Spec

```bash
# Option A — via running daemon
curl http://127.0.0.1:50051/api-doc/openapi.json | python -m json.tool > openapi.json

# Option B — offline, no daemon needed (preferred for CI)
cargo xtask openapi-gen > openapi.json
cargo xtask openapi-gen --out openapi.json
```

### Generating TypeScript Types

```bash
cd sdk/javascript/examples/nextjs-app-router
npm run generate:types        # requires running daemon (port 50051)
npm run generate:types:file   # uses committed openapi.json (offline)
```

Output: `src/types/api.ts` — auto-generated, committed to the repo.

### Drift Enforcement (CI)

```bash
pwsh scripts/check-drift.ps1
```

This script:
1. Runs `cargo xtask openapi-gen` to regenerate `openapi.json`
2. Runs `npm run generate:types:file` to regenerate `src/types/api.ts`
3. Checks `git diff` — if either file changed, exits 1 with instructions

**Rule:** Every PR that changes an API response shape MUST also regenerate and commit `openapi.json` and `src/types/api.ts`.

### Adding a New API Type

1. Define the struct in `crates/openfang-api/src/types.rs` with `#[derive(Serialize, ToSchema)]` (or `Deserialize` for requests).
2. Register it in `crates/openfang-api/src/openapi.rs` under `components(schemas(...))`.
3. Use it in the handler instead of `serde_json::json!({...})`.
4. Run `cargo xtask openapi-gen --out openapi.json` and commit the updated spec.
5. Run `npm run generate:types:file` and commit the updated `api.ts`.

### Architecture Decision Records

| Decision | Rationale |
|---|---|
| `utoipa` for OpenAPI generation | Derives from Rust types — zero manual spec maintenance |
| Spec committed to repo root | CI can check drift without running a server |
| `openapi-typescript` for TS types | Battle-tested, generates idiomatic TypeScript from any OpenAPI 3.x doc |
| `tsconfig.json` with `strict: true` | Catches type mismatches at compile time in the Next.js app |
| Unauthenticated `/api-doc/openapi.json` | Schema is a public contract; it contains no secrets |
| `xtask` binary for offline gen | Runs in CI without a daemon; no external HTTP calls |

---

## Invariants

These invariants are enforced by CI and must not be broken:

1. **Every public API response struct derives `ToSchema`.**
2. **Every `ToSchema` struct is registered in `openapi.rs`'s `components(schemas(...))`.**
3. **`openapi.json` at repo root is always in sync with the Rust source.**
4. **`src/types/api.ts` is always in sync with `openapi.json`.**
5. **No handler returns `serde_json::json!({...})` for a typed response that has a registered schema.**  (Violation = silent drift.)
6. **`ToolContract` is required for every tool that touches an external service.  No bare HTTP calls or shell execs from agent loops.**
7. **`ToolRegistry::preflight()` is called before every `execute()`.  Never bypass it.**
