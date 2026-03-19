'use client';

import { fmtCurrency, topRevenueLines } from '../lib/finance-ui';
import { REVENUE_SOURCE_LABELS } from '../lib/finance-copy';

export default function RevenueSummaryCard({ revenueLines, onOpenDetail }) {
  const total = revenueLines.reduce((s, l) => s + (l.amount || 0), 0);
  const top = topRevenueLines(revenueLines, 5);

  return (
    <div className="card" data-cy="revenue-summary-card">
      <div className="card-header">
        <span>Revenue</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontWeight: 700, color: 'var(--success)', fontSize: 15 }}>
          {fmtCurrency(total)}
        </span>
      </div>

      {revenueLines.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '8px 0' }}>
          No revenue lines connected yet.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {top.map((line) => (
            <div key={line.id} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{line.label}</div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                  {REVENUE_SOURCE_LABELS[line.source_type] || line.source_type}
                  {line.source_page && <> · {line.source_page}</>}
                </div>
              </div>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 600, color: 'var(--text)', whiteSpace: 'nowrap' }}>
                {fmtCurrency(line.amount)}
              </span>
            </div>
          ))}
          {revenueLines.length > 5 && (
            <div style={{ fontSize: 12, color: 'var(--text-muted)', paddingTop: 4 }}>
              +{revenueLines.length - 5} more revenue lines
            </div>
          )}
        </div>
      )}

      <button
        className="btn btn-ghost btn-sm"
        onClick={onOpenDetail}
        style={{ marginTop: 14, width: '100%' }}
        data-cy="revenue-detail-btn"
      >
        View all revenue →
      </button>
    </div>
  );
}
