import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET() {
  try {
    const data = await api.get('/api/investments/alerts');
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) return NextResponse.json([]);
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
