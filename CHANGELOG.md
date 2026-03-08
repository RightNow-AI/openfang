# Changelog

All notable changes to OpenFang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.29] - 2026-03-08
### Added
- **Phase 5.1: L1/L2 Caching Layer**
  - New `maestro-cache` crate providing a transparent 3-tier caching layer (L1 Moka → L2 Redis → L3 SurrealDB).
  - `CachingMemory` struct wraps `SurrealMemorySubstrate` and implements the `Memory` trait plus all 30+ substrate-specific methods.
  - Implements cache-aside pattern for reads and write-invalidate for writes.
  - L1 Moka cache is in-process with separate partitions for KV, sessions, and agents, each with configurable TTL and capacity.
  - L2 Redis cache is optional (feature-gated `redis-cache`), distributed, and designed for graceful degradation if Redis is unavailable.
  - `CacheConfig` struct allows for full configuration of all tiers.
  - Integrated into the kernel as a drop-in replacement for `Arc<SurrealMemorySubstrate>`.
  - 8 tests passing for L1 and L2 cache logic.

## [0.3.28] - 2026-03-08
### Added
- **Phase 4.3: SurrealDB v3 Upgrade**
  - Upgraded SurrealDB dependency from v2.x to v3.0.2.
  - Replaced `RocksDb` engine with `SurrealKv` engine (`kv-surrealkv` feature).
- **Phase 4.4: Full Workspace Async Propagation**
  - Removed all `block_on` calls from library code, making the entire workspace natively async.
  - Propagated `async`/`.await` through all 7 core crates: `kernel`, `api`, `cli`, `runtime`, `desktop`, `types`, and `surreal-memory`.
  - Established 7 distinct sync/async boundaries for entry points like `main.rs`, TUI, and WASM host functions.
  - 18 files changed, 339 insertions, 330 deletions.

## [0.3.27] - 2026-03-08
### Added
- **Phase 4.2: SurrealDB Query Implementation**
  - Implemented all 24 `SurrealMemorySubstrate` methods with real SurrealQL queries.
  - Implemented all 12 `SurrealUsageStore` methods.
  - Defined and initialized 8 SurrealDB tables: `memories`, `sessions`, `kv_store`, `agents`, `paired_devices`, `tasks`, `usage_records`, `llm_summaries`.

## [0.3.26] - 2026-03-08
### Added
- **Phase 4.1: Type Unification & Memory Trait Extension**
  - Unified `Session`, `UsageRecord`, and `Message` types in `openfang-types`.
  - Extended the `Memory` trait with `save_session` and other methods to make the runtime backend-agnostic.
  - Refactored the kernel to use a standalone SQLite connection for the `MeteringEngine`.

[Unreleased]: https://github.com/ParadiseAI/maestro-legacy/compare/v0.3.29...HEAD
[0.3.29]: https://github.com/ParadiseAI/maestro-legacy/releases/tag/v0.3.29
[0.3.28]: https://github.com/ParadiseAI/maestro-legacy/releases/tag/v0.3.28
[0.3.27]: https://github.com/ParadiseAI/maestro-legacy/releases/tag/v0.3.27
[0.3.26]: https://github.com/ParadiseAI/maestro-legacy/releases/tag/v0.3.26

## [Unreleased] - Phase 4 Complete ✅

### Added

#### Phase 4: SurrealDB Memory Substrate Replacement
- **Complete SurrealDB backend migration**: Replaced SQLite with SurrealDB graph database for memory persistence
- **New crate**: `maestro-surreal-memory` implementing full Memory trait with SurrealDB operations
- **Advanced knowledge graph**: Entity-relation graph operations with graph pattern queries
- **SurrealDB schema**: Tables for memory fragments, entities, relations, agents, and sessions
- **Export/Import system**: JSON format memory backup and restore capabilities
- **Verification system**: Gate-based verification script ensuring implementation quality
- **Session management**: Create, load, save, delete sessions with SurrealDB persistence

#### Enhanced Memory Capabilities
- **Graph operations**: Add/query entities and relations for knowledge management
- **Memory consolidation**: Decay confidence scores over time
- **KV operations**: Agent-scoped key-value storage
- **MessagePack support structure**: Ready for MessagePack export format
- **Async operations**: Full async/await implementation for database operations
- **Schema flexibility**: Runtime table creation and field validation

