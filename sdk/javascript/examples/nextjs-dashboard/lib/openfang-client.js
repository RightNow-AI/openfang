// Named exports only — safe in Server Components and Client Components.
// All async functions return { data, error } and never throw.

export function getBaseUrl() {
  return process.env.NEXT_PUBLIC_OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051'
}

async function apiFetch(path, init = {}) {
  try {
    const res = await fetch(`${getBaseUrl()}${path}`, {
      cache: 'no-store',
      ...init,
      headers: {
        'Content-Type': 'application/json',
        ...(init.headers ?? {}),
      },
    })
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      return { data: null, error: `HTTP ${res.status}${text ? ': ' + text : ''}` }
    }
    const data = await res.json()
    return { data, error: null }
  } catch (err) {
    return { data: null, error: err?.message ?? 'Network error' }
  }
}

/** GET /api/health → { status: "ok"|"degraded", version } */
export async function fetchHealth() {
  return apiFetch('/api/health')
}

/** GET /api/health/detail → { status, version, uptime_seconds, agent_count, database, ... } */
export async function fetchHealthDetail() {
  return apiFetch('/api/health/detail')
}

/** GET /api/agents → [{ id, name, state, ready, model_provider, model_name, ... }] */
export async function listAgents() {
  return apiFetch('/api/agents')
}

/** GET /api/agents/:id */
export async function getAgent(id) {
  return apiFetch(`/api/agents/${encodeURIComponent(id)}`)
}

/**
 * GET /api/agents/:id/session
 * Returns { messages: [{role: "User"|"Assistant", content, tools?, images?}] }
 */
export async function getAgentSession(id) {
  return apiFetch(`/api/agents/${encodeURIComponent(id)}/session`)
}

/** POST /api/agents/:id/message → { response, input_tokens, output_tokens, iterations, cost_usd } */
export async function sendMessage(agentId, text) {
  return apiFetch(`/api/agents/${encodeURIComponent(agentId)}/message`, {
    method: 'POST',
    body: JSON.stringify({ message: text }),
  })
}

/** GET /api/channels → { channels: [{name, display_name, icon, configured, ...}], total, configured_count } */
export async function listChannels() {
  return apiFetch('/api/channels')
}
