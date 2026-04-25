# Changelog

All notable changes to OpenFang will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.1] - 2026-04-25

Hardening release — pure additive primitives with no breaking API changes. Most new modules ship as standalone primitives behind explicit feature seams; the kernel/AppState wiring that turns them into end-to-end behavior is queued as follow-up plumbing and tracked in `docs/hardening-status.md`.

### Added

- **soul.md YAML frontmatter** — agents now carry structured `name / archetype / values / non_negotiables / memory_focus / last_reflection_at` in SOUL.md frontmatter. Persona is injected into the system prompt inside explicit `<persona>…</persona>` tags so prompt-injection in scraped tool output cannot impersonate persona content. Backward-compatible: SOUL.md without frontmatter renders the same content unchanged.
- **soul reflection pipeline** — `reflection.rs` provides the building blocks for the 6-hour cadence self-update: prompt builder, strict-JSON response parser with `deny_unknown_fields`, two-phase `soul_patch_proposal.md` write that's only applied on next boot, cadence log + `can_reflect_now` guard (4h min gap, 4 reflections per 24h max), and `check_immutable_fields` defending `name / archetype / values / non_negotiables`.
- **Ollama base_url loopback enforcement** — local LLM provider `base_url` must resolve to a loopback address. Override with `OPENFANG_OLLAMA_ALLOW_NON_LOOPBACK=1` for trusted-LAN deployments.
- **Ollama model-not-found enrichment** — opaque 404s on missing Ollama models now resolve through `/api/tags` and surface as `ModelNotFound` errors listing the available models.
- **External memory backend trait** — `openfang_memory::external::ExternalMemoryBackend` lets the kernel register additional memory backends with criticality-aware fanout. `ExternalBackends` registry implements `search_union`, `write_fanout` (errors only on `Critical` backends), `aggregate_health`.
- **Obsidian vault backend** — read-union and write-fanout against an Obsidian vault. Walks `.md` files under the vault root with hard caps (2000 files / 64 KiB per note). Writes land under `<vault>/OpenFang/inbox/<date>-<slug>.md` with YAML frontmatter (`agent_id, confidence, source_url, untrusted, source, scope`). Slug sanitisation + canonical-parent traversal guards. Defaults to `Degraded` criticality.
- **Mempalace MCP backend skeleton** — `MempalaceClient` trait + `MempalaceBackend` (`Critical` criticality by default). `verify_boot()` helper for the kernel's boot-warm path, returning the verbatim remediation string pointing at `~/Library/Mobile Documents/com~apple~CloudDocs/mempalace/INTEGRATION_PLAN.md` on failure. Concrete rmcp wire-up lives in a higher crate.
- **Universal untrusted-content channel** — `openfang_runtime::untrusted` with `wrap(source, body)`, `strip_jailbreak_markers` (16 markers including ChatML role tags, Llama `</s>`, `<persona>`/`</persona>`, and the tool-call delimiters consumed by `recover_text_tool_calls`), and `quarantine_write()` for isolation-first staging under `$XDG_DATA_HOME/openfang/quarantine/<agent_id>/<sha-prefix>/`. Hard path-traversal guards on agent-id and post-canonicalisation base-prefix check.
- **Triage scanners** — `openfang_runtime::triage` with `ContentScanner` trait, `Verdict {Safe, Suspicious, Malicious, ScanFailed}` + `worst_of` reducer (explicit fail-closed precedence: Malicious > ScanFailed > Suspicious > Safe). `HeuristicScanner` covers 12 regex rules (jailbreak preludes, credential exfil, SSRF / cloud-metadata, obfuscation). `MoonlockDeepscanner` shells out to `OPENFANG_MOONLOCK_PATH` or `which moonlock` with a 30s timeout and a permissive verdict-alias JSON parser; every failure path is categorised in `findings`.
- **Cyber-agent classifier pipeline** — `triage::classifier::run_classifier(driver, model, …)` invokes the bundled `agents/cyber/` (frontier-model only, claude-opus-4-7, temp 0.0) with scanner outcomes + content summary + cyber-intel excerpts. Strict JSON output schema, `deny_unknown_fields`, range-checked confidence, `questionable ↔ suspicious` alias. Fail-closed: any LLM or parse error yields `ClassifierDecision::scan_failed_pinboard()` so questionable content reaches the pinboard rather than memory. Operator doc at `docs/security/cyber-intel-vault-setup.md`.
- **Triage pinboard** — `triage::pinboard::PinboardStore` filesystem layer with `submit / list / get / decide`, an explicit state machine (`Pending → Allowed via Allow`, `Pending → Quarantined via Quarantine`, `Comment` is audit-only), append-only audit log, `render_for_obsidian()` producing a single Markdown doc with YAML frontmatter for `<vault>/OpenFang/pinboard/<id>.md`. Reverse transitions (`Allowed → Quarantined`) are rejected as `InvalidTransition` so a release cannot be silently undone.
- **Boot-warm registry** — `openfang_runtime::boot_warm` provides the data type the future `/api/health` change reads. `Criticality {Critical, NonCritical}`, `SubsystemStatus {Pending, Ok, Degraded, Failed}`, `AggregateState {Warming, Degraded, Failed, Ok}` with `http_status()` mapping (Warming/Failed → 503, others → 200). `tick_deadline()` auto-flips Pending NonCritical entries to a "warm timeout" Degraded after the configured window; Critical entries never auto-flip.
- **Deploy assets** under `deploy/` — macOS launchd plist, Linux user-mode systemd unit, Warp workflow pack, bash + zsh tab completion, Homebrew tap formula skeleton. The pre-existing system-wide `deploy/openfang.service` is unchanged. See `deploy/README.md`.
- **Hardening status doc** at `docs/hardening-status.md` — single source of truth tracking the v0.6.1 phase rollout and what remains as plumbing follow-up.

