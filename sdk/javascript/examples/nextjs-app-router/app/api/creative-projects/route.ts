import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

/** GET /api/creative-projects — list all creative projects */
export async function GET() {
  try {
    const res = await fetch(`${BASE}/api/creative-projects`, { cache: 'no-store' });
    if (!res.ok) {
      // Backend doesn't have this endpoint yet — return empty list gracefully
      return NextResponse.json({ items: [] }, { status: 200 });
    }
    const text = await res.text();
    return new NextResponse(text, {
      status: res.status,
      headers: { 'Content-Type': 'application/json' },
    });
  } catch {
    // Daemon unreachable — return empty list so the UI degrades gracefully
    return NextResponse.json({ items: [] }, { status: 200 });
  }
}

/** POST /api/creative-projects — create a new creative project */
export async function POST(req: NextRequest) {
  const body = await req.json().catch(() => ({}));
  const stub = () => NextResponse.json(
    { id: `local-${Date.now()}`, status: 'draft', created_at: new Date().toISOString(), updated_at: new Date().toISOString(), ...body },
    { status: 201 },
  );
  try {
    const res = await fetch(`${BASE}/api/creative-projects`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      // Backend doesn't have this endpoint yet — return a local stub so the wizard
      // can continue without crashing.
      return stub();
    }
    const text = await res.text();
    return new NextResponse(text, {
      status: res.status,
      headers: { 'Content-Type': 'application/json' },
    });
  } catch {
    // If daemon is unavailable, create a client-side stub project so the wizard
    // can continue without crashing.
    return stub();
  }
}
