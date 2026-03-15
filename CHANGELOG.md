# Changelog

All notable changes to OpenFang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Multi-Agent Foundation Release Candidate
- Finalized the multi-agent foundation release candidate handoff for `feat/multiagent-foundation-v1`, covering declarative workflow orchestration, durable workflow recovery, review/retry controls, audit/metrics hooks, session isolation, shadow routing, rollback controls, and OpenClaw migration compatibility.
- Confirmed the upstream delivery plan is split into six reviewable slices with rollback notes and focused gates recorded in `.codex-tasks/openfang-multiagent-foundation/PROGRESS.md`.
- Recorded the operator acceptance outcome: the two-user shadow drill passed with stable user-specific routing, no cross-session context bleed, successful shadow comparison capture, and rollback checklist execution inside the five-minute rollback window.

### Handoff Checklist
- [x] Operator guide is published in `docs/multi-agent-foundation.md` with shadow-run, rollout, and rollback instructions.
- [x] Upstream PR slicing plan and submission checklist are captured in `.codex-tasks/openfang-multiagent-foundation/PROGRESS.md`.
- [x] Two-user acceptance drill passed via focused gates for route specificity, session isolation, shadow comparison, and rollback controls.
- [x] Release handoff notes are consolidated in `CHANGELOG.md` so the final row is documentation-only and does not introduce unrelated runtime refactors.

### Known Risks
- Multi-session isolation spans kernel, runtime, API, and workspace persistence; keep rollout gradual and monitor for session-scoped memory regressions when traffic increases.
- Shadow and rollback controls are operator-facing guardrails; keep `stable_path` anchored to the last production route until each upstream slice is reviewed and landed.
- OpenClaw migration compatibility covers historical identity/provider/bindings variants, so representative production exports should still be revalidated before promotion.

### Operational Quality Gates
- Added CI workflow `.github/workflows/pre-pr-review-gate.yml` to enforce comprehensive pre-PR checklist structure on `pull_request -> main`.
- Added PR template `.github/pull_request_template.md` with mandatory sections: summary, scope, validation evidence, findings, risks, and rollback.
- Added branch-protection automation script `scripts/ci/configure_branch_protection.sh` to apply required checks/review rules for `main` (default checks include pre-PR gate plus CI check/test/clippy/format).
- Added operator docs: `docs/pr-quality-gates.md`; linked from `docs/README.md` and `CONTRIBUTING.md`.

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
