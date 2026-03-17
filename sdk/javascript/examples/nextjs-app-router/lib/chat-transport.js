/**
 * Chat transport adapters.
 *
 * sendViaRun  — creates a run via POST /api/runs and returns the runId.
 *               SSE wiring remains in ChatClient (it is UI-coupled and
 *               involves incremental state updates that don't belong here).
 *
 * sendDirect  — calls POST /api/agents/{id}/chat synchronously.
 *               Accepts an AbortSignal so callers can enforce timeouts.
 *               Returns { reply, latency_ms }.
 */

export async function sendViaRun(sessionId, message) {
  const res = await fetch('/api/runs', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sessionId, message }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
  return data.runId;
}

export async function sendDirect(agentId, message, signal) {
  const res = await fetch(`/api/agents/${encodeURIComponent(agentId)}/chat`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ message }),
    signal,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) throw new Error(data.error || `HTTP ${res.status}`);
  return { reply: data.reply ?? '', latency_ms: data.latency_ms ?? null };
}
