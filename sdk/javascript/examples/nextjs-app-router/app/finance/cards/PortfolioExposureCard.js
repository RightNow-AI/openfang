'use client';

import Link from 'next/link';
import { fmtCompact, fmtPctRaw, changeColor } from '../../investments/lib/investments-ui';

export default function PortfolioExposureCard({ summary }) {
  const s = summary || {};
  const hasPnl = s.unrealized_pnl_percent != null;
  const hasValue = s.portfolio_value != null;

  return (
    <div className="card">
      <div className="card-header" style={{ marginBottom: 10 }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>Portfolio Exposure</span>
      </div>

      {!hasValue && !hasPnl ? (
        <div style={{ fontSize: 12, color: 'var(--text-dim)', padding: '8px 0' }}>
          No portfolio data yet. Import positions in Investment Intelligence.
        </div>
      ) : (
        <div style={{ display: 'flex', gap: 16 }}>
          {hasValue && (
            <div>
              <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>Portfolio value</div>
              <div style={{ fontSize: 17, fontWeight: 800, color: 'var(--text)' }}>{fmtCompact(s.portfolio_value)}</div>
            </div>
          )}
          {hasPnl && (
            <div>
              <div style={{ fontSize: 10, color: 'var(--text-dim)', marginBottom: 3 }}>Unrealized P&amp;L</div>
              <div style={{ fontSize: 17, fontWeight: 800, color: changeColor(s.unrealized_pnl_percent) }}>
                {fmtPctRaw(s.unrealized_pnl_percent)}
              </div>
            </div>
          )}
        </div>
      )}

      <div style={{ marginTop: 12, textAlign: 'right' }}>
        <Link
          href="/investments?tab=portfolio"
          style={{ fontSize: 12, color: 'var(--accent)', fontWeight: 600, textDecoration: 'none' }}
        >
          View portfolio →
        </Link>
      </div>
    </div>
  );
}
