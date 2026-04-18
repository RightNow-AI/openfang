# Migrating to OpenFang

This guide covers migrating from OpenClaw (and other frameworks) to OpenFang. The migration engine handles config conversion, agent import, memory transfer, channel re-configuration, and skill scanning.

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
openfang migrate --from openclaw
```

This auto-detects your OpenClaw workspace at `~/.openclaw/` and imports everything into `~/.openfang/`.

### Options

```bash
# Specify a custom source directory
openfang migrate --from openclaw --source-dir /path/to/openclaw/workspace

# Dry run -- see what would be imported without making changes
openfang migrate --from openclaw --dry-run
```

### Migration Report

After a successful migration, a `migration_report.md` file is saved to `~/.openfang/` with a summary of everything that was imported, skipped, or needs manual attention.

### Other Frameworks

LangChain and AutoGPT migration support is planned:

```bash
openfang migrate --from langchain   # Coming soon
openfang migrate --from autogpt     # Coming soon
```

---

## What Gets Migrated

| Item | Source (OpenClaw) | Destination (OpenFang) | Status |
|------|-------------------|------------------------|--------|
| **Config** | `~/.openclaw/config.yaml` | `~/.openfang/config.toml` | Fully automated |
| **Agents** | `~/.openclaw/agents/*/agent.yaml` | `~/.openfang/agents/*/agent.toml` | Fully automated |
| **Memory** | `~/.openclaw/agents/*/MEMORY.md` | `~/.openfang/agents/*/imported_memory.md` | Fully automated |
| **Channels** | `~/.openclaw/messaging/*.yaml` | `~/.openfang/channels_import.toml` | Automated (manual merge) |
| **Skills** | `~/.openclaw/skills/` | Scanned and reported | Manual reinstall |
| **Sessions** | `~/.openclaw/agents/*/sessions/` | Not migrated | Fresh start recommended |
| **Workspace files** | `~/.openclaw/agents/*/workspace/` | Not migrated | Copy manually if needed |

### Channel Import Note

Channel configurations (Telegram, Discord, Slack) are exported to a `channels_import.toml` file. You must manually merge the `[channels]` section into your `~/.openfang/config.toml`.

### Skills Note

OpenClaw skills (Node.js) are detected and listed in the migration report but not automatically converted. After migration, reinstall skills using:

```bash
openfang skill install <skill-name-or-path>
```

OpenFang automatically detects OpenClaw-format skills and converts them during installation.

---

## Manual Migration Steps

If you prefer migrating by hand (or need to handle edge cases), follow these steps:

### 1. Initialize OpenFang

```bash
openfang init
```

This creates `~/.openfang/` with a default `config.toml`.

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

**OpenFang** (`~/.openfang/config.toml`):
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

**OpenFang** (`~/.openfang/agents/coder/agent.toml`):
```toml
name = "coder"
version = "0.1.0"
description = "A coding assistant"
author = "openfang"
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

**OpenFang** (add to `~/.openfang/config.toml`):
```toml
[channels.telegram]
bot_token_env = "TELEGRAM_BOT_TOKEN"
default_agent = "coder"
allowed_users = ["123456789"]
```

### 5. Import Memory

Copy any `MEMORY.md` files from OpenClaw agents to OpenFang agent directories:

```bash
cp ~/.openclaw/agents/coder/MEMORY.md ~/.openfang/agents/coder/imported_memory.md
```

The kernel will ingest these on first boot.

---

## Config Format Differences

| Aspect | OpenClaw | OpenFang |
|--------|----------|----------|
| Format | YAML | TOML |
| Config location | `~/.openclaw/config.yaml` | `~/.openfang/config.toml` |
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

Tools were renamed between OpenClaw and OpenFang for consistency. The migration engine handles this automatically.

| OpenClaw Tool | OpenFang Tool | Notes |
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

### New Tools in OpenFang

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

| OpenClaw Profile | OpenFang Tools |
|------------------|----------------|
| `minimal` | `file_read`, `file_list` |
| `coding` | `file_read`, `file_write`, `file_list`, `shell_exec`, `web_fetch` |
| `messaging` | `agent_send`, `agent_list`, `memory_store`, `memory_recall` |
| `research` | `web_fetch`, `web_search`, `file_read`, `file_write` |
| `full` | All 10 core tools |

---

## Provider Mapping

| OpenClaw Name | OpenFang Name | API Key Env Var |
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

### New Providers in OpenFang

| Provider | Description |
|----------|-------------|
| `vllm` | Self-hosted vLLM inference server |
| `lmstudio` | LM Studio local models |

---

## Feature Comparison

| Feature | OpenClaw | OpenFang |
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
| **Networking** | None | OFP (OpenFang Protocol) peer-to-peer |
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
openfang migrate --from openclaw --source-dir /path/to/your/workspace
```

### Agent fails to spawn after migration

Check the converted `agent.toml` for:
- Valid tool names (see the [Tool Name Mapping](#tool-name-mapping) table)
- A valid provider name (see the [Provider Mapping](#provider-mapping) table)
- Correct `module` field (should be `"builtin:chat"` for standard LLM agents)

### Skills not working

OpenClaw Node.js skills must be reinstalled:

```bash
openfang skill install /path/to/openclaw/skills/my-skill
```

The installer auto-detects OpenClaw format and converts the skill manifest.

### Channel not connecting

After migration, channels are exported to `channels_import.toml`. You must merge them into your `config.toml` manually:

```bash
cat ~/.openfang/channels_import.toml
# Copy the [channels.*] sections into ~/.openfang/config.toml
```

Then restart the daemon:

```bash
openfang start
```

---

## Breaking Changes: Pluggable Memory Backends

The pluggable-backends refactor in `openfang-memory` introduces a handful of
source-level breaking changes for anyone embedding the crate directly. The
OpenFang daemon, CLI, and config are unaffected.

### PR review resolution — blockers

Every blocker raised in the review is resolved in-tree. Each row below links
to the exact fix site.

| # | Blocker (as reported) | Resolution |
|---|-----------------------|------------|
| B1 | `HttpSemanticStore::remember` returned a random `MemoryId::new()` instead of the server's ID; `forget`/`update_embedding` then delegated to a fallback with wrong IDs. | New `parse_memory_id()` helper at [crates/openfang-memory/src/http/semantic.rs:63](crates/openfang-memory/src/http/semantic.rs#L63) converts the server's `serde_json::Value` to a real `MemoryId`. `remember` at [L311](crates/openfang-memory/src/http/semantic.rs#L311) returns the parsed server UUID. `forget` / `update_embedding` now return an explicit `Err` rather than silently hitting a disconnected store. |
| B2 | `HttpSemanticStore::recall` fabricated a fresh `MemoryId::new()` per row; IDs were unstable across calls. | [crates/openfang-memory/src/http/semantic.rs:345](crates/openfang-memory/src/http/semantic.rs#L345) parses each server row's `r.id` via `parse_memory_id`; malformed rows are dropped with a `warn!` instead of being admitted with random UUIDs. IDs are now stable. |
| B3 | `MemorySubstrate::open_postgres` called `Handle::current().block_on(...)` — panics on nested current-thread runtime. | New `async fn open_async` in [crates/openfang-memory/src/substrate.rs](crates/openfang-memory/src/substrate.rs) does all Postgres init (pool build + probe + migrations) via natural `.await`. No `Handle::current().block_on` remains anywhere. Sync `open()` errors cleanly for Postgres backends. Kernel boot at [crates/openfang-kernel/src/kernel.rs:613](crates/openfang-kernel/src/kernel.rs#L613) routes through `open_async(...).await`. |
| B4 | Docs listed `qdrant` / `postgres+qdrant` as valid `semantic_backend` values, but `select_semantic` had no matching arm — unknown values silently fell through to SQLite. | Config fields are now the typed enums `MemoryBackendKind` / `SemanticBackendKind` in [crates/openfang-types/src/config.rs](crates/openfang-types/src/config.rs) with `#[serde(rename_all="snake_case")]` and `deny_unknown_fields` on `MemoryConfig` — typos are rejected at parse time. `PostgresSemanticStore` implemented in [crates/openfang-memory/src/postgres/semantic.rs](crates/openfang-memory/src/postgres/semantic.rs). `select_semantic` is an exhaustive enum match; feature-off paths return `OpenFangError::Config` rather than silently degrading. |
| B5 | Cargo.lock showed workspace `0.5.9 → 0.5.5` and dropped rustls from `openfang-kernel` — wrong-base artifact. | Branch rebased onto `origin/main` (tip `d3d9fa8` = v0.5.10). `cargo generate-lockfile` regenerated; all crates at `0.5.10`; rustls remains in `openfang-kernel`. |
| B6 | Breaking API changes without a migration note (`usage_conn()` removed, modules moved under `sqlite::`). | This file — sections below document every break: `usage_conn()` → `usage()`/`usage_arc()`, module path moves, new feature flags, `HttpSemanticStore::forget`/`update_embedding` error change, async `open_async`, typed config enums, `postgres_pool_size` validation, fail-fast semantics. |
| B7 | `AuditLog::with_backend` used `if let Ok(rows) = ...` and silently started with an empty log on `Err` — unacceptable for a security audit trail. | [crates/openfang-runtime/src/audit.rs:134](crates/openfang-runtime/src/audit.rs#L134) signature is now `-> OpenFangResult<Self>`. Load errors and integrity-check failures both `tracing::error!(...)` and propagate via `?` (fail-closed). Kernel caller surfaces the error as `KernelError::BootFailed`. |

### PR review resolution — concerns

| # | Concern (as reported) | Resolution |
|---|----------------------|------------|
| C1 | `unsafe` FFI registration for `sqlite-vec` in the production `SqliteBackend::open` path — the PR claim "no new unsafe" was inaccurate. | Both `unsafe` blocks in [crates/openfang-memory/src/sqlite/mod.rs](crates/openfang-memory/src/sqlite/mod.rs) (`open` and `open_in_memory`) now carry explicit `// SAFETY:` comments covering ABI compatibility, idempotency, and process-global semantics. The dedicated subsection below explicitly corrects the "no new unsafe" claim and names the two blocks. |
| C2 | `QdrantSemanticStore::recall` returned `Ok(vec![])` when `query_embedding` was `None`. | [crates/openfang-memory/src/qdrant/semantic.rs:213](crates/openfang-memory/src/qdrant/semantic.rs#L213) now returns `OpenFangError::Memory("Qdrant semantic backend requires a query_embedding for recall(); enable an embedder or use a different semantic_backend")`. The Postgres semantic backend mirrors the same contract. |
| C3 | JSONL mirror writes `sessions_dir.join(format!("{}.jsonl", session.id.0))` — needs documentation that `SessionId` is UUID-only. | [crates/openfang-memory/src/jsonl.rs:26](crates/openfang-memory/src/jsonl.rs#L26) has a comment citing `openfang_types::agent::SessionId` as a `uuid::Uuid` newtype, so the filename component is path-traversal safe. |
| C4 | `docker-compose.yml` hardcoded `POSTGRES_PASSWORD: openfang` — needs a dev-only marker. | Every config value in [docker-compose.yml](docker-compose.yml) is env-overridable: `POSTGRES_{IMAGE,USER,PASSWORD,DB,PORT}`, `QDRANT_{IMAGE,HTTP_PORT,GRPC_PORT}`, `OPENFANG_PORT`. Defaults are clearly dev/CI; a comment above the service warns "DO NOT reuse these credentials in production — every setting below is overridable via the environment." |
| C5 | Whether `openfang-kernel` / the binary crates enable `postgres` / `qdrant` features on `openfang-memory` wasn't visible — please confirm end-to-end wiring. | Feature-forwarding is declared in every crate: `openfang-memory` exposes `postgres` / `qdrant` / `http-memory`. [crates/openfang-kernel/Cargo.toml](crates/openfang-kernel/Cargo.toml) has `postgres = ["openfang-memory/postgres"]`, `qdrant = ["openfang-memory/qdrant"]`. [crates/openfang-cli/Cargo.toml](crates/openfang-cli/Cargo.toml) has matching `postgres = ["openfang-kernel/postgres"]`, `qdrant = ["openfang-kernel/qdrant"]`. Verified by building all four combinations (`--no-default-features`, `--features postgres`, `--features qdrant`, `--features postgres,qdrant`); a unit test (`feature_gated_backend_errors_cleanly_when_feature_off`) locks in the fail-fast behavior when a feature-gated backend is configured without its cargo feature. |


### `MemorySubstrate::usage_conn()` removed

Use the trait-object accessors instead of reaching for a raw SQLite handle.

**Before:**
```rust
let conn = memory.usage_conn();
```

**After:**
```rust
// Borrowed trait object:
let usage: &dyn UsageBackend = memory.usage();

// Or an owned Arc for background tasks:
let usage: Arc<dyn UsageBackend> = memory.usage_arc();
```

### SQLite store modules moved under `openfang_memory::sqlite::`

`consolidation`, `knowledge`, `semantic`, `session`, `structured`, `usage`,
`audit`, `paired_devices`, and `task_queue` now live under the `sqlite::`
submodule. The top-level re-exports are gone.

**Before:**
```rust
use openfang_memory::knowledge::KnowledgeStore;
use openfang_memory::consolidation::ConsolidationEngine;
```

**After:**
```rust
use openfang_memory::sqlite::knowledge::KnowledgeStore;
use openfang_memory::sqlite::consolidation::ConsolidationEngine;
```

### New optional feature flags: `postgres`, `qdrant`

`openfang-memory` now gates its PostgreSQL and Qdrant backends behind Cargo
features. `openfang-kernel` and `openfang-cli` forward matching feature names.
The SQLite backend remains the default and requires no feature flag.

```toml
[dependencies]
openfang-memory = { version = "*", features = ["postgres", "qdrant"] }
```

### `HttpSemanticStore::forget` and `update_embedding` now return `Err`

Previously both methods silently fell through to the local SQLite fallback
and fabricated IDs on miss. They now return an explicit error on unknown IDs
instead of masking the failure. Callers that relied on the silent fallback
must handle the error path.

---

## Typed memory backend config (replaces string backend names)

`MemoryConfig::backend` and `MemoryConfig::semantic_backend` are now typed
enums instead of `String` / `Option<String>`. This is a source-level breaking
change for Rust code that constructs `MemoryConfig` directly. Configuration
files are unchanged.

### TOML is unchanged

- Both enums use `#[serde(rename_all = "snake_case")]`, so existing values
  (`"sqlite"`, `"postgres"`, `"qdrant"`, `"http"`) deserialize as before.
- No edits to `~/.openfang/config.toml` are required.

### Rust construction

**Before:**
```rust
MemoryConfig {
    backend: "postgres".to_string(),
    semantic_backend: Some("qdrant".to_string()),
    ..Default::default()
}
```

**After:**
```rust
use openfang_memory::{MemoryBackendKind, SemanticBackendKind};
// also re-exported at openfang_types::config::{MemoryBackendKind, SemanticBackendKind}

MemoryConfig {
    backend: MemoryBackendKind::Postgres,
    semantic_backend: Some(SemanticBackendKind::Qdrant),
    ..Default::default()
}
```

The `Kind` suffix disambiguates these config enums from the `SemanticBackend`
and `StructuredBackend` *traits* defined in `openfang_types::storage`.

### Fail-fast backend initialization

- Qdrant unreachable, HTTP gateway health check failure, or Postgres
  connection failure now exit the daemon at boot with a readable error.
- Previous builds silently degraded to SQLite on these failures.
- If silent SQLite was the desired behavior, set `backend = "sqlite"` (and
  omit or set `semantic_backend = "sqlite"`) explicitly.

### New validation: `postgres_pool_size`

- Must be in `1..=1000`. Zero or out-of-range values are rejected at config
  load time.

### Memory backend connection configs are nested (TOML unchanged)

Per-backend connection fields previously lived as flat fields on
`MemoryConfig`. They are now grouped into typed structs behind
`#[serde(flatten)]`, so existing `~/.openfang/config.toml` files keep working
unchanged.

- `postgres_url` now lives on `PostgresConnConfig` under `postgres:`.
- `qdrant_url`, `qdrant_api_key_env`, `qdrant_collection` now live on
  `QdrantConnConfig` under `qdrant:`.
- `http_url`, `http_token_env` now live on `HttpMemoryConnConfig` under
  `http:`.
- `postgres_pool_size` stays at the top level of `MemoryConfig` (its validation
  bounds live there).

**Before:**
```rust
MemoryConfig {
    postgres_url: Some("postgres://...".into()),
    ..Default::default()
}
```

**After:**
```rust
use openfang_types::config::{PostgresConnConfig, QdrantConnConfig, HttpMemoryConnConfig};

MemoryConfig {
    postgres: PostgresConnConfig { postgres_url: Some("postgres://...".into()) },
    ..Default::default()
}
```

The new structs are re-exported from
`openfang_types::config::{PostgresConnConfig, QdrantConnConfig, HttpMemoryConnConfig}`.

### New direct dependencies — supply-chain notes

The pluggable-memory work adds these direct dependencies. All match the
canonical publisher / crate name; none are typosquats:

| Crate | Purpose | Notes |
|-------|---------|-------|
| `sqlite-vec` (pre-v1) | SQLite vector extension for fast semantic recall | Actively maintained (v0.1.9, released 2026-03-31). Sponsored by Mozilla, Fly.io, Turso, SQLite Cloud, Shinkai. Supports Linux, macOS, Windows, WASM per upstream README. Our CI matrix (`.github/workflows/ci.yml`) builds + tests on `ubuntu-latest`, `macos-latest`, `windows-latest` — Windows compile regressions would be caught there. Release matrix (`.github/workflows/release.yml`) produces artifacts for the same three platforms. The upstream `pre-v1` warning means we should be careful when bumping the minor version; patch bumps within `0.1.x` are accepted by the default semver caret. |
| `hex`, `sha2` | Audit-chain hashing | Standard, widely audited. |
| `tokio-postgres`, `deadpool-postgres`, `pgvector` | Postgres backend (feature-gated) | Canonical Rust Postgres stack; `pgvector` is the crate maintained by pgvector's author. |
| `qdrant-client` | Qdrant backend (feature-gated) | Official Qdrant Rust client. |

The transitive set (`tonic`, `prost-types`, `hyper-timeout`, `deadpool`,
`stringprep`, etc.) is what you'd expect for gRPC + Postgres and contains
no surprises.

### `unsafe` FFI addition: `sqlite-vec` extension registration

- `SqliteBackend::open` and `SqliteBackend::open_in_memory` each contain one
  `unsafe` block that calls `rusqlite::ffi::sqlite3_auto_extension` with a
  transmuted pointer to `sqlite_vec::sqlite3_vec_init`. This is the
  documented registration pattern for the `sqlite-vec` crate — both blocks
  carry `// SAFETY:` comments explaining the ABI-compatible transmute,
  idempotency, and process-global semantics.
