# Agent Templates Catalog

OpenFang ships with **30 pre-built agent templates** organized into 4 performance tiers. Each template is a ready-to-spawn `agent.toml` manifest located in the `agents/` directory. Templates cover software engineering, business operations, personal productivity, and everyday tasks.

## Quick Start

Spawn any template from the CLI:

```bash
openfang spawn orchestrator
openfang spawn coder
openfang spawn --template agents/writer/agent.toml
```

Spawn via the REST API:

```bash
# Spawn from a built-in template name
curl -X POST http://localhost:4200/api/agents \
  -H "Content-Type: application/json" \
  -d '{"template": "coder"}'

# Spawn with overrides
curl -X POST http://localhost:4200/api/agents \
  -H "Content-Type: application/json" \
  -d '{"template": "writer", "model": "gemini-2.5-flash"}'
```

Send a message to a running agent:

```bash
curl -X POST http://localhost:4200/api/agents/{id}/message \
  -H "Content-Type: application/json" \
  -d '{"content": "Write unit tests for the auth module"}'
```

---

## Template Tiers

Templates are organized into 4 tiers based on task complexity and the LLM models they use. Higher tiers use more capable (and more expensive) models for tasks that require deep reasoning.

### Tier 1 -- Frontier (DeepSeek)

For tasks requiring the deepest reasoning: multi-agent orchestration, system architecture, and security analysis.

| Template | Provider | Model |
|----------|----------|-------|
| orchestrator | deepseek | deepseek-chat |
| architect | deepseek | deepseek-chat |
| security-auditor | deepseek | deepseek-chat |

All Tier 1 agents fall back to `groq/llama-3.3-70b-versatile` if the DeepSeek API key is unavailable.

### Tier 2 -- Smart (Gemini 2.5 Flash)

For tasks requiring strong analytical and coding abilities: software engineering, data science, research, testing, and legal review.

| Template | Provider | Model |
|----------|----------|-------|
| coder | gemini | gemini-2.5-flash |
| code-reviewer | gemini | gemini-2.5-flash |
| data-scientist | gemini | gemini-2.5-flash |
| debugger | gemini | gemini-2.5-flash |
| researcher | gemini | gemini-2.5-flash |
| analyst | gemini | gemini-2.5-flash |
| test-engineer | gemini | gemini-2.5-flash |
| legal-assistant | gemini | gemini-2.5-flash |

All Tier 2 agents fall back to `groq/llama-3.3-70b-versatile` if the Gemini API key is unavailable.

### Tier 3 -- Balanced (Groq + Gemini Fallback)

For everyday business and productivity tasks: planning, writing, email, customer support, sales, recruiting, and meetings.

| Template | Provider | Model | Fallback |
|----------|----------|-------|----------|
| planner | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| writer | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| doc-writer | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| devops-lead | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| assistant | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| email-assistant | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| social-media | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| customer-support | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| sales-assistant | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| recruiter | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |
| meeting-assistant | groq | llama-3.3-70b-versatile | gemini/gemini-2.0-flash |

### Tier 4 -- Fast (Groq Only)

For lightweight, high-speed tasks: ops monitoring, translation, tutoring, wellness tracking, budgeting, travel, and home automation. No fallback model configured (except `ops` which uses a smaller 8B model for speed).

| Template | Provider | Model |
|----------|----------|-------|
| ops | groq | llama-3.1-8b-instant |
| hello-world | groq | llama-3.3-70b-versatile |
| translator | groq | llama-3.3-70b-versatile |
| tutor | groq | llama-3.3-70b-versatile |
| health-tracker | groq | llama-3.3-70b-versatile |
| personal-finance | groq | llama-3.3-70b-versatile |
| travel-planner | groq | llama-3.3-70b-versatile |
| home-automation | groq | llama-3.3-70b-versatile |

---

## Template Catalog

### orchestrator

**Tier 1 -- Frontier** | `deepseek/deepseek-chat` | Fallback: `groq/llama-3.3-70b-versatile`

> Meta-agent that decomposes complex tasks, delegates to specialist agents, and synthesizes results.

