# Auth Matrix

Every endpoint in the OpenFang API must appear in this table. Use this as the authoritative reference when:
- Adding a new route (add a row here as part of the same PR)
- Reviewing auth middleware coverage
- Running the `test:routes` CI job (which reads this file to verify coverage)

**Legend**

| Symbol | Meaning |
|--------|---------|
| — | Not applicable / no auth required (public endpoint) |
| `api-key` | Requires `Authorization: Bearer <api-key>` or configured session token |
| `approval` | Requires server-side approval state `APPROVED` before execution (not merely authenticated) |
| `local-only` | Currently only reachable on loopback; must add `api-key` before exposing to network |
| `rate-limited` | Rate limiting enforced server-side (see [SECURITY.md rate limits](../SECURITY.md#rate-limiting)) |
| `audit` | Action is written to the audit log |

---

## Core / Health

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/health` | — | ✅ 120/min/IP | — | Public. No auth required. |
| GET | `/api/version` | — | ✅ 120/min/IP | — | Public. Returns build version. |
| GET | `/api/status` | `api-key` | ✅ | — | Full runtime status. |

---

## Agents

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/agents` | `api-key` | ✅ | — | List all agents. |
| POST | `/api/agents` | `api-key` | ✅ | ✅ | Spawn agent. Logged. |
| GET | `/api/agents/{id}` | `api-key` | ✅ | — | Agent detail. Caller must own or have access. |
| PUT | `/api/agents/{id}` | `api-key` | ✅ | ✅ | Update agent config. Logged. |
| DELETE | `/api/agents/{id}` | `api-key` | ✅ | ✅ | Kill and delete agent. Logged. |
| POST | `/api/agents/{id}/message` | `api-key` | ✅ 60/min/user | ✅ | Send message; triggers LLM. Logged. |
| GET | `/api/sessions` | `api-key` | ✅ | — | List active sessions. |

---

## Budget

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/budget` | `api-key` | ✅ | — | Global budget status. |
| PUT | `/api/budget` | `api-key` | ✅ | ✅ `approval` | Update budget ceiling. Requires approval. Logged. |
| GET | `/api/budget/agents` | `api-key` | ✅ | — | Per-agent cost ranking. |
| GET | `/api/budget/agents/{id}` | `api-key` | ✅ | — | Single agent budget detail. |
| GET | `/api/usage` | `api-key` | ✅ | — | Usage summary. |

---

## Work / Tasks

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/work` | `api-key` | ✅ | — | List work items. |
| POST | `/api/work` | `api-key` | ✅ | ✅ | Create work item. Logged. |
| GET | `/api/work/{id}` | `api-key` | ✅ | — | Work item detail. Ownership enforced. |
| PUT | `/api/work/{id}` | `api-key` | ✅ | ✅ | Update work item. Logged. |
| DELETE | `/api/work/{id}` | `api-key` | ✅ | ✅ | Delete work item. Logged. |
| GET | `/api/work/summary` | `api-key` | ✅ | — | Aggregate work stats. |
| GET | `/api/approvals` | `api-key` | ✅ | — | Pending approvals list. |
| POST | `/api/approvals/{id}/approve` | `api-key` | ✅ | ✅ `approval` | Grant approval. Logged. |
| POST | `/api/approvals/{id}/reject` | `api-key` | ✅ | ✅ `approval` | Reject approval. Logged. |

---

## Workflows / Scheduler / Orchestrator

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/workflows` | `api-key` | ✅ | — | List workflows. |
| POST | `/api/workflows` | `api-key` | ✅ | ✅ | Create workflow. Logged. |
| POST | `/api/workflows/{id}/trigger` | `api-key` | ✅ | ✅ `approval` | Trigger workflow with external effect. Logged. |
| GET | `/api/orchestrator/status` | `api-key` | ✅ | — | Orchestrator status. |
| GET | `/api/planner/today` | `api-key` | ✅ | — | Today's planned work. |
| POST | `/api/planner/today/rebuild` | `api-key` | ✅ | ✅ | Rebuild daily plan. Logged. |

---

## Skills

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/skills` | `api-key` | ✅ | — | List installed skills. |
| POST | `/api/skills` | `api-key` | ✅ 5/min/user | ✅ `approval` | Install skill. Quarantine validation runs. Requires approval. Logged. |
| GET | `/api/skills/{name}` | `api-key` | ✅ | — | Skill detail. |
| PUT | `/api/skills/{name}` | `api-key` | ✅ | ✅ | Update skill. Logged. |
| DELETE | `/api/skills/{name}` | `api-key` | ✅ | ✅ | Remove skill. Logged. |
| PUT | `/api/skills/{name}/enabled` | `api-key` | ✅ | ✅ | Enable/disable skill. Logged. |

---

## MCP / Providers / Config

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/mcp/servers` | `api-key` | ✅ | — | List MCP servers. |
| GET | `/api/providers` | `api-key` | ✅ | — | List configured LLM providers. |
| POST | `/api/providers/{id}/test` | `api-key` | ✅ 10/min/user | — | Test provider connectivity. |
| GET | `/api/models` | `api-key` | ✅ | — | List available models. |
| GET | `/api/config` | `api-key` | ✅ | — | Read non-sensitive config. Secrets are never returned. |
| PUT | `/api/config` | `api-key` | ✅ | ✅ `approval` | Update config. Logged. |
| GET | `/api/settings/providers/current` | `api-key` | ✅ | — | Active provider setting. |
| PUT | `/api/settings/providers/current` | `api-key` | ✅ | ✅ | Change active provider. Logged. |

---

## Channels / Comms / Integrations

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/channels` | `api-key` | ✅ | — | List channel adapters. |
| GET | `/api/integrations` | `api-key` | ✅ | — | List integrations. |
| GET | `/api/comms/topology` | `api-key` | ✅ | — | Comms topology. |
| GET | `/api/comms/events` | `api-key` | ✅ | — | Recent comms events. |
| POST | `/api/comms/send` | `api-key` | ✅ 30/min/user | ✅ `approval` | Send external message. Requires approval if configured. Logged. |

---

## Network / Peers / A2A

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/network/status` | `api-key` | ✅ | — | OFP network status. |
| GET | `/api/peers` | `api-key` | ✅ | — | Connected peers. |
| GET | `/api/a2a/agents` | `api-key` | ✅ | — | External A2A agents. |
| POST | `/api/a2a/discover` | `api-key` | ✅ | ✅ | Discover external A2A agent. SSRF filter applies. Logged. |
| POST | `/api/a2a/send` | `api-key` | ✅ 30/min/user | ✅ `approval` | Send task to external A2A agent. Requires approval. Logged. |
| GET | `/api/a2a/tasks/{id}/status` | `api-key` | ✅ | — | External task status. Ownership enforced. |

---

## Audit / Logs

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/audit/recent` | `api-key` | ✅ | — | Recent audit log entries. |
| GET | `/api/audit/{id}` | `api-key` | ✅ | — | Single audit entry. |

---

## Hands (Browser Automation)

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/hands` | `api-key` | ✅ | — | List Hands sessions. |
| POST | `/api/hands` | `api-key` `approval` | ✅ | ✅ | Create Hands session. High-risk: tool allowlist enforced. Logged. |
| DELETE | `/api/hands/{id}` | `api-key` | ✅ | ✅ | Terminate session. Logged. |

---

## Finance and Investments

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/finance/summary` | `api-key` | ✅ | — | Finance summary. Read-only. |
| POST | `/api/finance/profile` | `api-key` | ✅ | ✅ | Create finance profile. Logged. |
| PUT | `/api/finance/profile` | `api-key` | ✅ | ✅ | Update finance profile. Logged. |
| GET | `/api/investments/portfolio` | `api-key` | ✅ | — | Portfolio snapshot. Read-only. |
| GET | `/api/investments/watchlist` | `api-key` | ✅ | — | Watchlist. Read-only. |
| GET | `/api/investments/alerts` | `api-key` | ✅ | — | Alerts. Read-only. |
| POST | `/api/investments/order` | `api-key` `approval` | ✅ 10/min/user | ✅ | Place order. **Requires explicit approval.** Logged. |

---

## Command Center / Modes (Agency, Growth, School)

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| POST | `/clients` | `api-key` | ✅ | ✅ | Create client profile. Logged. |
| GET | `/clients/{id}` | `api-key` | ✅ | — | Client detail. Ownership enforced. |
| PUT | `/clients/{id}` | `api-key` | ✅ | ✅ | Update client. Logged. |
| POST | `/wizard/generate-plan` | `api-key` | ✅ | ✅ | Generate task plan via LLM. Logged. |
| GET | `/tasks` | `api-key` | ✅ | — | List tasks for client. |
| POST | `/tasks/{id}/approve` | `api-key` `approval` | ✅ | ✅ | Approve task for run. Logged. |
| POST | `/tasks/{id}/run` | `api-key` `approval` | ✅ | ✅ | Execute task. Requires prior approval. Logged. |
| POST | `/modes/{mode}/records` | `api-key` | ✅ | ✅ | Create mode record. Logged. |
| GET | `/modes/{mode}/records` | `api-key` | ✅ | — | List mode records. |
| GET | `/modes/{mode}/records/{id}` | `api-key` | ✅ | — | Mode record detail. Ownership enforced. |
| POST | `/modes/{mode}/generate-plan` | `api-key` | ✅ | ✅ | Generate mode plan. Logged. |
| POST | `/modes/{mode}/tasks/{id}/run` | `api-key` `approval` | ✅ | ✅ | Execute mode task. Approval required. Logged. |

---

## Creative Projects

| Method | Path | Auth | Rate-Limited | Audit | Notes |
|--------|------|------|-------------|-------|-------|
| GET | `/api/creative-projects` | `api-key` | ✅ | — | List projects. |
| POST | `/api/creative-projects` | `api-key` | ✅ | ✅ | Create project. Logged. |
| GET | `/api/creative-projects/{id}` | `api-key` | ✅ | — | Project detail. Ownership enforced. |
| POST | `/api/creative-projects/{id}/approve` | `api-key` `approval` | ✅ | ✅ | Approve creative output. Logged. |

---

## Next.js BFF Routes

These are handled by the Next.js app and proxy to the Rust backend. Auth is forwarded from the browser session. Rate limiting is applied at the Next.js layer and again at the Rust layer.

| Method | BFF Path | Proxies To | Notes |
|--------|----------|-----------|-------|
| GET | `/api/health` | `/api/health` | Public. |
| GET/POST | `/api/agents` | `/api/agents` | Auth forwarded. |
| POST | `/api/agents/{id}/chat` | `/api/agents/{id}/message` | Auth forwarded. SSE stream. |
| GET | `/api/runs` | `/api/sessions` | Auth forwarded. |
| GET | `/api/skills` | `/api/skills` | Auth forwarded. |
| GET | `/api/onboarding/status` | `/api/health` + LLM probe | ~10s; AbortSignal(10000). |
| GET | `/api/finance/summary` | `/api/finance/summary` | Auth forwarded. |
| POST/PUT | `/api/finance/profile` | `/api/finance/profile` | No GET export. Auth forwarded. |
| GET | `/api/investments/portfolio` | `/api/investments/portfolio` | Auth forwarded. |
| GET | `/api/creative-projects` | `/api/creative-projects` | Fallback to empty list on 404. |
| POST | `/api/creative-projects` | `/api/creative-projects` | Stub on backend 404. Auth forwarded. |

---

## Maintenance

**When adding a new endpoint:**
1. Add a row to this table in the same PR as the route implementation.
2. The `test:routes` CI job will fail if a registered route is missing from this document.
3. If the endpoint requires approval, ensure the approval check is in `docs/auth-matrix.md` AND tested with a negative-path test that attempts to bypass approval.

*Last updated: 2026-03-19*
