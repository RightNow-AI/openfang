/**
 * PUT /api/skills/[name]/enabled
 *
 * Toggles skill enabled state.
 *
 * Body:   { enabled: boolean }
 * Returns: { name, enabled }
 *
 * Errors:
 *   400 — missing or non-boolean enabled field
 *   404 — skill not found (forwarded from daemon)
 *   502 — daemon unreachable
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';
import { guardDevToken } from '../../../../../lib/dev-token-guard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function PUT(request, { params }) {
  const denied = guardDevToken(request);
  if (denied) return denied;
  const name = params?.name;
  if (!name) {
    return NextResponse.json({ error: 'Skill name is required.' }, { status: 400 });
  }

  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Request body must be valid JSON.' }, { status: 400 });
  }

  if (typeof body?.enabled !== 'boolean') {
    return NextResponse.json(
      { error: '`enabled` is required and must be a boolean.' },
      { status: 400 },
    );
  }

  try {
    const data = await api.put(
      `/api/skills/${encodeURIComponent(name)}/enabled`,
      { enabled: body.enabled },
    );
    // Normalize response — some daemon versions return the full skill object
    const enabled = typeof data?.enabled === 'boolean' ? data.enabled : body.enabled;
    return NextResponse.json({ name, enabled });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const status = typeof err.status === 'number' ? err.status : 502;
    return NextResponse.json({ error: message }, { status });
  }
}
