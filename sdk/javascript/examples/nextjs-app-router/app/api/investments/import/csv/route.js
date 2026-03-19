import { NextResponse } from 'next/server';
import { api } from '../../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(request) {
  const formData = await request.formData();
  try {
    const res = await api.postFormData('/api/investments/import/csv', formData);
    return NextResponse.json(res);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json({ ok: true, imported: 0, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
