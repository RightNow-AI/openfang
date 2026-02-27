# Dimension 13 — Security & Input Validation

**Auditor:** Copilot  
**Date:** 2025-07-15  
**Scope:** API key handling, path traversal, body size limits, URL safety, debug output, sensitive data in logs  
**Key paths examined:**  
- `crates/openfang-runtime/src/drivers/` (LLM providers, API keys)  
- `crates/openfang-memory/src/` (file-based storage)  
- `crates/openfang-api/src/` (request handling, input validation)  
- `crates/openfang-extensions/src/` (credential vault)  
- `crates/openfang-wire/src/` (P2P protocol security)  
- `crates/openfang-types/src/config.rs` (config structs, Debug impls)

---

## Summary

The codebase demonstrates **strong security awareness** across most attack surfaces. API keys are wrapped in `Zeroizing<String>`, the `KernelConfig` and `AuthProfile` types have custom `Debug` impls that redact secrets, SSRF protection is comprehensive with DNS resolution checks, and the credential vault uses industry-standard AES-256-GCM + Argon2id. Path traversal is prevented via canonicalize + starts_with checks and filename whitelisting. Authentication uses constant-time comparison via the `subtle` crate.

However, there are a few areas where hardening is incomplete: no global request body size limit on the Axum router, an embedding provider name used directly in URL construction, and the ElevenLabs `voice_id` interpolated into a URL without validation. These are low-to-medium severity since they require specific attack scenarios, but they represent gaps in an otherwise well-defended system.

**Risk level:** Low overall — findings are edge cases, not systemic weaknesses.

---

## Findings

### S1 — No global `DefaultBodyLimit` on Axum router (Medium)

**File:** `crates/openfang-api/src/server.rs`

The router has no `DefaultBodyLimit` layer. Axum defaults to 2 MB for `Json<T>` extractors, but raw `Bytes` extractors (used by `upload_file`) have no framework-level limit. While `upload_file` manually checks `body.len() > MAX_UPLOAD_SIZE` (10 MB), this check happens **after** the entire body is buffered in memory. A malicious client could send a multi-gigabyte request body and exhaust memory before the size check runs.

**Recommendation:** Add `.layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024))` to the router to enforce a global cap at the framework level, before body buffering.

---

### S2 — Embedding provider name interpolated into URL (Low)

**File:** `crates/openfang-runtime/src/embedding.rs:200`

```rust
other => {
    warn!("Unknown embedding provider '{other}', using OpenAI-compatible format");
    format!("https://{other}/v1")
}
```

When the embedding provider is not a known name (`openai`, `ollama`, etc.), the raw provider string from config is interpolated into a URL. If an attacker can control the config (e.g., via `/api/config/set`), they could set the provider to `evil.com/steal?x=` to redirect embedding requests to an attacker-controlled server. The SSRF protections in `web_fetch.rs` do **not** apply here since this is a direct `reqwest` call, not routed through the web fetch pipeline.

**Recommendation:** Validate the provider string against a URL-safe pattern (alphanumeric + dots + hyphens only) before URL interpolation, or require the full base URL to be specified explicitly via config.

---

### S3 — ElevenLabs `voice_id` interpolated into URL without validation (Low)

**File:** `crates/openfang-runtime/src/tts.rs:164`

```rust
let voice_id = voice_override.unwrap_or(&self.config.elevenlabs.voice_id);
let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}", voice_id);
```

The `voice_id` parameter (from config or per-request override) is interpolated directly into the API URL. If a user can supply an arbitrary `voice_override` via an agent tool call, they could inject path segments (e.g., `../admin`) to reach unintended ElevenLabs API endpoints. The impact is limited since ElevenLabs has its own auth, but this violates defense-in-depth.

**Recommendation:** Validate `voice_id` against a pattern like `^[a-zA-Z0-9]+$` before URL construction.

---

### S4 — Vault master key printed to stderr on init failure (Info)

**File:** `crates/openfang-extensions/src/vault.rs:113-117`

```rust
Err(e) => {
    warn!("Could not store in OS keyring: {e}. Set {} env var instead.", VAULT_KEY_ENV);
    eprintln!("Vault key (save this as {}): {}", VAULT_KEY_ENV, key_b64.as_str());
}
```

When keyring storage fails during `vault init`, the base64-encoded master key is printed to stderr. This is the intended flow (user needs to save the key), but if stderr is captured to a log file, the master key becomes persistent and recoverable from logs.

**Recommendation:** Add a comment warning operators about log capture, or consider writing the key to a separate restricted file instead of stderr.

---

### S5 — `/api/config` endpoint is public and exposes `api_key_env` names (Info)

**File:** `crates/openfang-api/src/middleware.rs:91`, `crates/openfang-api/src/routes.rs:4032-4042`

The auth middleware exempts `/api/config` from authentication. The `get_config` handler returns a redacted view (api_key shows as `"***"`), but it does expose the `api_key_env` field name (e.g., `"ANTHROPIC_API_KEY"`) and structural information about the kernel configuration including which providers and channels are configured.

This information disclosure is low-risk but could help an attacker enumerate which services are integrated.

**Recommendation:** Consider requiring authentication for `/api/config` when an API key is configured, or further reduce the information returned for unauthenticated requests.

---

### S6 — No `Debug` impl on driver structs protects API keys, but no guard against future derives (Info)

**Files:** `crates/openfang-runtime/src/drivers/openai.rs:15-19`, `anthropic.rs:18-22`, `gemini.rs:23-27`

The LLM driver structs (`OpenAIDriver`, `AnthropicDriver`, `GeminiDriver`) store `api_key: Zeroizing<String>` and intentionally do **not** derive `Debug`. This is correct — but there's no `#[non_exhaustive]` or doc comment warning against adding `#[derive(Debug)]` in the future. Since `Zeroizing<String>` implements `Debug` by delegating to the inner `String`, a future `#[derive(Debug)]` would expose API keys.