#### Development Infrastructure
- **Verification script**: `.maestro/verify/verify_phase4_surrealdb.sh` with 4-gate checking
- **Integration testing**: Live testing procedures for memory substrate validation
- **Documentation updates**: README and architecture documentation updated
- **Migration path**: Clear upgrade process from SQLite to SurrealDB backend

### Changed
- **Memory substrate**: Moved from SQLite to SurrealDB graph database
- **Architecture**: Updated crate descriptions and module dependencies
- **Dependencies**: Added SurrealDB workspace dependency
- **Session types**: Enhanced Session struct with SurrealDB compatibility

## [0.1.0] - 2026-02-24

### Added

#### Core Platform
- 15-crate Rust workspace: types, memory, runtime, kernel, api, channels, wire, cli, migrate, skills, hands, extensions, desktop, xtask
- Agent lifecycle management: spawn, list, kill, clone, mode switching (Full/Assist/Observe)
- SQLite-backed memory substrate with structured KV, semantic recall, vector embeddings
- 41 built-in tools (filesystem, web, shell, browser, scheduling, collaboration, image analysis, inter-agent, TTS, media)
- WASM sandbox with dual metering (fuel + epoch interruption with watchdog thread)
- Workflow engine with pipelines, fan-out parallelism, conditional steps, loops, and variable expansion
- Visual workflow builder with drag-and-drop node graph, 7 node types, and TOML export
- Trigger system with event pattern matching, content filters, and fire limits
- Event bus with publish/subscribe and correlation IDs
- 7 Hands packages for autonomous agent actions

#### LLM Support
- 3 native LLM drivers: Anthropic, Google Gemini, OpenAI-compatible
- 27 providers: Anthropic, Gemini, OpenAI, Groq, OpenRouter, DeepSeek, Together, Mistral, Fireworks, Cohere, Perplexity, xAI, AI21, Cerebras, SambaNova, Hugging Face, Replicate, Ollama, vLLM, LM Studio, and more
- Model catalog with 130+ built-in models, 23 aliases, tier classification
- Intelligent model routing with task complexity scoring
- Fallback driver for automatic failover between providers
- Cost estimation and metering engine with per-model pricing
- Streaming support (SSE) across all drivers

#### Token Management & Context
- Token-aware session compaction (chars/4 heuristic, triggers at 70% context capacity)
- In-loop emergency trimming at 70%/90% thresholds with summary injection
- Tool profile filtering (cuts default 41 tools to 4-10 for chat agents, saving 15-20K tokens)
- Context budget allocation for system prompt, tools, history, and response
- MAX_TOOL_RESULT_CHARS reduced from 50K to 15K to prevent tool result bloat
- Default token quota raised from 100K to 1M per hour

#### Security
- Capability-based access control with privilege escalation prevention
- Path traversal protection in all file tools
- SSRF protection blocking private IPs and cloud metadata endpoints
- Ed25519 signed agent manifests
- Merkle hash chain audit trail with tamper detection
- Information flow taint tracking
- HMAC-SHA256 mutual authentication for peer wire protocol
- API key authentication with Bearer token
- GCRA rate limiter with cost-aware token buckets
- Security headers middleware (CSP, X-Frame-Options, HSTS)
- Secret zeroization on all API key fields
- Subprocess environment isolation
- Health endpoint redaction (public minimal, auth full)
- Loop guard with SHA256-based detection and circuit breaker thresholds
- Session repair (validates and fixes orphaned tool results, empty messages)

#### Channels
- 40 channel adapters: Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email, Teams, Mattermost, Google Chat, Webex, Feishu/Lark, LINE, Viber, Facebook Messenger, Mastodon, Bluesky, Reddit, LinkedIn, Twitch, IRC, XMPP, and 18 more
- Unified bridge with agent routing, command handling, message splitting
- Per-channel user filtering and RBAC enforcement
- Graceful shutdown, exponential backoff, secret zeroization on all adapters

