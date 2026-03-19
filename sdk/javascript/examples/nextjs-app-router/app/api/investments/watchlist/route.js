import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  try {
    const data = await api.get('/api/investments/watchlist');
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) return NextResponse.json([]);
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}

export async function POST(request) {
  try {
    const body = await request.json();
    const data = await api.post('/api/investments/watchlist', body);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) return NextResponse.json({ ok: true, _mock: true });
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