The orchestrator is the command center of the agent fleet. It analyzes user requests, breaks them into subtasks, uses `agent_list` to discover available specialists, delegates work via `agent_send`, spawns new agents when needed, and synthesizes all responses into a coherent final answer. It explains its delegation strategy before executing and avoids delegating trivially simple tasks.

- **Tags**: none
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 500,000/hour
- **Schedule**: Continuous check every 120 seconds
- **Tools**: `agent_send`, `agent_spawn`, `agent_list`, `agent_kill`, `memory_store`, `memory_recall`, `file_read`, `file_write`
- **Capabilities**: `agent_spawn = true`, `agent_message = ["*"]`, `memory_read = ["*"]`, `memory_write = ["*"]`

```bash
openfang spawn orchestrator
# "Plan and execute a full security audit of the codebase"
```

---

### architect

**Tier 1 -- Frontier** | `deepseek/deepseek-chat` | Fallback: `groq/llama-3.3-70b-versatile`

> System architect. Designs software architectures, evaluates trade-offs, creates technical specifications.

Designs systems following principles of separation of concerns, performance-aware design, simplicity over cleverness, and designing for change without over-engineering. Clarifies requirements, identifies key components, defines interfaces and data flow, evaluates trade-offs (latency, throughput, complexity, maintainability), and documents decisions with rationale. Outputs use clear headings, ASCII diagrams, and structured reasoning.

- **Tags**: `architecture`, `design`, `planning`
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Tools**: `file_read`, `file_list`, `memory_store`, `memory_recall`, `agent_send`
- **Capabilities**: `agent_message = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn architect
# "Design a microservices architecture for the payment processing system"
```

---

### security-auditor

**Tier 1 -- Frontier** | `deepseek/deepseek-chat` | Fallback: `groq/llama-3.3-70b-versatile`

> Security specialist. Reviews code for vulnerabilities, checks configurations, performs threat modeling.

Focuses on OWASP Top 10, input validation, auth flaws, cryptographic misuse, injection attacks (SQL, command, XSS, SSTI), insecure deserialization, secrets management, dependency vulnerabilities, race conditions, and privilege escalation. Maps the attack surface, traces data flow from untrusted inputs, checks trust boundaries, reviews error handling, and assesses cryptographic implementations. Reports findings with severity levels (CRITICAL/HIGH/MEDIUM/LOW/INFO) in the format: Finding, Impact, Evidence, Remediation.

- **Tags**: `security`, `audit`, `vulnerability`
- **Temperature**: 0.2
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Schedule**: Proactive on `event:agent_spawned`, `event:agent_terminated`
- **Tools**: `file_read`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`
- **Shell access**: `cargo audit *`, `cargo tree *`, `git log *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn security-auditor
# "Audit the authentication module for vulnerabilities"
```

---

### coder

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Expert software engineer. Reads, writes, and analyzes code.

Writes clean, production-quality code with a step-by-step reasoning approach. Reads files first to understand context, then makes precise changes. Always writes tests for produced code. Supports Rust, Python, JavaScript, and other languages.

- **Tags**: `coding`, `implementation`, `rust`, `python`
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Max concurrent tools**: 10
- **Tools**: `file_read`, `file_write`, `file_list`, `shell_exec`
- **Shell access**: `cargo *`, `rustc *`, `git *`, `npm *`, `python *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*"]`

```bash
openfang spawn coder
# "Implement a rate limiter using the token bucket algorithm in Rust"
```

---

### code-reviewer

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Senior code reviewer. Reviews PRs, identifies issues, suggests improvements with production standards.

Reviews code by priority: correctness, security, performance, maintainability, style. Groups feedback by file with severity tags: `[MUST FIX]`, `[SHOULD FIX]`, `[NIT]`, `[PRAISE]`. Explains WHY, not just WHAT. Suggests specific code for proposed changes. Acknowledges good code, avoids bikeshedding on style when formatters exist.

