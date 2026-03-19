import { NextRequest, NextResponse } from 'next/server';

const BASE = process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051';

type Params = { params: Promise<{ id: string }> };

/** GET /api/creative-projects/[id]/results */
export async function GET(req: NextRequest, { params }: Params) {
  const { id } = await params;
  const format = req.nextUrl.searchParams.get('format');
  const url = format
    ? `${BASE}/api/creative-projects/${id}/results?format=${format}`
    : `${BASE}/api/creative-projects/${id}/results`;
  try {
    const res = await fetch(url, { cache: 'no-store' });
    const ct = res.headers.get('Content-Type') ?? 'application/json';
    const buf = await res.arrayBuffer();
    return new NextResponse(buf, {
      status: res.status,
      headers: { 'Content-Type': ct },
    });
  } catch {
    return NextResponse.json({ assets: [], status: 'pending' }, { status: 200 });
  }
}
