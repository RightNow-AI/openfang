<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# API Server Source Code

## Purpose
HTTP/WebSocket API server that boots the OpenFang kernel, bridges kernel to routes, handles authentication, rate limiting, channel bridging (Telegram, etc.), and serves the Alpine.js dashboard.

## Key Files
| File | Purpose |
|------|---------|
| `server.rs` | Daemon lifecycle, router construction, middleware stack, CORS, compression, tracing, daemon info file |
| `routes.rs` | REST endpoints: `/api/agents`, `/api/messages`, `/api/budget`, `/api/skills`, `/api/workflows`, `/api/a2a/*`, OpenAI-compatible chat completions |
| `ws.rs` | WebSocket upgrade, message streaming, subscription management, channel broadcasts |
| `middleware.rs` | Request/response logging, tracing span injection |
| `session_auth.rs` | Session-based auth (username/password) with secure cookies |
| `channel_bridge.rs` | Telegram, Discord, Slack, and other channel integrations |
| `openai_compat.rs` | OpenAI chat completions API compatibility layer |
| `rate_limiter.rs` | Token bucket rate limiting per API key |
| `stream_chunker.rs` | SSE chunk encoding for streaming responses |
| `stream_dedup.rs` | De-duplicate streaming events |
| `webchat.rs` | Web chat session management |
| `types.rs` | API request/response types |
| `lib.rs` | Module exports |

## Subdirectories
None — all route logic in `routes.rs` (large file by design for single-source-of-truth endpoint mapping).

## For AI Agents
When adding new API endpoints:
1. Add route handler in `routes.rs` and register in `server.rs` router
2. Ensure kernel state access via `AppState` (e.g., `state.kernel.agent_manager()`)
3. Return JSON responses with appropriate status codes
4. Add authentication check if needed (API key or session)
5. Test live integration: start daemon, send curl requests, verify data persists
6. If endpoint returns data from config/database, verify it's deserialized and accessible (common bugs)
7. Dashboard HTML/JS must match endpoint path and response shape
