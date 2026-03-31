# OpenFang Configuration Reference

Complete reference for `config.toml`, covering every configurable field in the OpenFang Agent OS.

---

## Table of Contents

- [Overview](#overview)
- [Minimal Configuration](#minimal-configuration)
- [Full Example](#full-example)
- [Section Reference](#section-reference)
  - [Top-Level Fields](#top-level-fields)
  - [\[default\_model\]](#default_model)
  - [\[memory\]](#memory)
  - [\[network\]](#network)
  - [\[web\]](#web)
  - [\[channels\]](#channels)
  - [\[\[mcp\_servers\]\]](#mcp_servers)
  - [\[a2a\]](#a2a)
  - [\[\[fallback\_providers\]\]](#fallback_providers)
  - [\[\[users\]\]](#users)
  - [Channel Overrides](#channel-overrides)
- [Environment Variables](#environment-variables)
- [Validation](#validation)

---

## Overview

OpenFang reads its configuration from a single TOML file:

```
~/.openfang/config.toml
```

On Windows, `~` resolves to `C:\Users\<username>`. If the home directory cannot be determined, the system temp directory is used as a fallback.

**Key behaviors:**

- Every struct in the configuration uses `#[serde(default)]`, which means **all fields are optional**. Omitted fields receive their documented default values.
- Channel sections (`[channels.telegram]`, `[channels.discord]`, etc.) are `Option<T>` -- when absent, the channel adapter is **disabled**. Including the section header (even empty) enables the adapter with defaults.
- Secrets are **never stored in config.toml** directly. Instead, fields like `api_key_env` and `bot_token_env` hold the **name** of an environment variable that contains the actual secret. This prevents accidental exposure in version control.
- Sensitive fields (`api_key`, `shared_secret`) are automatically redacted in debug output and logs.

---

## Minimal Configuration

The simplest working configuration only needs an LLM provider API key set as an environment variable. With no config file at all, OpenFang boots with Anthropic as the default provider:

```toml
# ~/.openfang/config.toml
# Minimal: just override the model if you want something other than defaults.
# Set ANTHROPIC_API_KEY in your environment.

[default_model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"
```

Or to use a local Ollama instance with no API key:

```toml
[default_model]
provider = "ollama"
model = "llama3.2:latest"
base_url = "http://localhost:11434"
api_key_env = ""
```

---

## Full Example

```toml
# ============================================================
# OpenFang Agent OS -- Complete Configuration Reference
# ============================================================

# --- Top-level fields ---
home_dir = "~/.openfang"             # OpenFang home directory
data_dir = "~/.openfang/data"        # SQLite databases and data files
log_level = "info"                   # trace | debug | info | warn | error
api_listen = "127.0.0.1:50051"      # HTTP/WS API bind address
network_enabled = false              # Enable OFP peer-to-peer network
api_key = ""                         # API Bearer token (empty = unauthenticated)
mode = "default"                     # stable | default | dev
language = "en"                      # Locale for CLI/messages
usage_footer = "full"                # off | tokens | cost | full

# --- Default LLM Provider ---
[default_model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"
# base_url = "https://api.anthropic.com"  # Optional override

# --- Fallback Providers ---
[[fallback_providers]]
provider = "ollama"
model = "llama3.2:latest"
api_key_env = ""
# base_url = "http://localhost:11434"  # Uses catalog default if omitted

[[fallback_providers]]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"

# --- Memory ---
[memory]
# sqlite_path = "~/.openfang/data/openfang.db"  # Auto-resolved if omitted
embedding_model = "all-MiniLM-L6-v2"
consolidation_threshold = 10000
decay_rate = 0.1

# --- Network (OFP Wire Protocol) ---
[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/0"]
bootstrap_peers = []
mdns_enabled = true
max_peers = 50
shared_secret = ""                   # Required when network_enabled = true

# --- Web Tools ---
[web]
search_provider = "auto"             # auto | brave | tavily | perplexity | duck_duck_go
cache_ttl_minutes = 15

[web.brave]
api_key_env = "BRAVE_API_KEY"
max_results = 5
country = ""
search_lang = ""
freshness = ""

[web.tavily]
api_key_env = "TAVILY_API_KEY"
search_depth = "basic"               # basic | advanced
max_results = 5
include_answer = true

[web.perplexity]
api_key_env = "PERPLEXITY_API_KEY"
model = "sonar"

[web.fetch]
max_chars = 50000
max_response_bytes = 10485760        # 10 MB
timeout_secs = 30
readability = true

# --- MCP Servers ---
[[mcp_servers]]
name = "filesystem"
timeout_secs = 30
env = []
[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]

[[mcp_servers]]
name = "remote-tools"
timeout_secs = 60
env = ["REMOTE_API_KEY"]
[mcp_servers.transport]
type = "sse"
url = "https://mcp.example.com/events"

# --- A2A Protocol ---
[a2a]
enabled = false
listen_path = "/a2a"

[[a2a.external_agents]]
name = "research-agent"
url = "https://agent.example.com/.well-known/agent.json"

# --- RBAC Users ---
[[users]]
name = "Alice"
role = "owner"                       # owner | admin | user | viewer
api_key_hash = ""
[users.channel_bindings]
telegram = "123456"
discord = "987654321"

[[users]]
name = "Bob"
role = "user"
[users.channel_bindings]
slack = "U0123ABCDEF"

# --- Channel Adapters ---
# (See "Channels" section below for all 40 adapters)

[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
allowed_users = []
# default_agent = "assistant"
poll_interval_secs = 1

[channels.discord]
bot_token_env = "DISCORD_BOT_TOKEN"
allowed_guilds = []
intents = 33280

[channels.slack]
app_token_env = "SLACK_APP_TOKEN"
bot_token_env = "SLACK_BOT_TOKEN"
allowed_channels = []
```

---

## Section Reference

### Top-Level Fields

These fields sit at the root of `config.toml` (not inside any `[section]`).

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `home_dir` | path | `~/.openfang` | OpenFang home directory. Stores config, agents, skills. |
| `data_dir` | path | `~/.openfang/data` | Directory for SQLite databases and persistent data. |
| `log_level` | string | `"info"` | Log verbosity. One of: `trace`, `debug`, `info`, `warn`, `error`. |
| `api_listen` | string | `"127.0.0.1:50051"` | Bind address for the HTTP/WebSocket/SSE API server. |
| `network_enabled` | bool | `false` | Enable the OFP peer-to-peer network layer. |
| `api_key` | string | `""` (empty) | API authentication key. When set, all endpoints except `/api/health` require `Authorization: Bearer <key>`. Empty means unauthenticated (local development only). |
| `mode` | string | `"default"` | Kernel operating mode. See below. |
| `language` | string | `"en"` | Language/locale code for CLI output and system messages. |
| `usage_footer` | string | `"full"` | Controls usage info appended to responses. See below. |

**`mode` values:**

| Value | Behavior |
|-------|----------|
| `stable` | Conservative: no auto-updates, pinned models, frozen skill registry. Uses `FallbackDriver`. |
| `default` | Balanced: standard operation. |
| `dev` | Developer: experimental features enabled. |

**`usage_footer` values:**

| Value | Behavior |
|-------|----------|
| `off` | No usage information shown. |
| `tokens` | Show token counts only. |
| `cost` | Show estimated cost only. |
| `full` | Show both token counts and estimated cost (default). |

---

### `[default_model]`

Configures the primary LLM provider used when agents do not specify their own model.

```toml
[default_model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"
# base_url = "https://api.anthropic.com"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | `"anthropic"` | Provider name. Supported: `anthropic`, `gemini`, `openai`, `groq`, `openrouter`, `deepseek`, `together`, `mistral`, `fireworks`, `ollama`, `vllm`, `lmstudio`, `perplexity`, `cohere`, `ai21`, `cerebras`, `sambanova`, `huggingface`, `xai`, `replicate`. |
| `model` | string | `"claude-sonnet-4-20250514"` | Model identifier. Aliases like `sonnet`, `haiku`, `gpt-4o`, `gemini-flash` are resolved by the model catalog. |
| `api_key_env` | string | `"ANTHROPIC_API_KEY"` | Name of the environment variable holding the API key. The actual key is read from this env var at runtime, never stored in config. |
| `base_url` | string or null | `null` | Override the API base URL. Useful for proxies or self-hosted endpoints. When `null`, the provider's default URL from the model catalog is used. |

---

### `[memory]`

Configures the SQLite-backed memory substrate, including vector embeddings and memory decay.

```toml
[memory]
# sqlite_path = "/custom/path/openfang.db"
embedding_model = "all-MiniLM-L6-v2"
consolidation_threshold = 10000
decay_rate = 0.1
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `sqlite_path` | path or null | `null` | Explicit path to the SQLite database file. When `null`, defaults to `{data_dir}/openfang.db`. |
| `embedding_model` | string | `"all-MiniLM-L6-v2"` | Model name used for generating vector embeddings for semantic memory search. |
| `consolidation_threshold` | u64 | `10000` | Number of stored memories before automatic consolidation is triggered to merge and prune old entries. |
| `decay_rate` | f32 | `0.1` | Memory confidence decay rate. `0.0` = no decay (memories never fade), `1.0` = aggressive decay. Values between 0.0 and 1.0. |

---

### `[network]`

Configures the OFP (OpenFang Protocol) peer-to-peer networking layer with HMAC-SHA256 mutual authentication.

```toml
[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/0"]
bootstrap_peers = []
mdns_enabled = true
max_peers = 50
shared_secret = "my-cluster-secret"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen_addresses` | list of strings | `["/ip4/0.0.0.0/tcp/0"]` | libp2p multiaddresses to listen on. Port `0` means auto-assign. |
| `bootstrap_peers` | list of strings | `[]` | Multiaddresses of bootstrap peers for DHT discovery. |
| `mdns_enabled` | bool | `true` | Enable mDNS for automatic local network peer discovery. |
| `max_peers` | u32 | `50` | Maximum number of simultaneously connected peers. |
| `shared_secret` | string | `""` (empty) | Pre-shared secret for OFP HMAC-SHA256 mutual authentication. **Required** when `network_enabled = true`. Both sides must use the same secret. Redacted in logs. |

---

### `[auth]`

Configures dashboard login with username/password authentication. Disabled by default.

```toml
[auth]
enabled = true
username = "admin"
password_hash = "$argon2id$v=19$m=19456,t=2,p=1$..."  # generate with: openfang auth hash-password
session_ttl_hours = 168
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Enable username/password authentication for the dashboard. |
| `username` | string | `"admin"` | Admin username. |
| `password_hash` | string | `""` (empty) | Argon2id password hash in PHC string format. Generate with `openfang auth hash-password`. |
| `session_ttl_hours` | u64 | `168` (7 days) | Session token lifetime in hours. |

**Generating a password hash:**

```bash
openfang auth hash-password
```

This prompts for a password and outputs an Argon2id PHC string to paste into `config.toml`.

> **Breaking change (v0.5.0):** Password hashes must be in Argon2id format. Older SHA256 hex hashes from versions prior to v0.5.0 are no longer accepted. Re-run `openfang auth hash-password` to generate a new hash.

---

### `[web]`

Configures web search and web fetch capabilities used by agent tools.

```toml
[web]
search_provider = "auto"
cache_ttl_minutes = 15
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `search_provider` | string | `"auto"` | Which search engine to use. See values below. |
| `cache_ttl_minutes` | u64 | `15` | Cache duration for search/fetch results in minutes. `0` = caching disabled. |

**`search_provider` values:**

| Value | Description |
|-------|-------------|
| `auto` | Cascading fallback: tries Tavily, then Brave, then Perplexity, then SearXNG, then DuckDuckGo, based on which API keys/configs are available. |
| `brave` | Brave Search API. Requires `BRAVE_API_KEY`. |
| `tavily` | Tavily AI-native search. Requires `TAVILY_API_KEY`. |
| `perplexity` | Perplexity AI search. Requires `PERPLEXITY_API_KEY`. |
| `searxng` | Self-hosted search engine aggregator. No API key required, just point to your SearXNG instance. |
| `duck_duck_go` | DuckDuckGo HTML scraping. No API key needed. |

#### `[web.brave]`

```toml
[web.brave]
api_key_env = "BRAVE_API_KEY"
max_results = 5
country = ""
search_lang = ""
freshness = ""
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key_env` | string | `"BRAVE_API_KEY"` | Environment variable name holding the Brave Search API key. |
| `max_results` | usize | `5` | Maximum number of search results to return. |
| `country` | string | `""` | Country code for localized results (e.g., `"US"`, `"GB"`). Empty = no filter. |
| `search_lang` | string | `""` | Language code (e.g., `"en"`, `"fr"`). Empty = no filter. |
| `freshness` | string | `""` | Freshness filter. `"pd"` = past day, `"pw"` = past week, `"pm"` = past month. Empty = no filter. |

#### `[web.tavily]`

```toml
[web.tavily]
api_key_env = "TAVILY_API_KEY"
search_depth = "basic"
max_results = 5
include_answer = true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key_env` | string | `"TAVILY_API_KEY"` | Environment variable name holding the Tavily API key. |
| `search_depth` | string | `"basic"` | Search depth: `"basic"` for fast results, `"advanced"` for deeper analysis. |
| `max_results` | usize | `5` | Maximum number of search results to return. |
| `include_answer` | bool | `true` | Whether to include Tavily's AI-generated answer summary in results. |

#### `[web.perplexity]`

```toml
[web.perplexity]
api_key_env = "PERPLEXITY_API_KEY"
model = "sonar"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_key_env` | string | `"PERPLEXITY_API_KEY"` | Environment variable name holding the Perplexity API key. |
| `model` | string | `"sonar"` | Perplexity model to use for search queries. |

#### `[web.searxng]`

**SearXNG** — Self-hosted search engine aggregator. No API key required, just point to your SearXNG instance. Supports 30+ search categories (general, images, news, videos, etc.) and pagination.

```toml
[web.searxng]
url = "https://searxng.example.com"    # SearXNG instance URL (required)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `url` | string | (required) | Full URL of your SearXNG instance (e.g., `https://searxng.example.com`). Must be accessible. |

#### `[web.fetch]`

```toml
[web.fetch]
max_chars = 50000
max_response_bytes = 10485760
timeout_secs = 30
readability = true
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_chars` | usize | `50000` | Maximum characters returned in fetched content. Content exceeding this is truncated. |
| `max_response_bytes` | usize | `10485760` (10 MB) | Maximum HTTP response body size in bytes. |
| `timeout_secs` | u64 | `30` | HTTP request timeout in seconds. |
| `readability` | bool | `true` | Enable HTML-to-Markdown readability extraction. When true, fetched HTML is converted to clean Markdown. |

---

### `[channels]`

All 40 channel adapters are configured under `[channels.<name>]`. Each channel is `Option<T>` -- omitting the section disables the adapter entirely. Including the section header (even empty) enables it with default values.

Every channel config includes a `default_agent` field (optional agent name to route messages to) and an `overrides` sub-table (see [Channel Overrides](#channel-overrides)).

#### `[channels.telegram]`

```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
allowed_users = []
# default_agent = "assistant"
poll_interval_secs = 1
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"TELEGRAM_BOT_TOKEN"` | Env var holding the Telegram Bot API token. |
| `allowed_users` | list of i64 | `[]` | Telegram user IDs allowed to interact. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |
| `poll_interval_secs` | u64 | `1` | Long-polling interval in seconds. |

#### `[channels.discord]`

```toml
[channels.discord]
bot_token_env = "DISCORD_BOT_TOKEN"
allowed_guilds = []
# default_agent = "assistant"
intents = 33280
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"DISCORD_BOT_TOKEN"` | Env var holding the Discord bot token. |
| `allowed_guilds` | list of u64 | `[]` | Guild (server) IDs allowed. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |
| `intents` | u64 | `33280` | Gateway intents bitmask. Default = `GUILD_MESSAGES \| MESSAGE_CONTENT`. |

#### `[channels.slack]`

```toml
[channels.slack]
app_token_env = "SLACK_APP_TOKEN"
bot_token_env = "SLACK_BOT_TOKEN"
allowed_channels = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `app_token_env` | string | `"SLACK_APP_TOKEN"` | Env var holding the Slack app-level token (`xapp-`) for Socket Mode. |
| `bot_token_env` | string | `"SLACK_BOT_TOKEN"` | Env var holding the Slack bot token (`xoxb-`) for REST API. |
| `allowed_channels` | list of strings | `[]` | Channel IDs allowed. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.whatsapp]`

```toml
[channels.whatsapp]
access_token_env = "WHATSAPP_ACCESS_TOKEN"
verify_token_env = "WHATSAPP_VERIFY_TOKEN"
phone_number_id = ""
webhook_port = 8443
allowed_users = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `access_token_env` | string | `"WHATSAPP_ACCESS_TOKEN"` | Env var holding the WhatsApp Cloud API access token. |
| `verify_token_env` | string | `"WHATSAPP_VERIFY_TOKEN"` | Env var holding the webhook verification token. |
| `phone_number_id` | string | `""` | WhatsApp Business phone number ID. |
| `webhook_port` | u16 | `8443` | Port to listen for incoming webhook callbacks. |
| `allowed_users` | list of strings | `[]` | Phone numbers allowed. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.signal]`

```toml
[channels.signal]
api_url = "http://localhost:8080"
phone_number = ""
allowed_users = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `api_url` | string | `"http://localhost:8080"` | URL of the signal-cli REST API. |
| `phone_number` | string | `""` | Registered phone number for the bot. |
| `allowed_users` | list of strings | `[]` | Allowed phone numbers. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.matrix]`

```toml
[channels.matrix]
homeserver_url = "https://matrix.org"
user_id = "@openfang:matrix.org"
access_token_env = "MATRIX_ACCESS_TOKEN"
allowed_rooms = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `homeserver_url` | string | `"https://matrix.org"` | Matrix homeserver URL. |
| `user_id` | string | `""` | Bot user ID (e.g., `"@openfang:matrix.org"`). |
| `access_token_env` | string | `"MATRIX_ACCESS_TOKEN"` | Env var holding the Matrix access token. |
| `allowed_rooms` | list of strings | `[]` | Room IDs to listen in. Empty = all joined rooms. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.email]`

```toml
[channels.email]
imap_host = "imap.gmail.com"
imap_port = 993
smtp_host = "smtp.gmail.com"
smtp_port = 587
username = "bot@example.com"
password_env = "EMAIL_PASSWORD"
poll_interval_secs = 30
folders = ["INBOX"]
allowed_senders = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `imap_host` | string | `""` | IMAP server hostname. |
| `imap_port` | u16 | `993` | IMAP server port (993 for TLS). |
| `smtp_host` | string | `""` | SMTP server hostname. |
| `smtp_port` | u16 | `587` | SMTP server port (587 for STARTTLS). |
| `username` | string | `""` | Email address for both IMAP and SMTP. |
| `password_env` | string | `"EMAIL_PASSWORD"` | Env var holding the email password or app password. |
| `poll_interval_secs` | u64 | `30` | IMAP polling interval in seconds. |
| `folders` | list of strings | `["INBOX"]` | IMAP folders to monitor. |
| `allowed_senders` | list of strings | `[]` | Only process emails from these senders. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.teams]`

```toml
[channels.teams]
app_id = ""
app_password_env = "TEAMS_APP_PASSWORD"
webhook_port = 3978
allowed_tenants = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `app_id` | string | `""` | Azure Bot App ID. |
| `app_password_env` | string | `"TEAMS_APP_PASSWORD"` | Env var holding the Azure Bot Framework app password. |
| `webhook_port` | u16 | `3978` | Port for the Bot Framework incoming webhook. |
| `allowed_tenants` | list of strings | `[]` | Azure AD tenant IDs allowed. Empty = allow all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.mattermost]`

```toml
[channels.mattermost]
server_url = "https://mattermost.example.com"
token_env = "MATTERMOST_TOKEN"
allowed_channels = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `""` | Mattermost server URL. |
| `token_env` | string | `"MATTERMOST_TOKEN"` | Env var holding the Mattermost bot token. |
| `allowed_channels` | list of strings | `[]` | Channel IDs to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.irc]`

```toml
[channels.irc]
server = "irc.libera.chat"
port = 6667
nick = "openfang"
# password_env = "IRC_PASSWORD"
channels = ["#openfang"]
use_tls = false
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server` | string | `"irc.libera.chat"` | IRC server hostname. |
| `port` | u16 | `6667` | IRC server port. |
| `nick` | string | `"openfang"` | Bot nickname. |
| `password_env` | string or null | `null` | Env var holding the server password (optional). |
| `channels` | list of strings | `[]` | IRC channels to join (e.g., `["#openfang", "#general"]`). |
| `use_tls` | bool | `false` | Use TLS for the connection. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.google_chat]`

```toml
[channels.google_chat]
service_account_env = "GOOGLE_CHAT_SERVICE_ACCOUNT"
space_ids = []
webhook_port = 8444
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `service_account_env` | string | `"GOOGLE_CHAT_SERVICE_ACCOUNT"` | Env var holding the service account JSON key. |
| `space_ids` | list of strings | `[]` | Google Chat space IDs to listen in. |
| `webhook_port` | u16 | `8444` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.twitch]`

```toml
[channels.twitch]
oauth_token_env = "TWITCH_OAUTH_TOKEN"
channels = ["mychannel"]
nick = "openfang"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `oauth_token_env` | string | `"TWITCH_OAUTH_TOKEN"` | Env var holding the Twitch OAuth token. |
| `channels` | list of strings | `[]` | Twitch channels to join (without `#` prefix). |
| `nick` | string | `"openfang"` | Bot nickname in Twitch chat. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.rocketchat]`

```toml
[channels.rocketchat]
server_url = "https://rocketchat.example.com"
token_env = "ROCKETCHAT_TOKEN"
user_id = ""
allowed_channels = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `""` | Rocket.Chat server URL. |
| `token_env` | string | `"ROCKETCHAT_TOKEN"` | Env var holding the Rocket.Chat auth token. |
| `user_id` | string | `""` | Bot user ID. |
| `allowed_channels` | list of strings | `[]` | Channel IDs to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.zulip]`

```toml
[channels.zulip]
server_url = "https://zulip.example.com"
bot_email = "bot@zulip.example.com"
api_key_env = "ZULIP_API_KEY"
streams = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `""` | Zulip server URL. |
| `bot_email` | string | `""` | Bot email address registered in Zulip. |
| `api_key_env` | string | `"ZULIP_API_KEY"` | Env var holding the Zulip API key. |
| `streams` | list of strings | `[]` | Stream names to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.xmpp]`

```toml
[channels.xmpp]
jid = "bot@jabber.org"
password_env = "XMPP_PASSWORD"
server = ""
port = 5222
rooms = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `jid` | string | `""` | XMPP JID (e.g., `"bot@jabber.org"`). |
| `password_env` | string | `"XMPP_PASSWORD"` | Env var holding the XMPP password. |
| `server` | string | `""` | XMPP server hostname. Defaults to the JID domain if empty. |
| `port` | u16 | `5222` | XMPP server port. |
| `rooms` | list of strings | `[]` | MUC (multi-user chat) rooms to join. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.line]`

```toml
[channels.line]
channel_secret_env = "LINE_CHANNEL_SECRET"
access_token_env = "LINE_CHANNEL_ACCESS_TOKEN"
webhook_port = 8450
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `channel_secret_env` | string | `"LINE_CHANNEL_SECRET"` | Env var holding the LINE channel secret. |
| `access_token_env` | string | `"LINE_CHANNEL_ACCESS_TOKEN"` | Env var holding the LINE channel access token. |
| `webhook_port` | u16 | `8450` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.viber]`

```toml
[channels.viber]
auth_token_env = "VIBER_AUTH_TOKEN"
webhook_url = ""
webhook_port = 8451
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `auth_token_env` | string | `"VIBER_AUTH_TOKEN"` | Env var holding the Viber Bot auth token. |
| `webhook_url` | string | `""` | Public URL for the Viber webhook endpoint. |
| `webhook_port` | u16 | `8451` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.messenger]`

```toml
[channels.messenger]
page_token_env = "MESSENGER_PAGE_TOKEN"
verify_token_env = "MESSENGER_VERIFY_TOKEN"
webhook_port = 8452
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `page_token_env` | string | `"MESSENGER_PAGE_TOKEN"` | Env var holding the Facebook page access token. |
| `verify_token_env` | string | `"MESSENGER_VERIFY_TOKEN"` | Env var holding the webhook verify token. |
| `webhook_port` | u16 | `8452` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.reddit]`

```toml
[channels.reddit]
client_id = ""
client_secret_env = "REDDIT_CLIENT_SECRET"
username = ""
password_env = "REDDIT_PASSWORD"
subreddits = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `client_id` | string | `""` | Reddit app client ID. |
| `client_secret_env` | string | `"REDDIT_CLIENT_SECRET"` | Env var holding the Reddit client secret. |
| `username` | string | `""` | Reddit bot username. |
| `password_env` | string | `"REDDIT_PASSWORD"` | Env var holding the Reddit bot password. |
| `subreddits` | list of strings | `[]` | Subreddit names to monitor. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.mastodon]`

```toml
[channels.mastodon]
instance_url = "https://mastodon.social"
access_token_env = "MASTODON_ACCESS_TOKEN"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `instance_url` | string | `""` | Mastodon instance URL (e.g., `"https://mastodon.social"`). |
| `access_token_env` | string | `"MASTODON_ACCESS_TOKEN"` | Env var holding the Mastodon access token. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.bluesky]`

```toml
[channels.bluesky]
identifier = "mybot.bsky.social"
app_password_env = "BLUESKY_APP_PASSWORD"
service_url = "https://bsky.social"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `identifier` | string | `""` | Bluesky handle or DID. |
| `app_password_env` | string | `"BLUESKY_APP_PASSWORD"` | Env var holding the Bluesky app password. |
| `service_url` | string | `"https://bsky.social"` | PDS (Personal Data Server) URL. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.feishu]`

```toml
[channels.feishu]
app_id = ""
app_secret_env = "FEISHU_APP_SECRET"
webhook_port = 8453
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `app_id` | string | `""` | Feishu/Lark app ID. |
| `app_secret_env` | string | `"FEISHU_APP_SECRET"` | Env var holding the Feishu app secret. |
| `webhook_port` | u16 | `8453` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.revolt]`

```toml
[channels.revolt]
bot_token_env = "REVOLT_BOT_TOKEN"
api_url = "https://api.revolt.chat"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"REVOLT_BOT_TOKEN"` | Env var holding the Revolt bot token. |
| `api_url` | string | `"https://api.revolt.chat"` | Revolt API base URL. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.nextcloud]`

```toml
[channels.nextcloud]
server_url = "https://nextcloud.example.com"
token_env = "NEXTCLOUD_TOKEN"
allowed_rooms = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `""` | Nextcloud server URL. |
| `token_env` | string | `"NEXTCLOUD_TOKEN"` | Env var holding the Nextcloud Talk auth token. |
| `allowed_rooms` | list of strings | `[]` | Room tokens to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.guilded]`

```toml
[channels.guilded]
bot_token_env = "GUILDED_BOT_TOKEN"
server_ids = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"GUILDED_BOT_TOKEN"` | Env var holding the Guilded bot token. |
| `server_ids` | list of strings | `[]` | Server IDs to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.keybase]`

```toml
[channels.keybase]
username = ""
paperkey_env = "KEYBASE_PAPERKEY"
allowed_teams = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `username` | string | `""` | Keybase username. |
| `paperkey_env` | string | `"KEYBASE_PAPERKEY"` | Env var holding the Keybase paper key. |
| `allowed_teams` | list of strings | `[]` | Team names to listen in. Empty = all DMs. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.threema]`

```toml
[channels.threema]
threema_id = ""
secret_env = "THREEMA_SECRET"
webhook_port = 8454
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `threema_id` | string | `""` | Threema Gateway ID. |
| `secret_env` | string | `"THREEMA_SECRET"` | Env var holding the Threema API secret. |
| `webhook_port` | u16 | `8454` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.nostr]`

```toml
[channels.nostr]
private_key_env = "NOSTR_PRIVATE_KEY"
relays = ["wss://relay.damus.io"]
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `private_key_env` | string | `"NOSTR_PRIVATE_KEY"` | Env var holding the Nostr private key (nsec or hex format). |
| `relays` | list of strings | `["wss://relay.damus.io"]` | Nostr relay WebSocket URLs to connect to. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.webex]`

```toml
[channels.webex]
bot_token_env = "WEBEX_BOT_TOKEN"
allowed_rooms = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"WEBEX_BOT_TOKEN"` | Env var holding the Webex bot token. |
| `allowed_rooms` | list of strings | `[]` | Room IDs to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.pumble]`

```toml
[channels.pumble]
bot_token_env = "PUMBLE_BOT_TOKEN"
webhook_port = 8455
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"PUMBLE_BOT_TOKEN"` | Env var holding the Pumble bot token. |
| `webhook_port` | u16 | `8455` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.flock]`

```toml
[channels.flock]
bot_token_env = "FLOCK_BOT_TOKEN"
webhook_port = 8456
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bot_token_env` | string | `"FLOCK_BOT_TOKEN"` | Env var holding the Flock bot token. |
| `webhook_port` | u16 | `8456` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.twist]`

```toml
[channels.twist]
token_env = "TWIST_TOKEN"
workspace_id = ""
allowed_channels = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `token_env` | string | `"TWIST_TOKEN"` | Env var holding the Twist API token. |
| `workspace_id` | string | `""` | Twist workspace ID. |
| `allowed_channels` | list of strings | `[]` | Channel IDs to listen in. Empty = all. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.mumble]`

```toml
[channels.mumble]
host = "mumble.example.com"
port = 64738
username = "openfang"
password_env = "MUMBLE_PASSWORD"
channel = ""
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `host` | string | `""` | Mumble server hostname. |
| `port` | u16 | `64738` | Mumble server port. |
| `username` | string | `"openfang"` | Bot username in Mumble. |
| `password_env` | string | `"MUMBLE_PASSWORD"` | Env var holding the Mumble server password. |
| `channel` | string | `""` | Mumble channel to join. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.dingtalk]`

```toml
[channels.dingtalk]
access_token_env = "DINGTALK_ACCESS_TOKEN"
secret_env = "DINGTALK_SECRET"
webhook_port = 8457
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `access_token_env` | string | `"DINGTALK_ACCESS_TOKEN"` | Env var holding the DingTalk webhook access token. |
| `secret_env` | string | `"DINGTALK_SECRET"` | Env var holding the DingTalk signing secret. |
| `webhook_port` | u16 | `8457` | Port for the incoming webhook. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.discourse]`

```toml
[channels.discourse]
base_url = "https://forum.example.com"
api_key_env = "DISCOURSE_API_KEY"
api_username = "system"
categories = []
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `base_url` | string | `""` | Discourse forum base URL. |
| `api_key_env` | string | `"DISCOURSE_API_KEY"` | Env var holding the Discourse API key. |
| `api_username` | string | `"system"` | Discourse API username. |
| `categories` | list of strings | `[]` | Category slugs to monitor. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.gitter]`

```toml
[channels.gitter]
token_env = "GITTER_TOKEN"
room_id = ""
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `token_env` | string | `"GITTER_TOKEN"` | Env var holding the Gitter auth token. |
| `room_id` | string | `""` | Gitter room ID to listen in. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.ntfy]`

```toml
[channels.ntfy]
server_url = "https://ntfy.sh"
topic = "my-agent-topic"
token_env = "NTFY_TOKEN"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `"https://ntfy.sh"` | ntfy server URL. Can be self-hosted. |
| `topic` | string | `""` | Topic to subscribe/publish to. |
| `token_env` | string | `"NTFY_TOKEN"` | Env var holding the auth token. Optional for public topics. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.gotify]`

```toml
[channels.gotify]
server_url = "https://gotify.example.com"
app_token_env = "GOTIFY_APP_TOKEN"
client_token_env = "GOTIFY_CLIENT_TOKEN"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `server_url` | string | `""` | Gotify server URL. |
| `app_token_env` | string | `"GOTIFY_APP_TOKEN"` | Env var holding the Gotify app token (for sending messages). |
| `client_token_env` | string | `"GOTIFY_CLIENT_TOKEN"` | Env var holding the Gotify client token (for receiving messages via WebSocket). |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.webhook]`

```toml
[channels.webhook]
secret_env = "WEBHOOK_SECRET"
listen_port = 8460
# callback_url = "https://example.com/webhook"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `secret_env` | string | `"WEBHOOK_SECRET"` | Env var holding the HMAC signing secret for verifying incoming webhooks. |
| `listen_port` | u16 | `8460` | Port to listen for incoming webhook requests. |
| `callback_url` | string or null | `null` | URL to POST outgoing messages to. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

#### `[channels.linkedin]`

```toml
[channels.linkedin]
access_token_env = "LINKEDIN_ACCESS_TOKEN"
organization_id = ""
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `access_token_env` | string | `"LINKEDIN_ACCESS_TOKEN"` | Env var holding the LinkedIn OAuth2 access token. |
| `organization_id` | string | `""` | LinkedIn organization ID for messaging. |
| `default_agent` | string or null | `null` | Agent name to route messages to. |

---

### `[[mcp_servers]]`

MCP (Model Context Protocol) server connections provide external tool integration. Each entry is a separate `[[mcp_servers]]` array element.

```toml
[[mcp_servers]]
name = "filesystem"
timeout_secs = 30
env = []

[mcp_servers.transport]
type = "stdio"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/home/user/docs"]
```

```toml
[[mcp_servers]]
name = "remote-api"
timeout_secs = 60
env = ["GITHUB_PERSONAL_ACCESS_TOKEN"]

[mcp_servers.transport]
type = "sse"
url = "https://mcp.example.com/sse"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | *required* | Display name for this MCP server. Tools are namespaced as `mcp_{name}_{tool}`. |
| `timeout_secs` | u64 | `30` | Request timeout in seconds. |
| `env` | list of strings | `[]` | Environment variable names to pass through to the subprocess (stdio transport only). |

**Transport variants** (tagged union on `type`):

| `type` | Fields | Description |
|--------|--------|-------------|
| `stdio` | `command` (string), `args` (list of strings, default `[]`) | Spawn a subprocess, communicate via JSON-RPC over stdin/stdout. |
| `sse` | `url` (string) | Connect to an HTTP Server-Sent Events endpoint. |

---

### `[a2a]`

Agent-to-Agent protocol configuration, enabling inter-agent communication across OpenFang instances.

```toml
[a2a]
enabled = true
listen_path = "/a2a"

[[a2a.external_agents]]
name = "research-agent"
url = "https://agent.example.com/.well-known/agent.json"

[[a2a.external_agents]]
name = "code-reviewer"
url = "https://reviewer.example.com/.well-known/agent.json"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Whether A2A protocol is enabled. |
| `listen_path` | string | `"/a2a"` | URL path prefix for A2A endpoints. |
| `external_agents` | list of objects | `[]` | External A2A agents to discover and interact with. |

**`external_agents` entries:**

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Display name for the external agent. |
| `url` | string | Agent card endpoint URL (typically `/.well-known/agent.json`). |

---

### `[[fallback_providers]]`

Fallback provider chain. When the primary LLM provider (`[default_model]`) fails, these are tried in order.

```toml
[[fallback_providers]]
provider = "ollama"
model = "llama3.2:latest"
api_key_env = ""
# base_url = "http://localhost:11434"

[[fallback_providers]]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | `""` | Provider name (e.g., `"ollama"`, `"groq"`, `"openai"`). |
| `model` | string | `""` | Model identifier for this provider. |
| `api_key_env` | string | `""` | Env var name for the API key. Empty for local providers (ollama, vllm, lmstudio). |
| `base_url` | string or null | `null` | Base URL override. Uses catalog default if null. |

---

### `[[users]]`

RBAC multi-user configuration. Users can be assigned roles and bound to channel platform identities.

```toml
[[users]]
name = "Alice"
role = "owner"
api_key_hash = "sha256_hash_of_api_key"

[users.channel_bindings]
telegram = "123456"
discord = "987654321"
slack = "U0ABCDEFG"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | string | *required* | User display name. |
| `role` | string | `"user"` | User role in the RBAC hierarchy. |
| `channel_bindings` | map of string to string | `{}` | Maps channel platform names to platform-specific user IDs, binding this user identity across channels. |
| `api_key_hash` | string or null | `null` | SHA256 hash of the user's personal API key for authenticated API access. |

**Role hierarchy** (highest to lowest privilege):

| Role | Description |
|------|-------------|
| `owner` | Full administrative access. Can manage all agents, users, and configuration. |
| `admin` | Can manage agents and most settings. Cannot modify owner accounts. |
| `user` | Can interact with agents. Limited management capabilities. |
| `viewer` | Read-only access. Can view agent responses but cannot send messages. |

---

### Channel Overrides

Every channel adapter supports an `[channels.<name>.overrides]` sub-table that customizes agent behavior per-channel.

```toml
[channels.telegram.overrides]
model = "claude-haiku-4-5-20251001"
system_prompt = "You are a concise Telegram assistant."
dm_policy = "respond"
group_policy = "mention_only"
rate_limit_per_user = 10
threading = true
output_format = "telegram_html"
usage_footer = "tokens"
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `model` | string or null | `null` | Model override for this channel. Uses the agent's default model when null. |
| `system_prompt` | string or null | `null` | System prompt override for this channel. |
| `dm_policy` | string | `"respond"` | How the bot handles direct messages. See below. |
| `group_policy` | string | `"mention_only"` | How the bot handles group messages. See below. |
| `rate_limit_per_user` | u32 | `0` | Maximum messages per user per minute. `0` = unlimited. |
| `threading` | bool | `false` | Enable thread replies (where supported by the platform). |
| `output_format` | string or null | `null` | Override output formatting. See below. |
| `usage_footer` | string or null | `null` | Override usage footer mode for this channel. Values: `off`, `tokens`, `cost`, `full`. |

**`dm_policy` values:**

| Value | Description |
|-------|-------------|
| `respond` | Respond to all direct messages (default). |
| `allowed_only` | Only respond to DMs from users in the allowed list. |
| `ignore` | Ignore all direct messages. |

**`group_policy` values:**

| Value | Description |
|-------|-------------|
| `all` | Respond to all messages in group chats. |
| `mention_only` | Only respond when the bot is @mentioned (default). |
| `commands_only` | Only respond to slash commands. |
| `ignore` | Ignore all group messages. |

**`output_format` values:**

| Value | Description |
|-------|-------------|
| `markdown` | Standard Markdown (default). |
| `telegram_html` | Telegram HTML subset (`<b>`, `<i>`, `<code>`, etc.). |
| `slack_mrkdwn` | Slack mrkdwn format (`*bold*`, `_italic_`, `` `code` ``). |
| `plain_text` | No formatting markup. |

---

## Environment Variables

Complete table of all environment variables referenced by the configuration. None of these are read by the config file itself -- they are read at runtime by the kernel and channel adapters.

### LLM Provider Keys

| Variable | Used By | Description |
|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | `[default_model]` | Anthropic API key (Claude models). |
| `GEMINI_API_KEY` | Gemini driver | Google Gemini API key. Alias: `GOOGLE_API_KEY`. |
| `OPENAI_API_KEY` | OpenAI-compat driver | OpenAI API key. |
| `GROQ_API_KEY` | Groq provider | Groq API key (fast Llama inference). |
| `DEEPSEEK_API_KEY` | DeepSeek provider | DeepSeek API key. |
| `PERPLEXITY_API_KEY` | Perplexity provider / web search | Perplexity API key. |
| `OPENROUTER_API_KEY` | OpenRouter provider | OpenRouter API key. |
| `TOGETHER_API_KEY` | Together AI provider | Together AI API key. |
| `MISTRAL_API_KEY` | Mistral provider | Mistral AI API key. |
| `FIREWORKS_API_KEY` | Fireworks provider | Fireworks AI API key. |
| `COHERE_API_KEY` | Cohere provider | Cohere API key. |
| `AI21_API_KEY` | AI21 provider | AI21 Labs API key. |
| `CEREBRAS_API_KEY` | Cerebras provider | Cerebras API key. |
| `SAMBANOVA_API_KEY` | SambaNova provider | SambaNova API key. |
| `HUGGINGFACE_API_KEY` | Hugging Face provider | Hugging Face Inference API key. |
| `XAI_API_KEY` | xAI provider | xAI (Grok) API key. |
| `REPLICATE_API_KEY` | Replicate provider | Replicate API key. |

### Web Search Keys

| Variable | Used By | Description |
|----------|---------|-------------|
| `BRAVE_API_KEY` | `[web.brave]` | Brave Search API key. |
| `TAVILY_API_KEY` | `[web.tavily]` | Tavily Search API key. |
| `PERPLEXITY_API_KEY` | `[web.perplexity]` | Perplexity Search API key (shared with LLM provider). |

### Channel Tokens

| Variable | Channel | Description |
|----------|---------|-------------|
| `TELEGRAM_BOT_TOKEN` | Telegram | Bot API token from @BotFather. |
| `DISCORD_BOT_TOKEN` | Discord | Discord bot token. |
| `SLACK_APP_TOKEN` | Slack | Slack app-level token (`xapp-`) for Socket Mode. |
| `SLACK_BOT_TOKEN` | Slack | Slack bot token (`xoxb-`) for REST API. |
| `WHATSAPP_ACCESS_TOKEN` | WhatsApp | WhatsApp Cloud API access token. |
| `WHATSAPP_VERIFY_TOKEN` | WhatsApp | Webhook verification token. |
| `MATRIX_ACCESS_TOKEN` | Matrix | Matrix homeserver access token. |
| `EMAIL_PASSWORD` | Email | Email account password or app password. |
| `TEAMS_APP_PASSWORD` | Teams | Azure Bot Framework app password. |
| `MATTERMOST_TOKEN` | Mattermost | Mattermost bot token. |
| `TWITCH_OAUTH_TOKEN` | Twitch | Twitch OAuth token. |
| `ROCKETCHAT_TOKEN` | Rocket.Chat | Rocket.Chat auth token. |
| `ZULIP_API_KEY` | Zulip | Zulip bot API key. |
| `XMPP_PASSWORD` | XMPP | XMPP account password. |
| `GOOGLE_CHAT_SERVICE_ACCOUNT` | Google Chat | Service account JSON key. |
| `LINE_CHANNEL_SECRET` | LINE | LINE channel secret. |
| `LINE_CHANNEL_ACCESS_TOKEN` | LINE | LINE channel access token. |
| `VIBER_AUTH_TOKEN` | Viber | Viber Bot auth token. |
| `MESSENGER_PAGE_TOKEN` | Messenger | Facebook page access token. |
| `MESSENGER_VERIFY_TOKEN` | Messenger | Webhook verification token. |
| `REDDIT_CLIENT_SECRET` | Reddit | Reddit app client secret. |
| `REDDIT_PASSWORD` | Reddit | Reddit bot account password. |
| `MASTODON_ACCESS_TOKEN` | Mastodon | Mastodon access token. |
| `BLUESKY_APP_PASSWORD` | Bluesky | Bluesky app password. |
| `FEISHU_APP_SECRET` | Feishu | Feishu/Lark app secret. |
| `REVOLT_BOT_TOKEN` | Revolt | Revolt bot token. |
| `NEXTCLOUD_TOKEN` | Nextcloud | Nextcloud Talk auth token. |
| `GUILDED_BOT_TOKEN` | Guilded | Guilded bot token. |
| `KEYBASE_PAPERKEY` | Keybase | Keybase paper key. |
| `THREEMA_SECRET` | Threema | Threema Gateway API secret. |
| `NOSTR_PRIVATE_KEY` | Nostr | Nostr private key (nsec or hex). |
| `WEBEX_BOT_TOKEN` | Webex | Webex bot token. |
| `PUMBLE_BOT_TOKEN` | Pumble | Pumble bot token. |
| `FLOCK_BOT_TOKEN` | Flock | Flock bot token. |
| `TWIST_TOKEN` | Twist | Twist API token. |
| `MUMBLE_PASSWORD` | Mumble | Mumble server password. |
| `DINGTALK_ACCESS_TOKEN` | DingTalk | DingTalk webhook access token. |
| `DINGTALK_SECRET` | DingTalk | DingTalk signing secret. |
| `DISCOURSE_API_KEY` | Discourse | Discourse API key. |
| `GITTER_TOKEN` | Gitter | Gitter auth token. |
| `NTFY_TOKEN` | ntfy | ntfy auth token (optional for public topics). |
| `GOTIFY_APP_TOKEN` | Gotify | Gotify app token (sending). |
| `GOTIFY_CLIENT_TOKEN` | Gotify | Gotify client token (receiving). |
| `WEBHOOK_SECRET` | Webhook | HMAC signing secret for webhook verification. |
| `LINKEDIN_ACCESS_TOKEN` | LinkedIn | LinkedIn OAuth2 access token. |

---

## Validation

`KernelConfig::validate()` runs at boot time and returns a list of **warnings** (non-fatal). The kernel still starts, but logs each warning.

### What is validated

For every **enabled channel** (i.e., its config section is present in the TOML), the validator checks that the corresponding environment variable(s) are set and non-empty:

| Channel | Env vars checked |
|---------|-----------------|
| Telegram | `bot_token_env` |
| Discord | `bot_token_env` |
| Slack | `app_token_env`, `bot_token_env` (both checked) |
| WhatsApp | `access_token_env` |
| Matrix | `access_token_env` |
| Email | `password_env` |
| Teams | `app_password_env` |
| Mattermost | `token_env` |
| Zulip | `api_key_env` |
| Twitch | `oauth_token_env` |
| Rocket.Chat | `token_env` |
| Google Chat | `service_account_env` |
| XMPP | `password_env` |
| LINE | `access_token_env` |
| Viber | `auth_token_env` |
| Messenger | `page_token_env` |
| Reddit | `client_secret_env` |
| Mastodon | `access_token_env` |
| Bluesky | `app_password_env` |
| Feishu | `app_secret_env` |
| Revolt | `bot_token_env` |
| Nextcloud | `token_env` |
| Guilded | `bot_token_env` |
| Keybase | `paperkey_env` |
| Threema | `secret_env` |
| Nostr | `private_key_env` |
| Webex | `bot_token_env` |
| Pumble | `bot_token_env` |
| Flock | `bot_token_env` |
| Twist | `token_env` |
| Mumble | `password_env` |
| DingTalk | `access_token_env` |
| Discourse | `api_key_env` |
| Gitter | `token_env` |
| ntfy | `token_env` (only if `token_env` is non-empty; public topics are OK without auth) |
| Gotify | `app_token_env` |
| Webhook | `secret_env` |
| LinkedIn | `access_token_env` |

For **web search providers**, the validator checks:

| Provider | Env var checked |
|----------|----------------|
| `brave` | `web.brave.api_key_env` |
| `tavily` | `web.tavily.api_key_env` |
| `perplexity` | `web.perplexity.api_key_env` |
| `duck_duck_go` | (no check -- no API key needed) |
| `auto` | (no check -- cascading fallback handles missing keys) |

### What is NOT validated

- The `api_key_env` in `[default_model]` is not checked by `validate()`. Missing LLM keys cause errors at runtime when the driver is first used.
- The `shared_secret` in `[network]` is not validated against `network_enabled`. If networking is enabled with an empty secret, authentication will fail at connection time.
- MCP server configurations are not validated at config load time. Connection errors surface during the background MCP connect phase.
- Agent manifests have their own separate validation.

---

## Related Configuration

Some subsystems have their own configuration that is not part of `config.toml` but is worth noting:

### Session Compaction (runtime)

Configured internally via `CompactionConfig` (not currently exposed in `config.toml`):

| Field | Default | Description |
|-------|---------|-------------|
| `threshold` | `80` | Compact when session message count exceeds this. |
| `keep_recent` | `20` | Number of recent messages preserved verbatim after compaction. |
| `max_summary_tokens` | `1024` | Maximum tokens for the LLM summary of compacted messages. |

### WASM Sandbox (runtime)

Configured internally via `SandboxConfig` (not currently exposed in `config.toml`):

| Field | Default | Description |
|-------|---------|-------------|
| `fuel_limit` | `1000000` | Maximum CPU instruction budget. `0` = unlimited. |
| `max_memory_bytes` | `16777216` (16 MB) | Maximum WASM linear memory. |
| `timeout_secs` | `null` (30s fallback) | Wall-clock timeout for epoch-based interruption. |

### Model Routing (per-agent manifest)

Configured in agent manifests via `ModelRoutingConfig`:

| Field | Default | Description |
|-------|---------|-------------|
| `simple_model` | `"claude-haiku-4-5-20251001"` | Model for simple queries. |
| `medium_model` | `"claude-sonnet-4-20250514"` | Model for medium-complexity queries. |
| `complex_model` | `"claude-sonnet-4-20250514"` | Model for complex queries. |
| `simple_threshold` | `100` | Token count below which a query is classified as simple. |
| `complex_threshold` | `500` | Token count above which a query is classified as complex. |

### Heartbeat Monitor

Global heartbeat settings in `[heartbeat]`:

```toml
[heartbeat]
# Seconds of inactivity before a reactive agent is marked as unresponsive.
# Increase this if you have hands that sit idle between infrequent requests.
# Default: 180
default_timeout_secs = 300
```

| Field | Default | Description |
|-------|---------|-------------|
| `default_timeout_secs` | `180` | Seconds of inactivity before marking an agent as unresponsive. Per-agent `heartbeat_interval_secs` in autonomous config overrides this. |

### Autonomous Guardrails (per-agent manifest)

Configured in agent manifests via `AutonomousConfig`:

| Field | Default | Description |
|-------|---------|-------------|
| `quiet_hours` | `null` | Cron expression for quiet hours (agent pauses during this window). |
| `max_iterations` | `50` | Maximum tool-use iterations per invocation. |
| `max_restarts` | `10` | Maximum automatic restarts before permanent stop. |
| `heartbeat_interval_secs` | `30` | Seconds between heartbeat health checks. |
| `heartbeat_channel` | `null` | Channel to send heartbeat status to (e.g., `"telegram"`). |
