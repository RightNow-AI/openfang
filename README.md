<p align="center">
  <img src="public/assets/openfang-logo.png" width="160" alt="OpenFang Logo" />
</p>

<h1 align="center">OpenFang</h1>
<h3 align="center">The Agent Operating System</h3>

<p align="center">
  Open-source Agent OS built in Rust. 150K+ LOC. 30 crates. 1,744+ tests. Zero clippy warnings.<br/>
  <strong>One binary. Battle-tested. Agents that actually work for you.</strong>
</p>

<p align="center">
  <a href="https://openfang.sh/docs">Documentation</a> &bull;
  <a href="https://openfang.sh/docs/getting-started">Quick Start</a> &bull;
  <a href="https://x.com/openfangg">Twitter / X</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square" alt="Rust" />
  <img src="https://img.shields.io/badge/license-MIT-blue?style=flat-square" alt="MIT" />
  <img src="https://img.shields.io/badge/version-v0.3.41-green?style=flat-square" alt="v0.3.41" />
  <img src="https://img.shields.io/badge/phase-18%20✅%20complete-blue?style=flat-square" alt="Phase 18 Complete" />
  <img src="https://img.shields.io/badge/tests-1,744%2B%20passing-brightgreen?style=flat-square" alt="Tests" />
  <img src="https://img.shields.io/badge/clippy-0%20warnings-brightgreen?style=flat-square" alt="Clippy" />
  <a href="https://www.buymeacoffee.com/openfang" target="_blank"><img src="https://img.shields.io/badge/Buy%20Me%20a%20Coffee-FFDD00?style=flat-square&logo=buy-me-a-coffee&logoColor=black" alt="Buy Me A Coffee" /></a>
</p>

---

> **v0.3.41 — Phase 18 (SWE Agent Framework) Complete (March 2026)**
>
> Phase 18 implemented a complete Software Engineering (SWE) Agent framework with dashboard visualization, APIs, and A2A (Agent-to-Agent) integration. This release includes critical security hardening: command injection prevention, path traversal protection, CVE updates, and 17 new security tests. See [ROADMAP.md](ROADMAP.md) for full details.

---

## What is OpenFang?

OpenFang is an **open-source Agent Operating System** — not a chatbot framework, not a Python wrapper around an LLM, not a "multi-agent orchestrator." It is a full operating system for autonomous agents, built from scratch in Rust.

Traditional agent frameworks wait for you to type something. OpenFang runs **autonomous agents that work for you** — on schedules, 24/7, building knowledge graphs, monitoring targets, generating leads, managing your social media, writing code, and reporting results to your dashboard.

The entire system compiles to a **single ~38MB binary**. One install, one command, your agents are live.

```bash
curl -fsSL https://openfang.sh/install | sh
openfang init
openfang start
# Dashboard live at http://localhost:4200
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

**Hands** are OpenFang's core innovation — pre-built autonomous capability packages that run independently, on schedules, without you having to prompt them. This is not a chatbot. This is an agent that wakes up at 6 AM, researches your competitors, builds a knowledge graph, scores the findings, writes some code to automate the process, and delivers a report to your dashboard and Telegram before you've had coffee.

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
| **Predictor** | Superforecasting engine. Collects signals from multiple sources, makes predictions with confidence intervals, and tracks its own accuracy using Brier scores. |
| **Researcher** | Deep autonomous researcher. Cross-references multiple sources, evaluates credibility using CRAAP criteria, generates cited reports with APA formatting, supports multiple languages. |
| **Twitter** | Autonomous Twitter/X account manager. Creates content in 7 rotating formats, schedules posts for optimal engagement, responds to mentions, tracks performance metrics. Has an approval queue — nothing posts without your OK. |
| **Browser** | Web automation agent. Navigates sites, fills forms, clicks buttons, handles multi-step workflows. Uses Playwright bridge with session persistence. **Mandatory purchase approval gate** — it will never spend your money without explicit confirmation. |

```bash
# Activate the Researcher Hand — it starts working immediately
openfang hand activate researcher

# Check its progress anytime
openfang hand status researcher

# Activate lead generation on a daily schedule
openfang hand activate lead

# See all available Hands
openfang hand list
```

**Build your own.** Define a `HAND.toml` with tools, settings, and a system prompt. Publish to FangHub.

---

## Software Engineering (SWE) Agent

<p align="center"><em>"Write code while your other assistants write code."</em></p>

**SWE Agent** is OpenFang's specialized software engineering agent — capable of autonomous code generation, debugging, file manipulation, command execution, and system administration. Unlike traditional code assistants that wait for prompts, the SWE Agent processes software engineering tasks scheduled via the API or triggered by the Supervisor Engine.

The SWE Agent features:
- **File Operations** — Read/write files with safety checks and content preview
- **Command Execution** — Execute shell commands with restricted privileges
- **IDE Integration** — Can integrate with development environments
- **Task Chaining** — Multiple file operations and commands chained into coherent workflows
- **Status Tracking** — Full task lifecycle tracking with event streaming

### Dashboard Integration
The new "Software Engineer" tab in the dashboard provides:
- Task queue visualization
- Real-time progress monitoring  
- Output preview with 200-character content previews
- Command execution results with exit codes
- Cancel and retry capabilities

```bash
# Submit a SWE task to the API
curl -X POST http://localhost:4200/api/swe/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Debug deployment script",
    "actions": [
      { "type": "ReadFile", "path": "/deploy.sh" },
      { "type": "ExecuteCommand", "command": "bash -n /deploy.sh" },
      { "type": "WriteFile", "path": "/deploy_debugged.sh", "content": "..." }
    ]
  }'

