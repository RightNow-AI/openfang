'use client';

import { fmtCurrency } from '../lib/finance-ui';

export default function ServerApiCostCard({ serverCostMonthly, apiCostMonthly, onOpenDetail }) {
  const total = (serverCostMonthly || 0) + (apiCostMonthly || 0);

  return (
    <div className="card" data-cy="server-api-cost-card">
      <div className="card-header">
        <span>Server &amp; API Costs</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>
          {fmtCurrency(total)} / mo
        </span>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
        {/* Server */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '7px 0', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 16 }}>🖥️</span>
            <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>Server / Hosting</span>
          </div>
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>
            {fmtCurrency(serverCostMonthly || 0)}
          </span>
        </div>

        {/* API */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', padding: '7px 0', borderBottom: '1px solid var(--border)' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: 16 }}>⚡</span>
            <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>AI / API Usage</span>
          </div>
          <span style={{ fontFamily: 'var(--font-mono)', fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>
            {fmtCurrency(apiCostMonthly || 0)}
          </span>
        </div>

        {/* Combined bar */}
        {total > 0 && (
          <div style={{ display: 'flex', gap: 2, height: 8, borderRadius: 4, overflow: 'hidden', marginTop: 4 }}>
            <div
              style={{
                width: `${total > 0 ? ((serverCostMonthly || 0) / total) * 100 : 50}%`,
                background: 'var(--accent)',
                borderRadius: '4px 0 0 4px',
              }}
            />
            <div
              style={{
                flex: 1,
                background: '#3b82f6',
                borderRadius: '0 4px 4px 0',
              }}
            />
          </div>
        )}

        <div style={{ display: 'flex', gap: 16, marginTop: 2 }}>
          <span style={{ fontSize: 11, color: 'var(--text-dim)', display: 'flex', alignItems: 'center', gap: 5 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: 'var(--accent)', display: 'inline-block' }} />
            Server
          </span>
          <span style={{ fontSize: 11, color: 'var(--text-dim)', display: 'flex', alignItems: 'center', gap: 5 }}>
            <span style={{ width: 8, height: 8, borderRadius: 2, background: '#3b82f6', display: 'inline-block' }} />
            API / AI
          </span>
        </div>
      </div>

      <button
        className="btn btn-ghost btn-sm"
        onClick={onOpenDetail}
        style={{ marginTop: 14, width: '100%' }}
        data-cy="server-api-detail-btn"
      >
        View infra cost detail →
      </button>
    </div>
  );
}
