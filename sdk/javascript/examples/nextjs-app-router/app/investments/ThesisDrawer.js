'use client';

import { HORIZON_LABELS, THESIS_STATUS_LABELS } from './lib/investments-copy';
import { thesisStatusColor } from './lib/investments-ui';

export default function ThesisDrawer({ thesis, onMarkIntact, onMarkWeakened, onMarkBroken, onRequestAction, onClose }) {
  if (!thesis) return null;

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
          width: '100%', maxWidth: 440, height: '100%',
          background: 'var(--background)', borderLeft: '1.5px solid var(--border)',
          overflowY: 'auto', padding: '24px 22px',
          display: 'flex', flexDirection: 'column', gap: 16,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div>
            <span style={{ fontFamily: 'monospace', fontSize: 18, fontWeight: 900, color: 'var(--text)' }}>{thesis.symbol}</span>
            <span style={{ fontSize: 11, color: 'var(--text-dim)', marginLeft: 8 }}>Thesis</span>
          </div>
          <button className="btn btn-ghost btn-xs" onClick={onClose}>✕</button>
        </div>

        {thesis.status && (
          <div>
            <span style={{
              fontSize: 12,
              fontWeight: 700,
              textTransform: 'uppercase',
              color: thesisStatusColor(thesis.status),
            }}>
              {THESIS_STATUS_LABELS[thesis.status] || thesis.status}
            </span>
          </div>
        )}

        <Section title="Main thesis">
          <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{thesis.thesis_text || '—'}</p>
        </Section>

        <Section title="Time horizon">
          <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{HORIZON_LABELS[thesis.horizon] || thesis.horizon || '—'}</p>
        </Section>

        {thesis.entry_zone && (
          <Section title="Entry zone">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{thesis.entry_zone}</p>
          </Section>
        )}

        {thesis.invalidation_rule && (
          <Section title="Invalidation rule">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{thesis.invalidation_rule}</p>
          </Section>
        )}

        {thesis.target_zone && (
          <Section title="Target zone">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{thesis.target_zone}</p>
          </Section>
        )}

        {thesis.confidence != null && (
          <Section title="Confidence">
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <div style={{ flex: 1, height: 4, background: 'var(--border)', borderRadius: 2 }}>
                <div style={{ width: `${Math.min(thesis.confidence * 100, 100)}%`, height: '100%', background: 'var(--accent)', borderRadius: 2 }} />
              </div>
              <span style={{ fontSize: 12, color: 'var(--text-dim)' }}>{(thesis.confidence * 100).toFixed(0)}%</span>
            </div>
          </Section>
        )}

        {thesis.last_review && (
          <Section title="Last reviewed">
            <p style={{ fontSize: 12, color: 'var(--text-dim)' }}>{new Date(thesis.last_review).toLocaleDateString()}</p>
          </Section>
        )}

        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 'auto', paddingTop: 16, borderTop: '1px solid var(--border)' }}>
          <button className="btn btn-success btn-sm" onClick={() => onMarkIntact(thesis.id)}>Intact</button>
          <button className="btn btn-warn btn-sm" onClick={() => onMarkWeakened(thesis.id)}>Weakened</button>
          <button className="btn btn-error btn-sm" onClick={() => onMarkBroken(thesis.id)}>Broken</button>
          <button className="btn btn-ghost btn-sm" onClick={() => onRequestAction(thesis.id)}>Request action</button>
          <button className="btn btn-ghost btn-sm" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

function Section({ title, children }) {
  return (
    <div>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.5, marginBottom: 5 }}>
        {title}
      </div>
      {children}
    </div>
  );
}
