<h1 align="center">OpenFang — Aldine Fork</h1>
<h3 align="center">Agent OS + Business Mode Wizards + Next.js Dashboard</h3>

<p align="center">
  Fork of <a href="https://github.com/RightNow-AI/openfang">RightNow-AI/openfang</a> — synced to v0.4.6 with new Business Mode workflows, Command Center, and a full Next.js frontend.
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT" />
  <img src="https://img.shields.io/badge/synced-upstream%20v0.4.6-blue?style=flat-square" alt="upstream v0.4.6" />
  <img src="https://img.shields.io/badge/frontend-Next.js%2015-black?style=flat-square" alt="Next.js" />
  <img src="https://img.shields.io/badge/backend-Rust%20%2B%20Axum-orange?style=flat-square" alt="Axum" />
</p>

---

## What's in This Fork

This fork extends the upstream OpenFang Agent OS with:

1. **Business Mode Wizards** — three full wizard flows (Agency, Growth, School) with per-mode task catalogs, approval queues, and results tracking
2. **Command Center** — client onboarding wizard with task planning, approval, and execution pipeline
3. **Next.js 15 dashboard** — primary frontend replacing the Alpine static UI, running on port 3002
4. **Rust API routes** — `openfang-api` additions for all business modes and command center (new `modes.rs`, `command_center.rs` files)
5. **Sidebar reorder** — Brand, Work, and Agents sections moved above Monitor for faster access to core workflows
6. **Error handling fix** — `mode-api.ts` properly unwraps structured `{code, message}` backend errors instead of showing `[object Object]`

---

## Running This Fork

### Prerequisites
- Rust toolchain (`rustup`)
- Node.js 18+
- An API key for at least one LLM provider (Groq, OpenAI, Anthropic, etc.)

### 1. Build and start the backend

```bash
cargo build --release -p openfang-cli

# Start the daemon (needs at least one LLM API key)
GROQ_API_KEY=your_key_here ./target/release/openfang.exe start

# Verify it's up
curl http://127.0.0.1:50051/api/health
# → {"status":"ok","version":"0.4.x"}
```

### 2. Start the Next.js frontend

```bash
cd sdk/javascript/examples/nextjs-app-router
npm install
npm run dev -- --port 3002
# Open http://localhost:3002
```

> The backend dashboard root (`GET /`) redirects to `http://localhost:3002`. Override with `OPENFANG_DASHBOARD_URL=http://...` or set `OPENFANG_LEGACY_UI=1` to restore the Alpine UI.

---

## New API Endpoints

### Business Modes (`modes.rs`)

All three modes (Agency, Growth, School) share the same route shape:

| Method | Route | Description |
|--------|-------|-------------|
| `POST` | `/modes/{mode}/records` | Create a new mode record (program, campaign, or client brief) |
| `GET` | `/modes/{mode}/records` | List all records for a mode |
| `GET` | `/modes/{mode}/records/{id}` | Get a single record |
| `PUT` | `/modes/{mode}/records/{id}` | Update a record |
| `POST` | `/modes/{mode}/generate-plan` | Generate task plan from a mode-specific catalog |
| `GET` | `/modes/{mode}/tasks?record_id=…` | List tasks for a record |
| `POST` | `/modes/{mode}/tasks/{id}/approve` | Approve a task |
| `POST` | `/modes/{mode}/tasks/{id}/run` | Run an approved task |
| `GET` | `/modes/{mode}/approvals?record_id=…` | List pending approvals |
| `GET` | `/modes/{mode}/results?record_id=…` | List completed results |

`{mode}` must be one of `agency`, `growth`, or `school`.

**Quick test:**

```bash
# Create a School Mode program
curl -X POST http://127.0.0.1:50051/modes/school/records \
  -H "Content-Type: application/json" \
  -d '{"title":"Rust Fundamentals","goal":"Teach Rust to 50 students"}'

# Generate a task plan
curl -X POST http://127.0.0.1:50051/modes/school/generate-plan \
  -H "Content-Type: application/json" \
  -d '{"record_id":"rec_xxx","selected_task_ids":["define_program_brief","write_lessons"]}'
```

### Command Center (`command_center.rs`)