- **Tags**: `review`, `code-quality`, `best-practices`
- **Temperature**: 0.3
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`
- **Shell access**: `cargo clippy *`, `cargo fmt *`, `git diff *`, `git log *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn code-reviewer
# "Review the changes in the last 3 commits for production readiness"
```

---

### data-scientist

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Data scientist. Analyzes datasets, builds models, creates visualizations, performs statistical analysis.

Follows a structured methodology: understand the question, explore data (shape, distributions, missing values), analyze with appropriate statistical methods, build predictive models when needed, and communicate findings clearly. Toolkit includes descriptive stats, hypothesis testing (t-test, chi-squared, ANOVA), correlation/regression, time series, clustering, dimensionality reduction, and A/B test design.

- **Tags**: none
- **Temperature**: 0.3
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`
- **Shell access**: `python *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn data-scientist
# "Analyze this CSV dataset and identify the top 3 factors correlated with churn"
```

---

### debugger

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Expert debugger. Traces bugs, analyzes stack traces, performs root cause analysis.

Follows a strict methodology: reproduce, isolate (binary search through code/data), identify root cause (not just symptoms), fix (minimal correct fix), verify (regression tests). Looks for common patterns: off-by-one, null/None, race conditions, resource leaks. Checks error handling paths and recent changes. Presents findings as Bug Report, Root Cause, Fix, Prevention.

- **Tags**: none
- **Temperature**: 0.2
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`
- **Shell access**: `cargo *`, `git log *`, `git diff *`, `git show *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn debugger
# "The API returns 500 on POST /api/agents when the name contains unicode -- find the root cause"
```

---

### researcher

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Research agent. Fetches web content and synthesizes information.

Fetches web pages, reads documents, and synthesizes findings into clear, structured reports. Always cites sources, separates facts from analysis, and flags uncertainty. Breaks research tasks into sub-questions and investigates each systematically.

- **Tags**: `research`, `analysis`, `web`
- **Temperature**: 0.5
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `web_fetch`, `file_read`, `file_write`, `file_list`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn researcher
# "Research the current state of WebAssembly component model and summarize the key proposals"
```

---

### analyst

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Data analyst. Processes data, generates insights, creates reports.

Analyzes data, finds patterns, generates insights, and creates structured reports. Shows methodology, uses numbers and evidence to support conclusions. Reads files first to understand data structure, then presents findings with summary, key metrics, detailed analysis, and recommendations.

- **Tags**: none
- **Temperature**: 0.4
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`, `shell_exec`
- **Shell access**: `python *`, `cargo *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn analyst
# "Analyze the server access logs and report traffic patterns by hour and endpoint"
```

---

### test-engineer

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Quality assurance engineer. Designs test strategies, writes tests, validates correctness.

Tests document behavior, not implementation. Prefers fast, deterministic tests. Designs unit tests, integration tests, property-based tests, edge case tests, and regression tests. Follows the Arrange-Act-Assert pattern with descriptive test names (`test_X_when_Y_should_Z`). Reviews test coverage to identify untested paths and missing edge cases.

- **Tags**: `testing`, `qa`, `validation`
- **Temperature**: 0.3
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`
- **Shell access**: `cargo test *`, `cargo check *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn test-engineer
# "Write comprehensive tests for the rate limiter module covering edge cases"
```

---

### legal-assistant

**Tier 2 -- Smart** | `gemini/gemini-2.5-flash` | Fallback: `groq/llama-3.3-70b-versatile`

> Legal assistant for contract review, legal research, compliance checking, and document drafting.

Systematically reviews contracts covering parties, termination provisions, payment terms, indemnification, IP provisions, confidentiality, governing law, and force majeure. Drafts NDAs, service agreements, terms of service, privacy policies, and employment agreements. Checks compliance against GDPR, SOC 2, HIPAA, PCI DSS, CCPA/CPRA, ADA, and OSHA. Always includes a disclaimer that output does not constitute legal advice.

