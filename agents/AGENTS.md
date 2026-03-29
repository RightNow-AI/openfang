<!-- Generated: 2026-03-29 -->

# agents

## Purpose

This directory contains 31 pre-configured agent definitions for OpenFang. Each agent is a specialized AI persona with defined capabilities, tools, resource limits, and system instructions. Agents can be spawned to handle specific tasks or collaborate on complex problems.

## Key Files

| File | Description |
|------|-------------|
| `*/agent.toml` | Agent manifest defining name, description, model config, system prompt, tools, and capabilities |
| `*/README.md` | (Optional) Extended documentation for agent behavior and usage |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| analyst | Data analyst. Processes data, generates insights, creates reports. |
| architect | System architect. Designs software architectures, evaluates trade-offs, creates technical specifications. |
| assistant | General-purpose assistant agent. The default OpenClaw agent for everyday tasks, questions, and conversations. |
| code-reviewer | Senior code reviewer. Reviews PRs, identifies issues, suggests improvements with production standards. |
| coder | Expert software engineer. Reads, writes, and analyzes code. |
| customer-support | Customer support agent for ticket handling, issue resolution, and customer communication. |
| data-scientist | Data scientist. Analyzes datasets, builds models, creates visualizations, performs statistical analysis. |
| debugger | Expert debugger. Traces bugs, analyzes stack traces, performs root cause analysis. |
| devops-lead | DevOps lead. Manages CI/CD, infrastructure, deployments, monitoring, and incident response. |
| doc-writer | Technical writer. Creates documentation, README files, API docs, tutorials, and architecture guides. |
| email-assistant | Email triage, drafting, scheduling, and inbox management agent. |
| health-tracker | Wellness tracking agent for health metrics, medication reminders, fitness goals, and lifestyle habits. |
| hello-world | A friendly greeting agent that can read files, search the web, and answer everyday questions. |
| home-automation | Smart home control agent for IoT device management, automation rules, and home monitoring. |
| langchain-code-reviewer | LangChain integration code reviewer. |
| legal-assistant | Legal assistant agent for contract review, legal research, compliance checking, and document drafting. |
| meeting-assistant | Meeting notes, action items, agenda preparation, and follow-up tracking agent. |
| ops | DevOps agent. Monitors systems, runs diagnostics, manages deployments. |
| orchestrator | Meta-agent that decomposes complex tasks, delegates to specialist agents, and synthesizes results. |
| personal-finance | Personal finance agent for budget tracking, expense analysis, savings goals, and financial planning. |
| planner | Project planner. Creates project plans, breaks down epics, estimates effort, identifies risks and dependencies. |
| recruiter | Recruiting agent for resume screening, candidate outreach, job description writing, and hiring pipeline management. |
| researcher | Research agent. Fetches web content and synthesizes information. |
| sales-assistant | Sales assistant agent for CRM updates, outreach drafting, pipeline management, and deal tracking. |
| security-auditor | Security specialist. Reviews code for vulnerabilities, checks configurations, performs threat modeling. |
| social-media | Social media content creation, scheduling, and engagement strategy agent. |
| test-engineer | Quality assurance engineer. Designs test strategies, writes tests, validates correctness. |
| translator | Multi-language translation agent for document translation, localization, and cross-cultural communication. |
| travel-planner | Trip planning agent for itinerary creation, booking research, budget estimation, and travel logistics. |
| tutor | Teaching and explanation agent for learning, tutoring, and educational content creation. |
| writer | Content writer. Creates documentation, articles, and technical writing. |

## For AI Agents

### Working In This Directory

1. **Creating a new agent:** Create a directory with the agent name, then add `agent.toml` with the required structure.
2. **Modifying an agent:** Edit `agent.toml` to adjust tools, model settings, system prompt, or resource limits.
3. **Testing an agent:** Use the OpenFang API to spawn and interact with the agent at `/api/agents`.

### Common Patterns

**agent.toml structure:**

```toml
name = "agent-name"
version = "0.1.0"
description = "One-line description"
author = "openfang"
module = "builtin:chat"
tags = ["tag1", "tag2"]

[model]
provider = "default"          # or "openai", "gemini", "groq"
model = "default"             # provider-specific model ID
api_key_env = "API_KEY_NAME"  # Optional: specify which env var to use
max_tokens = 4096
temperature = 0.5
system_prompt = """Your system instructions here."""

[[fallback_models]]           # Optional: fallback model config
provider = "default"
model = "default"
api_key_env = "GROQ_API_KEY"

[resources]
max_llm_tokens_per_hour = 150000
max_concurrent_tools = 10     # Optional

[capabilities]
tools = ["tool1", "tool2"]    # Available tools: file_read, file_write, file_list, shell_exec, web_search, web_fetch, memory_store, memory_recall
network = ["*"]               # Network access: ["*"] for all, [] for none
memory_read = ["*"]           # Read from all agents' memory, or specific patterns
memory_write = ["self.*"]     # Write scope: self.*, shared.*, etc.
agent_spawn = false           # Can this agent spawn other agents?
shell = ["cargo *", "npm *"]  # Optional: allowed shell commands
```

**Required fields:**
- `name`, `version`, `description`, `module`
- `[model]` section with `provider`, `model`, `max_tokens`, `temperature`, `system_prompt`
- `[capabilities]` section with `tools`, `network`, `memory_read`, `memory_write`

**Optional fields:**
- `author`, `tags`, `api_key_env` (in model)
- `[[fallback_models]]` (for multi-provider setups)
- `[resources]`, `agent_spawn`, `shell` (in capabilities)

<!-- MANUAL: If you add new agents or modify existing ones, keep this document in sync. -->
