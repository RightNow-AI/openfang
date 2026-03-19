'use client';
import { useState, useEffect } from 'react';

export default function HandDetailDrawer({ open, handId, onClose, onConfigure, onTurnOff }) {
  const [hand, setHand] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  useEffect(() => {
    if (!open || !handId) return;
    setHand(null);
    setError('');
    setLoading(true);
    fetch(`/api/hands/${handId}`)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then(d => setHand(d?.hand ?? d))
      .catch(e => setError(e?.message || 'Could not load hand details.'))
      .finally(() => setLoading(false));
  }, [open, handId]);

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,.6)', backdropFilter: 'blur(3px)', display: 'flex', justifyContent: 'flex-end' }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div data-cy="hand-detail-drawer" style={{ width: 460, background: 'var(--bg-elevated,#111)', borderLeft: '1px solid var(--border,#333)', overflowY: 'auto', padding: '28px 24px', display: 'flex', flexDirection: 'column', gap: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{hand ? `${hand.icon ?? '🤝'} ${hand.name}` : 'Hand detail'}</div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 22, color: 'var(--text-dim,#888)', lineHeight: 1 }}>✕</button>
        </div>

        {loading && <div style={{ color: 'var(--text-dim,#888)', fontSize: 14 }}>Loading…</div>}
        {error && <div style={{ color: 'var(--error,#ef4444)', fontSize: 13, padding: '10px 14px', borderRadius: 8, background: 'rgba(239,68,68,.08)', border: '1px solid rgba(239,68,68,.2)' }}>{error}</div>}

        {hand && (
          <>
            {/* Status + category */}
            <div style={{ display: 'flex', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
              <StatusBadge status={hand.status} />
              {hand.category && <span style={{ fontSize: 11, padding: '3px 9px', borderRadius: 999, background: 'rgba(255,255,255,.06)', color: 'var(--text-dim,#888)', border: '1px solid var(--border,#333)' }}>{hand.category}</span>}
            </div>

            {/* Description */}
            {hand.description && (
              <div style={{ fontSize: 14, lineHeight: 1.65, color: 'var(--text-secondary,#bbb)', marginBottom: 18 }}>{hand.description}</div>
            )}

            {/* Tools */}
            {hand.tools?.length > 0 && (
              <Section title="Tools">
                {hand.tools.map(t => (
                  <div key={t} style={{ fontSize: 12, padding: '4px 10px', borderRadius: 6, background: 'var(--surface2,#1a1a2e)', border: '1px solid var(--border,#333)', display: 'inline-block', marginRight: 6, marginBottom: 6 }}>{t}</div>
                ))}
              </Section>
            )}

            {/* Requirements */}
            {hand.requirements?.length > 0 && (
              <Section title="Requirements">
                {hand.requirements.map((r, i) => (
                  <div key={i} style={{ fontSize: 13, color: 'var(--text-dim,#888)', marginBottom: 4 }}>· {r}</div>
                ))}
              </Section>
            )}

            {/* Config values */}
            {hand.config && Object.keys(hand.config).length > 0 && (
              <Section title="Configuration">
                {Object.entries(hand.config).map(([k, v]) => (
                  <div key={k} style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, marginBottom: 5 }}>
                    <span style={{ color: 'var(--text-dim,#888)' }}>{k}</span>
                    <span style={{ color: 'var(--text-primary,#f1f1f1)', fontFamily: 'monospace' }}>{String(v)}</span>
                  </div>
                ))}
              </Section>
            )}

            {/* Actions */}
            <div style={{ display: 'flex', gap: 10, marginTop: 'auto', paddingTop: 20, borderTop: '1px solid var(--border,#333)', flexWrap: 'wrap' }}>
              {hand.status === 'needs_setup' && (
                <button onClick={() => { onConfigure?.(handId); onClose(); }} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 700, fontSize: 14 }}>Configure</button>
              )}
              {hand.status === 'active' && onTurnOff && (
                <button onClick={() => { onTurnOff(handId); onClose(); }} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13 }}>Turn off</button>
              )}
              <button onClick={onClose} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13 }}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function StatusBadge({ status }) {
  const MAP = { active: '#10b981', needs_setup: '#f59e0b', inactive: '#6b7280', error: '#ef4444' };
  const c = MAP[status] ?? '#6b7280';
  return <span style={{ fontSize: 11, padding: '3px 9px', borderRadius: 999, background: `${c}22`, color: c, border: `1px solid ${c}44` }}>{(status ?? '').replace(/_/g, ' ')}</span>;
}

function Section({ title, children }) {
  return (
    <div style={{ marginBottom: 18 }}>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 8 }}>{title}</div>
      {children}
    </div>
  );
}