- **Tags**: `legal`, `contracts`, `compliance`, `research`, `review`, `documents`
- **Temperature**: 0.2
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn legal-assistant
# "Review this NDA and flag any one-sided or problematic clauses"
```

---

### planner

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Project planner. Creates project plans, breaks down epics, estimates effort, identifies risks and dependencies.

Follows a structured methodology: scope (in/out), decompose (epics to stories to tasks), sequence (dependencies and critical path), estimate (S/M/L/XL with rationale), risk (technical and schedule), milestones (with acceptance criteria). Estimates ranges (best/likely/worst), tackles riskiest parts first, and builds in 20-30% buffer for unknowns.

- **Tags**: none
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Tools**: `file_read`, `file_list`, `memory_store`, `memory_recall`, `agent_send`
- **Capabilities**: `agent_message = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn planner
# "Create a project plan for migrating our monolith to microservices over 6 months"
```

---

### writer

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Content writer. Creates documentation, articles, and technical writing.

Excels at documentation, technical writing, blog posts, and clear communication. Writes concisely with active voice, structures content with headers and bullet points. Reads existing files for context and writes output to files when asked.

- **Tags**: none
- **Temperature**: 0.7
- **Max tokens**: 4096
- **Token quota**: 100,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*"]`

```bash
openfang spawn writer
# "Write a blog post about the benefits of agent-based architectures"
```

---

### doc-writer

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Technical writer. Creates documentation, README files, API docs, tutorials, and architecture guides.

Writes for the reader: starts with WHY, then WHAT, then HOW. Uses progressive disclosure (overview to details). Creates READMEs, API docs, architecture docs, tutorials, reference docs, and Architecture Decision Records (ADRs). Uses active voice, short sentences, and includes code examples for every non-trivial concept.

- **Tags**: none
- **Temperature**: 0.4
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn doc-writer
# "Write API documentation for all the /api/agents endpoints"
```

---

### devops-lead

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> DevOps lead. Manages CI/CD, infrastructure, deployments, monitoring, and incident response.

Covers CI/CD pipeline design, container orchestration (Docker, Kubernetes), Infrastructure as Code (Terraform, Pulumi), monitoring and observability (Prometheus, Grafana, OpenTelemetry), incident response, security hardening, and capacity planning. Designs pipelines with fast feedback loops, immutable artifacts, and automated rollback.

- **Tags**: none
- **Temperature**: 0.2
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Tools**: `file_read`, `file_write`, `file_list`, `shell_exec`, `memory_store`, `memory_recall`, `agent_send`
- **Shell access**: `docker *`, `git *`, `cargo *`, `kubectl *`
- **Capabilities**: `agent_message = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn devops-lead
# "Design a CI/CD pipeline for our Rust workspace with staging and production environments"
```

---

### assistant

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> General-purpose assistant. The default OpenFang agent for everyday tasks, questions, and conversations.

The versatile default agent covering conversational intelligence, task execution, research and synthesis, writing and communication, problem solving, agent delegation (routes specialized tasks to the right specialist), knowledge management, and creative brainstorming. Acts as the user's trusted first point of contact -- handles most tasks directly and delegates to specialists when they would do better.

- **Tags**: `general`, `assistant`, `default`, `multipurpose`, `conversation`, `productivity`
- **Temperature**: 0.5
- **Max tokens**: 8192
- **Token quota**: 300,000/hour
- **Max concurrent tools**: 10
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`, `shell_exec`, `agent_send`, `agent_list`
- **Shell access**: `python *`, `cargo *`, `git *`, `npm *`
- **Capabilities**: `network = ["*"]`, `agent_message = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn assistant
# "Help me plan my week and draft replies to these three emails"
```

---

### email-assistant

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Email triage, drafting, scheduling, and inbox management agent.

Rapidly triages incoming email by urgency, category, and required action. Drafts professional emails adapted to recipient and situation. Manages email-based scheduling and follow-up obligations. Recognizes recurring email patterns and generates reusable templates. Produces concise digests for long threads and high-volume inboxes.

