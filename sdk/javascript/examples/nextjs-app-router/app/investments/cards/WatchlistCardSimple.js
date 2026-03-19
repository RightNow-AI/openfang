'use client';

import { WATCHLIST_STATUS_LABELS, ASSET_CLASS_LABELS } from '../lib/investments-copy';
import { fmtPrice, fmtPctRaw, changeColor } from '../lib/investments-ui';

const STATUS_BADGE = {
  watching: 'badge-dim',
  candidate: 'badge-success',
  hold: 'badge-warn',
  reduce: 'badge-error',
  archived: 'badge-dim',
};

export default function WatchlistCardSimple({ item, onOpenDetail }) {
  return (
    <div
      className="card"
      style={{ cursor: 'pointer' }}
      onClick={() => onOpenDetail(item.id)}
      data-cy={`watchlist-simple-${item.id}`}
    >
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 10 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 7, marginBottom: 3 }}>
            <span style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)', fontFamily: 'monospace' }}>{item.symbol}</span>
            <span className={`badge ${STATUS_BADGE[item.status] || 'badge-dim'}`} style={{ fontSize: 10 }}>
              {WATCHLIST_STATUS_LABELS[item.status] || item.status}
            </span>
            {item.asset_class && (
              <span className="badge badge-dim" style={{ fontSize: 10 }}>
                {ASSET_CLASS_LABELS[item.asset_class] || item.asset_class}
              </span>
            )}
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', lineClamp: 2 }}>{item.name}</div>
        </div>
        <div style={{ textAlign: 'right', flexShrink: 0 }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>{fmtPrice(item.current_price)}</div>
          {item.change_percent_1d != null && (
            <div style={{ fontSize: 11, fontWeight: 600, color: changeColor(item.change_percent_1d) }}>
              {fmtPctRaw(item.change_percent_1d)}
            </div>
          )}
        </div>
      </div>
      {item.next_catalyst_label && (
        <div style={{ marginTop: 8, fontSize: 11, color: 'var(--text-dim)' }}>
          <span style={{ fontWeight: 600 }}>Next: </span>{item.next_catalyst_label}
        </div>
      )}
    </div>
  );
}
