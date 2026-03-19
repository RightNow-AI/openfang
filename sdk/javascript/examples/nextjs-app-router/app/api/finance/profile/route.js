import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  try {
    const body = await request.json();
    const data = await api.post('/api/finance/profile', body);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json({ ok: true, profile: null, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed to create finance profile' }, { status: err.status || 502 });
  }
}

export async function PUT(request) {
  try {
    const body = await request.json();
    const data = await api.put('/api/finance/profile', body);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json({ ok: true, profile: null, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed to update finance profile' }, { status: err.status || 502 });
  }
}
