# Migrating to OMTAE

This guide covers migrating from OpenClaw (and other frameworks) to OMTAE. The migration engine handles config conversion, agent import, memory transfer, channel re-configuration, and skill scanning.

## Table of Contents

- [Quick Migration](#quick-migration)
- [What Gets Migrated](#what-gets-migrated)
- [Manual Migration Steps](#manual-migration-steps)
- [Config Format Differences](#config-format-differences)
- [Tool Name Mapping](#tool-name-mapping)
- [Provider Mapping](#provider-mapping)
- [Feature Comparison](#feature-comparison)

---

## Quick Migration

Run a single command to migrate your entire OpenClaw workspace:

```bash
omtae migrate --from openclaw
```

This auto-detects your OpenClaw workspace at `~/.openclaw/` and imports everything into `~/.omtae/`.

### Options

```bash
# Specify a custom source directory
omtae migrate --from openclaw --source-dir /path/to/openclaw/workspace

# Dry run -- see what would be imported without making changes
omtae migrate --from openclaw --dry-run
```

### Migration Report

After a successful migration, a `migration_report.md` file is saved to `~/.omtae/` with a summary of everything that was imported, skipped, or needs manual attention.

### Other Frameworks

LangChain and AutoGPT migration support is planned:

```bash
omtae migrate --from langchain   # Coming soon
omtae migrate --from autogpt     # Coming soon
```

---

## What Gets Migrated

| Item | Source (OpenClaw) | Destination (OMTAE) | Status |
|------|-------------------|------------------------|--------|
| **Config** | `~/.openclaw/config.yaml` | `~/.omtae/config.toml` | Fully automated |
| **Agents** | `~/.openclaw/agents/*/agent.yaml` | `~/.omtae/agents/*/agent.toml` | Fully automated |
| **Memory** | `~/.openclaw/agents/*/MEMORY.md` | `~/.omtae/agents/*/imported_memory.md` | Fully automated |
| **Channels** | `~/.openclaw/messaging/*.yaml` | `~/.omtae/channels_import.toml` | Automated (manual merge) |
| **Skills** | `~/.openclaw/skills/` | Scanned and reported | Manual reinstall |
| **Sessions** | `~/.openclaw/agents/*/sessions/` | Not migrated | Fresh start recommended |
| **Workspace files** | `~/.openclaw/agents/*/workspace/` | Not migrated | Copy manually if needed |

### Channel Import Note

Channel configurations (Telegram, Discord, Slack) are exported to a `channels_import.toml` file. You must manually merge the `[channels]` section into your `~/.omtae/config.toml`.

### Skills Note

OpenClaw skills (Node.js) are detected and listed in the migration report but not automatically converted. After migration, reinstall skills using:

```bash
omtae skill install <skill-name-or-path>
```

OMTAE automatically detects OpenClaw-format skills and converts them during installation.

---

## Manual Migration Steps

If you prefer migrating by hand (or need to handle edge cases), follow these steps:

### 1. Initialize OMTAE

```bash
omtae init
```

This creates `~/.omtae/` with a default `config.toml`.

### 2. Convert Your Config

Translate your `config.yaml` to `config.toml`:

**OpenClaw** (`~/.openclaw/config.yaml`):
```yaml
provider: anthropic
model: claude-sonnet-4-20250514
api_key_env: ANTHROPIC_API_KEY
temperature: 0.7
memory:
  decay_rate: 0.05
```

**OMTAE** (`~/.omtae/config.toml`):
```toml
[default_model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"
api_key_env = "ANTHROPIC_API_KEY"

[memory]
decay_rate = 0.05

[network]
listen_addr = "127.0.0.1:4200"
```

### 3. Convert Agent Manifests

Translate each `agent.yaml` to `agent.toml`:

**OpenClaw** (`~/.openclaw/agents/coder/agent.yaml`):
```yaml
name: coder
description: A coding assistant
provider: anthropic
model: claude-sonnet-4-20250514
tools:
  - read_file
  - write_file
  - execute_command
tags:
  - coding
  - dev
```

**OMTAE** (`~/.omtae/agents/coder/agent.toml`):
```toml
name = "coder"
version = "0.1.0"
description = "A coding assistant"
author = "omtae"
module = "builtin:chat"
tags = ["coding", "dev"]

[model]
provider = "anthropic"
model = "claude-sonnet-4-20250514"

[capabilities]
tools = ["file_read", "file_write", "shell_exec"]
memory_read = ["*"]
memory_write = ["self.*"]
```

### 4. Convert Channel Configs

**OpenClaw** (`~/.openclaw/messaging/telegram.yaml`):
```yaml
type: telegram
bot_token_env: TELEGRAM_BOT_TOKEN
default_agent: coder
allowed_users:
  - "123456789"
```

**OMTAE** (add to `~/.omtae/config.toml`):
```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
default_agent = "coder"
allowed_users = ["123456789"]
```

### 5. Import Memory

Copy any `MEMORY.md` files from OpenClaw agents to OMTAE agent directories:

```bash
cp ~/.openclaw/agents/coder/MEMORY.md ~/.omtae/agents/coder/imported_memory.md
```

The kernel will ingest these on first boot.

---

## Config Format Differences

| Aspect | OpenClaw | OMTAE |
|--------|----------|----------|
| Format | YAML | TOML |
| Config location | `~/.openclaw/config.yaml` | `~/.omtae/config.toml` |
| Agent definition | `agent.yaml` | `agent.toml` |
| Channel config | Separate files per channel | Unified in `config.toml` |
| Tool permissions | Implicit (tool list) | Capability-based (tools, memory, network, shell) |
| Model config | Flat (top-level fields) | Nested (`[model]` section) |
| Agent module | Implicit | Explicit (`module = "builtin:chat"` / `"wasm:..."` / `"python:..."`) |
| Scheduling | Not supported | Built-in (`[schedule]` section: reactive, continuous, periodic, proactive) |
| Resource quotas | Not supported | Built-in (`[resources]` section: tokens/hour, memory, CPU time) |
| Networking | Not supported | OFP protocol (`[network]` section) |

---

## Tool Name Mapping

Tools were renamed between OpenClaw and OMTAE for consistency. The migration engine handles this automatically.

| OpenClaw Tool | OMTAE Tool | Notes |
|---------------|---------------|-------|
| `read_file` | `file_read` | Noun-first naming |
| `write_file` | `file_write` | |
| `list_files` | `file_list` | |
| `execute_command` | `shell_exec` | Capability-gated |
| `web_search` | `web_search` | Unchanged |
| `fetch_url` | `web_fetch` | |
| `browser_navigate` | `browser_navigate` | Unchanged |
| `memory_search` | `memory_recall` | |
| `memory_recall` | `memory_recall` | |
| `memory_save` | `memory_store` | |
| `memory_store` | `memory_store` | |
| `sessions_send` | `agent_send` | |
| `agent_message` | `agent_send` | |
| `agents_list` | `agent_list` | |
| `agent_list` | `agent_list` | |

### New Tools in OMTAE

These tools have no OpenClaw equivalent:

| Tool | Description |
|------|-------------|
| `agent_spawn` | Spawn a new agent from within an agent |
| `agent_kill` | Terminate another agent |
| `agent_find` | Search for agents by name, tag, or description |
| `memory_store` | Store key-value data in shared memory |
| `memory_recall` | Recall key-value data from shared memory |
| `task_post` | Post a task to the shared task board |
| `task_claim` | Claim an available task |
| `task_complete` | Mark a task as complete |
| `task_list` | List tasks by status |
| `event_publish` | Publish a custom event to the event bus |
| `schedule_create` | Create a scheduled job |
| `schedule_list` | List scheduled jobs |
| `schedule_delete` | Delete a scheduled job |
| `image_analyze` | Analyze an image |
| `location_get` | Get location information |

### Tool Profiles

OpenClaw's tool profiles map to explicit tool lists:

| OpenClaw Profile | OMTAE Tools |
|------------------|----------------|
| `minimal` | `file_read`, `file_list` |
| `coding` | `file_read`, `file_write`, `file_list`, `shell_exec`, `web_fetch` |
| `messaging` | `agent_send`, `agent_list`, `memory_store`, `memory_recall` |
| `research` | `web_fetch`, `web_search`, `file_read`, `file_write` |
| `full` | All 10 core tools |

---

## Provider Mapping

| OpenClaw Name | OMTAE Name | API Key Env Var |
|---------------|---------------|-----------------|
| `anthropic` | `anthropic` | `ANTHROPIC_API_KEY` |
| `claude` | `anthropic` | `ANTHROPIC_API_KEY` |
| `openai` | `openai` | `OPENAI_API_KEY` |
| `gpt` | `openai` | `OPENAI_API_KEY` |
| `groq` | `groq` | `GROQ_API_KEY` |
| `ollama` | `ollama` | (none required) |
| `openrouter` | `openrouter` | `OPENROUTER_API_KEY` |
| `deepseek` | `deepseek` | `DEEPSEEK_API_KEY` |
| `together` | `together` | `TOGETHER_API_KEY` |
| `mistral` | `mistral` | `MISTRAL_API_KEY` |
| `fireworks` | `fireworks` | `FIREWORKS_API_KEY` |

### New Providers in OMTAE

| Provider | Description |
|----------|-------------|
| `vllm` | Self-hosted vLLM inference server |
| `lmstudio` | LM Studio local models |

---

## Feature Comparison

| Feature | OpenClaw | OMTAE |
|---------|----------|----------|
| **Language** | Node.js / TypeScript | Rust |
| **Config format** | YAML | TOML |
| **Agent manifests** | YAML | TOML |
| **Multi-agent** | Basic (message passing) | First-class (spawn, kill, find, workflows, triggers) |
| **Agent scheduling** | Manual | Built-in (reactive, continuous, periodic, proactive) |
| **Memory** | Markdown files | SQLite + KV store + semantic search + knowledge graph |
| **Session management** | JSONL files | SQLite with context window tracking |
| **LLM providers** | ~5 | 11 (Anthropic, OpenAI, Groq, OpenRouter, DeepSeek, Together, Mistral, Fireworks, Ollama, vLLM, LM Studio) |
| **Per-agent models** | No | Yes (per-agent provider + model override) |
| **Security** | None | Capability-based (tools, memory, network, shell, agent spawn) |
| **Resource quotas** | None | Per-agent token/hour limits, memory limits, CPU time limits |
| **Workflow engine** | None | Built-in (sequential, fan-out, collect, conditional, loop) |
| **Event triggers** | None | Pattern-matching event triggers with templated prompts |
| **WASM sandbox** | None | Wasmtime-based sandboxed execution |
| **Python runtime** | None | Subprocess-based Python agent execution |
| **Networking** | None | OFP (OMTAE Protocol) peer-to-peer |
| **API server** | Basic REST | REST + WebSocket + SSE streaming |
| **WebChat UI** | Separate | Embedded in daemon |
| **Channel adapters** | Telegram, Discord | Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email |
| **Skills/Plugins** | npm packages | TOML + Python/WASM/Node.js, FangHub marketplace |
| **CLI** | Basic | Full CLI with daemon auto-detect, MCP server |
| **MCP support** | No | Built-in MCP server (stdio) |
| **Process supervisor** | None | Health monitoring, panic/restart tracking |
| **Persistence** | File-based | SQLite (agents survive restarts) |

---

## Troubleshooting

### Migration reports "Source directory not found"

The migration engine looks for `~/.openclaw/` by default. If your OpenClaw workspace is elsewhere:

```bash
omtae migrate --from openclaw --source-dir /path/to/your/workspace
```

### Agent fails to spawn after migration

Check the converted `agent.toml` for:
- Valid tool names (see the [Tool Name Mapping](#tool-name-mapping) table)
- A valid provider name (see the [Provider Mapping](#provider-mapping) table)
- Correct `module` field (should be `"builtin:chat"` for standard LLM agents)

### Skills not working

OpenClaw Node.js skills must be reinstalled:

```bash
omtae skill install /path/to/openclaw/skills/my-skill
```

The installer auto-detects OpenClaw format and converts the skill manifest.

### Channel not connecting

After migration, channels are exported to `channels_import.toml`. You must merge them into your `config.toml` manually:

```bash
cat ~/.omtae/channels_import.toml
# Copy the [channels.*] sections into ~/.omtae/config.toml
```

Then restart the daemon:

```bash
omtae start
```
