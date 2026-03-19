import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';
type Params = { params: { id: string } };

/** GET /api/creative-projects/[id]/references */
export async function GET(_req: NextRequest, { params }: Params) {
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/references`, { cache: 'no-store' });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ references: [] }, { status: 200 });
  }
}

/** POST /api/creative-projects/[id]/references — upload file(s) or add URL */
export async function POST(req: NextRequest, { params }: Params) {
  const contentType = req.headers.get('content-type') ?? '';

  // URL-based reference (JSON body)
  if (contentType.includes('application/json')) {
    const body = await req.json().catch(() => ({}));
    try {
      const res = await fetch(`${BASE}/api/creative-projects/${params.id}/references`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const text = await res.text();
      return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
    } catch {
      // Stub: echo a fake reference when daemon is offline
      const stub = { id: `ref-${Date.now()}`, url: body.url, label: body.url, created_at: new Date().toISOString() };
      return NextResponse.json({ reference: stub }, { status: 200 });
    }
  }

  // File upload (multipart/form-data) — pass through as-is
  try {
    const form = await req.formData();
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/references`, {
      method: 'POST',
      body: form,
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ references: [] }, { status: 200 });
  }
}
