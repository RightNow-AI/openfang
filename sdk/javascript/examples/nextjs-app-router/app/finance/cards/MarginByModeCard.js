'use client';

import { fmtCurrency, mapRevenueLinesByMode } from '../lib/finance-ui';

const MODE_LABELS = { agency: 'Agency', growth: 'Growth', school: 'School', other: 'Other' };
const MODE_COLORS = {
  agency: 'var(--accent)',
  growth: 'var(--success)',
  school: '#3b82f6',
  other: 'var(--text-dim)',
};

export default function MarginByModeCard({ summary, onOpenDetail }) {
  const revenueLines = summary?.revenue_lines || [];
  const expenseLines = summary?.expense_lines || [];
  const kpis = summary?.kpis || {};

  const byMode = mapRevenueLinesByMode(revenueLines);
  const totalRevenue = Object.values(byMode).reduce((s, v) => s + v, 0);
  const totalExpenses = expenseLines.reduce((s, l) => s + (l.amount || 0), 0);
  const margin = totalRevenue > 0 ? ((totalRevenue - totalExpenses) / totalRevenue) * 100 : null;

  const modes = Object.entries(byMode).filter(([, v]) => v > 0);

  return (
    <div className="card" data-cy="margin-by-mode-card">
      <div className="card-header">
        <span>Margin by Business Mode</span>
        {margin != null && (
          <span
            style={{
              fontFamily: 'var(--font-mono)',
              fontSize: 13,
              fontWeight: 700,
              color: margin >= 20 ? 'var(--success)' : margin >= 0 ? 'var(--warning)' : 'var(--error)',
            }}
          >
            {margin.toFixed(1)}% overall
          </span>
        )}
      </div>

      {modes.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '6px 0' }}>
          No mode-tagged revenue yet.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {modes.map(([mode, amount]) => {
            const pct = totalRevenue > 0 ? (amount / totalRevenue) * 100 : 0;
            return (
              <div key={mode}>
                <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 4 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: MODE_COLORS[mode] || 'var(--text)' }}>
                    {MODE_LABELS[mode] || mode}
                  </span>
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text)' }}>
                    {fmtCurrency(amount)}
                  </span>
                </div>
                <div style={{ height: 5, borderRadius: 3, background: 'var(--border)' }}>
                  <div
                    style={{
                      height: '100%',
                      width: `${Math.min(100, pct)}%`,
                      background: MODE_COLORS[mode] || 'var(--accent)',
                      borderRadius: 3,
                    }}
                  />
                </div>
              </div>
            );
          })}
        </div>
      )}

      {kpis.margin_percent != null && (
        <div style={{ marginTop: 12, paddingTop: 12, borderTop: '1px solid var(--border)', fontSize: 12, color: 'var(--text-dim)' }}>
          Reported margin: <strong style={{ color: 'var(--text)', fontFamily: 'var(--font-mono)' }}>{kpis.margin_percent.toFixed(1)}%</strong>
        </div>
      )}

      {onOpenDetail && (
        <button
          className="btn btn-ghost btn-sm"
          onClick={onOpenDetail}
          style={{ marginTop: 14, width: '100%' }}
        >
          Margin detail →
        </button>
      )}
    </div>
  );
}
