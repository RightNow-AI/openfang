# Next.js App Router Example

This example shows the same backend-owned integration pattern as `backend-proxy-server.js`, but in Next.js App Router route handlers.

It also includes a minimal client page so a developer can run the example, type one prompt, and watch a streamed reply render in the browser.

## Contract

```text
Frontend -> Next.js route handlers -> OpenFang
```

The example exposes:

- `GET /api/session`
- `POST /api/ai/chat`
- `POST /api/ai/chat/stream`
- `GET /api/ai/chat/history`
- `GET /api/health`

It now persists example infrastructure in a local JSON store under `.data/openfang-sessions.json` with three collections:

- `users`
- `agent_sessions`
- `conversation_messages`

That file-backed store is example-only. Do not copy `.data/openfang-sessions.json` into production.

## Files

- `app/page.js`: minimal client page with health badge, textarea, send button, and streamed response area
- `lib/env.js`: centralized config validation for OpenFang environment variables
- `lib/auth.js`: custom session-cookie identity example
- `lib/openfang-proxy.js`: shared OpenFang client and per-user agent lifecycle helper
- `lib/session-store.js`: example-only JSON persistence for users, agent sessions, and conversation history
- `app/api/ai/chat/route.js`: non-streaming JSON chat route
- `app/api/ai/chat/stream/route.js`: SSE streaming route
- `app/api/ai/chat/history/route.js`: recent conversation history for the current server-derived user
- `app/api/session/route.js`: derives the current user identity from the server-side session cookie
- `app/api/health/route.js`: backend health route
- `.env.example`: required environment variables

## Install

```bash
cd sdk/javascript/examples/nextjs-app-router
npm install
```

This folder now includes its own `package.json`, so you can run it directly from here.

It now also includes the standard Next.js app files you would expect from a small `create-next-app` scaffold: `app/layout.js`, `app/globals.css`, `next.config.mjs`, and `jsconfig.json`.

Copy `.env.example` to `.env.local`.

## Environment

```bash
OPENFANG_BASE_URL=http://127.0.0.1:50051
OPENFANG_API_KEY=replace-me
OPENFANG_DEFAULT_TEMPLATE=assistant
OPENFANG_TIMEOUT_MS=15000
```

## Run

```bash
cd sdk/javascript/examples/nextjs-app-router
npm run dev
```

The bundled scripts force `--no-webstorage` for Node. That avoids a Node 25 runtime issue on some Windows setups where server-side `localStorage` is exposed in a broken state and crashes Next.js dev rendering.

Then open `http://127.0.0.1:3000`, type a prompt, and the page will:

- call `/api/health` on load
- derive user identity from `/api/session`
- load recent turns from `/api/ai/chat/history`
- post to `/api/ai/chat/stream` when you click Send
- append streamed chunks live
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
