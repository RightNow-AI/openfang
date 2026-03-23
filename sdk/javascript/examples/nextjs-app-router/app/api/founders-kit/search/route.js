import { NextResponse } from 'next/server';
import { searchFoundersKit } from '../../../../lib/founders-kit-search';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(request) {
  const { searchParams } = new URL(request.url);
  const query = String(searchParams.get('q') ?? '').trim();
  const category = String(searchParams.get('category') ?? '').trim() || null;
  const limit = Number.parseInt(String(searchParams.get('limit') ?? '8'), 10);

  if (!query && !category) {
    return NextResponse.json({ error: 'q or category is required' }, { status: 400 });
  }

  const results = searchFoundersKit({
    query,
    category,
    limit: Number.isFinite(limit) ? Math.min(Math.max(limit, 1), 20) : 8,
  });

  return NextResponse.json({ results });
}