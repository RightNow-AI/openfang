# API Reference

OpenFang exposes a REST API, WebSocket endpoints, and SSE streaming when the daemon is running. The default listen address is `http://127.0.0.1:4200`.

All responses include security headers (CSP, X-Frame-Options, X-Content-Type-Options, HSTS) and are protected by a GCRA cost-aware rate limiter with per-IP token bucket tracking and automatic stale entry cleanup. OpenFang implements 16 security systems including Merkle audit trails, taint tracking, WASM dual metering, Ed25519 manifest signing, SSRF protection, subprocess sandboxing, and secret zeroization.

## Table of Contents

- [Authentication](#authentication)
- [Agent Endpoints](#agent-endpoints)
- [Workflow Endpoints](#workflow-endpoints)
- [Trigger Endpoints](#trigger-endpoints)
- [Memory Endpoints](#memory-endpoints)
- [Channel Endpoints](#channel-endpoints)
- [Template Endpoints](#template-endpoints)
- [System Endpoints](#system-endpoints)
- [Model Catalog Endpoints](#model-catalog-endpoints)
- [Provider Configuration Endpoints](#provider-configuration-endpoints)
- [Skills & Marketplace Endpoints](#skills--marketplace-endpoints)
- [ClawHub Endpoints](#clawhub-endpoints)
- [MCP & A2A Protocol Endpoints](#mcp--a2a-protocol-endpoints)
- [Audit & Security Endpoints](#audit--security-endpoints)
- [Usage & Analytics Endpoints](#usage--analytics-endpoints)
- [Migration Endpoints](#migration-endpoints)
- [Session Management Endpoints](#session-management-endpoints)
- [Cron/Scheduler Endpoints](#cronscheduler-endpoints)
- [WebSocket Protocol](#websocket-protocol)
- [SSE Streaming](#sse-streaming)
- [OpenAI-Compatible API](#openai-compatible-api)
- [Error Responses](#error-responses)

---

## Authentication

When an API key is configured in `config.toml`, all endpoints (except `/api/health` and `/`) require a Bearer token:

```
Authorization: Bearer <your-api-key>
```

### Setting the API Key

Add to `~/.openfang/config.toml`:

```toml
api_key = "your-secret-api-key"
```

### No Authentication

If `api_key` is empty or not set, the API is accessible without authentication. CORS is restricted to localhost origins in this mode.

### Public Endpoints (No Auth Required)

- `GET /api/health`
- `GET /` (WebChat UI)

---

## Agent Endpoints

### GET /api/agents

List all running agents.

**Response** `200 OK`:

```json
[
  {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "name": "hello-world",
    "state": "Running",
    "created_at": "2025-01-15T10:30:00Z",
    "model_provider": "groq",
    "model_name": "llama-3.3-70b-versatile"
  }
]
```

### GET /api/agents/{id}

Returns detailed information about a single agent.

**Response** `200 OK`:

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "name": "hello-world",
  "state": "Running",
  "created_at": "2025-01-15T10:30:00Z",
  "session_id": "s1b2c3d4-...",
  "model": {
    "provider": "groq",
    "model": "llama-3.3-70b-versatile"
  },
  "capabilities": {
    "tools": ["file_read", "file_list", "web_fetch"],
    "network": []
  },
  "description": "A friendly greeting agent",
  "tags": []
}
```

### POST /api/agents

Spawn a new agent from a TOML manifest.

**Request Body** (JSON):

```json
{
  "manifest_toml": "name = \"my-agent\"\nversion = \"0.1.0\"\ndescription = \"Test agent\"\nauthor = \"me\"\nmodule = \"builtin:chat\"\n\n[model]\nprovider = \"groq\"\nmodel = \"llama-3.3-70b-versatile\"\n\n[capabilities]\ntools = [\"file_read\", \"web_fetch\"]\nmemory_read = [\"*\"]\nmemory_write = [\"self.*\"]\n"
}
```

**Response** `201 Created`:

```json
{
  "agent_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "name": "my-agent"
}
```

### PUT /api/agents/{id}/update

Update an agent's configuration at runtime.

**Request Body**:

```json
{
  "description": "Updated description",
  "system_prompt": "You are a specialized assistant.",
  "tags": ["updated", "v2"]
}
```

**Response** `200 OK`:

```json
{
  "status": "updated",
  "agent_id": "a1b2c3d4-..."
}
```

### PUT /api/agents/{id}/mode

Set an agent's operating mode. `Stable` mode pins the current model and freezes the skill registry. `Normal` mode restores default behavior.

**Request Body**:

```json
{
  "mode": "Stable"
}
```

**Response** `200 OK`:

```json
{
  "status": "updated",
  "mode": "Stable",
  "agent_id": "a1b2c3d4-..."
}
```

### POST /api/agents/{id}/message

Send a message to an agent and receive the complete response.

**Request Body**:

```json
{
  "message": "What files are in the current directory?"
}
```

**Response** `200 OK`:

```json
{
  "response": "Here are the files in the current directory:\n- Cargo.toml\n- README.md\n...",
  "input_tokens": 142,
  "output_tokens": 87,
  "iterations": 1
}
```

### GET /api/agents/{id}/session

Returns the agent's conversation history.

**Response** `200 OK`:

```json
{
  "session_id": "s1b2c3d4-...",
  "agent_id": "a1b2c3d4-...",
  "message_count": 4,
  "context_window_tokens": 1250,
  "messages": [
    {
      "role": "User",
      "content": "Hello"
    },
    {
      "role": "Assistant",
      "content": "Hello! How can I help you?"
    }
  ]
}
```

### DELETE /api/agents/{id}

Terminate an agent and remove it from the registry.

**Response** `200 OK`:

```json
{
  "status": "killed",
  "agent_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890"
}
```

---

## Workflow Endpoints

### GET /api/workflows

List all registered workflows.

**Response** `200 OK`:

```json
[
  {
    "id": "w1b2c3d4-...",
    "name": "code-review-pipeline",
    "description": "Automated code review workflow",
    "steps": 3,
    "created_at": "2025-01-15T10:30:00Z"
  }
]
```

### POST /api/workflows

Create a new workflow definition.

**Request Body** (JSON):

```json
{
  "name": "code-review-pipeline",
  "description": "Review code changes with multiple agents",
  "steps": [
    {
      "name": "analyze",
      "agent_name": "coder",
      "prompt": "Analyze this code for potential issues: {{input}}",
      "mode": "sequential",
      "timeout_secs": 120,
      "error_mode": "fail",
      "output_var": "analysis"
    },
    {
      "name": "security-check",
      "agent_name": "security-auditor",
      "prompt": "Review this code analysis for security vulnerabilities: {{analysis}}",
      "mode": "sequential",
      "timeout_secs": 120,
      "error_mode": "skip"
    },
    {
      "name": "summarize",
      "agent_name": "writer",
      "prompt": "Write a concise code review summary based on: {{analysis}}",
      "mode": "sequential",
      "timeout_secs": 60,
      "error_mode": "fail"
    }
  ]
}
```

**Step configuration options:**

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Step name |
| `agent_id` | string | Agent UUID (use either this or `agent_name`) |
| `agent_name` | string | Agent name (use either this or `agent_id`) |
| `prompt` | string | Prompt template with `{{input}}` and `{{output_var}}` placeholders |
| `mode` | string | `"sequential"`, `"fan_out"`, `"collect"`, `"conditional"`, `"loop"` |
| `timeout_secs` | integer | Timeout per step (default: 120) |
| `error_mode` | string | `"fail"`, `"skip"`, `"retry"` |
| `max_retries` | integer | For `"retry"` error mode (default: 3) |
| `output_var` | string | Variable name to store output for later steps |
| `condition` | string | For `"conditional"` mode |
| `max_iterations` | integer | For `"loop"` mode (default: 5) |
| `until` | string | For `"loop"` mode: stop condition |

**Response** `201 Created`:

```json
{
  "workflow_id": "w1b2c3d4-..."
}
```

### POST /api/workflows/{id}/run

Execute a workflow.

**Request Body**:

```json
{
  "input": "Review this pull request: ..."
}
```

**Response** `200 OK`:

```json
{
  "run_id": "r1b2c3d4-...",
  "output": "Code review summary:\n- No critical issues found\n...",
  "status": "completed"
}
```

### GET /api/workflows/{id}/runs

List execution history for a workflow.

**Response** `200 OK`:

```json
[
  {
    "id": "r1b2c3d4-...",
    "workflow_name": "code-review-pipeline",
    "state": "Completed",
    "steps_completed": 3,
    "started_at": "2025-01-15T10:30:00Z",
    "completed_at": "2025-01-15T10:32:15Z"
  }
]
```

---

## Trigger Endpoints

### GET /api/triggers

List all triggers. Optionally filter by agent.

**Query Parameters:**
- `agent_id` (optional): Filter by agent UUID

**Response** `200 OK`:

```json
[
  {
    "id": "t1b2c3d4-...",
    "agent_id": "a1b2c3d4-...",
    "pattern": {"lifecycle": {}},
    "prompt_template": "Event: {{event}}",
    "enabled": true,
    "fire_count": 5,
    "max_fires": 0,
    "created_at": "2025-01-15T10:30:00Z"
  }
]
```

### POST /api/triggers

Create a new event trigger.

**Request Body**:

```json
{
  "agent_id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "pattern": {
    "agent_spawned": {
      "name_pattern": "*"
    }
  },
  "prompt_template": "A new agent was spawned: {{event}}. Review its capabilities.",
  "max_fires": 0
}
```

**Supported pattern types:**

| Pattern | Description |
|---------|-------------|
| `{"lifecycle": {}}` | All lifecycle events |
| `{"agent_spawned": {"name_pattern": "*"}}` | Agent spawn events |
| `{"agent_terminated": {}}` | Agent termination events |
| `{"all": {}}` | All events |

**Response** `201 Created`:

```json
{
  "trigger_id": "t1b2c3d4-...",
  "agent_id": "a1b2c3d4-..."
}
```

### PUT /api/triggers/{id}

Update an existing trigger's configuration.

**Request Body**:

```json
{
  "prompt_template": "Updated template: {{event}}",
  "enabled": false,
  "max_fires": 10
}
```

**Response** `200 OK`:

```json
{
  "status": "updated",
  "trigger_id": "t1b2c3d4-..."
}
```

### DELETE /api/triggers/{id}

Remove a trigger.

**Response** `200 OK`:

```json
{
  "status": "removed",
  "trigger_id": "t1b2c3d4-..."
}
```

---

## Memory Endpoints

### GET /api/memory/agents/{id}/kv

List all key-value pairs for an agent.

**Response** `200 OK`:

```json
{
  "kv_pairs": [
    {"key": "preferences", "value": {"theme": "dark"}},
    {"key": "state", "value": {"step": 3}}
  ]
}
```

### GET /api/memory/agents/{id}/kv/{key}

Get a specific key-value pair.

**Response** `200 OK`:

```json
{
  "key": "preferences",
  "value": {"theme": "dark"}
}
```

**Response** `404 Not Found` (key does not exist):

```json
{
  "error": "Key 'preferences' not found"
}
```

### PUT /api/memory/agents/{id}/kv/{key}

Set a key-value pair. Creates or overwrites.

**Request Body**:

```json
{
  "value": {"theme": "dark", "language": "en"}
}
```

**Response** `200 OK`:

```json
{
  "status": "stored",
  "key": "preferences"
}
```

### DELETE /api/memory/agents/{id}/kv/{key}

Delete a key-value pair.

**Response** `200 OK`:

```json
{
  "status": "deleted",
  "key": "preferences"
}
```

---

## Channel Endpoints

### GET /api/channels

List configured channel adapters and their status. Supports 40 channel adapters including Telegram, Discord, Slack, WhatsApp, Matrix, Email, Teams, Mattermost, IRC, Google Chat, Twitch, Rocket.Chat, Zulip, XMPP, LINE, Viber, Messenger, Reddit, Mastodon, Bluesky, and more.

**Response** `200 OK`:

```json
{
  "channels": [
    {
      "name": "telegram",
      "enabled": true,
      "has_token": true
    },
    {
      "name": "discord",
      "enabled": true,
      "has_token": false
    }
  ],
  "total": 2
}
```

---

## Template Endpoints

### GET /api/templates

List available agent templates from the agents directory.

**Response** `200 OK`:

```json
{
  "templates": [
    {
      "name": "hello-world",
      "description": "A friendly greeting agent",
      "path": "/home/user/.openfang/agents/hello-world/agent.toml"
    },
    {
      "name": "coder",
      "description": "Expert coding assistant",
      "path": "/home/user/.openfang/agents/coder/agent.toml"
    }
  ],
  "total": 30
}
```

### GET /api/templates/{name}

Get a specific template's manifest and raw TOML.

**Response** `200 OK`:

```json
{
  "name": "hello-world",
  "manifest": {
    "name": "hello-world",
    "description": "A friendly greeting agent",
    "module": "builtin:chat",
    "tags": [],
    "model": {
      "provider": "groq",
      "model": "llama-3.3-70b-versatile"
    },
    "capabilities": {
      "tools": ["file_read", "file_list", "web_fetch"],
      "network": []
    }
  },
  "manifest_toml": "name = \"hello-world\"\nversion = \"0.1.0\"\n..."
}
```

---

## System Endpoints

### GET /api/health

Public health check. Does not require authentication. Returns a redacted subset of system status (no database or agent_count details).

**Response** `200 OK`:

```json
{
  "status": "ok",
  "uptime_seconds": 3600,
  "panic_count": 0,
  "restart_count": 0
}
```

The `status` field is `"ok"` when all systems are healthy, or `"degraded"` when the database is unreachable.

### GET /api/health/detail

Full health check with all dependency status. Requires authentication. Unlike the public `/api/health`, this endpoint includes database connectivity and agent counts.

**Response** `200 OK`:

```json
{
  "status": "ok",
  "uptime_seconds": 3600,
  "panic_count": 0,
  "restart_count": 0,
  "agent_count": 3,
  "database": "connected",
  "config_warnings": []
}
```

### GET /api/status

Detailed kernel status including all agents.

**Response** `200 OK`:

```json
{
  "status": "running",
  "agent_count": 2,
  "data_dir": "/home/user/.openfang/data",
  "default_provider": "groq",
  "default_model": "llama-3.3-70b-versatile",
  "uptime_seconds": 3600,
  "agents": [
    {
      "id": "a1b2c3d4-...",
      "name": "hello-world",
      "state": "Running",
      "created_at": "2025-01-15T10:30:00Z",
      "model_provider": "groq",
      "model_name": "llama-3.3-70b-versatile"
    }
  ]
}
```

### GET /api/version

Build and version information.

**Response** `200 OK`:

```json
{
  "name": "openfang",
  "version": "0.1.0",
  "build_date": "2025-01-15",
  "git_sha": "abc1234",
  "rust_version": "1.82.0",
  "platform": "linux",
  "arch": "x86_64"
}
```

### POST /api/shutdown

Initiate graceful shutdown. Agent states are preserved to SQLite for restore on next boot.

**Response** `200 OK`:

```json
{
  "status": "shutting_down"
}
```

### GET /api/profiles

List available agent profiles (predefined configurations for common use cases).

**Response** `200 OK`:

```json
{
  "profiles": [
    {
      "name": "coder",
      "tier": "smart",
      "description": "Expert coding assistant"
    },
    {
      "name": "researcher",
      "tier": "frontier",
      "description": "Deep research and analysis"
    }
  ]
}
```

### GET /api/tools

List all available tools that agents can use.

**Response** `200 OK`:

```json
{
  "tools": [
    "file_read",
    "file_write",
    "file_list",
    "web_fetch",
    "web_search",
    "shell_exec",
    "kv_get",
    "kv_set",
    "agent_call"
  ],
  "total": 23
}
```

### GET /api/config

Retrieve current kernel configuration (secrets are redacted).

**Response** `200 OK`:

```json
{
  "data_dir": "/home/user/.openfang/data",
  "default_provider": "groq",
  "default_model": "llama-3.3-70b-versatile",
  "listen_addr": "127.0.0.1:4200",
  "api_key_set": true,
  "channels_configured": 2,
  "mcp_servers": 1
}
```

### GET /api/peers

List OFP (OpenFang Protocol) wire peers and their connection status.

**Response** `200 OK`:

```json
{
  "peers": [
    {
      "node_id": "peer-1",
      "address": "192.168.1.100:4000",
      "state": "connected",
      "authenticated": true,
      "last_seen": "2025-01-15T10:30:00Z"
    }
  ]
}
```

### GET /api/sessions

List all active sessions across agents.

**Response** `200 OK`:

```json
{
  "sessions": [
    {
      "id": "s1b2c3d4-...",
      "agent_id": "a1b2c3d4-...",
      "agent_name": "coder",
      "message_count": 12,
      "created_at": "2025-01-15T10:30:00Z"
    }
  ]
}
```

### DELETE /api/sessions/{id}

Delete a specific session and its conversation history.

**Response** `200 OK`:

```json
{
  "status": "deleted",
  "session_id": "s1b2c3d4-..."
}
```

---

## Model Catalog Endpoints

OpenFang maintains a built-in catalog of 51+ models across 20 providers. These endpoints allow you to browse available models, check provider authentication status, and resolve model aliases.

### GET /api/models

List the full model catalog. Returns all known models with their provider, tier, context window, and pricing information.

**Response** `200 OK`:

```json
{
  "models": [
    {
      "id": "claude-sonnet-4-20250514",
      "provider": "anthropic",
      "display_name": "Claude Sonnet 4",
      "tier": "frontier",
      "context_window": 200000,
      "input_cost_per_1m": 3.0,
      "output_cost_per_1m": 15.0,
      "supports_tools": true,
      "supports_vision": true,
      "supports_streaming": true
    },
    {
      "id": "gemini-2.5-flash",
      "provider": "gemini",
      "display_name": "Gemini 2.5 Flash",
      "tier": "smart",
      "context_window": 1048576,
      "input_cost_per_1m": 0.15,
      "output_cost_per_1m": 0.6,
      "supports_tools": true,
      "supports_vision": true,
      "supports_streaming": true
    }
  ],
  "total": 51
}
```

### GET /api/models/{id}

Get detailed information about a specific model.

**Response** `200 OK`:

```json
{
  "id": "llama-3.3-70b-versatile",
  "provider": "groq",
  "display_name": "Llama 3.3 70B",
  "tier": "fast",
  "context_window": 131072,
  "input_cost_per_1m": 0.59,
  "output_cost_per_1m": 0.79,
  "supports_tools": true,
  "supports_vision": false,
  "supports_streaming": true
}
```

**Response** `404 Not Found`:

```json
{
  "error": "Model 'unknown-model' not found in catalog"
}
```

### GET /api/models/aliases

List all model aliases. Aliases provide short names that resolve to full model IDs (e.g., `sonnet` resolves to `claude-sonnet-4-20250514`).

**Response** `200 OK`:

```json
{
  "aliases": {
    "sonnet": "claude-sonnet-4-20250514",
    "opus": "claude-opus-4-20250514",
    "haiku": "claude-3-5-haiku-20241022",
    "flash": "gemini-2.5-flash",
    "gpt4": "gpt-4o",
    "llama": "llama-3.3-70b-versatile",
    "deepseek": "deepseek-chat",
    "grok": "grok-2",
    "jamba": "jamba-1.5-large"
  },
  "total": 23
}
```

### GET /api/providers

List all known LLM providers and their authentication status. Auth status is detected by checking environment variable presence (never reads secret values).

**Response** `200 OK`:

```json
{
  "providers": [
    {
      "name": "anthropic",
      "display_name": "Anthropic",
      "auth_status": "configured",
      "env_var": "ANTHROPIC_API_KEY",
      "base_url": "https://api.anthropic.com",
      "model_count": 3
    },
    {
      "name": "groq",
      "display_name": "Groq",
      "auth_status": "configured",
      "env_var": "GROQ_API_KEY",
      "base_url": "https://api.groq.com/openai",
      "model_count": 4
    },
    {
      "name": "ollama",
      "display_name": "Ollama",
      "auth_status": "no_key_needed",
      "base_url": "http://localhost:11434",
      "model_count": 0
    }
  ],
  "total": 20
}
```

---

## Provider Configuration Endpoints

Manage LLM provider API keys at runtime without editing config files or restarting the daemon.

### POST /api/providers/{name}/key

Set an API key for a provider. The key is stored securely and takes effect immediately.

**Request Body**:

```json
{
  "api_key": "sk-..."
}
```

**Response** `200 OK`:

```json
{
  "status": "configured",
  "provider": "anthropic"
}
```

### DELETE /api/providers/{name}/key

Remove the API key for a provider. Agents using this provider will fall back to the FallbackDriver or fail.

**Response** `200 OK`:

```json
{
  "status": "removed",
  "provider": "anthropic"
}
```

### POST /api/providers/{name}/test

Test provider connectivity by making a minimal API call. Verifies that the configured API key is valid and the provider endpoint is reachable.

**Response** `200 OK`:

```json
{
  "status": "ok",
  "provider": "anthropic",
  "latency_ms": 245,
  "model_tested": "claude-sonnet-4-20250514"
}
```

**Response** `401 Unauthorized`:

```json
{
  "status": "failed",
  "provider": "anthropic",
  "error": "Invalid API key"
}
```

---

## Skills & Marketplace Endpoints

Manage the skill registry. Skills extend agent capabilities with Python, Node.js, WASM, or prompt-only modules. All skill installations go through SHA256 verification and prompt injection scanning.

### GET /api/skills

List all installed skills.

**Response** `200 OK`:

```json
{
  "skills": [
    {
      "name": "github",
      "version": "1.0.0",
      "runtime": "prompt_only",
      "description": "GitHub integration for issues, PRs, and repos",
      "bundled": true
    },
    {
      "name": "docker",
      "version": "1.0.0",
      "runtime": "prompt_only",
      "description": "Docker container management",
      "bundled": true
    }
  ],
  "total": 60
}
```

### POST /api/skills/install

Install a skill from a local path or URL. The skill manifest is verified (SHA256 checksum) and scanned for prompt injection before installation.

**Request Body**:

```json
{
  "source": "/path/to/skill",
  "verify": true
}
```

**Response** `201 Created`:

```json
{
  "status": "installed",
  "skill": "my-custom-skill",
  "version": "1.0.0"
}
```

### POST /api/skills/uninstall

Remove an installed skill. Bundled skills cannot be uninstalled.

**Request Body**:

```json
{
  "name": "my-custom-skill"
}
```

**Response** `200 OK`:

```json
{
  "status": "uninstalled",
  "skill": "my-custom-skill"
}
```

### POST /api/skills/create

Create a new skill from a template.

**Request Body**:

```json
{
  "name": "my-skill",
  "runtime": "python",
  "description": "A custom skill"
}
```

**Response** `201 Created`:

```json
{
  "status": "created",
  "skill": "my-skill",
  "path": "/home/user/.openfang/skills/my-skill"
}
```

### GET /api/marketplace/search

Search the FangHub marketplace for community skills.

**Query Parameters:**
- `q` (required): Search query string
- `page` (optional): Page number (default: 1)

**Response** `200 OK`:

```json
{
  "results": [
    {
      "name": "weather-api",
      "author": "community",
      "description": "Real-time weather data integration",
      "downloads": 1250,
      "version": "2.1.0"
    }
  ],
  "total": 1,
  "page": 1
}
```

---

## ClawHub Endpoints

Browse and install skills from ClawHub (OpenClaw ecosystem compatibility). All installations go through the full security pipeline: SHA256 verification, SKILL.md security scanning, and trust boundary enforcement.

### GET /api/clawhub/search

Search ClawHub for compatible skills.

**Query Parameters:**
- `q` (required): Search query

**Response** `200 OK`:

```json
{
  "results": [
    {
      "slug": "data-pipeline",
      "name": "Data Pipeline",
      "description": "ETL data pipeline automation",
      "author": "clawhub-community",
      "version": "1.2.0"
    }
  ],
  "total": 1
}
```

### GET /api/clawhub/browse

Browse ClawHub categories.

**Query Parameters:**
- `category` (optional): Filter by category
- `page` (optional): Page number (default: 1)

**Response** `200 OK`:

```json
{
  "skills": [
    {
      "slug": "data-pipeline",
      "name": "Data Pipeline",
      "category": "data",
      "description": "ETL data pipeline automation"
    }
  ],
  "total": 15,
  "page": 1
}
```

### GET /api/clawhub/skill/{slug}

Get detailed information about a specific ClawHub skill.

**Response** `200 OK`:

```json
{
  "slug": "data-pipeline",
  "name": "Data Pipeline",
  "description": "ETL data pipeline automation",
  "author": "clawhub-community",
  "version": "1.2.0",
  "runtime": "python",
  "readme": "# Data Pipeline\n\nAutomated ETL...",
  "sha256": "a1b2c3d4..."
}
```

### POST /api/clawhub/install

Install a skill from ClawHub. Downloads, verifies SHA256 checksum, scans for prompt injection, and converts SKILL.md format to OpenFang skill.toml automatically.

**Request Body**:

```json
{
  "slug": "data-pipeline"
}
```

**Response** `201 Created`:

```json
{
  "status": "installed",
  "skill": "data-pipeline",
  "version": "1.2.0",
  "converted_from": "SKILL.md"
}
```

---

## MCP & A2A Protocol Endpoints

OpenFang supports both Model Context Protocol (MCP) for tool interoperability and Agent-to-Agent (A2A) protocol for cross-system agent communication.

### GET /api/mcp/servers

List configured and connected MCP servers with their available tools.

**Response** `200 OK`:

```json
{
  "servers": [
    {
      "name": "filesystem",
      "transport": "stdio",
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem"],
      "connected": true,
      "tools": [
        {
          "name": "mcp_filesystem_read_file",
          "description": "Read a file from the filesystem"
        },
        {
          "name": "mcp_filesystem_write_file",
          "description": "Write content to a file"
        }
      ]
    }
  ],
  "total": 1
}
```

### POST /mcp

MCP HTTP transport endpoint. Accepts JSON-RPC 2.0 requests and exposes OpenFang tools via the MCP protocol to external clients.

**Request Body** (JSON-RPC 2.0):

```json
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "id": 1
}
```

**Response** `200 OK`:

```json
{
  "jsonrpc": "2.0",
  "result": {
    "tools": [
      {
        "name": "file_read",
        "description": "Read a file's contents",
        "inputSchema": {
          "type": "object",
          "properties": {
            "path": {"type": "string"}
          }
        }
      }
    ]
  },
  "id": 1
}
```

### GET /.well-known/agent.json

A2A agent card discovery endpoint. Returns the server's A2A agent card, which describes its capabilities, supported protocols, and available agents.

**Response** `200 OK`:

```json
{
  "name": "OpenFang",
  "description": "OpenFang Agent Operating System",
  "url": "http://127.0.0.1:4200",
  "version": "0.1.0",
  "capabilities": {
    "streaming": true,
    "pushNotifications": false
  },
  "skills": [
    {
      "id": "chat",
      "name": "Chat",
      "description": "General-purpose chat with any agent"
    }
  ]
}
```

### GET /a2a/agents

List agents available via A2A protocol.

**Response** `200 OK`:

```json
{
  "agents": [
    {
      "id": "a1b2c3d4-...",
      "name": "coder",
      "description": "Expert coding assistant",
      "skills": ["code-review", "debugging", "refactoring"]
    }
  ]
}
```

### POST /a2a/tasks/send

Send a task to an agent via A2A protocol. Follows the Google A2A specification for inter-agent task delegation.

**Request Body**:

```json
{
  "agent_id": "a1b2c3d4-...",
  "message": {
    "role": "user",
    "parts": [
      {"text": "Review this code for security issues"}
    ]
  }
}
```

**Response** `200 OK`:

```json
{
  "task_id": "task-1234-...",
  "status": "completed",
  "result": {
    "role": "agent",
    "parts": [
      {"text": "I found 2 potential security issues..."}
    ]
  }
}
```

### GET /a2a/tasks/{id}

Get the status and result of an A2A task.

**Response** `200 OK`:

```json
{
  "task_id": "task-1234-...",
  "status": "completed",
  "created_at": "2025-01-15T10:30:00Z",
  "completed_at": "2025-01-15T10:30:05Z",
  "result": {
    "role": "agent",
    "parts": [
      {"text": "Analysis complete..."}
    ]
  }
}
```

### POST /a2a/tasks/{id}/cancel

Cancel a running A2A task.

**Response** `200 OK`:

```json
{
  "task_id": "task-1234-...",
  "status": "cancelled"
}
```

---

## Audit & Security Endpoints

OpenFang maintains a Merkle hash chain audit trail for all security-relevant operations. These endpoints allow inspection and verification of the audit log integrity.

### GET /api/audit/recent

Retrieve recent audit log entries.

**Query Parameters:**
- `limit` (optional): Number of entries to return (default: 50, max: 500)

**Response** `200 OK`:

```json
{
  "entries": [
    {
      "id": 1042,
      "timestamp": "2025-01-15T10:30:00Z",
      "event_type": "agent_spawned",
      "agent_id": "a1b2c3d4-...",
      "details": "Agent 'coder' spawned with model groq/llama-3.3-70b-versatile",
      "hash": "a1b2c3d4e5f6...",
      "prev_hash": "f6e5d4c3b2a1..."
    }
  ],
  "total": 1042
}
```

### GET /api/audit/verify

Verify the integrity of the Merkle hash chain audit trail. Walks the entire chain and reports any broken links.

**Response** `200 OK`:

```json
{
  "status": "valid",
  "chain_length": 1042,
  "first_entry": "2025-01-10T08:00:00Z",
  "last_entry": "2025-01-15T10:30:00Z"
}
```

**Response** `200 OK` (chain broken):

```json
{
  "status": "broken",
  "chain_length": 1042,
  "break_at": 847,
  "error": "Hash mismatch at entry 847"
}
```

### GET /api/security

Security status overview showing the state of all 16 security systems.

**Response** `200 OK`:

```json
{
  "security_systems": {
    "merkle_audit_trail": "active",
    "taint_tracking": "active",
    "wasm_dual_metering": "active",
    "security_headers": "active",
    "health_redaction": "active",
    "subprocess_sandbox": "active",
    "manifest_signing": "active",
    "gcra_rate_limiter": "active",
    "secret_zeroization": "active",
    "path_traversal_prevention": "active",
    "ssrf_protection": "active",
    "capability_inheritance_validation": "active",
    "ofp_hmac_auth": "active",
    "prompt_injection_scanning": "active",
    "loop_guard": "active",
    "session_repair": "active"
  },
  "total_systems": 16,
  "all_active": true
}
```

---

## Usage & Analytics Endpoints

Track token usage, costs, and model utilization across all agents. Powered by the metering engine with cost estimation from the model catalog.

### GET /api/usage

Get overall usage statistics.

**Query Parameters:**
- `period` (optional): Time period (`hour`, `day`, `week`, `month`; default: `day`)

**Response** `200 OK`:

```json
{
  "period": "day",
  "total_input_tokens": 125000,
  "total_output_tokens": 87000,
  "total_cost_usd": 0.42,
  "request_count": 156,
  "active_agents": 5
}
```

### GET /api/usage/summary

Get a high-level usage summary with quota information.

**Response** `200 OK`:

```json
{
  "today": {
    "input_tokens": 125000,
    "output_tokens": 87000,
    "cost_usd": 0.42,
    "requests": 156
  },
  "quota": {
    "hourly_token_limit": 1000000,
    "hourly_tokens_used": 45000,
    "hourly_reset_at": "2025-01-15T11:00:00Z"
  }
}
```

### GET /api/usage/by-model

Get usage breakdown by model.

**Response** `200 OK`:

```json
{
  "models": [
    {
      "model": "llama-3.3-70b-versatile",
      "provider": "groq",
      "input_tokens": 80000,
      "output_tokens": 55000,
      "cost_usd": 0.09,
      "request_count": 120
    },
    {
      "model": "gemini-2.5-flash",
      "provider": "gemini",
      "input_tokens": 45000,
      "output_tokens": 32000,
      "cost_usd": 0.33,
      "request_count": 36
    }
  ]
}
```

---

## Migration Endpoints

Import data from OpenClaw or other agent frameworks. The migration engine handles YAML-to-TOML manifest conversion, SKILL.md parsing, and session history import.

### GET /api/migrate/detect

Auto-detect migration sources on the system. Scans common locations for OpenClaw installations, config files, and agent data.

**Response** `200 OK`:

```json
{
  "sources": [
    {
      "type": "openclaw",
      "path": "/home/user/.openclaw",
      "version": "2.1.0",
      "agents_found": 12,
      "skills_found": 8
    }
  ]
}
```

### POST /api/migrate/scan

Scan a specific path for importable data.

**Request Body**:

```json
{
  "path": "/home/user/.openclaw"
}
```

**Response** `200 OK`:

```json
{
  "agents": [
    {
      "name": "my-agent",
      "format": "yaml",
      "convertible": true
    }
  ],
  "skills": [
    {
      "name": "custom-skill",
      "format": "SKILL.md",
      "convertible": true
    }
  ],
  "sessions": 45
}
```

### POST /api/migrate

Run the migration. Converts manifests, imports skills, and optionally imports session history.

**Request Body**:

```json
{
  "source": "/home/user/.openclaw",
  "import_agents": true,
  "import_skills": true,
  "import_sessions": false
}
```

**Response** `200 OK`:

```json
{
  "status": "completed",
  "agents_imported": 12,
  "skills_imported": 8,
  "sessions_imported": 0,
  "warnings": [
    "Skill 'legacy-plugin' uses unsupported runtime 'ruby', skipped"
  ]
}
```

---

## Session Management Endpoints

### POST /api/agents/{id}/session/reset

Reset an agent's session, clearing all conversation history.

**Response** `200 OK`:

```json
{
  "status": "reset",
  "agent_id": "a1b2c3d4-...",
  "new_session_id": "s5e6f7g8-..."
}
```

### POST /api/agents/{id}/session/compact

Trigger LLM-based session compaction. The agent's conversation is summarized by an LLM, keeping only the most recent messages plus a generated summary.

**Response** `200 OK`:

```json
{
  "status": "compacted",
  "message": "Session compacted: 80 messages summarized, 20 kept"
}
```

**Response** `200 OK` (no compaction needed):

```json
{
  "status": "ok",
  "message": "Session does not need compaction (below threshold)"
}
```

### POST /api/agents/{id}/stop

Cancel the agent's current LLM run. Aborts any in-progress generation.

**Response** `200 OK`:

```json
{
  "status": "stopped",
  "message": "Agent run cancelled"
}
```

### PUT /api/agents/{id}/model

Switch an agent's LLM model at runtime.

**Request Body**:

```json
{
  "model": "claude-sonnet-4-20250514"
}
```

**Response** `200 OK`:

```json
{
  "status": "updated",
  "model": "claude-sonnet-4-20250514"
}
```

---

## Cron/Scheduler Endpoints

Manage recurring and one-shot scheduled jobs. Jobs can trigger agent turns, system events, or workflow runs on a schedule.

### GET /api/cron/jobs

List all cron jobs. Optionally filter by agent with `?agent_id=<uuid>`.

**Response** `200 OK`:

```json
{
  "jobs": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "agent_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
      "name": "daily-report",
      "enabled": true,
      "schedule": { "kind": "every", "every_secs": 3600 },
      "action": {
        "kind": "agent_turn",
        "message": "Generate the daily report",
        "timeout_secs": 120
      },
      "delivery": {
        "kind": "channel",
        "channel": "slack",
        "to": "#reports"
      },
      "created_at": "2026-03-15T10:30:00Z",
      "last_run": "2026-03-16T09:00:00Z",
      "next_run": "2026-03-16T10:00:00Z"
    }
  ],
  "total": 1
}
```

### POST /api/cron/jobs

Create a new cron job.

**Request Body**:

```json
{
  "agent_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  "name": "daily-report",
  "schedule": { "kind": "every", "every_secs": 3600 },
  "action": {
    "kind": "agent_turn",
    "message": "Generate the daily report",
    "timeout_secs": 120
  },
  "delivery": {
    "kind": "channel",
    "channel": "slack",
    "to": "#reports"
  }
}
```

**Response** `201 Created`:

```json
{
  "result": "{\"job_id\":\"550e8400-e29b-41d4-a716-446655440000\",\"status\":\"created\"}"
}
```

### DELETE /api/cron/jobs/{id}

Delete a cron job by ID.

**Response** `200 OK`:

```json
{ "status": "deleted" }
```

### PUT /api/cron/jobs/{id}/enable

Enable or disable a cron job.

**Request Body**:

```json
{ "enabled": false }
```

**Response** `200 OK`:

```json
{ "status": "updated", "enabled": false }
```

### GET /api/cron/jobs/{id}/status

Get job metadata including last run time, status, and error history.

**Response** `200 OK`:

```json
{
  "job": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "agent_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    "name": "daily-report",
    "enabled": true,
    "schedule": { "kind": "every", "every_secs": 3600 },
    "action": {
      "kind": "agent_turn",
      "message": "Generate the daily report",
      "timeout_secs": 120
    },
    "delivery": { "kind": "none" },
    "created_at": "2026-03-15T10:30:00Z",
    "last_run": "2026-03-16T09:00:00Z",
    "next_run": "2026-03-16T10:00:00Z"
  },
  "one_shot": false,
  "last_status": "ok",
  "consecutive_errors": 0
}

### POST /api/cron/jobs/{id}/run

Trigger a cron job immediately. The job executes asynchronously in the background — this endpoint returns immediately without waiting for completion. Poll `GET /api/cron/jobs/{id}/status` to check the result.

**Response** `200 OK`:

```json
{
  "status": "triggered",
  "job_id": "550e8400-e29b-41d4-a716-446655440000"
}
```

**Error Responses**:

- `400 Bad Request` — Invalid job ID or job is disabled
- `404 Not Found` — Job not found

---

## WebSocket Protocol

### Connecting

```
GET /api/agents/{id}/ws
```

Upgrades to a WebSocket connection for real-time bidirectional chat with an agent. Returns `400` if the agent ID is invalid, or `404` if the agent does not exist.

### Message Format

All messages are JSON-encoded strings.

### Client to Server

**Send a message:**

```json
{
  "type": "message",
  "content": "What is the weather like?"
}
```

Plain text (non-JSON) is also accepted and treated as a message.

**Chat commands** (sent as messages with `/` prefix):

| Command | Description |
|---------|-------------|
| `/new` | Start a new session (clear history) |
| `/compact` | Trigger LLM session compaction |
| `/model <name>` | Switch the agent's model |
| `/stop` | Cancel current LLM run |
| `/usage` | Show token usage and cost |
| `/think` | Toggle extended thinking mode |
| `/models` | List available models |
| `/providers` | List LLM providers and auth status |

**Ping:**

```json
{
  "type": "ping"
}
```

### Server to Client

**Connection confirmed** (sent immediately on connect):

```json
{
  "type": "connected",
  "agent_id": "a1b2c3d4-..."
}
```

**Thinking indicator** (sent when agent starts processing):

```json
{
  "type": "thinking"
}
```

**Text delta** (streaming token, sent as the LLM generates output):

```json
{
  "type": "text_delta",
  "content": "The weather"
}
```

**Tool use started** (sent when the agent invokes a tool):

```json
{
  "type": "tool_start",
  "tool": "web_fetch"
}
```

**Complete response** (sent when agent finishes, contains final aggregated response):

```json
{
  "type": "response",
  "content": "The weather today is sunny with a high of 72F.",
  "input_tokens": 245,
  "output_tokens": 32,
  "iterations": 2,
  "cost_usd": 0.0012
}
```

**Error:**

```json
{
  "type": "error",
  "content": "Agent not found"
}
```

**Agent list update** (sent every 5 seconds with current agent states):

```json
{
  "type": "agents_updated",
  "agents": [
    {
      "id": "a1b2c3d4-...",
      "name": "hello-world",
      "state": "Running",
      "model_provider": "groq",
      "model_name": "llama-3.3-70b-versatile"
    }
  ]
}
```

**Pong** (response to ping):

```json
{
  "type": "pong"
}
```

### Connection Lifecycle

1. Client connects to `ws://host:port/api/agents/{id}/ws`.
2. Server sends `{"type": "connected"}`.
3. Client sends `{"type": "message", "content": "..."}`.
4. Server sends `{"type": "thinking"}`, then zero or more `{"type": "text_delta"}` events, then `{"type": "response"}`.
5. Server periodically sends `{"type": "agents_updated"}` every 5 seconds.
6. Client sends a Close frame or disconnects to end the session.

---

## SSE Streaming

### POST /api/agents/{id}/message/stream

Send a message and receive the response as a Server-Sent Events stream. This enables real-time token-by-token streaming.

**Request Body** (JSON):

```json
{
  "message": "Explain quantum computing"
}
```

**SSE Event Stream:**

```
event: chunk
data: {"content":"Quantum","done":false}

event: chunk
data: {"content":" computing","done":false}

event: chunk
data: {"content":" is a type","done":false}

event: tool_use
data: {"tool":"web_search"}

event: tool_result
data: {"tool":"web_search","input":{"query":"quantum computing basics"}}

event: done
data: {"done":true,"usage":{"input_tokens":150,"output_tokens":340}}
```

### SSE Event Types

| Event Name | Description |
|------------|-------------|
| `chunk` | Text delta from the LLM. `"done": false` indicates more tokens are coming. |
| `tool_use` | The agent is invoking a tool. Contains the tool name. |
| `tool_result` | A tool invocation has completed. Contains the tool name and input. |
| `done` | Final event. Contains `"done": true` and token usage statistics. |

---

## OpenAI-Compatible API

OpenFang exposes an OpenAI-compatible API for drop-in integration with tools that support the OpenAI API format (Cursor, Continue, Open WebUI, etc.).

### POST /v1/chat/completions

Send a chat completion request using the OpenAI message format.

**Request Body**:

```json
{
  "model": "openfang:coder",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "Hello!"}
  ],
  "stream": false,
  "temperature": 0.7,
  "max_tokens": 1024
}
```

**Model resolution** (the `model` field maps to an OpenFang agent):

| Format | Example | Behavior |
|--------|---------|----------|
| `openfang:<name>` | `openfang:coder` | Find agent by name |
| UUID | `a1b2c3d4-...` | Find agent by ID |
| Plain string | `coder` | Try as agent name |
| Any other | `gpt-4o` | Falls back to first registered agent |

**Image support** --- messages can include image content parts:

```json
{
  "model": "openfang:analyst",
  "messages": [
    {
      "role": "user",
      "content": [
        {"type": "text", "text": "Describe this image"},
        {"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBOR..."}}
      ]
    }
  ]
}
```

**Response (non-streaming)** `200 OK`:

```json
{
  "id": "chatcmpl-a1b2c3d4-...",
  "object": "chat.completion",
  "created": 1708617600,
  "model": "coder",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you today?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 25,
    "completion_tokens": 12,
    "total_tokens": 37
  }
}
```

**Streaming** --- Set `"stream": true` for SSE:

```
data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":null}]}

data: {"id":"chatcmpl-...","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":25,"completion_tokens":12,"total_tokens":37}}

data: [DONE]
```

### GET /v1/models

List available models (agents) in OpenAI format.

**Response** `200 OK`:

```json
{
  "object": "list",
  "data": [
    {
      "id": "openfang:coder",
      "object": "model",
      "created": 1708617600,
      "owned_by": "openfang"
    },
    {
      "id": "openfang:researcher",
      "object": "model",
      "created": 1708617600,
      "owned_by": "openfang"
    }
  ]
}
```

---

## Error Responses

All error responses use a consistent JSON format:

```json
{
  "error": "Description of what went wrong"
}
```

### HTTP Status Codes

| Code | Meaning |
|------|---------|
| `200` | Success |
| `201` | Created (spawn agent, create workflow, create trigger, install skill) |
| `400` | Bad request (invalid UUID, missing required fields, malformed TOML/JSON) |
| `401` | Unauthorized (missing or invalid `Authorization: Bearer` header) |
| `404` | Not found (agent, workflow, trigger, template, model, skill, or KV key does not exist) |
| `429` | Too many requests (GCRA rate limit exceeded) |
| `500` | Internal server error (agent loop failure, database error, driver error) |

### Request IDs

Every response includes an `x-request-id` header with a UUID for tracing:

```
x-request-id: 550e8400-e29b-41d4-a716-446655440000
```

Use this value when reporting issues or correlating requests in logs.

### Security Headers

Every response includes security headers:

| Header | Value |
|--------|-------|
| `Content-Security-Policy` | `default-src 'self'` (with appropriate directives) |
| `X-Frame-Options` | `DENY` |
| `X-Content-Type-Options` | `nosniff` |
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains` |
| `X-Request-Id` | Unique UUID per request |

### Rate Limiting

The GCRA (Generic Cell Rate Algorithm) rate limiter provides cost-aware token bucket rate limiting with per-IP tracking and automatic stale entry cleanup. Different endpoints consume different token costs (e.g., `/api/agents/{id}/message` costs more than `/api/health`). When the limit is exceeded, the server returns `429 Too Many Requests`:

```
HTTP/1.1 429 Too Many Requests
Retry-After: 60

{"error": "Rate limit exceeded"}
```

The `Retry-After` header indicates the window duration in seconds.

---

## Endpoint Summary

**76 endpoints total** across 15 groups.

| Method | Path | Description |
|--------|------|-------------|
| **System** | | |
| GET | `/` | WebChat UI |
| GET | `/api/health` | Health check (no auth, redacted) |
| GET | `/api/health/detail` | Full health check (auth required) |
| GET | `/api/status` | Kernel status |
| GET | `/api/version` | Version info |
| POST | `/api/shutdown` | Graceful shutdown |
| GET | `/api/profiles` | List agent profiles |
| GET | `/api/tools` | List available tools |
| GET | `/api/config` | Configuration (secrets redacted) |
| GET | `/api/peers` | List OFP wire peers |
| **Agents** | | |
| GET | `/api/agents` | List agents |
| POST | `/api/agents` | Spawn agent |
| GET | `/api/agents/{id}` | Get agent details |
| PUT | `/api/agents/{id}/update` | Update agent config |
| PUT | `/api/agents/{id}/mode` | Set agent mode (Stable/Normal) |
| DELETE | `/api/agents/{id}` | Kill agent |
| POST | `/api/agents/{id}/message` | Send message (blocking) |
| POST | `/api/agents/{id}/message/stream` | Send message (SSE stream) |
| GET | `/api/agents/{id}/session` | Get conversation history |
| GET | `/api/agents/{id}/ws` | WebSocket chat |
| POST | `/api/agents/{id}/session/reset` | Reset session |
| POST | `/api/agents/{id}/session/compact` | LLM-based compaction |
| POST | `/api/agents/{id}/stop` | Cancel current run |
| PUT | `/api/agents/{id}/model` | Switch model |
| **Workflows** | | |
| GET | `/api/workflows` | List workflows |
| POST | `/api/workflows` | Create workflow |
| POST | `/api/workflows/{id}/run` | Run workflow |
| GET | `/api/workflows/{id}/runs` | List workflow runs |
| **Triggers** | | |
| GET | `/api/triggers` | List triggers |
| POST | `/api/triggers` | Create trigger |
| PUT | `/api/triggers/{id}` | Update trigger |
| DELETE | `/api/triggers/{id}` | Delete trigger |
| **Memory** | | |
| GET | `/api/memory/agents/{id}/kv` | List KV pairs |
| GET | `/api/memory/agents/{id}/kv/{key}` | Get KV value |
| PUT | `/api/memory/agents/{id}/kv/{key}` | Set KV value |
| DELETE | `/api/memory/agents/{id}/kv/{key}` | Delete KV value |
| **Channels** | | |
| GET | `/api/channels` | List channels (40 adapters) |
| **Templates** | | |
| GET | `/api/templates` | List templates |
| GET | `/api/templates/{name}` | Get template |
| **Sessions** | | |
| GET | `/api/sessions` | List sessions |
| DELETE | `/api/sessions/{id}` | Delete session |
| **Model Catalog** | | |
| GET | `/api/models` | Full model catalog (51+ models) |
| GET | `/api/models/{id}` | Model details |
| GET | `/api/models/aliases` | List 23 model aliases |
| GET | `/api/providers` | Provider list with auth status |
| **Provider Config** | | |
| POST | `/api/providers/{name}/key` | Set provider API key |
| DELETE | `/api/providers/{name}/key` | Remove provider API key |
| POST | `/api/providers/{name}/test` | Test provider connectivity |
| **Skills & Marketplace** | | |
| GET | `/api/skills` | List installed skills (60 bundled) |
| POST | `/api/skills/install` | Install skill |
| POST | `/api/skills/uninstall` | Uninstall skill |
| POST | `/api/skills/create` | Create new skill |
| GET | `/api/marketplace/search` | Search FangHub |
| **ClawHub** | | |
| GET | `/api/clawhub/search` | Search ClawHub |
| GET | `/api/clawhub/browse` | Browse ClawHub |
| GET | `/api/clawhub/skill/{slug}` | Skill details |
| POST | `/api/clawhub/install` | Install from ClawHub |
| **MCP & A2A** | | |
| GET | `/api/mcp/servers` | MCP server connections |
| POST | `/mcp` | MCP HTTP transport (JSON-RPC 2.0) |
| GET | `/.well-known/agent.json` | A2A agent card |
| GET | `/a2a/agents` | A2A agent list |
| POST | `/a2a/tasks/send` | Send A2A task |
| GET | `/a2a/tasks/{id}` | Get A2A task status |
| POST | `/a2a/tasks/{id}/cancel` | Cancel A2A task |
| **Audit & Security** | | |
| GET | `/api/audit/recent` | Recent audit logs |
| GET | `/api/audit/verify` | Verify Merkle chain integrity |
| GET | `/api/security` | Security status (16 systems) |
| **Usage & Analytics** | | |
| GET | `/api/usage` | Usage statistics |
| GET | `/api/usage/summary` | Usage summary with quota |
| GET | `/api/usage/by-model` | Usage by model breakdown |
| **Migration** | | |
| GET | `/api/migrate/detect` | Detect migration sources |
| POST | `/api/migrate/scan` | Scan for importable data |
| POST | `/api/migrate` | Run migration |
| **OpenAI Compatible** | | |
| POST | `/v1/chat/completions` | OpenAI-compatible chat |
| GET | `/v1/models` | OpenAI-compatible model list |
