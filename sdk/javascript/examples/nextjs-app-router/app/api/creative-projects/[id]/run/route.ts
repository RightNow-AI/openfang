import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

type Params = { params: Promise<{ id: string }> };

/** POST /api/creative-projects/[id]/run — kick off the generation pipeline */
export async function POST(_req: NextRequest, { params }: Params) {
  const { id } = await params;
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${id}/run`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ ok: true, status: 'running' }, { status: 200 });
  }
}
