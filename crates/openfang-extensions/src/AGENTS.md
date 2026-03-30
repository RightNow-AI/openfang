<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# openfang-extensions/src

## Purpose

Core extension and integration system for OpenFang: 25 bundled MCP server templates (GitHub, Slack, Google, AWS, etc.), AES-256-GCM encrypted credential vault with OS keyring support, OAuth2 PKCE flows for third-party authentication, health monitoring with auto-reconnect, one-click installer (`openfang add <name>`), and marketplace syncing via ClawHub.

## Key Files

| File | Description |
|------|-------------|
| `lib.rs` | Core types: `IntegrationTemplate`, `IntegrationStatus`, `IntegrationInfo`, `InstalledIntegration`, error types. Public API. |
| `registry.rs` | Integration Registry: loads bundled templates, merges with user's installed state from `~/.openfang/integrations.toml`, converts to kernel `McpServerConfigEntry`, provides query API. |
| `vault.rs` | Credential Vault: AES-256-GCM encryption with Argon2 key derivation, OS keyring storage (macOS Keychain / Windows Credential Manager / Linux Secret Service), fallback to `OPENFANG_VAULT_KEY` env var. |
| `oauth.rs` | OAuth2 PKCE handler: localhost callback flow, state parameter validation, token exchange for Google, GitHub, Microsoft, Slack. |
| `credentials.rs` | Credential resolution: retrieves secrets from vault, environment variables, OS APIs, with fallback chain. |
| `installer.rs` | One-click installation flow: validate integration, resolve credentials, write config, start MCP server. |
| `health.rs` | Health monitor: periodic ping checks, exponential backoff reconnect, status reporting, auto-remediation. |
| `bundled.rs` | Compile-time embedded TOML templates (25 integrations) loaded as const data. |

## For AI Agents

### Working In This Directory

- `IntegrationRegistry` is the main API: call `load_bundled()` then `load_installed()` to populate, then query via `list_all()`, `get_by_id()`, `install()`, `uninstall()`.
- Each integration is identified by ID (TOML filename without extension): `get_by_id("github")` returns the GitHub integration template.
- `IntegrationTemplate` defines: ID, name, transport (stdio/SSE/HTTP), required env vars, optional OAuth config, health check config.
- `CredentialVault` is lazy-initialized: call `CredentialVault::new()` then `init()` or `unlock()`. Master key comes from env var, OS keyring, or is auto-generated.
- `OAuthTemplate` specifies: provider (google/github/microsoft/slack), auth_url, token_url, required scopes.
- Health checks run on background tasks; status is stored in `InstalledIntegration.config["health_status"]`.
- Installer validates all required credentials are present before writing config and starting MCP server.

### Testing Requirements

- Test vault init/unlock: verify encryption/decryption roundtrip, wrong-key rejection, OS keyring storage.
- Test registry: verify bundled templates load, installed state merges, `get_by_id()` returns combined view.
- Test credential resolution: direct value → env var → vault → missing (error).
- Test OAuth PKCE: localhost callback server starts, state round-trips, token exchange succeeds.
- Test health monitor: background task detects failures and reconnects with exponential backoff.
- Test installer: validation (missing creds → Setup status), config writing, MCP server start.
- Test marketplace: mock ClawHub API, verify template updates fetch and merge.

### Common Patterns

- `IntegrationStatus` transitions: Available → Setup (installed, no creds) → Ready (fully configured) or Error/Disabled.
- Vault secrets are `Zeroizing<String>` for automatic memory clearing; never clone sensitive data.
- Registry `get_by_id()` returns `IntegrationInfo` which combines template + status + installed record + tool count.
- Health config is optional; defaults to 60-second checks and 3-strike failure threshold.
- OAuth uses PKCE (code challenge/verifier) for enhanced security on public clients.
- MCP server config is stored in `~/.openfang/integrations.toml` (user-managed state) + bundled templates (read-only).

## Dependencies

### Internal
- `openfang-types` — config types, MCP server config entry

### External
- **Serialization:** `serde`/`serde_json`/`toml` — TOML template parsing and state persistence
- **Encryption:** `aes-gcm` — AES-256-GCM cipher, `argon2` — key derivation, `sha2` — hashing, `zeroize` — secure memory clearing
- **Async:** `tokio` — background tasks (health checks), `axum` — OAuth callback HTTP server
- **HTTP:** `reqwest` — OAuth token exchange, health pings, ClawHub API calls
- **Security:** `rand` — nonce/salt generation
- **Crypto:** `base64` — encoding/decoding secrets
- **Utilities:** `uuid` — integration instance IDs, `chrono` — timestamps, `dashmap` — concurrent status cache, `url` — parsing URLs, `dirs` — home directory, `thiserror` — error types, `tracing` — logging

<!-- MANUAL: -->
