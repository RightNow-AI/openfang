/**
 * POST /api/agents/spawn
 *
 * Proxy to daemon POST /api/agents — spawns a new agent from a TOML manifest.
 *
 * Body: { manifest_toml: string }
 * Response: { agent_id, name, status }
 *
 * Server-side validation mirrors client-side rules in lib/spawn-validation.js.
 * Any rule change must be made in that shared module.
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';
import { validateSpawnName } from '../../../../lib/spawn-validation';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Request body must be valid JSON.' }, { status: 400 });
  }

  if (!body?.manifest_toml) {
    return NextResponse.json({ error: 'manifest_toml is required.' }, { status: 400 });
  }

  // Extract and validate the name from the TOML before forwarding.
  // The daemon does its own validation too; this gives the user a fast, clear error.
  const nameMatch = body.manifest_toml.match(/^name\s*=\s*"([^"]*)"/m);
  if (nameMatch) {
    const { error } = validateSpawnName(nameMatch[1]);
    if (error) {
      return NextResponse.json({ error }, { status: 400 });
    }
  }

  try {
    const data = await api.post('/api/agents', { manifest_toml: body.manifest_toml });
    return NextResponse.json(data, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const status = typeof err.status === 'number' ? err.status : 502;
    return NextResponse.json({ error: message }, { status });
  }
}