- **Tags**: `email`, `communication`, `triage`, `drafting`, `scheduling`, `productivity`
- **Temperature**: 0.4
- **Max tokens**: 8192
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn email-assistant
# "Triage these 15 emails and draft responses for the urgent ones"
```

---

### social-media

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Social media content creation, scheduling, and engagement strategy agent.

Crafts platform-optimized content for Twitter/X, LinkedIn, Instagram, Facebook, TikTok, Reddit, Mastodon, Bluesky, and Threads. Plans content calendars, designs engagement strategies, analyzes engagement data, defines brand voice guidelines, and optimizes hashtags and SEO. Adapts tone from professional thought leadership to casual and punchy depending on platform.

- **Tags**: `social-media`, `content`, `marketing`, `engagement`, `scheduling`, `analytics`
- **Temperature**: 0.7
- **Max tokens**: 4096
- **Token quota**: 120,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn social-media
# "Create a week of LinkedIn posts about our open-source launch"
```

---

### customer-support

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Customer support agent for ticket handling, issue resolution, and customer communication.

Triages support tickets by category, severity, product area, and customer tier. Follows systematic troubleshooting workflows for issue diagnosis. Writes empathetic, solution-oriented customer responses. Manages knowledge base content and escalation handoffs. Monitors customer sentiment and generates support metrics summaries.

- **Tags**: `support`, `customer-service`, `tickets`, `helpdesk`, `communication`, `resolution`
- **Temperature**: 0.3
- **Max tokens**: 4096
- **Token quota**: 200,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn customer-support
# "Triage this batch of support tickets and draft responses for the top 5 urgent ones"
```

---

### sales-assistant

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Sales assistant for CRM updates, outreach drafting, pipeline management, and deal tracking.

Drafts personalized cold outreach emails using the AIDA framework. Manages CRM data with structured updates. Analyzes sales pipelines with weighted values, at-risk deals, and conversion rates. Prepares pre-call briefs with prospect research. Builds competitive battle cards and performs win/loss analysis.

- **Tags**: `sales`, `crm`, `outreach`, `pipeline`, `prospecting`, `deals`
- **Temperature**: 0.5
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn sales-assistant
# "Draft a 3-touch outreach sequence for CTOs at mid-market SaaS companies"
```

---

### recruiter

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Recruiting agent for resume screening, candidate outreach, job description writing, and hiring pipeline management.

Evaluates resumes against job requirements with structured match scoring. Writes inclusive, searchable job descriptions. Drafts personalized candidate outreach sequences. Prepares structured interview guides with STAR-format behavioral questions. Tracks candidates through hiring pipeline stages and generates reports. Actively supports inclusive hiring practices.

- **Tags**: `recruiting`, `hiring`, `resume`, `outreach`, `talent`, `hr`
- **Temperature**: 0.4
- **Max tokens**: 4096
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn recruiter
# "Screen these 10 resumes against the senior backend engineer job requirements"
```

---

### meeting-assistant

**Tier 3 -- Balanced** | `groq/llama-3.3-70b-versatile` | Fallback: `gemini/gemini-2.0-flash`

> Meeting notes, action items, agenda preparation, and follow-up tracking agent.

Creates structured, time-boxed agendas. Transforms raw meeting notes or transcripts into clean, structured minutes with executive summaries, key discussion points, decisions, and action items. Extracts every commitment with owner, deadline, and priority. Drafts follow-up emails and schedules reminders. Synthesizes across multiple related meetings to identify themes and gaps.

- **Tags**: `meetings`, `notes`, `action-items`, `agenda`, `follow-up`, `productivity`
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn meeting-assistant
# "Process this meeting transcript and extract all action items with owners and deadlines"
```

---

### ops

**Tier 4 -- Fast** | `groq/llama-3.1-8b-instant` | No fallback

> DevOps agent. Monitors systems, runs diagnostics, manages deployments.

Monitors system health, runs diagnostics, and helps with deployments. Precise and cautious -- explains what a command does before running it. Prefers read-only operations unless explicitly asked to make changes. Reports in structured format: status, details, recommended action. Uses the smallest model in the fleet (8B) for maximum speed on routine ops checks.

- **Tags**: none
- **Temperature**: 0.2
- **Max tokens**: 2048
- **Token quota**: 50,000/hour
- **Schedule**: Periodic every 5 minutes
- **Tools**: `shell_exec`, `file_read`, `file_list`
- **Shell access**: `docker *`, `git *`, `cargo *`, `systemctl *`, `ps *`, `df *`, `free *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*"]`