| Method | Route | Description |
|--------|-------|-------------|
| `POST` | `/clients` | Create a new client profile |
| `GET` | `/clients/{id}` | Get a client |
| `PUT` | `/clients/{id}` | Update a client |
| `POST` | `/wizard/generate-plan` | Generate a task plan for a client |
| `GET` | `/tasks?client_id=…` | List tasks for a client |
| `POST` | `/tasks/{id}/approve` | Approve a task |
| `POST` | `/tasks/{id}/run` | Run an approved task |
| `GET` | `/approvals?client_id=…` | List pending approvals |
| `GET` | `/results?client_id=…` | List completed results |

---

## Mode Task Catalogs

Each mode has a curated task catalog. When you call `generate-plan`, you pass a list of catalog IDs and the API creates `ModeTask` records with the appropriate agent assignments, tools, and approval requirements.

### Agency Mode (12 tasks)
`intake_client_brief` → `scope_service` → `summarize_business` → `research_competitors` → `build_brand_voice` → `build_delivery_plan` → `assign_tasks` → `draft_client_copy` _(approval)_ → `send_client_email` _(approval)_ → `package_delivery` _(approval)_ → `capture_followups` → `identify_upsells`

### Growth Mode (15 tasks)
`write_campaign_brief` → `sharpen_offer` → `research_competitor_ads` → `build_creative_intelligence` → `generate_hooks` _(approval)_ → `develop_angles` _(approval)_ → `write_scripts` _(approval)_ → `write_email_variants` _(approval)_ → `video_studio_flow` _(approval)_ → `design_statics` _(approval)_ → `creative_qa` _(approval)_ → `publish_assets` _(approval)_ → `read_performance` → `build_optimization_plan` → `plan_next_experiments` _(approval)_

### School Mode (13 tasks)
`define_program_brief` → `sharpen_enrollment_offer` → `map_student_needs` → `build_curriculum_outline` _(approval)_ → `write_lessons` _(approval)_ → `design_assignments` _(approval)_ → `build_resources` _(approval)_ → `run_cohort_onboarding` _(approval)_ → `send_email_reminders` _(approval)_ → `review_assignments` _(approval)_ → `track_student_health` → `capture_testimonials` → `find_upsells`

---

## Frontend Structure (`sdk/javascript/examples/nextjs-app-router/`)

```
app/
  agency/             Agency Mode wizard + detail pages
  growth/             Growth Mode wizard + studio + detail pages
  school/             School Mode wizard + cohort + detail pages
  command-center/     Command Center wizard + client detail pages
  components/
    Sidebar.js        Sidebar nav (Brand, Work, Agents above Monitor)
  api/modes/          Next.js proxy routes → backend /modes/* endpoints
lib/
  mode-api.ts         Typed client for all mode API calls
  mode-types.ts       Shared TypeScript types for modes, tasks, approvals
```

### Sidebar order (top to bottom)
Planner → Chat → **Brand** → **Work** (Command Center, Agency, Growth, School) → **Agents** (Sessions, Approvals, Comms) → Monitor → Automation → Extensions → System

---

## Changed Files

### Rust (`crates/openfang-api/src/`)

| File | Change |
|------|--------|
| `modes.rs` | **New** — in-memory store + HTTP handlers for Agency/Growth/School modes |
| `command_center.rs` | **New** — in-memory store + HTTP handlers for Command Center clients/tasks |
| `lib.rs` | Added `pub mod modes;` and `pub mod command_center;` |
| `server.rs` | Registered both new routers into the Axum app + added `ModesState` / `CommandCenterState` to `AppState` |

> **Axum route syntax note:** Routes use `{param}` syntax (not `:param`) required by the axum version in use. Handler extractors use `Path(id): Path<String>` and `Path((mode, id)): Path<(String, String)>` as needed.

### Next.js (`sdk/javascript/examples/nextjs-app-router/`)

