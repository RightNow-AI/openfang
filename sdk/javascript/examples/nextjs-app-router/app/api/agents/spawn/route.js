/**
 * POST /api/agents/spawn
 *
 * Proxy to daemon POST /api/agents — spawns a new agent from a TOML manifest.
 *
 * Body: { manifest_toml: string }
 * Response: { agent_id, name, status }
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  try {
    const body = await request.json();
    if (!body?.manifest_toml) {
      return NextResponse.json({ error: 'manifest_toml is required' }, { status: 400 });
    }
    const data = await api.post('/api/agents', { manifest_toml: body.manifest_toml });
    return NextResponse.json(data, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: err.status ?? 502 });
  }
}