```bash
openfang spawn ops
# "Check disk usage, memory, and running containers"
```

---

### hello-world

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> A friendly greeting agent that can read files and fetch web pages.

The simplest agent template -- a minimal starter agent with basic read-only capabilities. No system prompt, no tags, no shell access. Useful as a starting point for custom agents or for testing that the agent system is working.

- **Tags**: none
- **Temperature**: default
- **Max tokens**: default
- **Token quota**: 100,000/hour
- **Tools**: `file_read`, `file_list`, `web_fetch`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*"]`, `agent_spawn = false`

```bash
openfang spawn hello-world
# "Hello! What can you do?"
```

---

### translator

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Multi-language translation agent for document translation, localization, and cross-cultural communication.

Translates between 20+ major languages with high fidelity to meaning, tone, and intent. Handles contextual and cultural adaptation, document format preservation, software localization (JSON, YAML, PO/POT, XLIFF), technical/specialized translation, translation quality assurance (back-translation, consistency checks), and glossary management. Flags ambiguous phrases with multiple translation options.

- **Tags**: `translation`, `languages`, `localization`, `multilingual`, `communication`, `i18n`
- **Temperature**: 0.3
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn translator
# "Translate this README from English to Japanese and Spanish, preserving code blocks"
```

---

### tutor

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Teaching and explanation agent for learning, tutoring, and educational content creation.

Explains concepts at the learner's level using the Feynman Technique. Uses Socratic questioning to guide discovery. Teaches across mathematics, computer science, natural sciences, humanities, social sciences, and professional skills. Walks through problems step-by-step showing reasoning, not just solutions. Creates structured learning plans with spaced repetition. Provides practice questions with detailed, constructive feedback.

- **Tags**: `education`, `teaching`, `tutoring`, `learning`, `explanation`, `knowledge`
- **Temperature**: 0.5
- **Max tokens**: 8192
- **Token quota**: 200,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `shell_exec`, `web_fetch`
- **Shell access**: `python *`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn tutor
# "Teach me how binary search trees work, starting from the basics"
```

---

### health-tracker

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Wellness tracking agent for health metrics, medication reminders, fitness goals, and lifestyle habits.

Tracks weight, blood pressure, heart rate, sleep, water intake, steps, mood, and custom metrics. Manages medication schedules with dosage, timing, and refill dates. Sets SMART fitness goals with progressive training plans. Logs meals and estimates nutritional content. Applies evidence-based habit formation principles. Generates periodic wellness reports. Always includes a disclaimer that it is not a medical professional.

- **Tags**: `health`, `wellness`, `fitness`, `medication`, `habits`, `tracking`
- **Temperature**: 0.3
- **Max tokens**: 4096
- **Token quota**: 100,000/hour
- **Max concurrent tools**: 5
- **Schedule**: Periodic every 1 hour
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*"]`

```bash
openfang spawn health-tracker
# "Log today's metrics: weight 175lbs, sleep 7.5 hours, mood 8/10, 8000 steps"
```

---

### personal-finance

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Personal finance agent for budget tracking, expense analysis, savings goals, and financial planning.

Creates detailed budgets using frameworks like 50/30/20, zero-based budgeting, and envelope method. Processes expense data in any format (CSV, manual lists) and categorizes transactions. Defines and tracks savings goals with projected timelines. Analyzes debt portfolios and models avalanche vs. snowball payoff strategies. Produces financial health reports with net worth, debt-to-income ratio, and savings rate. Always disclaims that output is not financial advice.

- **Tags**: `finance`, `budget`, `expenses`, `savings`, `planning`, `money`
- **Temperature**: 0.2
- **Max tokens**: 8192
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `shell_exec`
- **Shell access**: `python *`
- **Capabilities**: `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn personal-finance
# "Analyze this month's expense CSV and show me where I'm over budget"
```

---

### travel-planner

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Trip planning agent for itinerary creation, booking research, budget estimation, and travel logistics.