| File | Change |
|------|--------|
| `lib/mode-api.ts` | **New** — typed fetch wrapper for all `/api/modes/*` routes; extracts `.message` from structured `{code, message}` backend errors |
| `lib/mode-types.ts` | **New** — TypeScript types for `ModeRecord`, `ModeTask`, `ModeApproval`, `ModeResult`, `BusinessMode` |
| `app/api/modes/…/route.ts` | **New** — 8 Next.js API proxy routes forwarding to `http://127.0.0.1:50051/modes/*` |
| `app/school/…` | **New** — `SchoolWizard.tsx`, `StudentHealthDashboard.tsx`, cohort page, program detail page, approvals, results |
| `app/agency/…` | **New** — `AgencyWizard.tsx`, client detail page, approvals, results |
| `app/growth/…` | **New** — `GrowthWizard.tsx`, `VideoAdStudio.tsx`, campaign detail page, studio, approvals, results |
| `app/command-center/…` | Updated — approvals + results detail pages |
| `app/components/Sidebar.js` | Reordered sections: Brand, Work, Agents now appear directly below Chat |

---

## Development

```bash
# Build workspace libs
cargo build --workspace --lib

# Run all tests
cargo test --workspace

# Lint
cargo clippy --workspace --all-targets -- -D warnings
```

---

## Upstream

