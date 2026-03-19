'use client';

import Link from 'next/link';

export default function MarketRiskCard({ summary }) {
  const s = summary || {};
  const hasRisk = s.concentration_risk_flag || s.high_severity_alerts > 0;

  return (
    <div className="card">
      <div className="card-header" style={{ marginBottom: 10 }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>Market Risk</span>
      </div>

      {!hasRisk ? (
        <div style={{ fontSize: 12, color: 'var(--text-dim)', padding: '8px 0' }}>
          No elevated risk signals at this time.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {s.concentration_risk_flag && (
            <div style={{
              padding: '8px 11px',
              background: 'rgba(239,68,68,0.07)',
              borderRadius: 7,
              fontSize: 12,
              color: '#ef4444',
            }}>
              ⚠ Concentration risk — your portfolio may be over-exposed to a single position or sector.
            </div>
          )}
          {s.high_severity_alerts > 0 && (
            <div style={{
              padding: '8px 11px',
              background: 'rgba(239,68,68,0.07)',
              borderRadius: 7,
              fontSize: 12,
              color: '#ef4444',
            }}>
              {s.high_severity_alerts} high-severity alert{s.high_severity_alerts !== 1 ? 's' : ''} on your watchlist.
            </div>
          )}
        </div>
      )}

      <div style={{ marginTop: 12, textAlign: 'right' }}>
        <Link
          href="/investments?tab=alerts"
          style={{ fontSize: 12, color: 'var(--accent)', fontWeight: 600, textDecoration: 'none' }}
        >
          View alerts →
        </Link>
      </div>
    </div>
  );
}