Builds day-by-day itineraries with estimated times, transportation, meal recommendations, and contingency plans. Provides comprehensive destination guides covering best times to visit, attractions, customs, safety, cuisine, and visa requirements. Creates detailed travel budgets at multiple price tiers. Recommends accommodations by type, neighborhood, and budget. Plans transportation logistics including flights, trains, and local transit. Generates customized packing lists.

- **Tags**: `travel`, `planning`, `itinerary`, `booking`, `logistics`, `vacation`
- **Temperature**: 0.5
- **Max tokens**: 8192
- **Token quota**: 150,000/hour
- **Max concurrent tools**: 5
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `web_fetch`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn travel-planner
# "Plan a 10-day trip to Japan for 2 people, mid-range budget, mix of culture and food"
```

---

### home-automation

**Tier 4 -- Fast** | `groq/llama-3.3-70b-versatile` | No fallback

> Smart home control agent for IoT device management, automation rules, and home monitoring.

Manages smart home devices (lights, thermostats, security, appliances, sensors). Designs automation workflows using event-condition-action patterns. Configures multi-device scenes for common scenarios (morning routine, movie night, bedtime, away mode). Monitors energy consumption and recommends optimizations. Configures home security workflows. Troubleshoots IoT connectivity and bridges different ecosystems (Home Assistant, HomeKit, SmartThings). Understands Matter/Thread protocol adoption.

- **Tags**: `smart-home`, `iot`, `automation`, `devices`, `monitoring`, `home`
- **Temperature**: 0.2
- **Max tokens**: 4096
- **Token quota**: 100,000/hour
- **Max concurrent tools**: 10
- **Tools**: `file_read`, `file_write`, `file_list`, `memory_store`, `memory_recall`, `shell_exec`, `web_fetch`
- **Shell access**: `curl *`, `python *`, `ping *`
- **Capabilities**: `network = ["*"]`, `memory_read = ["*"]`, `memory_write = ["self.*", "shared.*"]`

```bash
openfang spawn home-automation
# "Create a bedtime automation: lock doors, arm cameras, dim lights, set thermostat to 68F"
```

---

## Custom Templates

The `agents/custom/` directory is reserved for your own agent templates. Create a new `agent.toml` file following the manifest format below.

### Manifest Format

```toml
# Required fields
name = "my-agent"
version = "0.1.0"
description = "What this agent does in one sentence."
author = "your-name"
module = "builtin:chat"

# Optional metadata
tags = ["tag1", "tag2"]

# Model configuration (required)
[model]
provider = "gemini"                  # Provider: gemini, deepseek, groq, openai, anthropic, etc.
model = "gemini-2.5-flash"           # Model identifier
api_key_env = "GEMINI_API_KEY"       # Env var holding the API key
max_tokens = 4096                    # Max output tokens per response
temperature = 0.3                    # Creativity (0.0 = deterministic, 1.0 = creative)
system_prompt = """Your agent's personality, capabilities, and instructions go here.
Be specific about what the agent should and should not do."""

# Optional fallback model (used when primary is unavailable)
[[fallback_models]]
provider = "groq"
model = "llama-3.3-70b-versatile"
api_key_env = "GROQ_API_KEY"

# Optional schedule (for autonomous/background agents)
[schedule]
periodic = { cron = "every 5m" }                                     # Periodic execution
# continuous = { check_interval_secs = 120 }                         # Continuous loop
# proactive = { conditions = ["event:agent_spawned"] }               # Event-triggered

# Resource limits
[resources]
max_llm_tokens_per_hour = 150000    # Token budget per hour
max_concurrent_tools = 5            # Max parallel tool executions

# Capability grants (principle of least privilege)
[capabilities]
tools = ["file_read", "file_write", "file_list", "shell_exec",
         "memory_store", "memory_recall", "web_fetch",
         "agent_send", "agent_list", "agent_spawn", "agent_kill"]
         # Or use wildcards: "mcp_filesystem_*", "mcp_*" for MCP tools
