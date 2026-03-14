export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

import { env } from '../../../../lib/env';

/**
 * GET /api/onboarding/status
 *
 * Returns a quick health snapshot for the onboarding wizard:
 *   daemon   — 'ok' | 'error'
 *   llm      — 'ok' | 'unconfigured' | 'error' | 'idle'
 *   agentCount — number of agents loaded by the daemon
 *   error    — human-readable error string | null
 */
export async function GET() {
  const result = { daemon: 'error', llm: 'idle', agentCount: 0, error: null };

  // ── 1. Check daemon health ────────────────────────────────────────────────
  try {
    const r = await fetch(`${env.OPENFANG_BASE_URL}/api/health`, {
      signal: AbortSignal.timeout(5000),
    });
    if (!r.ok) throw new Error(`Health returned ${r.status}`);
    result.daemon = 'ok';
  } catch (err) {
    result.error = 'Could not reach the app backend. Is it running?';
    return Response.json(result);
  }

  // ── 2. Fetch agent count ──────────────────────────────────────────────────
  let agents = [];
  try {
    const r = await fetch(`${env.OPENFANG_BASE_URL}/api/agents`, {
      signal: AbortSignal.timeout(5000),
    });
    if (r.ok) {
      const data = await r.json();
      agents = Array.isArray(data) ? data : (data?.agents ?? []);
      result.agentCount = agents.length;
    }
  } catch (_) {}

  if (agents.length === 0) {
    result.llm = 'unconfigured';
    result.error = 'No agents loaded. Check that agent files exist in ~/.openfang/agents/.';
    return Response.json(result);
  }

  // ── 3. Quick LLM smoke-test (8 s timeout = key is missing / hanging) ─────
  const testAgent = agents.find((a) => (a.name ?? '').toLowerCase().includes('assistant')) ?? agents[0];
  try {
    const r = await fetch(`${env.OPENFANG_BASE_URL}/api/agents/${testAgent.id}/message`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ message: 'Reply with exactly the word: ok', stream: false }),
      signal: AbortSignal.timeout(10000),
    });

    if (r.ok) {
      const data = await r.json();
      // Any non-empty response field means the LLM is wired up
      if (data.response || data.message || data.content || data.text) {
        result.llm = 'ok';
      } else {
        result.llm = 'unconfigured';
      }
    } else {
      const body = await r.text().catch(() => '');
      const lower = body.toLowerCase();
      if (lower.includes('api_key') || lower.includes('api key') || lower.includes('unauthorized') || lower.includes('auth')) {
        result.llm = 'unconfigured';
        result.error = 'API key is missing or invalid.';
      } else {
        result.llm = 'error';
        result.error = body.slice(0, 200) || `Agent returned ${r.status}`;
      }
    }
  } catch (err) {
    // TimeoutError (10 s) almost always means the LLM is waiting for an API key
    if (err.name === 'TimeoutError' || err.name === 'AbortError') {
      result.llm = 'unconfigured';
      result.error = 'AI timed out — the API key is probably not set up yet.';
    } else {
      result.llm = 'error';
      result.error = err.message;
    }
  }

  return Response.json(result);
}
