# Configuration

This guide documents the configuration model that the current code actually loads. The authoritative schema is `crates/openfang-types/src/config.rs`.

## 1. Configuration Inputs

OpenFang combines several inputs at boot:

| Source | Purpose |
| --- | --- |
| `~/.openfang/config.toml` | primary runtime configuration |
| `include = ["..."]` | extra TOML fragments deep-merged before the root config |
| `~/.openfang/.env` + `~/.openfang/secrets.env` | runtime helper files for provider and channel secrets |
| process environment | deploy-time overrides and secret values |
| `vault.enc` | encrypted credential store when enabled |

Repository note:

- `.env.example` in the repo is only a reference template
- the runtime reads `~/.openfang/.env` plus `~/.openfang/secrets.env`, not the repo root `.env.example`

## 2. Precedence Rules

Important precedence behavior verified in the current code:

1. `OPENFANG_HOME` changes the runtime home before config lookup
2. `config.toml` is loaded and deep-merged with any `include` files
3. `OPENFANG_LISTEN` overrides top-level `api_listen`
4. `OPENFANG_API_KEY` overrides `api_key` when it is set to a non-empty value
5. provider credentials are resolved through vault -> runtime env files -> process environment
6. within runtime env files, `~/.openfang/secrets.env` overrides `~/.openfang/.env` when both define the same key

Important boundary:

- `OPENFANG_LISTEN` and `OPENFANG_API_KEY` are real process-environment overrides. The daemon does not read those two keys from `~/.openfang/.env` or `~/.openfang/secrets.env`.
- Put deploy-time runtime overrides in the actual service/container environment, or in an external env source such as `/etc/openfang/env` that your supervisor exports into the process.
- Use `~/.openfang/.env` and `~/.openfang/secrets.env` for provider/channel credentials like `GROQ_API_KEY`, not for daemon bind/auth overrides.

## 3. Runtime Home Layout

By default the runtime home is `~/.openfang`.

Typical layout:

```text
~/.openfang/
  config.toml
  .env
  secrets.env
  daemon.json
  vault.enc
  custom_models.json
  integrations.toml
  data/
  agents/
  skills/
  workspaces/
  workflows/
```

## 4. Minimal Working Config

```toml
api_listen = "127.0.0.1:4200"

[default_model]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"

[memory]
decay_rate = 0.05
```

With that config, set:

```bash
export GROQ_API_KEY=...
```

You can place the same key in `~/.openfang/secrets.env` (preferred for dashboard/API-managed keys) or `~/.openfang/.env`.

## 5. Canonical Example

This example matches the current schema shape.

```toml
api_listen = "127.0.0.1:4200"
api_key = ""
network_enabled = false
usage_footer = "full"

[default_model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"

[memory]
decay_rate = 0.05

[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/0"]
shared_secret = ""

[web]
search_provider = "auto"
cache_ttl_minutes = 15

[browser]
headless = true

[reload]
mode = "hybrid"
debounce_ms = 500

[budget]
alert_threshold = 0.8

[auth]
enabled = false
username = "admin"
# Generate with: openfang security hash-password
# Required whenever auth.enabled = true.
# Legacy 64-character SHA-256 hex digests still work, but Argon2id is recommended.
password_hash = ""

[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
allowed_users = []
poll_interval_secs = 1
download_enabled = false
use_local_api = false

[channels.discord]
bot_token_env = "DISCORD_BOT_TOKEN"
allowed_guilds = []

[channels.slack]
bot_token_env = "SLACK_BOT_TOKEN"
app_token_env = "SLACK_APP_TOKEN"

[[mcp_servers]]
name = "filesystem"
timeout_secs = 30
env = []

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
```

## 6. Key Top-Level Fields

These live at the root of `config.toml`:

| Key | Meaning |
| --- | --- |
| `home_dir` | optional override for the runtime home |
| `data_dir` | optional override for the data directory |
| `log_level` | tracing verbosity |
| `api_listen` | HTTP API bind address |
| `network_enabled` | enables the OFP network layer |
| `api_key` | bearer token for API auth |
| `mode` | `stable`, `default`, or `dev` |
| `usage_footer` | response footer mode |
| `workspaces_dir` | overrides the default `~/.openfang/workspaces` |
| `workflows_dir` | overrides the default workflow auto-load directory |
| `max_cron_jobs` | global cron job cap |
| `include` | extra TOML fragments merged before the root file |

## 7. Important Sections

### `[default_model]`

Primary LLM routing target.
Fields:

- `provider`
- `model`
- `api_key_env`
- `base_url` optional

### `[[fallback_providers]]`

