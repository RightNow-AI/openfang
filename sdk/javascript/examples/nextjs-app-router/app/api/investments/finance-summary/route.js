import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const EMPTY_SUMMARY = {
  watchlist_count: 0,
  high_severity_alerts: 0,
  portfolio_value: null,
  unrealized_pnl_percent: null,
  concentration_risk_flag: false,
  approvals_waiting: 0,
};

export async function GET() {
  try {
    const data = await api.get('/api/investments/finance-summary');
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) return NextResponse.json(EMPTY_SUMMARY);
    return NextResponse.json({ error: err.message || 'Failed' }, { status: 502 });
  }
}
