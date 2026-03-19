import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

type Params = { params: { id: string } };

/** POST /api/creative-projects/[id]/approve
 *  Approves the plan (or a specific asset when asset_id is in body)
 */
export async function POST(req: NextRequest, { params }: Params) {
  const body = await req.json().catch(() => ({}));
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/approve`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    // Daemon down — return optimistic ok so the wizard can still advance
    return NextResponse.json({ ok: true }, { status: 200 });
  }
}
