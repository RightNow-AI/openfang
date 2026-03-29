<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-api

## Purpose
HTTP/WebSocket API server for the OpenFang Agent OS daemon. Exposes agent management, chat, and status endpoints via REST and WebSocket. Serves the Alpine.js dashboard SPA, handles rate limiting, session authentication, and bridges external channels (Telegram, WhatsApp, Discord). The kernel runs in-process; the CLI and web UI connect over HTTP.

## Key Files
| File | Description |
|------|-------------|
| `src/server.rs` | Daemon lifecycle, router setup, CORS config, listener initialization |
| `src/routes.rs` | All REST endpoints (agents, budget, config, LLM, A2A, network, etc.) |
| `src/ws.rs` | WebSocket handler for real-time agent chat and streaming responses |
| `src/channel_bridge.rs` | Telegram, WhatsApp, Discord channel integrations |
| `src/middleware.rs` | Request/response middleware (logging, auth, compression) |
| `src/session_auth.rs` | Session and API key validation |
| `src/openai_compat.rs` | OpenAI-compatible `/v1/chat/completions` wrapper for agent inference |
| `src/webchat.rs` | Web-based chat widget initialization and session setup |
| `src/rate_limiter.rs` | Token bucket rate limiting for API endpoints |
| `src/stream_chunker.rs` | Chunks LLM streaming responses for SSE delivery |
| `src/stream_dedup.rs` | Deduplicates streaming chunks to prevent client-side jitter |
| `src/types.rs` | API-specific request/response types |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Server logic, routes, middleware, WebSocket handler |
| `static/` | Alpine.js dashboard SPA, HTML, CSS, JavaScript |
| `tests/` | Integration tests for API endpoints and websocket |

## For AI Agents

### Working In This Directory
- Edit `src/routes.rs` to add or modify REST endpoints
- Edit `src/ws.rs` to change WebSocket chat protocol
- Edit `src/server.rs` to change listener config, CORS, middleware stack
- New routes must be registered in `server.rs` router AND implemented in `routes.rs`
- Dashboard UI lives in `static/index_body.html` — new tabs need both HTML markup and JS handlers in `routes.rs` (via `/api/dashboard/...` endpoints)
- Config fields added to `KernelConfig` must also be added to the Default impl in `openfang-types/src/config.rs`
- WebSocket messages use `serde_json` — ensure serialization roundtrips match `openfang_types` definitions
- Rate limiter uses token bucket algorithm — configure limits in `AppState` constructor

### Testing Requirements
- Run `cargo build --workspace --lib && cargo test --workspace` after changes
- For new endpoints: run live integration tests (see CLAUDE.md)
  - Start daemon: `GROQ_API_KEY=... target/release/openfang.exe start &`
  - Test endpoint: `curl -s http://127.0.0.1:4200/api/<endpoint>`
  - Verify response shape matches OpenFang types
- For WebSocket: `curl -i -N -H "Connection: Upgrade" -H "Upgrade: websocket" -H "Sec-WebSocket-Key: ..." http://127.0.0.1:4200/ws/<agent-id>`
- Dashboard changes: open `http://127.0.0.1:4200/` in browser and test new UI element

### Common Patterns
- `AppState` holds shared `Arc<OpenFangKernel>` — clone freely for async tasks
- Routes use `Path<AgentId>`, `Query<Params>`, `Json<Body>` extractors
- Streaming responses use `axum::response::sse::Sse` (server-sent events) for real-time updates
- WebSocket uses `tokio_tungstenite` for bidirectional messages
- Rate limiting via `governor::RateLimiter` on per-endpoint basis
- Auth checks: verify session token or API key in `session_auth.rs` before accessing protected resources
- Dashboard data endpoints should return minimal JSON (avoid full agent state dump) — UI is thin

## Dependencies

### Internal
- `openfang-kernel` — orchestrator and subsystems
- `openfang-runtime` — agent execution loop and LLM drivers
- `openfang-types` — shared types
- `openfang-memory` — memory substrate
- `openfang-channels` — channel integrations (Telegram, etc.)
- `openfang-wire` — wire protocol serialization
- `openfang-skills` — plugin skills registry
- `openfang-hands` — web/browser automation tools
- `openfang-extensions` — extension system
- `openfang-migrate` — database migrations

### External
- `axum` — HTTP framework and router
- `tower-http` — middleware (compression, CORS, tracing)
- `tokio` — async runtime
- `tokio-tungstenite` — WebSocket
- `serde_json` — JSON serialization
- `reqwest` — HTTP client for webhooks
- `governor` — rate limiting
- `chrono`, `uuid` — time and ID generation

<!-- MANUAL: -->