Ordered backup providers tried when the primary path fails.

### `[memory]`

Controls SQLite path, embedding behavior, consolidation settings, and decay.
If `memory.embedding_provider` is left unset, OpenFang now auto-detects embeddings in this order:

- reuse the current `default_model` provider when it is OpenAI-compatible (including custom providers wired through `base_url`)
- otherwise fall back to `OPENAI_API_KEY` if configured
- otherwise probe local Ollama and finally degrade to text-search recall

### `[network]`

This config is for the OFP peer layer, not the HTTP API. Use `listen_addresses`, not `listen_addr`.

### `[channels.<name>]`

Channel configs live under `channels`.
Examples:

- `[channels.telegram]`
- `[channels.discord]`
- `[channels.slack]`

This is a common gotcha: `[telegram]` is not the canonical shape for the current code.

### `[[mcp_servers]]`

Defines external MCP server connections. Each entry needs:

- `name`
- `timeout_secs`
- optional `env`
- a `[mcp_servers.transport]` block

### `[auth]`

Dashboard session auth. This is separate from `api_key`, which protects the HTTP API with bearer auth.

### `[budget]`

Global spend and token-limit controls.

### `[reload]`

Controls config reload behavior with:

- `mode = "off" | "restart" | "hot" | "hybrid"`
- `debounce_ms`

## 8. Secrets and Sensitive Fields

Most provider and channel secrets should be referenced by env-var name rather than written directly in `config.toml`.

Examples:

- `api_key_env = "GROQ_API_KEY"`
- `bot_token_env = "TELEGRAM_BOT_TOKEN"`
- `telegram_api_hash_env = "TELEGRAM_API_HASH"`

However, some sensitive values do exist directly in the schema:

- top-level `api_key`
- `[network].shared_secret`
- `[auth].password_hash`

Operational requirement:

- if `[auth].enabled = true`, you must also set a non-empty `[auth].password_hash`
- `password_hash` must be a valid Argon2id PHC string or a 64-character legacy SHA-256 hex digest
- there is no bootstrap route that creates the first dashboard password for you at runtime
- for production, prefer `OPENFANG_API_KEY` even if dashboard auth is also enabled

Keep those out of version control and prefer env or vault workflows when practical.

Useful deployment environment variables:

| Variable | Effect |
| --- | --- |
| `OPENFANG_HOME` | change runtime home directory |
| `OPENFANG_LISTEN` | override `api_listen` at boot |
| `OPENFANG_API_KEY` | override `api_key` at boot when set to a non-empty value |
| provider keys such as `GROQ_API_KEY` | authenticate LLM calls |
| channel tokens such as `TELEGRAM_BOT_TOKEN` | authenticate channel adapters |

## 9. Include Files

The loader supports:

```toml
include = ["base.toml", "channels.toml"]
```

Behavior:

- included files are deep-merged first
- the root config overrides included values
- absolute paths and `..` traversal are rejected
- circular includes are rejected

Use includes to separate operator config by concern.

## 10. Hot Reload Boundaries

The reload planner currently classifies config changes like this:

| Category | Fields |
| --- | --- |
| detected as hot-reloadable | channels, skills, usage footer, web, browser, approval, cron limits, webhook config, extensions, MCP, A2A, fallback providers, provider URLs, default model |
| no-op until later | `log_level`, `language`, `mode`, `provider_api_keys` |
| restart required | `api_listen`, `api_key`, `auth`, `network_enabled`, `network`, `memory`, `home_dir`, `data_dir`, `vault` |

The current kernel only auto-applies a subset immediately:

- `approval`
- `max_cron_jobs`
- `provider_urls`
- `default_model`

For other "hot-reloadable" fields, the planner will detect the change and report it, but operators should still treat it as pending follow-up until the specific subsystem is restarted or reinitialized. When in doubt, restart after config changes.

Reload planning uses the same effective-config precedence as boot: `config.toml` + includes, then runtime env overrides such as `OPENFANG_LISTEN` and `OPENFANG_API_KEY`.

## 11. Current Schema Areas

The current `KernelConfig` is broad. In addition to the sections above, maintainers should expect config surfaces for:

- `web`
- `browser`
- `extensions`
- `vault`
- `media`
- `links`
- `approval`
- `exec_policy`
- `bindings`
- `broadcast`
- `auto_reply`
- `canvas`
- `tts`
- `docker`
- `pairing`
- `auth_profiles`
- `thinking`
- `provider_urls`
- `provider_api_keys`
- `oauth`

For exact fields and defaults, inspect `crates/openfang-types/src/config.rs`.
