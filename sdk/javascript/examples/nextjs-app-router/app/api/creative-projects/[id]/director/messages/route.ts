import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';
type Params = { params: { id: string } };

/** GET /api/creative-projects/[id]/director/messages */
export async function GET(_req: NextRequest, { params }: Params) {
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/director/messages`, { cache: 'no-store' });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    return NextResponse.json({ messages: [] }, { status: 200 });
  }
}

/** POST /api/creative-projects/[id]/director/messages */
export async function POST(req: NextRequest, { params }: Params) {
  const body = await req.json().catch(() => ({}));
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${params.id}/director/messages`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    const text = await res.text();
    return new NextResponse(text, { status: res.status, headers: { 'Content-Type': 'application/json' } });
  } catch {
    const stub = {
      id: `stub-${Date.now()}`,
      role: 'director',
      text: `[Offline] The Creative Director is not available right now. Your message has been noted.`,
      created_at: new Date().toISOString(),
    };
    return NextResponse.json({ message: stub }, { status: 200 });
  }
}
