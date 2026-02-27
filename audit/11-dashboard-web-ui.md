# Dimension 11 — Dashboard & Web UI

**Auditor:** Copilot  
**Date:** 2025-07-15  
**Scope:** Embedded SPA dashboard, REST API structure, security headers, CORS, authentication, rate limiting, input validation, XSS surface

---

## Summary

OpenFang ships a **well-secured embedded dashboard** compiled directly into the binary via `include_str!()` / `include_bytes!()`. The Alpine.js SPA includes robust security headers (CSP, X-Frame-Options, X-Content-Type-Options, Referrer-Policy), cost-aware GCRA rate limiting, bearer token auth with constant-time comparison, per-IP WebSocket limits, loopback-only fallback when no API key is configured, and consistent JSON error responses. User text is escaped via `escapeHtml()` before rendering, and agent/system messages go through `marked.parse()` for Markdown. The main concerns are: `x-html` usage with `marked.parse()` output (no DOMPurify sanitization), the CSP allows `'unsafe-inline' 'unsafe-eval'` for scripts, WebSocket auth uses direct string comparison instead of constant-time comparison, and several auth-exempt endpoints expose data without authentication when `api_key` is set.

**Overall Grade: A-** — Strong security posture with a few XSS hardening and auth consistency gaps.

---

## Findings

### UI1 — Markdown Rendering via `x-html` Without DOMPurify Sanitization
**Severity:** Medium  
**Location:** `crates/openfang-api/static/index_body.html:595`, `crates/openfang-api/static/js/app.js:24-33`

Agent and system messages are rendered with `marked.parse(text)` and injected via Alpine.js `x-html` directive. While user messages are safely escaped with `escapeHtml()`, the `marked.parse()` output is raw HTML. If an LLM response (or a tool output relayed through the agent) contains malicious HTML/JS, it will be rendered in the browser.

The `marked` library does not sanitize output by default — it faithfully converts Markdown to HTML, including raw HTML blocks. An attacker who can influence agent output (e.g., via prompt injection or a malicious MCP tool response) could inject `<script>` tags or event handlers.

The CSP `script-src 'unsafe-inline' 'unsafe-eval'` (see UI2) makes this exploitable.

**Recommendation:** Add DOMPurify (or a similar HTML sanitizer) and wrap `marked.parse()` output: `DOMPurify.sanitize(marked.parse(text))`. Alternatively, configure `marked` with a custom renderer that strips dangerous tags.

---

### UI2 — CSP Allows `'unsafe-inline'` and `'unsafe-eval'` for Scripts
**Severity:** Medium  
**Location:** `crates/openfang-api/src/middleware.rs:160-163`

The Content-Security-Policy `script-src` directive includes both `'unsafe-inline'` and `'unsafe-eval'`. While `'unsafe-inline'` is needed because all JS is bundled inline via `include_str!()`, `'unsafe-eval'` is a significant weakening — it allows `eval()`, `Function()`, and similar dynamic code execution, which amplifies any XSS vector.

Alpine.js does use `eval` internally for `x-data` expressions, making `'unsafe-eval'` currently necessary for the framework to function. This is an inherent trade-off of using Alpine.js with an inline-bundled SPA.

**Recommendation:** If Alpine.js is kept, `'unsafe-eval'` is unavoidable. Consider adding nonce-based script loading in a future refactor, or evaluate CSP-compatible Alpine.js alternatives (Alpine.js v3 has a `@alpinejs/csp` build that avoids `eval`). At minimum, ensure DOMPurify sanitization (UI1) is added to neutralize the `'unsafe-eval'` risk.

---

### UI3 — WebSocket Auth Uses Direct String Comparison (Not Constant-Time)
**Severity:** Low  
**Location:** `crates/openfang-api/src/ws.rs:151-156`

The HTTP middleware in `middleware.rs:105-127` correctly uses `subtle::ConstantTimeEq` for API key comparison to prevent timing attacks. However, the WebSocket upgrade handler in `ws.rs:151-156` uses a direct `token == api_key` comparison, which is vulnerable to timing side-channels.

```rust
// ws.rs:156 — direct comparison
.map(|token| token == api_key)

// middleware.rs:110 — constant-time comparison
token.as_bytes().ct_eq(api_key.as_bytes()).into()
```

**Recommendation:** Replace the direct comparison in `ws.rs` with `subtle::ConstantTimeEq` to match the middleware's security level.

---

### UI4 — Auth-Exempt Endpoints Expose Operational Data
**Severity:** Low  
**Location:** `crates/openfang-api/src/middleware.rs:83-94`

When `api_key` is configured, the auth middleware exempts several endpoints from authentication:

- `/` (dashboard)
- `/api/health`, `/api/health/detail`, `/api/status`, `/api/version`
- `/api/agents` (full agent list with names, models, profiles)
- `/api/profiles` (profile list)
- `/api/config` (full runtime config)
- `/api/uploads/*` (uploaded files)

The `/api/agents`, `/api/config`, and `/api/uploads/*` exemptions are concerning. An unauthenticated user can enumerate all agents (including model/provider info), read the runtime configuration, and access any uploaded file by ID. The agent list and config reveal architectural details that should be auth-gated.

**Recommendation:** Remove `/api/agents`, `/api/config`, and `/api/uploads/*` from the auth-exempt list. If the dashboard needs these without auth, consider a session cookie flow or a separate public-safe subset of data.

---

### UI5 — Default Bind Address Is Loopback (Safe), But `0.0.0.0` Is Documented
**Severity:** Low  
**Location:** `crates/openfang-types/src/config.rs:1177`, `openfang.toml.example:6`

