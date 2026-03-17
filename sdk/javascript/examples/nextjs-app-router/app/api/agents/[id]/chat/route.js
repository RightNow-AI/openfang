/**
 * POST /api/agents/[id]/chat
 *
 * Direct synchronous chat with a specific agent — bypasses alive routing.
 * Used by the direct-agent chat mode launched from the Agent Catalog spawn flow.
 *
 * Body:    { message: string }
 * Response: { reply, agentId, latency_ms }
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request, { params }) {
  try {
    const agentId = params?.id;
    if (!agentId) {
      return NextResponse.json({ error: 'Agent ID is required' }, { status: 400 });
    }

    const body = await request.json();
    const message = String(body?.message ?? '').trim();
    if (!message) {
      return NextResponse.json({ error: 'message is required' }, { status: 400 });
    }

    const data = await api.post(`/api/agents/${agentId}/message`, { message });

    return NextResponse.json({
      reply: data.response ?? data.reply ?? '',
      agentId,
      latency_ms: data.latency_ms ?? 0,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: err.status ?? 502 });
  }
}
