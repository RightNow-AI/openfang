'use client';
import { useState, useEffect } from 'react';

export default function IntegrationDetailDrawer({ open, integrationId, onClose, onConnect, onDisconnect, onTest }) {
  const [intg, setIntg] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState(null);

  useEffect(() => {
    if (!open || !integrationId) return;
    setIntg(null);
    setError('');
    setTestResult(null);
    setLoading(true);
    fetch(`/api/integrations/${integrationId}`)
      .then(r => r.ok ? r.json() : Promise.reject(r.statusText))
      .then(d => setIntg(d?.integration ?? d))
      .catch(e => setError(e?.message || 'Could not load integration.'))
      .finally(() => setLoading(false));
  }, [open, integrationId]);

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    try {
      const res = await onTest?.(integrationId);
      setTestResult({ ok: true, message: res?.message ?? 'Connection successful.' });
    } catch (e) {
      setTestResult({ ok: false, message: e?.message ?? 'Test failed.' });
    }
    setTesting(false);
  };

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,.6)', backdropFilter: 'blur(3px)', display: 'flex', justifyContent: 'flex-end' }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div data-cy="integration-detail-drawer" style={{ width: 460, background: 'var(--bg-elevated,#111)', borderLeft: '1px solid var(--border,#333)', overflowY: 'auto', padding: '28px 24px', display: 'flex', flexDirection: 'column', gap: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 20 }}>
          <div style={{ fontWeight: 700, fontSize: 18 }}>{intg ? `${intg.icon ?? '🔌'} ${intg.name}` : 'Integration detail'}</div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 22, color: 'var(--text-dim,#888)', lineHeight: 1 }}>✕</button>
        </div>

        {loading && <div style={{ color: 'var(--text-dim,#888)', fontSize: 14 }}>Loading…</div>}
        {error && <div style={{ color: 'var(--error,#ef4444)', fontSize: 13, padding: '10px 14px', borderRadius: 8, background: 'rgba(239,68,68,.08)', border: '1px solid rgba(239,68,68,.2)' }}>{error}</div>}

        {intg && (
          <>
            <div style={{ display: 'flex', gap: 8, marginBottom: 14, flexWrap: 'wrap' }}>
              <ConnStatusBadge status={intg.status} />
              {intg.category && <span style={{ fontSize: 11, padding: '3px 9px', borderRadius: 999, background: 'rgba(255,255,255,.06)', color: 'var(--text-dim,#888)', border: '1px solid var(--border,#333)' }}>{intg.category}</span>}
            </div>

            {intg.description && (
              <div style={{ fontSize: 14, lineHeight: 1.65, color: 'var(--text-secondary,#bbb)', marginBottom: 18 }}>{intg.description}</div>
            )}

            {/* Permissions */}
            {intg.permissions?.length > 0 && (
              <Section title="Permissions required">
                {intg.permissions.map((p, i) => (
                  <div key={i} style={{ fontSize: 12, color: 'var(--text-dim,#888)', marginBottom: 4 }}>· {p}</div>
                ))}
              </Section>
            )}

            {/* Tools unlocked */}
            {intg.tools_unlocked?.length > 0 && (
              <Section title="Tools unlocked">
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
                  {intg.tools_unlocked.map(t => (
                    <span key={t} style={{ fontSize: 12, padding: '4px 10px', borderRadius: 6, background: 'rgba(16,185,129,.10)', color: '#10b981', border: '1px solid rgba(16,185,129,.25)' }}>{t}</span>
                  ))}
                </div>
              </Section>
            )}

            {/* Used by agents */}
            {intg.used_by?.length > 0 && (
              <Section title="Used by agents">
                {intg.used_by.map((a, i) => (
                  <span key={i} style={{ fontSize: 12, padding: '3px 9px', borderRadius: 6, background: 'var(--surface2,#1a1a2e)', border: '1px solid var(--border,#333)', marginRight: 6, display: 'inline-block', marginBottom: 5 }}>{a}</span>
                ))}
              </Section>
            )}

            {/* Test result */}
            {testResult && (
              <div style={{ padding: '10px 14px', borderRadius: 8, background: testResult.ok ? 'rgba(16,185,129,.08)' : 'rgba(239,68,68,.08)', border: `1px solid ${testResult.ok ? 'rgba(16,185,129,.3)' : 'rgba(239,68,68,.3)'}`, fontSize: 13, color: testResult.ok ? '#10b981' : '#ef4444', marginBottom: 14 }}>
                {testResult.ok ? '✓ ' : '✗ '}{testResult.message}
              </div>
            )}

            <div style={{ display: 'flex', gap: 10, marginTop: 'auto', paddingTop: 20, borderTop: '1px solid var(--border,#333)', flexWrap: 'wrap' }}>
              {intg.status !== 'connected' && (
                <button onClick={() => { onConnect?.(integrationId); onClose(); }} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 700, fontSize: 14 }}>Connect</button>
              )}
              {intg.status === 'connected' && (
                <>
                  <button onClick={handleTest} disabled={testing} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-primary,#f1f1f1)', cursor: 'pointer', fontSize: 13 }}>
                    {testing ? 'Testing…' : 'Test connection'}
                  </button>
                  <button onClick={() => { onDisconnect?.(integrationId); onClose(); }} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid rgba(239,68,68,.4)', color: '#ef4444', cursor: 'pointer', fontSize: 13 }}>Disconnect</button>
                </>
              )}
              <button onClick={onClose} style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13 }}>Close</button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

function ConnStatusBadge({ status }) {
  const MAP = { connected: '#10b981', disconnected: '#6b7280', error: '#ef4444', pending: '#f59e0b' };
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
