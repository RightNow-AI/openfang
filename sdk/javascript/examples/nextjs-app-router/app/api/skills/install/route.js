/**
 * POST /api/skills/install
 *
 * Install a skill from the ClaWhub registry into the local skill inventory.
 * Installing a skill DOES NOT automatically enable it, attach it to any agent,
 * or affect runtime usage.
 *
 * Request body: { name: string, source?: string }
 *
 * Success response: { name, installed: true, enabled, bundled, version }
 *
 * Errors:
 *   400 — missing/invalid name, or attempted install of a bundled skill
 *   404 — skill not found in registry
 *   409 — skill is already installed
 *   502 — daemon unreachable
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';
import { buildLocalSets } from '../../../../lib/skill-registry';
import { guardDevToken } from '../../../../lib/dev-token-guard';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  const denied = guardDevToken(request);
  if (denied) return denied;
  let body;
  try {
    body = await request.json();
  } catch {
    return NextResponse.json({ error: 'Request body must be valid JSON.' }, { status: 400 });
  }

  const name = typeof body?.name === 'string' ? body.name.trim() : '';
  if (!name) {
    return NextResponse.json({ error: 'Field "name" is required.' }, { status: 400 });
  }

  const source = typeof body?.source === 'string' ? body.source.trim() : undefined;

  // --- Guard: bundled and already-installed checks ---
  try {
    const localSkills = await api.get('/api/skills');
    const skills = Array.isArray(localSkills)
      ? localSkills
      : localSkills?.skills ?? [];

    const { installed, bundled } = buildLocalSets(skills);

    if (bundled.has(name)) {
      return NextResponse.json(
        { error: `"${name}" is a bundled skill and cannot be re-installed.` },
        { status: 400 },
      );
    }

    if (installed.has(name)) {
      return NextResponse.json(
        { error: `"${name}" is already installed.` },
        { status: 409 },
      );
    }
  } catch (localErr) {
    // If we can't reach the daemon to check local state, let the upstream
    // daemon install call be the authority (it will return 409 if duplicate).
    if (localErr?.status >= 500 || localErr == null || !(localErr?.status < 500)) {
      // swallow; fall through to install call
    }
  }

  // --- Forward install request to daemon ---
  try {
    const payload = source ? { name, source } : { name };
    const res = await api.post('/api/skills/install', payload);

    return NextResponse.json({
      name,
      installed: true,
      enabled: res?.enabled ?? false,
      bundled: false,
      version: res?.version ?? '',
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const status = typeof err?.status === 'number' ? err.status : 502;

    if (status === 404) {
      return NextResponse.json({ error: `Skill "${name}" was not found in the registry.` }, { status: 404 });
    }
    if (status === 409) {
      return NextResponse.json({ error: `"${name}" is already installed.` }, { status: 409 });
    }
    if (status === 400) {
      return NextResponse.json({ error: message || 'Invalid install request.' }, { status: 400 });
    }

    return NextResponse.json({ error: message }, { status: status >= 400 ? 502 : 502 });
  }
}