### Fixed

- Heartbeat monitor no longer marks idle reactive agents as `Crashed`. Reactive agents legitimately wait between messages; flagging them was producing a crash/recover loop on otherwise healthy Slack/Discord-bound agents. Fixes #1102.
- Streaming LLM calls keep `last_active` fresh via a background 30-second `touch_agent` ticker around `stream_with_retry`. Long-running local-LLM streams (Ollama cold-load, large context windows) no longer trip the unresponsive-agent threshold mid-stream. Fixes #1089.

### Security

- Local LLM `base_url` is now loopback-only by default. A stray `provider="ollama"` with `base_url="http://192.168.1.5:11434"` previously exposed the agent's tool-calling surface to the LAN with no auth; this configuration is now refused unless the operator opts in via `OPENFANG_OLLAMA_ALLOW_NON_LOOPBACK=1`.
- The persona section of the system prompt is now wrapped in explicit `<persona>…</persona>` tags. Body text (including SOUL.md edits) is sanitised so a hostile edit cannot close the persona tag early or open a fake one.
- Tool-call delimiters consumed by the text-fallback parser at `agent_loop.rs:2232` (`<tool_use>`, `<tool_call>`, `<function_call>`) are now neutralised inside `untrusted::wrap()`. Scraped pages cannot smuggle fake tool calls through tool output.
- All known prompt-injection role delimiters (ChatML `<|im_start|>` / role tags, Llama `</s>`, Anthropic `<thinking>`) are stripped from external content before the model sees it.
- Triage pipeline's filesystem layer (quarantine, pinboard, Obsidian inbox) enforces post-canonicalisation parent-prefix checks so id-derived paths cannot escape their roots even if the sanitiser regresses.

### Notes for v0.6.0 → v0.6.1 upgrade

- No breaking API changes. All new behaviour is either default-on for new code paths or gated behind explicit config keys / env vars.
- The CLI subcommand surface advertised by the new shell completions (`pinboard`, `soul`, `reflection`, …) reflects what the primitives support; the actual subcommand handlers land in a follow-up commit.
- See `docs/hardening-status.md` for the still-pending plumbing (AppState wiring of `BootWarmRegistry`, `/api/health` enrichment, `/api/pinboard` routes, kernel call sites for the new modules).

## [0.5.10] - 2026-04-17

### Fixed

- Non-loopback requests with no `api_key` configured now return 401 by default. Opt out with `OPENFANG_ALLOW_NO_AUTH=1`. Fixes the B1/B2 authentication bypass from #1034.
- Agent `context.md` is re-read on every turn so external updates take effect mid-session. Opt out per agent with `cache_context = true` on the manifest. Fixes #843.
- `openfang config get default_model.base_url` now prints the configured URL instead of an empty string. Missing keys return a clear "not found" error. Fixes #905.
- `schedule_create`, `schedule_list`, and `schedule_delete` tools plus the `/api/schedules` routes now use the kernel cron scheduler, so scheduled jobs actually fire. One-shot idempotent migration imports legacy shared-memory entries at startup. Fixes #1069.
- Multimodal user messages now combine text and image blocks into a single message so the LLM sees both. Fixes #1043.

### Added

- `openfang hand config <id>` subcommand: get, set, unset, and list settings on an active hand instance. Fixes #809.
- Optional per-channel `prefix_agent_name` setting (`off` / `bracket` / `bold_bracket`). Wraps outbound agent responses so users in multi-agent channels can see which agent replied. Default is off, byte-identical to prior behavior. Fixes #980.

### Closed as invalid

- #818 and #819. Both reference a knowledge-domain API that does not exist on `main`. Filed against an unmerged feature branch (`plan/013-audit-remediation`). Close with a note to build the proposed validation and stale-timestamp surfacing into that feature when it lands.

## [0.5.9] - 2026-04-10

### Changed

- **BREAKING:** Dashboard password hashing switched from SHA256 to Argon2id. Existing `password_hash` values in `config.toml` must be regenerated with `openfang auth hash-password`. Only affects users with `[auth] enabled = true`.

### Fixed

- Dashboard passwords were hashed with plain SHA256 (no salt), making them vulnerable to rainbow table and GPU-accelerated brute force attacks. Now uses Argon2id with random salts.

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
