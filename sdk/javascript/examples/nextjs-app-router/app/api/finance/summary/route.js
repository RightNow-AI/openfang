import { NextResponse } from 'next/server';
import { api } from '../../../../lib/api-server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const EMPTY_SUMMARY = {
  profile: null,
  kpis: {
    cash_on_hand: 0,
    monthly_revenue: 0,
    monthly_expenses: 0,
    net_profit: 0,
    runway_months: null,
    overdue_invoices_count: 0,
    average_invoice_age_days: null,
    ad_spend_monthly: 0,
    server_cost_monthly: 0,
    api_cost_monthly: 0,
    margin_percent: null,
  },
  revenue_lines: [],
  expense_lines: [],
  risks: [],
  approvals_waiting: 0,
};

export async function GET() {
  try {
    const data = await api.get('/api/finance/summary');
    return NextResponse.json(data);
  } catch (err) {
    if (err.status === 404 || err.status === 405) {
      return NextResponse.json(EMPTY_SUMMARY);
    }
    return NextResponse.json({ error: err.message || 'Failed to load finance summary' }, { status: 502 });
  }
}
