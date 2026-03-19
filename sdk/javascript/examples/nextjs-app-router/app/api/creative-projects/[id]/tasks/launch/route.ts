import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';
type Params = { params: Promise<{ id: string }> };

/** POST /api/creative-projects/[id]/tasks/launch */
export async function POST(req: NextRequest, { params }: Params) {
  const { id } = await params;
  const body = await req.json().catch(() => ({}));
  try {
    const res = await fetch(`${BASE}/api/creative-projects/${id}/tasks/launch`, {
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
      text: `[Offline] Task "${body.task_type ?? 'unknown'}" queued — daemon is not running.`,
      created_at: new Date().toISOString(),
    };
    return NextResponse.json({ message: stub }, { status: 200 });
  }
}