This fork tracks [RightNow-AI/openfang](https://github.com/RightNow-AI/openfang). Currently synced to **v0.4.6**.

To pull future upstream changes:

```bash
git fetch upstream
git merge upstream/main -X theirs   # accept upstream for any core conflicts
```

---

## License

MIT — same as upstream.


OpenFang is an **open-source Agent Operating System** — not a chatbot framework, not a Python wrapper around an LLM, not a "multi-agent orchestrator." It is a full operating system for autonomous agents, built from scratch in Rust.

Traditional agent frameworks wait for you to type something. OpenFang runs **autonomous agents that work for you** — on schedules, 24/7, building knowledge graphs, monitoring targets, generating leads, managing your social media, and reporting results to your dashboard.

The entire system compiles to a **single ~32MB binary**. One install, one command, your agents are live.

```bash
curl -fsSL https://openfang.sh/install | sh
openfang init
openfang start
# Dashboard live at http://localhost:50051
```

<details>
<summary><strong>Windows</strong></summary>

```powershell
irm https://openfang.sh/install.ps1 | iex
openfang init
openfang start
```

</details>

---

## Hands: Agents That Actually Do Things

<p align="center"><em>"Traditional agents wait for you to type. Hands work <strong>for</strong> you."</em></p>

**Hands** are OpenFang's core innovation — pre-built autonomous capability packages that run independently, on schedules, without you having to prompt them. This is not a chatbot. This is an agent that wakes up at 6 AM, researches your competitors, builds a knowledge graph, scores the findings, and delivers a report to your Telegram before you've had coffee.

Each Hand bundles:
- **HAND.toml** — Manifest declaring tools, settings, requirements, and dashboard metrics
- **System Prompt** — Multi-phase operational playbook (not a one-liner — these are 500+ word expert procedures)
- **SKILL.md** — Domain expertise reference injected into context at runtime
- **Guardrails** — Approval gates for sensitive actions (e.g. Browser Hand requires approval before any purchase)

All compiled into the binary. No downloading, no pip install, no Docker pull.

### The 7 Bundled Hands

| Hand | What It Actually Does |
|------|----------------------|
| **Clip** | Takes a YouTube URL, downloads it, identifies the best moments, cuts them into vertical shorts with captions and thumbnails, optionally adds AI voice-over, and publishes to Telegram and WhatsApp. 8-phase pipeline. FFmpeg + yt-dlp + 5 STT backends. |
| **Lead** | Runs daily. Discovers prospects matching your ICP, enriches them with web research, scores 0-100, deduplicates against your existing database, and delivers qualified leads in CSV/JSON/Markdown. Builds ICP profiles over time. |
| **Collector** | OSINT-grade intelligence. You give it a target (company, person, topic). It monitors continuously — change detection, sentiment tracking, knowledge graph construction, and critical alerts when something important shifts. |
| **Predictor** | Superforecasting engine. Collects signals from multiple sources, builds calibrated reasoning chains, makes predictions with confidence intervals, and tracks its own accuracy using Brier scores. Has a contrarian mode that deliberately argues against consensus. |
| **Researcher** | Deep autonomous researcher. Cross-references multiple sources, evaluates credibility using CRAAP criteria (Currency, Relevance, Authority, Accuracy, Purpose), generates cited reports with APA formatting, supports multiple languages. |
| **Twitter** | Autonomous Twitter/X account manager. Creates content in 7 rotating formats, schedules posts for optimal engagement, responds to mentions, tracks performance metrics. Has an approval queue — nothing posts without your OK. |
| **Browser** | Web automation agent. Navigates sites, fills forms, clicks buttons, handles multi-step workflows. Uses Playwright bridge with session persistence. **Mandatory purchase approval gate** — it will never spend your money without explicit confirmation. |

```bash
# Activate the Researcher Hand — it starts working immediately
openfang hand activate researcher

# Check its progress anytime
openfang hand status researcher

# Activate lead generation on a daily schedule
openfang hand activate lead

# Pause without losing state
openfang hand pause lead

# See all available Hands
openfang hand list
```

**Build your own.** Define a `HAND.toml` with tools, settings, and a system prompt. Publish to FangHub.

---

## OpenFang vs The Landscape

<p align="center">
  <img src="public/assets/openfang-vs-claws.png" width="600" alt="OpenFang vs OpenClaw vs ZeroClaw" />
</p>

### Benchmarks: Measured, Not Marketed

All data from official documentation and public repositories — February 2026.

#### Cold Start Time (lower is better)

```
ZeroClaw   ██░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10 ms
OpenFang   ██████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  180 ms    ★
LangGraph  █████████████████░░░░░░░░░░░░░░░░░░░░░░░░░  2.5 sec
CrewAI     ████████████████████░░░░░░░░░░░░░░░░░░░░░░  3.0 sec
AutoGen    ██████████████████████████░░░░░░░░░░░░░░░░░  4.0 sec
OpenClaw   █████████████████████████████████████████░░  5.98 sec
```

#### Idle Memory Usage (lower is better)

```
ZeroClaw   █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    5 MB
OpenFang   ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   40 MB    ★
LangGraph  ██████████████████░░░░░░░░░░░░░░░░░░░░░░░░░  180 MB
CrewAI     ████████████████████░░░░░░░░░░░░░░░░░░░░░░░  200 MB
AutoGen    █████████████████████████░░░░░░░░░░░░░░░░░░  250 MB
OpenClaw   ████████████████████████████████████████░░░░  394 MB
```

#### Install Size (lower is better)

```
ZeroClaw   █░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  8.8 MB
OpenFang   ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   32 MB    ★
CrewAI     ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  100 MB
LangGraph  ████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  150 MB
AutoGen    ████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░  200 MB
OpenClaw   ████████████████████████████████████████░░░░  500 MB
```

#### Security Systems (higher is better)

```
OpenFang   ████████████████████████████████████████████   16      ★
ZeroClaw   ███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░    6
OpenClaw   ████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    3
AutoGen    █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    2
LangGraph  █████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    2
CrewAI     ███░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    1
```

#### Channel Adapters (higher is better)

```
OpenFang   ████████████████████████████████████████████   40      ★
ZeroClaw   ███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░   15
OpenClaw   █████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   13
CrewAI     ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
AutoGen    ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
LangGraph  ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    0
```

#### LLM Providers (higher is better)

```
ZeroClaw   ████████████████████████████████████████████   28
OpenFang   ██████████████████████████████████████████░░   27      ★
LangGraph  ██████████████████████░░░░░░░░░░░░░░░░░░░░░   15
CrewAI     ██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10
OpenClaw   ██████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░   10
AutoGen    ███████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░    8
```

### Feature-by-Feature Comparison

| Feature | OpenFang | OpenClaw | ZeroClaw | CrewAI | AutoGen | LangGraph |
|---------|----------|----------|----------|--------|---------|-----------|
| **Language** | **Rust** | TypeScript | **Rust** | Python | Python | Python |
| **Autonomous Hands** | **7 built-in** | None | None | None | None | None |
| **Security Layers** | **16 discrete** | 3 basic | 6 layers | 1 basic | Docker | AES enc. |
| **Agent Sandbox** | **WASM dual-metered** | None | Allowlists | None | Docker | None |
| **Channel Adapters** | **40** | 13 | 15 | 0 | 0 | 0 |
| **Built-in Tools** | **53 + MCP + A2A** | 50+ | 12 | Plugins | MCP | LC tools |
| **Memory** | **SQLite + vector** | File-based | SQLite FTS5 | 4-layer | External | Checkpoints |
| **Desktop App** | **Tauri 2.0** | None | None | None | Studio | None |
| **Audit Trail** | **Merkle hash-chain** | Logs | Logs | Tracing | Logs | Checkpoints |
| **Cold Start** | **<200ms** | ~6s | ~10ms | ~3s | ~4s | ~2.5s |
| **Install Size** | **~32 MB** | ~500 MB | ~8.8 MB | ~100 MB | ~200 MB | ~150 MB |
| **License** | MIT | MIT | MIT | MIT | Apache 2.0 | MIT |

---

## 16 Security Systems — Defense in Depth

OpenFang doesn't bolt security on after the fact. Every layer is independently testable and operates without a single point of failure.

| # | System | What It Does |
|---|--------|-------------|
| 1 | **WASM Dual-Metered Sandbox** | Tool code runs in WebAssembly with fuel metering + epoch interruption. A watchdog thread kills runaway code. |
| 2 | **Merkle Hash-Chain Audit Trail** | Every action is cryptographically linked to the previous one. Tamper with one entry and the entire chain breaks. |
| 3 | **Information Flow Taint Tracking** | Labels propagate through execution — secrets are tracked from source to sink. |
| 4 | **Ed25519 Signed Agent Manifests** | Every agent identity and capability set is cryptographically signed. |
| 5 | **SSRF Protection** | Blocks private IPs, cloud metadata endpoints, and DNS rebinding attacks. |
| 6 | **Secret Zeroization** | `Zeroizing<String>` auto-wipes API keys from memory the instant they're no longer needed. |
| 7 | **OFP Mutual Authentication** | HMAC-SHA256 nonce-based, constant-time verification for P2P networking. |
| 8 | **Capability Gates** | Role-based access control — agents declare required tools, the kernel enforces it. |
| 9 | **Security Headers** | CSP, X-Frame-Options, HSTS, X-Content-Type-Options on every response. |
| 10 | **Health Endpoint Redaction** | Public health check returns minimal info. Full diagnostics require authentication. |
| 11 | **Subprocess Sandbox** | `env_clear()` + selective variable passthrough. Process tree isolation with cross-platform kill. |
| 12 | **Prompt Injection Scanner** | Detects override attempts, data exfiltration patterns, and shell reference injection in skills. |
| 13 | **Loop Guard** | SHA256-based tool call loop detection with circuit breaker. Handles ping-pong patterns. |
| 14 | **Session Repair** | 7-phase message history validation and automatic recovery from corruption. |
| 15 | **Path Traversal Prevention** | Canonicalization with symlink escape prevention. `../` doesn't work here. |
| 16 | **GCRA Rate Limiter** | Cost-aware token bucket rate limiting with per-IP tracking and stale cleanup. |

---

## Architecture

14 Rust crates. 137,728 lines of code. Modular kernel design.

```
openfang-kernel      Orchestration, workflows, metering, RBAC, scheduler, budget tracking
openfang-runtime     Agent loop, 3 LLM drivers, 53 tools, WASM sandbox, MCP, A2A
openfang-api         140+ REST/WS/SSE endpoints, OpenAI-compatible API, dashboard
openfang-channels    40 messaging adapters with rate limiting, DM/group policies
openfang-memory      SQLite persistence, vector embeddings, canonical sessions, compaction
openfang-types       Core types, taint tracking, Ed25519 manifest signing, model catalog
openfang-skills      60 bundled skills, SKILL.md parser, FangHub marketplace
openfang-hands       7 autonomous Hands, HAND.toml parser, lifecycle management
openfang-extensions  25 MCP templates, AES-256-GCM credential vault, OAuth2 PKCE
openfang-wire        OFP P2P protocol with HMAC-SHA256 mutual authentication
openfang-cli         CLI with daemon management, TUI dashboard, MCP server mode
openfang-desktop     Tauri 2.0 native app (system tray, notifications, global shortcuts)
openfang-migrate     OpenClaw, LangChain, AutoGPT migration engine
xtask                Build automation
```

---

## 40 Channel Adapters

Connect your agents to every platform your users are on.

**Core:** Telegram, Discord, Slack, WhatsApp, Signal, Matrix, Email (IMAP/SMTP)
**Enterprise:** Microsoft Teams, Mattermost, Google Chat, Webex, Feishu/Lark, Zulip
**Social:** LINE, Viber, Facebook Messenger, Mastodon, Bluesky, Reddit, LinkedIn, Twitch
**Community:** IRC, XMPP, Guilded, Revolt, Keybase, Discourse, Gitter
**Privacy:** Threema, Nostr, Mumble, Nextcloud Talk, Rocket.Chat, Ntfy, Gotify
**Workplace:** Pumble, Flock, Twist, DingTalk, Zalo, Webhooks

Each adapter supports per-channel model overrides, DM/group policies, rate limiting, and output formatting.

---

## WhatsApp Web Gateway (QR Code)

Connect your personal WhatsApp account to OpenFang via QR code — just like WhatsApp Web. No Meta Business account required.

### Prerequisites

- **Node.js >= 18** installed ([download](https://nodejs.org/))
- OpenFang installed and initialized

### Setup

**1. Install the gateway dependencies:**

```bash
cd packages/whatsapp-gateway
npm install
```

**2. Configure `config.toml`:**

```toml
[channels.whatsapp]
mode = "web"
default_agent = "assistant"
```

**3. Set the gateway URL (choose one):**

Add to your shell profile for persistence:

```bash
# macOS / Linux
echo 'export WHATSAPP_WEB_GATEWAY_URL="http://127.0.0.1:3009"' >> ~/.zshrc
source ~/.zshrc
```

Or set it inline when starting the gateway:

```bash
export WHATSAPP_WEB_GATEWAY_URL="http://127.0.0.1:3009"
```

**4. Start the gateway:**

```bash
node packages/whatsapp-gateway/index.js
```

The gateway listens on port `3009` by default. Override with `WHATSAPP_GATEWAY_PORT`.

**5. Start OpenFang:**

```bash
openfang start
# Dashboard at http://localhost:4200
```

**6. Scan the QR code:**

Open the dashboard → **Channels** → **WhatsApp**. A QR code will appear. Scan it with your phone:

> **WhatsApp** → **Settings** → **Linked Devices** → **Link a Device**

Once scanned, the status changes to `connected` and incoming messages are routed to your configured agent.

### Gateway Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `WHATSAPP_WEB_GATEWAY_URL` | Gateway URL for OpenFang to connect to | _(empty = disabled)_ |
| `WHATSAPP_GATEWAY_PORT` | Port the gateway listens on | `3009` |
| `OPENFANG_URL` | OpenFang API URL the gateway reports to | `http://127.0.0.1:4200` |
| `OPENFANG_DEFAULT_AGENT` | Agent that handles incoming messages | `assistant` |

### Gateway API Endpoints

| Method | Route | Description |
|--------|-------|-------------|
| `POST` | `/login/start` | Generate QR code (returns base64 PNG) |
| `GET` | `/login/status` | Connection status (`disconnected`, `qr_ready`, `connected`) |
| `POST` | `/message/send` | Send a message (`{ "to": "5511999999999", "text": "Hello" }`) |
| `GET` | `/health` | Health check |

### Alternative: WhatsApp Cloud API

For production workloads, use the [WhatsApp Cloud API](https://developers.facebook.com/docs/whatsapp/cloud-api) with a Meta Business account. See the [Cloud API configuration docs](https://openfang.sh/docs/channels/whatsapp).



---

## 27 LLM Providers — 123+ Models

3 native drivers (Anthropic, Gemini, OpenAI-compatible) route to 27 providers:

Anthropic, Gemini, OpenAI, Groq, DeepSeek, OpenRouter, Together, Mistral, Fireworks, Cohere, Perplexity, xAI, AI21, Cerebras, SambaNova, HuggingFace, Replicate, Ollama, vLLM, LM Studio, Qwen, MiniMax, Zhipu, Moonshot, Qianfan, Bedrock, and more.

Intelligent routing with task complexity scoring, automatic fallback, cost tracking, and per-model pricing.

---

## Migrate from OpenClaw

Already running OpenClaw? One command:

```bash
# Migrate everything — agents, memory, skills, configs
openfang migrate --from openclaw

# Migrate from a specific path
openfang migrate --from openclaw --path ~/.openclaw

# Dry run first to see what would change
openfang migrate --from openclaw --dry-run
```

The migration engine imports your agents, conversation history, skills, and configuration. OpenFang reads SKILL.md natively and is compatible with the ClawHub marketplace.

---

## OpenAI-Compatible API

Drop-in replacement. Point your existing tools at OpenFang:

```bash
curl -X POST localhost:50051/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $OPENFANG_API_KEY" \
  -d '{
    "model": "researcher",
    "messages": [{"role": "user", "content": "Analyze Q4 market trends"}],
    "stream": true
  }'
```

140+ REST/WS/SSE endpoints covering agents, memory, workflows, channels, models, skills, A2A, Hands, and more.

---

## Quick Start

```bash
# 1. Install (macOS/Linux)
curl -fsSL https://openfang.sh/install | sh

# 2. Initialize — walks you through provider setup
openfang init

# 3. Start the daemon
openfang start

# 4. App backends should use OPENFANG_BASE_URL=http://127.0.0.1:50051
#    and send Authorization: Bearer $OPENFANG_API_KEY when auth is enabled.

# 5. Dashboard is live at http://localhost:50051

# 6. Activate a Hand — it starts working for you
openfang hand activate researcher

# 7. Chat with an agent
openfang chat researcher
> "What are the emerging trends in AI agent frameworks?"

# 8. Spawn a pre-built agent
openfang agent spawn coder
```

<details>
<summary><strong>Windows (PowerShell)</strong></summary>

```powershell
irm https://openfang.sh/install.ps1 | iex
openfang init
openfang start
```

</details>

---

## Development

```bash
# Build the workspace
cargo build --workspace --lib

# Run all tests (1,767+)
cargo test --workspace

# Lint (must be 0 warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all -- --check
```

---

## Stability Notice

OpenFang v0.3.30 is pre-1.0. The architecture is solid, the test suite is comprehensive, and the security model is comprehensive. That said:

- **Breaking changes** may occur between minor versions until v1.0
- **Some Hands** are more mature than others (Browser and Researcher are the most battle-tested)
- **Edge cases** exist — if you find one, [open an issue](https://github.com/RightNow-AI/openfang/issues)
- **Pin to a specific commit** for production deployments until v1.0

We ship fast and fix fast. The goal is a rock-solid v1.0 by mid-2026.

---

## Security

To report a security vulnerability, email **jaber@rightnowai.co**. We take all reports seriously and will respond within 48 hours.

---

## License

MIT — use it however you want.

---

## Links

- [Website & Documentation](https://openfang.sh)
- [Quick Start Guide](https://openfang.sh/docs/getting-started)
- [GitHub](https://github.com/RightNow-AI/openfang)
- [Discord](https://discord.gg/sSJqgNnq6X)
- [Twitter / X](https://x.com/openfangg)

---

## Built by RightNow

<p align="center">
  <a href="https://www.rightnowai.co/">
    <img src="public/assets/rightnow-logo.webp" width="60" alt="RightNow Logo" />
  </a>
</p>

<p align="center">
  OpenFang is built and maintained by <a href="https://x.com/Akashi203"><strong>Jaber</strong></a>, Founder of <a href="https://www.rightnowai.co/"><strong>RightNow</strong></a>.
</p>

<p align="center">
  <a href="https://www.rightnowai.co/">Website</a> &bull;
  <a href="https://x.com/Akashi203">Twitter / X</a> &bull;
  <a href="https://www.buymeacoffee.com/openfang" target="_blank"><img src="https://cdn.buymeacoffee.com/buttons/v2/default-yellow.png" alt="Buy Me A Coffee" style="height: 60px !important;width: 217px !important;" ></a>
</p>

---

<p align="center">
  <strong>Built with Rust. Secured with 16 layers. Agents that actually work for you.</strong>
</p>
