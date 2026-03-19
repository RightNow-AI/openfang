'use client';

import { WATCHLIST_STATUS_LABELS, ASSET_CLASS_LABELS } from './lib/investments-copy';
import { fmtPrice, fmtPctRaw, changeColor, thesisStatusColor } from './lib/investments-ui';

export default function InvestmentDetailDrawer({ item, onUpdateThesis, onArchive, onClose }) {
  if (!item) return null;

  return (
    <div
      style={{
        position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.4)', zIndex: 8000,
        display: 'flex', justifyContent: 'flex-end',
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        style={{
          width: '100%', maxWidth: 420, height: '100%',
          background: 'var(--background)', borderLeft: '1.5px solid var(--border)',
          overflowY: 'auto', padding: '24px 22px',
          display: 'flex', flexDirection: 'column', gap: 18,
        }}
      >
        {/* Close */}
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div>
            <span style={{ fontFamily: 'monospace', fontSize: 20, fontWeight: 900, color: 'var(--text)' }}>{item.symbol}</span>
            {item.asset_class && (
              <span className="badge badge-dim" style={{ fontSize: 10, marginLeft: 8 }}>
                {ASSET_CLASS_LABELS[item.asset_class] || item.asset_class}
              </span>
            )}
          </div>
          <button className="btn btn-ghost btn-xs" onClick={onClose}>✕</button>
        </div>

        {/* Status + price */}
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span className="badge badge-dim" style={{ fontSize: 11 }}>
            {WATCHLIST_STATUS_LABELS[item.status] || item.status}
          </span>
          {item.current_price && (
            <span style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>{fmtPrice(item.current_price)}</span>
          )}
          {item.change_percent_1d != null && (
            <span style={{ fontSize: 12, color: changeColor(item.change_percent_1d) }}>
              {fmtPctRaw(item.change_percent_1d)}
            </span>
          )}
        </div>

        <Section title="Why we care">
          <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{item.thesis_excerpt || 'No thesis yet. Add one to give context.'}</p>
        </Section>

        <Section title="Current status">
          <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{item.status_note || '—'}</p>
        </Section>

        <Section title="Risk level">
          <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{item.risk_label || item.risk_comfort || '—'}</p>
        </Section>

        {item.next_catalyst_label && (
          <Section title="Next catalyst">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>
              {item.next_catalyst_label}
              {item.next_catalyst_date ? <span style={{ marginLeft: 8, opacity: 0.6 }}>{item.next_catalyst_date}</span> : null}
            </p>
          </Section>
        )}

        {item.thesis_status && (
          <Section title="Thesis summary">
            <div style={{ fontSize: 12, fontWeight: 700, color: thesisStatusColor(item.thesis_status), marginBottom: 4, textTransform: 'uppercase' }}>
              {item.thesis_status.replace(/_/g, ' ')}
            </div>
            {item.thesis_full && <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{item.thesis_full}</p>}
          </Section>
        )}

        {item.recent_updates && item.recent_updates.length > 0 && (
          <Section title="Recent updates">
            <ul style={{ fontSize: 12, color: 'var(--text-dim)', paddingLeft: 18, margin: 0 }}>
              {item.recent_updates.map((u, i) => <li key={i} style={{ marginBottom: 4 }}>{u}</li>)}
            </ul>
          </Section>
        )}

        {/* Actions */}
        <div style={{ display: 'flex', gap: 8, marginTop: 'auto', paddingTop: 16, borderTop: '1px solid var(--border)' }}>
          <button className="btn btn-primary btn-sm" onClick={() => onUpdateThesis(item.id)}>
            Update thesis
          </button>
          <button className="btn btn-ghost btn-sm" onClick={() => onArchive(item.id)}>
            Archive
          </button>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function Section({ title, children }) {
  return (
    <div>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.5, marginBottom: 6 }}>
        {title}
      </div>
      {children}
    </div>
  );
}
