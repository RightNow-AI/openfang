import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

/**
 * Normalized quote endpoint.
 * Proxies to backend which handles provider routing.
 * Usage: GET /api/investments/quote?symbol=AAPL&provider=alpha_vantage
 */
export async function GET(request) {
  const { searchParams } = new URL(request.url);
  const symbol = searchParams.get('symbol');
  const provider = searchParams.get('provider') || 'alpha_vantage';

  if (!symbol) {
    return NextResponse.json({ error: 'symbol is required' }, { status: 400 });
  }

  try {
    const data = await api.get(`/api/investments/quote?symbol=${encodeURIComponent(symbol)}&provider=${provider}`);
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json({ symbol, provider, price: null, change_percent_1d: null, _mock: true });
    }
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
