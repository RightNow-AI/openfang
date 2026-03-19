'use client';

import { fmtCurrency, fmtPercent } from './lib/finance-ui';

function Kpi({ label, value, sub, valueColor, cy }) {
  return (
    <div
      className="stat-card"
      style={{ flex: '1 1 130px', minWidth: 110 }}
      data-cy={cy}
    >
      <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 4 }}>{label}</div>
      <div style={{ fontSize: 18, fontWeight: 800, color: valueColor || 'var(--text)', lineHeight: 1.15 }}>{value}</div>
      {sub && <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 3 }}>{sub}</div>}
    </div>
  );
}

export default function FinanceKpiStrip({ summary }) {
  const kpis = summary?.kpis || {};
  const revenueTotal = (summary?.revenue_lines ?? []).reduce((s, l) => s + (l.amount ?? 0), 0);
  const expenseTotal = (summary?.expense_lines ?? []).reduce((s, l) => s + (l.amount ?? 0), 0);
  const netProfit = revenueTotal - expenseTotal;
  const margin = revenueTotal > 0 ? netProfit / revenueTotal : null;
  const cashOnHand = kpis.cash_on_hand ?? 0;
  const monthlyBurn = kpis.monthly_burn ?? expenseTotal;
  const runway = monthlyBurn > 0 ? Math.floor(cashOnHand / monthlyBurn) : null;

  return (
    <div
      style={{ display: 'flex', flexWrap: 'wrap', gap: 10 }}
      data-cy="finance-kpi-strip"
    >
      <Kpi
        label="Cash on hand"
        value={fmtCurrency(cashOnHand, true)}
        sub={runway != null ? `${runway}mo runway` : null}
        cy="kpi-cash"
      />
      <Kpi
        label="Monthly revenue"
        value={fmtCurrency(revenueTotal, true)}
        valueColor="var(--color-success, #22c55e)"
        cy="kpi-revenue"
      />
      <Kpi
        label="Monthly expenses"
        value={fmtCurrency(expenseTotal, true)}
        valueColor="var(--color-error, #ef4444)"
        cy="kpi-expenses"
      />
      <Kpi
        label="Net profit"
        value={fmtCurrency(netProfit, true)}
        valueColor={netProfit >= 0 ? 'var(--color-success, #22c55e)' : 'var(--color-error, #ef4444)'}
        cy="kpi-net"
      />
      {margin != null && (
        <Kpi
          label="Margin"
          value={fmtPercent(margin)}
          valueColor={margin >= 0.2 ? 'var(--color-success, #22c55e)' : margin >= 0 ? 'var(--text-warn, #e5a00d)' : 'var(--color-error, #ef4444)'}
          cy="kpi-margin"
        />
      )}
      {(summary?.approvals_waiting ?? 0) > 0 && (
        <Kpi
          label="Needs approval"
          value={String(summary.approvals_waiting)}
          valueColor="var(--text-warn, #e5a00d)"
          sub="action required"
          cy="kpi-approvals"
        />
      )}
    </div>
  );
}
