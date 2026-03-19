import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

type Params = { params: Promise<{ id: string }> };

/** GET /api/creative-projects/[id] */
export async function GET(_req: NextRequest, { params }: Params) {
  const { id } = await params;
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${id}`, { cache: 'no-store' });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ error: 'Daemon unavailable' }, { status: 503 });
  }
}

/** PUT /api/creative-projects/[id] */
export async function PUT(req: NextRequest, { params }: Params) {
  const { id } = await params;
  const body = await req.json().catch(() => ({}));
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${id}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ error: 'Daemon unavailable' }, { status: 503 });
  }
}