#### API
- 100+ REST/WS/SSE API endpoints (axum 0.8)
- WebSocket real-time streaming with per-agent connections
- OpenAI-compatible `/v1/chat/completions` API (streaming SSE + non-streaming)
- OpenAI-compatible `/v1/models` endpoint
- WebChat embedded UI with Alpine.js
- Google A2A protocol support (agent card, task send/get/cancel)
- Prometheus text-format `/api/metrics` endpoint for monitoring
- Multi-session management: list, create, switch, label sessions per agent
- Usage analytics: summary, by-model, daily breakdown
- Config hot-reload via polling (30-second interval, no restart required)

#### Web UI
- Chat message search with Ctrl+F, real-time filtering, text highlighting
- Voice input with hold-to-record mic button (WebM/Opus codec)
- TTS audio playback inline in tool cards
- Browser screenshot rendering in chat (inline images)
- Canvas rendering with iframe sandbox and CSP support
- Session switcher dropdown in chat header
- 6-step first-run setup wizard with provider API key help (12 providers)
- Skill marketplace with 4 tabs (Installed, ClawHub, MCP Servers, Quick Start)
- Copy-to-clipboard on messages, message timestamps
- Visual workflow builder with drag-and-drop canvas

#### Client SDKs
- JavaScript SDK (`@openfang/sdk`): full REST API client with streaming, TypeScript declarations
- Python client SDK (`openfang_client`): zero-dependency stdlib client with SSE streaming
- Python agent SDK (`openfang_sdk`): decorator-based framework for writing Python agents
- Usage examples for both languages (basic + streaming)

#### CLI
- 14+ subcommands: init, start, agent, workflow, trigger, migrate, skill, channel, config, chat, status, doctor, dashboard, mcp
- Daemon auto-detection via PID file
- Shell completion generation (bash, zsh, fish, PowerShell)
- MCP server mode for IDE integration

#### Skills Ecosystem
- 60 bundled skills across 14 categories
- Skill registry with TOML manifests
- 4 runtimes: Python, Node.js, WASM, PromptOnly
- FangHub marketplace with search/install
- ClawHub client for OpenClaw skill compatibility
- SKILL.md parser with auto-conversion
- SHA256 checksum verification
- Prompt injection scanning on skill content

#### Desktop App
- Tauri 2.0 native desktop app
- System tray with status and quick actions
- Single-instance enforcement
- Hide-to-tray on close
- Updated CSP for media, frame, and blob sources

#### Session Management
- LLM-based session compaction with token-aware triggers
- Multi-session per agent with named labels
- Session switching via API and UI
- Cross-channel canonical sessions
- Extended chat commands: `/new`, `/compact`, `/model`, `/stop`, `/usage`, `/think`

#### Image Support
- `ContentBlock::Image` with base64 inline data
- Media type validation (png, jpeg, gif, webp only)
- 5MB size limit enforcement
- Mapped to all 3 native LLM drivers

#### Usage Tracking
- Per-response cost estimation with model-aware pricing
- Usage footer in WebSocket responses and WebChat UI
- Usage events persisted to SQLite
- Quota enforcement with hourly windows

#### Interoperability
- OpenClaw migration engine (YAML/JSON5 to TOML)
- MCP client (JSON-RPC 2.0 over stdio/SSE, tool namespacing)
- MCP server (exposes OpenFang tools via MCP protocol)
- A2A protocol client and server
- Tool name compatibility mappings (21 OpenClaw tool names)

#### Infrastructure
- Multi-stage Dockerfile (debian:bookworm-slim runtime)
- docker-compose.yml with volume persistence
- GitHub Actions CI (check, test, clippy, format)
- GitHub Actions release (multi-platform, GHCR push, SHA256 checksums)
- Cross-platform install script (curl/irm one-liner)
- systemd service file for Linux deployment

#### Multi-User
- RBAC with Owner/Admin/User/Viewer roles
- Channel identity resolution
- Per-user authorization checks
- Device pairing and approval system

#### Production Readiness
- 1731+ tests across 15 crates, 0 failures
- Cross-platform support (Linux, macOS, Windows)
- Graceful shutdown with signal handling (SIGINT/SIGTERM on Unix, Ctrl+C on Windows)
- Daemon PID file with stale process detection
- Release profile with LTO, single codegen unit, symbol stripping
- Prometheus metrics for monitoring
- Config hot-reload without restart

[0.1.0]: https://github.com/RightNow-AI/openfang/releases/tag/v0.1.0
