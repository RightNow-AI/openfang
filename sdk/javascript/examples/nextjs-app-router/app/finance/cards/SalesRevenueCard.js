'use client';

import { fmtCurrency } from '../lib/finance-ui';
import { REVENUE_SOURCE_LABELS } from '../lib/finance-copy';

const SALES_SOURCES = new Set(['project', 'one_time_sale', 'upsell', 'retainer', 'subscription']);

export default function SalesRevenueCard({ revenueLines, onOpenDetail }) {
  const salesLines = revenueLines.filter(
    (l) => l.source_page === 'sales' || SALES_SOURCES.has(l.source_type),
  );
  const total = salesLines.reduce((s, l) => s + (l.amount || 0), 0);

  return (
    <div className="card" data-cy="sales-revenue-card">
      <div className="card-header">
        <span>Sales Revenue</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontWeight: 700, color: 'var(--success)', fontSize: 15 }}>
          {fmtCurrency(total)}
        </span>
      </div>

      {salesLines.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '6px 0' }}>
          No sales-linked revenue yet. Connect your sales pipeline to track closed deals.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {salesLines.slice(0, 5).map((line) => (
            <div key={line.id} style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{line.label}</div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>
                  {REVENUE_SOURCE_LABELS[line.source_type] || line.source_type}
                </div>
              </div>
              <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 600, color: 'var(--success)', whiteSpace: 'nowrap' }}>
                {fmtCurrency(line.amount)}
              </span>
            </div>
          ))}
        </div>
      )}

      <button
        className="btn btn-ghost btn-sm"
        onClick={onOpenDetail}
        style={{ marginTop: 14, width: '100%' }}
        data-cy="sales-revenue-detail-btn"
      >
        Sales revenue detail →
      </button>
    </div>
  );
}
