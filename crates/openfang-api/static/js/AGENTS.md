<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# Dashboard JavaScript

## Purpose
Client-side logic for the Alpine.js dashboard SPA. Each page module manages its own data, API calls, and interactivity (agents, chat, workflows, logs, settings, etc.).

## Key Files
| File | Purpose |
|------|---------|
| `api.js` | HTTP client wrapper, WebSocket manager, auth injection, toast notifications |
| `katex.js` | Math rendering library |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `pages/` | Page-specific controllers and logic |

## Pages (in `pages/` subdirectory)
| File | Purpose |
|------|---------|
| `agents.js` | Agent listing, creation, editing, deletion, deletion |
| `chat.js` | Chat interface: message input, streaming responses, message history |
| `overview.js` | Dashboard overview: agent count, status, recent activity |
| `hands.js` | Hand requests: approval UI for user-confirmation actions |
| `channels.js` | Channel integrations: Telegram, Discord, Slack setup and status |
| `comms.js` | Communications history and management |
| `logs.js` | Real-time logs: agent execution, errors, debugging |
| `scheduler.js` | Scheduled tasks: cron jobs, event triggers |
| `skills.js` | Available skills: list, enable/disable, documentation |
| `sessions.js` | Agent sessions: view, resume, export conversation |
| `settings.js` | Global settings: API keys, model selection, budget limits |
| `usage.js` | Token usage, cost tracking, per-agent spending |
| `approvals.js` | Approval workflows: prompt user for dangerous actions |
| `runtime.js` | Runtime status: kernel health, provider status |
| `workflow-builder.js` | Workflow designer: drag-drop agent graph, trigger config |
| `workflows.js` | Workflow listing and execution |
| `wizard.js` | Setup wizard: onboarding, initial config |

## For AI Agents
When modifying dashboard pages:
- Each page is self-contained: data fetching, Alpine.js store updates, event handlers
- API calls use `api.js` wrapper: handles auth, WebSocket fallback, error toasts
- Page layout goes in `index_body.html` main content area (Alpine.js `x-show`)
- Data flows: API response → Alpine.js store → template binding → DOM update
- WebSocket streams real-time updates: override HTTP polling with WS for latency-sensitive data
- Always check `index_body.html` to understand page structure before writing JS
- Test in dev: `openfang start`, navigate to `http://127.0.0.1:4200/`, check browser console for errors