**Recommendation:** Add a `// SECURITY: Do not derive Debug — api_key would be exposed` comment to each driver struct, or implement a manual `Debug` that redacts the key.

---

### S7 — Gemini model name interpolated into URL path (Info)

**File:** `crates/openfang-runtime/src/drivers/gemini.rs:377-378`

```rust
let url = format!(
    "{}/v1beta/models/{}:generateContent",
    self.base_url, request.model
);
```

The model name from the request is interpolated into the URL path. The model name originates from the agent's config or the model catalog, so this is not directly user-controlled in normal flows. However, if an attacker can influence the model name (e.g., via `/api/agents/{id}/model` PUT), they could inject path segments.

**Recommendation:** URL-encode the model name or validate it against `^[a-zA-Z0-9._-]+$`.

---

## Strengths

### ✅ API keys wrapped in `Zeroizing<String>` across all drivers

All LLM driver structs (`OpenAIDriver`, `AnthropicDriver`, `GeminiDriver`, `CopilotDriver`) and the `CredentialVault` use `Zeroizing<String>` from the `zeroize` crate, ensuring API keys are zeroed from memory on drop. (`openai.rs:16`, `anthropic.rs:19`, `gemini.rs:24`, `copilot.rs:27`, `vault.rs:60`)

### ✅ Custom `Debug` impl on `KernelConfig` redacts `api_key`

The main config struct has a hand-written `Debug` impl that shows `<redacted>` instead of the actual API key. The `AuthProfile` struct also redacts `api_key_env`. (`config.rs:1229-1280`, `config.rs:839-848`)

### ✅ Constant-time API key comparison prevents timing attacks

The auth middleware uses `subtle::ConstantTimeEq` for both bearer token and query parameter authentication, preventing timing side-channel attacks. (`middleware.rs:104-127`)

### ✅ Comprehensive SSRF protection in web fetch

The `check_ssrf()` function blocks localhost, cloud metadata endpoints (AWS, GCP, Azure, Alibaba), private RFC 1918 IPs, and resolves DNS to verify IPs before any network I/O. IPv6 loopback (`[::1]`) is also blocked. Non-HTTP schemes (file://, ftp://, gopher://) are rejected. Comprehensive test coverage exists. (`web_fetch.rs:133-185`)

### ✅ Path traversal prevention in file access endpoints

- `get_agent_file` and `set_agent_file`: Filename whitelist + `canonicalize()` + `starts_with()` workspace check. (`routes.rs:7271-7324`, `7406-7440`)
- `serve_upload`: UUID validation prevents path traversal. (`routes.rs:7637-7638`)
- `set_agent_file`: Content size capped at 32 KB. (`routes.rs:7377-7384`)

### ✅ AES-256-GCM credential vault with Argon2id KDF

The vault uses proper cryptographic primitives: random 16-byte salt, 12-byte nonce, Argon2id key derivation, AES-256-GCM authenticated encryption. Vault entries use `Zeroizing<String>`. The `Drop` impl clears all entries. (`vault.rs:1-398`)

### ✅ HMAC-SHA256 with constant-time verification in wire protocol

P2P message authentication uses HMAC-SHA256 with `subtle::ConstantTimeEq` for signature verification. Message size is capped at 16 MB. (`peer.rs:28-38`, `peer.rs:58`)

### ✅ Security headers on all responses

CSP, X-Frame-Options (DENY), X-Content-Type-Options (nosniff), X-XSS-Protection, Referrer-Policy, and Cache-Control (no-store) are applied to all API responses. (`middleware.rs:152-174`)

### ✅ CORS restricted to localhost origins

Even with auth enabled, CORS is restricted to localhost variants — not `CorsLayer::permissive()`. The code includes an explicit security comment about this. (`server.rs:54-102`)

### ✅ Rate limiting with per-operation cost model

GCRA rate limiter assigns different token costs per endpoint (health=1, spawn=50, message=30, run=100), providing intelligent throttling. (`rate_limiter.rs:14-36`)

### ✅ Config stores env var names, not actual secrets

Channel configs (Telegram, Discord, Slack, etc.) store environment variable *names* (e.g., `bot_token_env: "TELEGRAM_BOT_TOKEN"`) rather than actual token values. This means `#[derive(Debug)]` on these structs is safe. (`config.rs:1521-2095`)

### ✅ No `println!` / `dbg!` / `eprintln!` in production library code

All `println!`/`eprintln!` usage is confined to CLI code (`openfang-cli/src/`) and migration reports — appropriate for user-facing output. No accidental debug output exists in runtime, API, or kernel crates.

### ✅ Daemon info file permissions restricted

On Unix systems, `daemon.json` (containing PID and port) has permissions set to 0600 (owner-only). (`server.rs:743-752`)

### ✅ MCP message size limit prevents OOM

MCP stdio transport enforces a 10 MB message size limit with proper draining of oversized messages to prevent stream desync. (`mcp.rs:187-204`)

### ✅ Upload size limits enforced

File uploads are capped at 10 MB (`MAX_UPLOAD_SIZE`), workspace file writes at 32 KB, and outbound responses checked at content-length level for TTS (audio) and web fetch operations. (`routes.rs:7492`, `tts.rs:121-125`, `web_fetch.rs:59-62`, `tool_runner.rs:1220-1224`)

### ✅ Copilot driver validates HTTPS on proxy endpoint

When the Copilot token exchange returns a proxy-ep URL, the driver validates it starts with `https://` before using it, falling back to the default URL if not. (`copilot.rs:123-125`)
