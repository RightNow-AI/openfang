import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';
type Params = { params: { id: string } };

/** GET /api/creative-projects/[id]/approvals */
export async function GET(_req: NextRequest, { params }: Params) {
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/approvals`, { cache: 'no-store' });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ approvals: [] }, { status: 200 });
  }
}

/** POST /api/creative-projects/[id]/approvals — approve an asset or gate */
export async function POST(req: NextRequest, { params }: Params) {
  const body = await req.json().catch(() => ({}));
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/approvals`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ ok: true }, { status: 200 });
  }
}