network = ["*"]                     # Network access patterns
memory_read = ["*"]                 # Memory namespaces agent can read
memory_write = ["self.*"]           # Memory namespaces agent can write
agent_spawn = true                  # Can this agent spawn other agents?
agent_message = ["*"]               # Which agents can it message?
shell = ["python *", "cargo *"]     # Allowed shell command patterns (whitelist)
```

### Available Tools

| Tool | Description |
|------|-------------|
| `file_read` | Read file contents |
| `file_write` | Write/create files |
| `file_list` | List directory contents |
| `shell_exec` | Execute shell commands (restricted by `shell` whitelist) |
| `memory_store` | Persist key-value data to memory |
| `memory_recall` | Retrieve data from memory |
| `web_fetch` | Fetch content from URLs (SSRF-protected) |
| `agent_send` | Send a message to another agent |
| `agent_list` | List all running agents |
| `agent_spawn` | Spawn a new agent |
| `agent_kill` | Terminate a running agent |

### Tips for Custom Agents

1. **Start minimal**. Grant only the tools and capabilities the agent actually needs. You can always add more later.
2. **Write a clear system prompt**. The system prompt is the most important part of the template. Be specific about the agent's role, methodology, output format, and limitations.
3. **Set appropriate temperature**. Use 0.2 for precise/analytical tasks, 0.5 for balanced tasks, 0.7+ for creative tasks.
4. **Use shell whitelists**. Never grant `shell = ["*"]`. Whitelist specific command patterns like `shell = ["python *", "cargo test *"]`.
5. **Set token budgets**. Use `max_llm_tokens_per_hour` to prevent runaway costs. Start with 100,000 and adjust based on usage.
6. **Add fallback models**. If your primary model has rate limits or availability issues, add a `[[fallback_models]]` entry.
7. **Use memory for continuity**. Grant `memory_store` and `memory_recall` so the agent can persist context across sessions.

---

## Spawning Agents

### CLI

```bash
# Spawn by template name
openfang spawn coder

# Spawn with a custom name
openfang spawn coder --name "backend-coder"

# Spawn from a TOML file path
openfang spawn --template agents/custom/my-agent.toml

# List running agents
openfang agents

# Send a message
openfang message <agent-id> "Write a function to parse TOML files"

# Kill an agent
openfang kill <agent-id>
```

### REST API

```bash
# Spawn from template
POST /api/agents
{"template": "coder"}

# Spawn with overrides
POST /api/agents
{"template": "coder", "name": "backend-coder", "model": "deepseek-chat"}

# Send message
POST /api/agents/{id}/message
{"content": "Implement the auth module"}

# WebSocket (streaming)
WS /api/agents/{id}/ws

# List agents
GET /api/agents

# Delete agent
DELETE /api/agents/{id}
```

### OpenAI-Compatible API

```bash
# Use any agent through the OpenAI-compatible endpoint
POST /v1/chat/completions
{
  "model": "openfang:coder",
  "messages": [{"role": "user", "content": "Write a Rust HTTP server"}],
  "stream": true
}

# List available models
GET /v1/models
```

### Orchestrator Delegation

The orchestrator agent can spawn and delegate to any other agent programmatically:

```
User: "Build a REST API with tests and documentation"

Orchestrator:
1. agent_send(coder, "Implement the REST API endpoints")
2. agent_send(test-engineer, "Write integration tests for these endpoints")
3. agent_send(doc-writer, "Document the API endpoints")
4. Synthesize all results into a final report
```

---

## Environment Variables

Set the following API keys to enable the corresponding model providers:

| Variable | Provider | Used By |
|----------|----------|---------|
| `DEEPSEEK_API_KEY` | DeepSeek | Tier 1 (orchestrator, architect, security-auditor) |
| `GEMINI_API_KEY` | Google Gemini | Tier 2 primary, Tier 3 fallback |
| `GROQ_API_KEY` | Groq | Tier 3 primary, Tier 1/2 fallback, Tier 4 |

At minimum, set `GROQ_API_KEY` to enable all Tier 3 and Tier 4 agents. Add `GEMINI_API_KEY` for Tier 2 agents. Add `DEEPSEEK_API_KEY` for Tier 1 frontier agents.
