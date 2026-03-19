import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(_request, { params }) {
  const { id } = params;
  try {
    const data = await api.get(`/api/finance/items/${id}`);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404) {
      return NextResponse.json({ id, detail: null, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed to load detail' }, { status: err.status || 502 });
  }
}
