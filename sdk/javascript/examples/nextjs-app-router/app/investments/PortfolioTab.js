'use client';

import PositionCard from './cards/PositionCard';
import { fmtPctRaw, changeColor, fmtCompact } from './lib/investments-ui';

export default function PortfolioTab({ positions, onOpenPosition }) {
  const totalValue = (positions || []).reduce((sum, p) => sum + (p.market_value || 0), 0);
  const totalPnl = (positions || []).reduce((sum, p) => sum + (p.unrealized_pnl || 0), 0);
  const pnlPct = totalValue > 0 ? (totalPnl / (totalValue - totalPnl)) * 100 : 0;
  const largest = (positions || []).slice().sort((a, b) => (b.allocation_pct || 0) - (a.allocation_pct || 0))[0];

  return (
    <div>
      {totalValue > 0 && (
        <div className="grid grid-3" style={{ gap: 10, marginBottom: 20 }}>
          {[
            { label: 'Portfolio value', value: fmtCompact(totalValue) },
            { label: 'Unrealized P&L', value: fmtPctRaw(pnlPct), color: changeColor(pnlPct) },
            { label: 'Largest position', value: largest ? `${largest.symbol} ${(largest.allocation_pct || 0).toFixed(1)}%` : '—' },
          ].map((s) => (
            <div key={s.label} className="stat-card" style={{ padding: '12px 14px' }}>
              <div style={{ fontSize: 11, color: 'var(--text-dim)', marginBottom: 4 }}>{s.label}</div>
              <div style={{ fontSize: 16, fontWeight: 800, color: s.color || 'var(--text)' }}>{s.value}</div>
            </div>
          ))}
        </div>
      )}

      {!positions || positions.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '44px 20px', color: 'var(--text-dim)' }}>
          <div style={{ fontSize: 14, fontWeight: 700, marginBottom: 6 }}>No positions tracked</div>
          <p style={{ fontSize: 12 }}>
            Import your portfolio via CSV or connect a data source in Advanced.
          </p>
        </div>
      ) : (
        <div className="grid grid-2" style={{ gap: 12 }}>
          {positions.map((pos) => (
            <PositionCard key={pos.id} position={pos} onOpenPosition={onOpenPosition} />
          ))}
        </div>
      )}
    </div>
  );
}