# Or delegate through the Supervisor (automatic classification)
curl -X POST http://localhost:4200/api/supervisor/delegate \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Implement a function to download and archive files in Rust"
  }'
```

### A2A Integration
The SWE Agent participates in Agent-to-Agent communication via the new A2A Registry system:
- Supervisor engine automatically detects and routes software engineering tasks
- Direct internal handler communication bypassing transport overhead
- Event streaming and progress tracking
- Collaborative task processing between multiple agents

### Evaluation Suite
The SWE Agent includes a comprehensive evaluation framework for testing agent capabilities:
- **Four Difficulty Levels:** Beginner, Intermediate, Advanced, Expert
- **Task Types:** FileRead, FileWrite, CommandExecution, CodeGeneration, BugFix, Refactoring, MultiStep
- **Validation:** Automatic checking of file creation, content patterns, command outputs, and compilation
- **Scoring:** 0.0-1.0 score based on validation checks, with 0.8 pass threshold

```bash
# Run the basic evaluation suite
curl "http://localhost:4200/api/swe/evaluate?suite=basic"

# List available evaluation suites
curl http://localhost:4200/api/swe/evaluate/suites
```

The dashboard includes an evaluation UI for running suites and viewing results with pass/fail counts, scores, and detailed validation output.

## Architecture

30 Rust crates. 150,000+ lines of code. Modular kernel design.

```
openfang-kernel      Orchestration, workflows, metering, RBAC, scheduler, budget tracking
openfang-runtime     Agent loop, 3 LLM drivers, 53+ tools, WASM sandbox, MCP, A2A
openfang-api         150+ REST/WS/SSE endpoints, OpenAI-compatible API, dashboard
openfang-channels    40+ messaging adapters with rate limiting, DM/group policies
openfang-memory       SurrealDB v3 persistence, vector embeddings, knowledge graph
maestro-cache        L1 (Moka) + L2 (Redis) caching layer for memory, models, and skills
maestro-algorithm    The core MAESTRO algorithm: PLAN, EXECUTE, LEARN, EVALUATE  
maestro-observability OpenTelemetry traces, metrics, cost tracking, alerts, audit log
maestro-guardrails   PII scanner, prompt injection detector, topic control, custom regex
maestro-model-hub    Capability-aware model router, 11+ pre-configured models
maestro-knowledge    SurrealDB-backed RAG pipeline, HNSW vector search, chunking
maestro-eval         ScoringEngine, SuiteRunner, RegressionTracker, BenchmarkRunner
maestro-sdk          Rust embedding SDK for OpenFang agents (AgentHandle, SessionHandle)
maestro-marketplace  Local agent marketplace (install, search, publish, update_all)
maestro-pai          Self-evolution engine (hooks, patterns, telos, wisdom)
maestro-rlm          Recursive Language Model (RLM) via PyO3 for long-context processing
maestro-swe          Software Engineering Agent core (file ops, code gen, command exec)
openfang-a2a         Agent-to-agent communication protocols and direct handlers
maestro-integration-tests  Black-box integration test suite (44+ tests)
openfang-types       Core types, taint tracking, Ed25519 manifest signing, model catalog
openfang-skills      60+ bundled skills, SKILL.md parser, FangHub marketplace
openfang-cli         CLI with daemon management, TUI dashboard, MCP server mode
openfang-desktop     Tauri 2.0 native app (system tray, notifications, global shortcuts)
xtask                Build automation
```

---

## OpenAI-Compatible API

Drop-in replacement. Point your existing tools at OpenFang:

```bash
curl -X POST localhost:4200/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "researcher",
    "messages": [{"role": "user", "content": "Analyze Q4 market trends"}],
    "stream": true
  }'
```

150+ REST/WS/SSE endpoints covering agents, memory, workflows, channels, models, skills, A2A, SWE, Hands, mesh networking, and more.

---

## Development

```bash
# Build the workspace
cargo build --workspace --lib

# Run all tests (2,010+)
cargo test --workspace

# Lint (must be 0 warnings)
cargo clippy --workspace --all-targets -- -D warnings

# Format
cargo fmt --all -- --check
```

---

## Stability Notice

OpenFang v0.3.41 is the culmination of Phase 18 implementation, adding a complete Software Engineering Agent framework with critical security hardening. The architecture is solid, the test suite is comprehensive, and the security model is robust. That said:

- **Breaking changes** in the SWE-specific APIs may occur between minor versions until v1.0
- **SWE Agent maturity** varies — file operations are the most battle-tested, command execution requires proper sandboxing
- **Edge cases** with file system operations exist — always test in development first
- **Pin to a specific commit** for production deployments until v1.0

We ship fast and fix fast. The goal is a rock-solid v1.0 by mid-2026.

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
</p>
</pre>
</string>