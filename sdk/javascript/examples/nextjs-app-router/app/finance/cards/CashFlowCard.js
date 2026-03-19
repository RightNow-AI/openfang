'use client';

import { fmtCurrency } from '../lib/finance-ui';

function FlowRow({ label, value, color }) {
  return (
    <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '7px 0', borderBottom: '1px solid var(--border)' }}>
      <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>{label}</span>
      <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 700, color: color || 'var(--text)' }}>
        {fmtCurrency(value)}
      </span>
    </div>
  );
}

export default function CashFlowCard({ kpis, onOpenDetail }) {
  const {
    cash_on_hand = 0,
    monthly_revenue = 0,
    monthly_expenses = 0,
    net_profit = 0,
    runway_months = null,
  } = kpis || {};

  const netColor = net_profit >= 0 ? 'var(--success)' : 'var(--error)';
  const runwayColor =
    runway_months == null ? 'var(--text-dim)'
    : runway_months < 2 ? 'var(--error)'
    : runway_months < 4 ? 'var(--warning)'
    : 'var(--success)';

  return (
    <div className="card" data-cy="cash-flow-card">
      <div className="card-header">Cash Flow</div>

      <FlowRow label="Money in (monthly)" value={monthly_revenue} color="var(--success)" />
      <FlowRow label="Money out (monthly)" value={monthly_expenses} color="var(--error)" />
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '10px 0', borderBottom: '1px solid var(--border)' }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>Net</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontSize: 15, fontWeight: 700, color: netColor }}>
          {net_profit >= 0 ? '+' : ''}{fmtCurrency(net_profit)}
        </span>
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '10px 0', borderBottom: '1px solid var(--border)' }}>
        <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>Cash on hand</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>
          {fmtCurrency(cash_on_hand)}
        </span>
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', paddingTop: 10 }}>
        <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>Runway estimate</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 700, color: runwayColor }}>
          {runway_months != null ? `${runway_months} months` : '—'}
        </span>
      </div>

      {onOpenDetail && (
        <button
          className="btn btn-ghost btn-sm"
          onClick={onOpenDetail}
          style={{ marginTop: 14, width: '100%' }}
          data-cy="cash-flow-detail-btn"
        >
          View cash flow detail →
        </button>
      )}
    </div>
  );
}
