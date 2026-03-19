import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const EMPTY = { lines: [], total_monthly: 0 };

export async function GET() {
  try {
    const data = await api.get('/api/finance/costs/server');
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json(EMPTY);
    }
    return NextResponse.json({ error: err.message || 'Failed to load server costs' }, { status: 502 });
  }
}