The default `api_listen` is `"127.0.0.1:50051"` (loopback-only), and the daemon's `run_daemon()` generates config defaults with `"127.0.0.1:4200"`. This is the correct secure default. The example config comments mention `# use 0.0.0.0 for public` without warning about the security implications.

When no `api_key` is set, the auth middleware restricts to loopback connections, which is a good defense-in-depth. However, if a user sets `api_listen = "0.0.0.0:4200"` without also setting `api_key`, the loopback check in the middleware is the only protection — and it correctly blocks remote access.

**Recommendation:** Add a startup warning log when `api_listen` binds to a non-loopback address without `api_key` set. Consider adding a comment in the example config: `# WARNING: 0.0.0.0 exposes to all interfaces — always set api_key for non-loopback`.

---

### UI6 — Dashboard Is Not Feature-Gated
**Severity:** Low  
**Location:** `crates/openfang-api/Cargo.toml`, `crates/openfang-api/src/webchat.rs`

The embedded dashboard (HTML, CSS, JS, Alpine.js, marked.js, highlight.js, logo, favicon) is always compiled into the binary via `include_str!()` and `include_bytes!()`. There is no Cargo feature flag to build a headless API-only binary. The SPA is approximately 300KB+ of embedded assets.

For production deployments that only use the CLI or API programmatically (e.g., in a container), the dashboard adds unnecessary binary size. A `dashboard` feature gate would allow opt-out.

**Recommendation:** Add a `dashboard` feature flag to `openfang-api` that conditionally compiles the webchat module and its static assets. Default to `dashboard` enabled.

---

### UI7 — `config_set` Endpoint Allows Arbitrary Config Key Writes
**Severity:** Medium  
**Location:** `crates/openfang-api/src/routes.rs:7947-8020`

The `POST /api/config/set` endpoint accepts a `path` (dotted key) and `value` and writes them directly to `config.toml`. While it limits path depth to 3 levels, it does not validate the key name against a whitelist. An attacker with API access could write arbitrary keys to the config file, potentially injecting unexpected configuration values that could be parsed by future code.

The endpoint is behind auth, but given the auth-exempt list (UI4), the combination is worth noting.

**Recommendation:** Validate `path` against a whitelist of known config keys, or at minimum reject paths that don't correspond to known `OpenFangConfig` fields.

---

### UI8 — CORS `allow_methods` and `allow_headers` Use `Any`
**Severity:** Low  
**Location:** `crates/openfang-api/src/server.rs:76-77, 99-100`

Both CORS branches (with and without API key) restrict origins to localhost addresses — this is good. However, both use `allow_methods(tower_http::cors::Any)` and `allow_headers(tower_http::cors::Any)`, which permits all HTTP methods and headers. While origins are restricted, explicit method/header whitelists provide defense-in-depth.

**Recommendation:** Replace `Any` methods with an explicit list: `GET, POST, PUT, PATCH, DELETE, OPTIONS`. Replace `Any` headers with `Authorization, Content-Type, X-Request-Id, X-Filename`.

---

## Strengths

1. **Comprehensive security headers middleware** — CSP, X-Frame-Options (DENY), X-Content-Type-Options (nosniff), X-XSS-Protection, Referrer-Policy, and Cache-Control (no-store) are applied to all responses via a single middleware layer (`middleware.rs:152-174`).

2. **Cost-aware GCRA rate limiting** — The rate limiter assigns weighted costs per operation type (health=1, spawn=50, message=30, workflow_run=100) with a 500-token-per-minute budget per IP. This prevents both brute-force and resource-exhaustion attacks while allowing normal dashboard usage (`rate_limiter.rs`).

3. **Constant-time API key comparison** — The HTTP auth middleware uses `subtle::ConstantTimeEq` for bearer token validation, preventing timing side-channel attacks (`middleware.rs:105-127`).

4. **Loopback-only fallback** — When no `api_key` is configured, the auth middleware restricts all requests to loopback IP addresses using `ConnectInfo<SocketAddr>`, preventing accidental exposure to the network (`middleware.rs:56-79`).

5. **Per-IP WebSocket connection limits** — WebSocket connections are tracked per IP with a maximum of 5 concurrent connections and a 30-minute idle timeout, preventing connection exhaustion (`ws.rs:37-129`).

6. **Compile-time SPA embedding** — All dashboard assets are embedded via `include_str!()` / `include_bytes!()` at compile time, eliminating filesystem dependencies and path traversal risks. Vendor libraries (Alpine.js, marked.js, highlight.js) are bundled locally with no CDN dependency (`webchat.rs:72-132`).

7. **Input validation on mutation endpoints** — The `spawn_agent` handler rejects manifests >1MB, `send_message` rejects messages >64KB, `upload_file` validates content types against an allowlist and checks file sizes, and attachment file IDs are validated as UUIDs to prevent path traversal (`routes.rs:44-46, 247-253, 7532-7556, 170-173`).

8. **Structured JSON error responses** — All error responses consistently return `{"error": "..."}` JSON objects with appropriate HTTP status codes (400, 401, 403, 404, 413, 429, 500), making client-side error handling predictable.

9. **Well-separated API crate** — `openfang-api` is a dedicated implementation crate with clear module separation (server, routes, middleware, rate_limiter, webchat, ws, types), following the workspace's crate-per-concern architecture.

10. **User text HTML escaping** — User-authored messages use `escapeHtml()` (DOM-based `textContent→innerHTML` encoding) before rendering, correctly preventing XSS from user input. Only agent/system Markdown output bypasses this via `marked.parse()` (`app.js:18-22`, `index_body.html:595`).
