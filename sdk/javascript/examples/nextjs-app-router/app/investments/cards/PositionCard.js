'use client';

import { fmtPrice, fmtPctRaw, changeColor, thesisStatusColor } from '../lib/investments-ui';

export default function PositionCard({ position, onOpenPosition }) {
  const alloc = position.allocation_pct ?? 0;
  const pnlColor = changeColor(position.unrealized_pnl_pct);

  return (
    <div
      className="card"
      style={{ cursor: 'pointer' }}
      onClick={() => onOpenPosition(position.id)}
      data-cy={`position-${position.id}`}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 8 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
            <span style={{ fontFamily: 'monospace', fontWeight: 800, fontSize: 15, color: 'var(--text)' }}>
              {position.symbol}
            </span>
            {position.thesis_status && (
              <span style={{
                fontSize: 9,
                fontWeight: 700,
                textTransform: 'uppercase',
                letterSpacing: 0.5,
                color: thesisStatusColor(position.thesis_status),
                border: `1px solid ${thesisStatusColor(position.thesis_status)}`,
                borderRadius: 4,
                padding: '1px 5px',
              }}>
                {position.thesis_status.replace(/_/g, ' ')}
              </span>
            )}
          </div>
          <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>{position.name}</div>
        </div>
        <div style={{ textAlign: 'right', flexShrink: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)' }}>{alloc.toFixed(1)}%</div>
          <div style={{ fontSize: 10, color: 'var(--text-dim)' }}>of portfolio</div>
        </div>
      </div>

      {/* Allocation bar */}
      <div style={{ marginTop: 8, height: 4, background: 'var(--border)', borderRadius: 2 }}>
        <div style={{
          height: '100%',
          borderRadius: 2,
          width: `${Math.min(alloc, 100)}%`,
          background: alloc > 20 ? '#ef4444' : alloc > 10 ? '#f59e0b' : 'var(--accent)',
        }} />
      </div>

      <div style={{ display: 'flex', gap: 16, marginTop: 10, fontSize: 12 }}>
        <div>
          <div style={{ color: 'var(--text-dim)', marginBottom: 2 }}>Avg cost</div>
          <div style={{ fontWeight: 700, color: 'var(--text)' }}>{fmtPrice(position.avg_cost)}</div>
        </div>
        <div>
          <div style={{ color: 'var(--text-dim)', marginBottom: 2 }}>Current</div>
          <div style={{ fontWeight: 700, color: 'var(--text)' }}>{fmtPrice(position.current_price)}</div>
        </div>
        {position.unrealized_pnl_pct != null && (
          <div>
            <div style={{ color: 'var(--text-dim)', marginBottom: 2 }}>Unrealized</div>
            <div style={{ fontWeight: 700, color: pnlColor }}>{fmtPctRaw(position.unrealized_pnl_pct)}</div>
          </div>
        )}
      </div>
    </div>
  );
}
