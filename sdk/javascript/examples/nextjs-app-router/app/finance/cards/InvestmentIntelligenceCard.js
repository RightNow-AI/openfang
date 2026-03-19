'use client';

import Link from 'next/link';
import { fmtPctRaw, changeColor, fmtCompact } from '../../investments/lib/investments-ui';

export default function InvestmentIntelligenceCard({ summary, loading }) {
  if (loading) {
    return (
      <div className="card">
        <div className="card-header">
          <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>Investment Intelligence</span>
        </div>
        <div style={{ display: 'flex', justifyContent: 'center', padding: '24px 0' }}>
          <span className="spinner" style={{ width: 16, height: 16 }} />
        </div>
      </div>
    );
  }

  const s = summary || {};

  return (
    <div className="card">
      <div className="card-header" style={{ marginBottom: 12 }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>Investment Intelligence</span>
        <span style={{ fontSize: 11, color: 'var(--text-dim)' }}>
          Market watch, theses &amp; approvals
        </span>
      </div>

      <div className="grid grid-2" style={{ gap: 8, marginBottom: 14 }}>
        <div className="stat-card" style={{ padding: '10px 12px' }}>
          <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>Watching</div>
          <div style={{ fontSize: 17, fontWeight: 800, color: 'var(--text)' }}>{s.watchlist_count ?? '—'}</div>
        </div>
        <div className="stat-card" style={{ padding: '10px 12px' }}>
          <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>High alerts</div>
          <div style={{ fontSize: 17, fontWeight: 800, color: (s.high_severity_alerts > 0) ? '#ef4444' : 'var(--text)' }}>
            {s.high_severity_alerts ?? 0}
          </div>
        </div>
        {s.portfolio_value != null && (
          <div className="stat-card" style={{ padding: '10px 12px' }}>
            <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>Portfolio value</div>
            <div style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)' }}>{fmtCompact(s.portfolio_value)}</div>
          </div>
        )}
        {s.unrealized_pnl_percent != null && (
          <div className="stat-card" style={{ padding: '10px 12px' }}>
            <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>Unrealized P&amp;L</div>
            <div style={{ fontSize: 15, fontWeight: 800, color: changeColor(s.unrealized_pnl_percent) }}>
              {fmtPctRaw(s.unrealized_pnl_percent)}
            </div>
          </div>
        )}
      </div>

      {s.approvals_waiting > 0 && (
        <div style={{
          marginBottom: 12,
          padding: '8px 11px',
          background: 'rgba(255,106,26,0.08)',
          borderRadius: 7,
          fontSize: 12,
          color: 'var(--accent)',
          fontWeight: 600,
        }}>
          {s.approvals_waiting} action{s.approvals_waiting !== 1 ? 's' : ''} waiting for your approval
        </div>
      )}

      {s.concentration_risk_flag && (
        <div style={{
          marginBottom: 12,
          padding: '7px 11px',
          background: 'rgba(239,68,68,0.07)',
          borderRadius: 7,
          fontSize: 11,
          color: '#ef4444',
        }}>
          Concentration risk detected in portfolio
        </div>
      )}

      <div style={{ textAlign: 'right' }}>
        <Link href="/investments" style={{ fontSize: 12, color: 'var(--accent)', fontWeight: 600, textDecoration: 'none' }}>
          Open Investment Intelligence →
        </Link>
      </div>
    </div>
  );
}
