import { NextResponse } from 'next/server';
import { api } from '../../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(_request, { params }) {
  const { id } = params;
  try {
    const data = await api.post(`/api/finance/actions/${id}/approve`, {});
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json({ ok: true, approved: id, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed to approve action' }, { status: err.status || 502 });
  }
}
