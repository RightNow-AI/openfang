'use client';
import { useState, useEffect } from 'react';

export default function AgentTemplateDetailDrawer({ open, templateId, onClose, onSpawn }) {
  const [tpl, setTpl] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [spawning, setSpawning] = useState(false);

  useEffect(() => {
    if (!open || !templateId) return;
    setTpl(null);
    setError('');
    setLoading(true);
    fetch(`/api/agent-templates/${templateId}`)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then(d => setTpl(d?.template ?? d))
      .catch(e => setError(e?.message || 'Could not load template.'))
      .finally(() => setLoading(false));
  }, [open, templateId]);

  const handleSpawn = async () => {
    setSpawning(true);
    try { await onSpawn?.(templateId); onClose(); } catch {}
    setSpawning(false);
  };

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,.6)', backdropFilter: 'blur(3px)', display: 'flex', justifyContent: 'flex-end' }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div data-cy="agent-template-detail-drawer" style={{ width: 480, background: 'var(--bg-elevated)', borderLeft: '1px solid var(--border)', overflowY: 'auto', padding: '28px 24px', display: 'flex', flexDirection: 'column', gap: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{tpl ? `${tpl.icon ?? '🤖'} ${tpl.name}` : 'Agent template'}</div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 22, color: 'var(--text-dim)', lineHeight: 1 }}>✕</button>
        </div>

        {loading && <div style={{ color: 'var(--text-dim)', fontSize: 14 }}>Loading…</div>}
        {error && <div style={{ color: 'var(--error,#ef4444)', fontSize: 13, padding: '10px 14px', borderRadius: 8, background: 'rgba(239,68,68,.08)', border: '1px solid rgba(239,68,68,.2)' }}>{error}</div>}

        {tpl && (
          <>
            {/* Model pill */}
            {tpl.model && (
              <div style={{ marginBottom: 14 }}>
                <span style={{ fontSize: 11, padding: '3px 10px', borderRadius: 999, background: 'rgba(124,58,237,.15)', color: 'var(--accent)', border: '1px solid rgba(124,58,237,.3)' }}>
                  {tpl.model}
                </span>
              </div>
            )}

            {tpl.description && (
              <div style={{ fontSize: 14, lineHeight: 1.65, color: 'var(--text-secondary,#bbb)', marginBottom: 18 }}>{tpl.description}</div>
            )}

            {/* Bound skills */}
            {tpl.skills?.length > 0 && (
              <Section title="Skills">
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                  {tpl.skills.map(s => (
                    <span key={s} style={{ fontSize: 12, padding: '4px 10px', borderRadius: 6, background: 'rgba(16,185,129,.12)', color: '#10b981', border: '1px solid rgba(16,185,129,.25)' }}>{s}</span>
                  ))}
                </div>
              </Section>
            )}

            {/* Tools */}
            {tpl.tools?.length > 0 && (
              <Section title="Tools">
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                  {tpl.tools.map(t => (
                    <span key={t} style={{ fontSize: 12, padding: '4px 10px', borderRadius: 6, background: 'var(--surface2)', border: '1px solid var(--border)' }}>{t}</span>
                  ))}
                </div>
              </Section>
            )}

            {/* System prompt preview */}
            {tpl.system_prompt && (
              <Section title="System prompt">
                <pre style={{ margin: 0, fontSize: 12, color: 'var(--text-secondary,#bbb)', whiteSpace: 'pre-wrap', background: 'var(--surface2)', padding: '10px 12px', borderRadius: 8, border: '1px solid var(--border)', maxHeight: 140, overflow: 'auto' }}>
                  {tpl.system_prompt.slice(0, 400)}{tpl.system_prompt.length > 400 ? '\n…' : ''}
                </pre>
              </Section>
            )}

            <div style={{ display: 'flex', gap: 10, marginTop: 'auto', paddingTop: 20, borderTop: '1px solid var(--border)', flexWrap: 'wrap' }}>
              <button onClick={handleSpawn} disabled={spawning} style={{ padding: '9px 22px', borderRadius: 8, background: 'var(--accent)', color: '#fff', border: 'none', cursor: spawning ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 14, opacity: spawning ? 0.7 : 1 }}>
                {spawning ? 'Spawning…' : 'Spawn agent'}
              </button>
              <button onClick={onClose} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 13 }}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function Section({ title, children }) {
  return (
    <div style={{ marginBottom: 18 }}>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 8 }}>{title}</div>
      {children}
    </div>
  );
}
