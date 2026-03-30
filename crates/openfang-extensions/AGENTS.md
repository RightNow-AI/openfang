<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-extensions

## Purpose
Extension and integration system for OpenFang: 25 bundled MCP server templates (GitHub, Slack, Google, etc.), AES-256-GCM encrypted credential vault with OS keyring support, OAuth2 PKCE flows for third-party authentication, health monitoring with auto-reconnect, and one-click installer (`openfang add <name>`).

## Key Files
| File | Description |
|------|-------------|
| `src/lib.rs` | Core types: `IntegrationTemplate`, `IntegrationStatus`, `IntegrationInfo`, `InstalledIntegration`, error types. |
| `src/vault.rs` | Credential Vault: AES-256-GCM encryption with Argon2 key derivation, OS keyring storage (macOS Keychain / Windows Credential Manager / Linux Secret Service), fallback to `OPENFANG_VAULT_KEY` env var. |
| `src/registry.rs` | Integration Registry: loads bundled templates, merges with user's installed state from `~/.openfang/integrations.toml`, converts to kernel `McpServerConfigEntry`. |
| `src/oauth.rs` | OAuth2 PKCE handler: localhost callback flow for Google, GitHub, Microsoft, Slack. |
| `src/credentials.rs` | Credential resolution: retrieves secrets from vault, environment, or OS APIs. |
| `src/installer.rs` | One-click installation flow: validate integration, resolve credentials, write config, start MCP server. |
| `src/health.rs` | Health monitor: periodic ping checks, exponential backoff reconnect, status reporting. |
| `src/bundled.rs` | Compile-time embedded TOML templates (25 integrations) loaded as const data. |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `src/` | Core extension system and credential management. |
| `integrations/` | Bundled MCP server templates (TOML format, included at compile time via `bundled.rs`). |

## For AI Agents

### Working In This Directory
- `IntegrationTemplate` defines what an integration needs: ID, name, transport (stdio/SSE/HTTP), required env vars, OAuth config (optional), health check interval.
- `IntegrationRegistry` is the main API: call `load_bundled()` then `load_installed()` to populate, then query via `list_all()`, `get_by_id()`, `install()`, `uninstall()`.
- Vault is initialized lazily: `CredentialVault::new()` then `init()` or `unlock()`. Master key comes from env var `OPENFANG_VAULT_KEY`, OS keyring, or is auto-generated.
- OAuth provider (`OAuthTemplate`) specifies `provider` (google/github/microsoft/slack), `auth_url`, `token_url`, and required `scopes`.
- Health checks run on a background task; status is stored in `InstalledIntegration.config["health_status"]`.
- Bundled templates are embedded as TOML strings in `integrations/` directory and processed by `bundled.rs`.

### Testing Requirements
- Test vault init/unlock with explicit keys: verify encryption/decryption roundtrip and wrong-key rejection.
- Test registry: verify bundled templates load, installed state merges correctly, and `get_by_id()` returns combined view.
- Test credential resolution order: direct value → env var → vault → missing (error).
- Test OAuth PKCE: verify localhost callback server starts, state parameter rounds-trips, token exchange succeeds.
- Test health monitor: verify background task detects failures and reconnects with backoff.
- Test installer: verify validation (missing creds → Setup status), config writing, and MCP server start.

### Common Patterns
- `IntegrationStatus` transitions: Available → Setup (installed but no creds) → Ready (fully configured) or Error/Disabled.
- Vault secrets are `Zeroizing<String>` for automatic memory clearing; never clone sensitive data.
- Registry `get_by_id()` returns `IntegrationInfo` which combines template + status + installed record + tool count.
- Health config is optional; defaults to 60-second checks and 3-strike failure threshold.
- OAuth uses PKCE (code challenge/verifier) for enhanced security on public clients.

## Dependencies

### Internal
- `openfang-types` — config types, MCP server config entry.

### External
- **Serialization:** `serde`/`serde_json`/`toml` — TOML template parsing and state persistence.
- **Encryption:** `aes-gcm` — AES-256-GCM cipher, `argon2` — key derivation, `sha2` — hashing, `zeroize` — secure memory clearing.
- **Async:** `tokio` — background tasks (health checks), `axum` — OAuth callback HTTP server.
- **HTTP:** `reqwest` — OAuth token exchange and health pings.
- **Security:** `rand` — nonce/salt generation.
- **Crypto:** `base64` — encoding/decoding secrets.
- **Utilities:** `uuid`, `chrono`, `dashmap` (concurrent map), `url`, `dirs` (home dir), `thiserror` (error types), `tracing` (logging).

<!-- MANUAL: -->
