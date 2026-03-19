import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  let body;
  try { body = await request.json(); } catch { body = {}; }
  try {
    const data = await api.post('/api/investments/setup', body);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) return NextResponse.json({ ok: true, _mock: true });
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
