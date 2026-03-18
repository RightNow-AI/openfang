# OpenFang Next.js Dashboard

Full-stack dashboard for the OpenFang Agent OS.  Runs on port 3002 and proxies all backend calls to the Rust daemon at `http://127.0.0.1:50051`.

The original SDK example routes (`/api/ai/chat`, `/api/session`, etc.) are retained for backward compatibility but are **not** part of the current dashboard architecture.  See [§ Legacy Compatibility Routes](#legacy-compatibility-routes) below.

---

## Quick Start

### 1 — Start the daemon

```bash
# from repo root
cargo build --release -p openfang-cli
GROQ_API_KEY=<your-key> target/release/openfang.exe start
```

Verify: `curl http://127.0.0.1:50051/api/health`

### 2 — Start the frontend

```bash
cd sdk/javascript/examples/nextjs-app-router
cp .env.example .env.local   # edit as needed
npm install
npm run dev -- --port 3002
```

Open `http://localhost:3002`.

---

## Environment Variables

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `OPENFANG_BASE_URL` | yes | `http://127.0.0.1:50051` | Daemon base URL |
| `OPENFANG_API_KEY` | no | — | API key forwarded to daemon |
| `OPENFANG_DEFAULT_TEMPLATE` | no | `assistant` | Default agent template for new spawns |
| `OPENFANG_TIMEOUT_MS` | no | `15000` | Request timeout in ms |
| `NEXT_PUBLIC_OPENFANG_BASE_URL` | no | same as above | Client-side daemon URL (feature flags) |
| `OPENFANG_REQUIRE_DEV_TOKEN` | no | — | Set to a secret string to enable the dev-token guard (see [§ Production](#production-hardening)) |

---

## Scripts

| Command | Purpose |
|---|---|
| `npm run dev` | Start dev server (add `-- --port 3002` to pin port) |
| `npm run build` | Production build |
| `npm start` | Start production server |
| `npm test` | Run Vitest unit tests |
| `npm run test:watch` | Vitest in watch mode |
| `npm run cy:run` | Cypress end-to-end tests (requires running daemon + frontend) |

---

## Architecture

```
Browser
  └─ Next.js pages / Client Components
       └─ lib/api-client.js   (fetch wrapper, base URL from env)
            └─ /api/*  Next.js Route Handlers   (server-side)
                  └─ lib/api-server.js   (authenticated fetch to daemon)
                        └─ Daemon  http://127.0.0.1:50051
                              ├─ /api/agents
                              ├─ /api/skills
                              ├─ /api/templates
                              └─ /api/budget  (etc.)
```

---

## API Routes

### Primary dashboard routes

| Method | Path | Purpose |
|---|---|---|
| GET | `/api/health` | Daemon health passthrough |
| GET | `/api/agents` | List all agents |
| POST | `/api/agents/spawn` | Spawn agent from TOML manifest |
| POST | `/api/agents/preflight` | Pre-spawn skill compatibility check |
| POST | `/api/agents/[id]/chat` | Send message to agent |
| GET | `/api/templates` | List templates |
| GET/PUT | `/api/templates/[name]` | Read / update template |
| GET | `/api/skills` | List installed skills |
| POST | `/api/skills/install` | Install skill from registry |
| PUT | `/api/skills/[name]/enabled` | Enable / disable skill |
| GET | `/api/skills/collisions` | Check tool-name collisions |
| GET | `/api/budget` | Global budget status |
| GET | `/api/budget/agents` | Per-agent cost ranking |
| GET | `/api/runs` | List active runs |

### Legacy compatibility routes

These routes are **not** called by any current dashboard page.  They exist for projects that depend on the original SDK example contract.  A `LEGACY COMPATIBILITY ROUTE` comment is present at the top of each file.

| Method | Path | Notes |
|---|---|---|
| POST | `/api/ai/chat` | Non-streaming JSON chat (legacy) |
| POST | `/api/ai/chat/stream` | SSE streaming chat (legacy) |
| GET | `/api/ai/chat/history` | Conversation history (legacy) |
| GET | `/api/session` | Cookie-based session identity (legacy) |

`lib/auth.js`, `lib/session-store.js`, and `lib/openfang-proxy.js` exist solely to support these routes.  If the compatibility layer is removed, those three files can be deleted together.

---

## Testing

All tests live next to their source files in `__tests__/` subdirectories.

```bash
npm test            # run full suite and exit
npm run test:watch  # live re-run on file change
```

Rules:
- Any new `lib/` file must have a corresponding `lib/__tests__/*.test.js`.
- New route files must have a corresponding `app/api/**/__tests__/*.test.js`.
- The full suite must pass before merging: `npm test`.

---

## Production Hardening

### Dev-Token Guard

The dashboard has no authentication by default.  This is intentional for local development.

**Before deploying to any networked environment**, set `OPENFANG_REQUIRE_DEV_TOKEN` to a strong random secret.  When set, every state-changing request (`POST /api/agents/spawn`, `POST /api/skills/install`, `PUT /api/skills/*/enabled`, `PUT /api/templates/*`) must include the header:

```
X-Dev-Token: <your-secret>
```

Requests without a matching token receive HTTP 401.

```bash
# Example .env.local for a shared test server
OPENFANG_REQUIRE_DEV_TOKEN=replace-with-a-real-secret
```

**This is a single shared secret, not per-user auth.**  It prevents accidental open writes, not a determined attacker.  Replace with OAuth2, JWTs, or per-API-key auth before public-facing deployment.

### Known Limitations

| Item | Status |
|---|---|
| No per-user authentication | Mitigated by `OPENFANG_REQUIRE_DEV_TOKEN` guard; full auth deferred |
| `AgentCatalogClient.js` still ~730 lines | `DetailModal` not extracted; next refactor pass |
| TOML `patchTomlName` is regex-based | Only the write path; read path uses char scanner |
| No HTTPS enforcement in the Next.js layer | Enforce at reverse proxy / load balancer |

---

## Phase Status

| Phase | Description | Status |
|---|---|---|
| 1 | Agent listing, health badge, basic scaffolding | ✅ shipped |
| 2 | Agent detail, preflight, skill binding UI | ✅ shipped |
| 3 | Spawn flow, success banner, collision detection | ✅ shipped |
| 4 | Skills management, ClaWhub browse/install, budget, runs | ✅ shipped |
| 5 | Auth layer, multi-user, production hardening | 🔜 planned |

- restore recent conversation on refresh

## How It Works

- The browser talks to Next.js route handlers only.
- The Next.js route handlers talk to OpenFang.
- The server derives identity from a custom session cookie. The browser sends only the message.
- Agent lookup, spawn, reuse, and persistence stay on the server.
- Streaming is an SSE passthrough normalized into simple `ready`, `text_delta`, `complete`, and `done` events.
- If stream setup fails, the route falls back to non-stream chat instead of leaving the UI stuck.
- If a live stream stalls after it starts, the UI shows a failed assistant turn with retry.
- The JSON session store is example-only. For real app work, move to SQLite or Postgres with `users`, `agent_sessions`, and `conversation_messages` tables.

## Warning

This example is intentionally minimal. It is:

- not production auth
- not production-grade persistence
- not rate limited
- not secret-managed

## Notes

- Set `runtime = "nodejs"` for each route so the OpenFang SDK and server-side fetch APIs run in the Node runtime.
- The generated per-user agent is intentionally minimal: no tools, a small prompt, and `metadata.skip_prompt_builder = true` for low-latency local chat.
- Health states are explicit: `Connected`, `Degraded`, and `Offline`.
- `.data/openfang-sessions.json` is example-only infrastructure. Replace it with SQLite or Postgres before treating this as real app architecture.

## Integration Contract

See [docs/integration-contract.md](../../../docs/integration-contract.md) for the repo's application-facing contract.

## Example request

```bash
curl -X POST http://127.0.0.1:3000/api/ai/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"What can you help me with?"}'
```
