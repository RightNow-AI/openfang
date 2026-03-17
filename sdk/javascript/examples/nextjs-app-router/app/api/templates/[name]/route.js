/**
 * GET /api/templates/[name]
 *
 * Proxy to daemon GET /api/templates/{name} — returns template manifest + raw TOML.
 *
 * Response: { name, manifest, manifest_toml }
 */
import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(request, { params }) {
  try {
    const name = params?.name;
    if (!name) {
      return NextResponse.json({ error: 'Template name is required' }, { status: 400 });
    }
    const data = await api.get(`/api/templates/${encodeURIComponent(name)}`);
    return NextResponse.json(data);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: err.status ?? 502 });
  }
}
