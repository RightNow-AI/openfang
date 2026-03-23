'use client';
import { useState, useEffect, useRef } from 'react';

// ─── SkillDrawer ──────────────────────────────────────────────────────────────
// Read-only detail view for a single skill.
// Opened from SkillsClient when a user clicks "Details".
// Fetches on open; aborts in-flight fetch if skill switches or closes.

export default function SkillDrawer({ skillName, onClose, onToggle, togglePending }) {
  const [detail, setDetail] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const abortRef = useRef(null);

  // Fetch skill detail whenever skillName changes
  useEffect(() => {
    if (!skillName) return;

    // Cancel previous in-flight request
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    const loadDetail = async () => {
      setDetail(null);
      setError('');
      setLoading(true);

      try {
        const response = await fetch(`/api/skills/${encodeURIComponent(skillName)}`, {
          signal: controller.signal,
        });
        const data = await response.json();
        if (data?.error) throw new Error(data.error);
        setDetail(data);
        setLoading(false);
      } catch (e) {
        if (e.name === 'AbortError') return;
        setError(e.message || 'Could not load skill details.');
        setLoading(false);
      }
    };

    void loadDetail();

    return () => controller.abort();
  }, [skillName]);

  // Escape to close
  useEffect(() => {
    const handler = (e) => { if (e.key === 'Escape') onClose(); };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [onClose]);

  if (!skillName) return null;

  const isUsed = (detail?.used_by?.length ?? 0) > 0;

  return (
    // Overlay
    <div
      data-cy="skill-drawer-overlay"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
      style={{
        position: 'fixed', inset: 0, zIndex: 1000,
        background: 'rgba(0,0,0,0.45)', backdropFilter: 'blur(2px)',
        display: 'flex', justifyContent: 'flex-end',
      }}
    >
      {/* Panel */}
      <div
        data-cy="skill-drawer-panel"
        style={{
          width: '100%', maxWidth: 480,
          height: '100%',
          background: 'var(--bg-elevated)',
          borderLeft: '1px solid var(--border)',
          boxShadow: 'var(--shadow-lg, -16px 0 48px rgba(0,0,0,.35))',
          display: 'flex', flexDirection: 'column',
          overflow: 'hidden',
        }}
      >
        {/* Header */}
        <div style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '16px 20px', borderBottom: '1px solid var(--border)', flexShrink: 0,
        }}>
          <div style={{ minWidth: 0 }}>
            <div style={{ fontWeight: 700, fontSize: 15 }}>{skillName}</div>
            {detail && (
              <div style={{ display: 'flex', gap: 5, marginTop: 4, flexWrap: 'wrap' }}>
                {detail.version && (
                  <span className="badge badge-muted" style={{ fontSize: 10 }}>v{detail.version}</span>
                )}
                {detail.runtime && (
                  <span className="badge badge-info" style={{ fontSize: 10 }}>{detail.runtime}</span>
                )}
                <span
                  className={`badge ${detail.bundled ? 'badge-success' : 'badge-warning'}`}
                  style={{ fontSize: 10 }}
                >
                  {detail.bundled ? 'Bundled' : 'Custom'}
                </span>
              </div>
            )}
          </div>
          <button
            className="btn btn-ghost btn-sm"
            onClick={onClose}
            style={{ flexShrink: 0, marginLeft: 12, fontSize: 16, padding: '2px 8px' }}
            aria-label="Close drawer"
          >✕</button>
        </div>

        {/* Scrollable body */}
        <div style={{ overflowY: 'auto', flex: 1, padding: '16px 20px', display: 'flex', flexDirection: 'column', gap: 18 }}>
          {loading && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: 'var(--text-dim)', fontSize: 13 }}>
              <div className="spinner" style={{ width: 14, height: 14 }} />
              Loading…
            </div>
          )}

          {error && (
            <div data-cy="skill-drawer-error" className="error-state" style={{ fontSize: 12 }}>⚠ {error}</div>
          )}

          {detail && (
            <>
              {/* Description */}
              {detail.description && (
                <section>
                  <p style={{ margin: 0, fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.6 }}>
                    {detail.description}
                  </p>
                </section>
              )}

              {/* Tools */}
              <section>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
                  Tools ({detail.tools.length})
                </div>
                {detail.tools.length > 0 ? (
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                    {detail.tools.map(t => (
                      <span key={t} className="badge badge-dim" style={{ fontFamily: 'var(--font-mono,monospace)', fontSize: 11 }}>{t}</span>
                    ))}
                  </div>
                ) : (
                  <p style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)' }}>No tools defined.</p>
                )}
              </section>

              {/* Entrypoint / Prompt context */}
              {(detail.entrypoint || detail.prompt_context) && (
                <section>
                  <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
                    {detail.prompt_context ? 'Prompt Context' : 'Entrypoint'}
                  </div>
                  <pre style={{
                    margin: 0, padding: '10px 12px',
                    background: 'var(--surface2)',
                    border: '1px solid var(--border)',
                    borderRadius: 'var(--radius-sm)',
                    fontSize: 11, lineHeight: 1.6,
                    color: 'var(--text-secondary)',
                    fontFamily: 'var(--font-mono,monospace)',
                    whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                    maxHeight: 180, overflowY: 'auto',
                  }}>
                    {detail.prompt_context || detail.entrypoint}
                  </pre>
                </section>
              )}

              {/* Source */}
              {detail.source && (
                <section>
                  <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 6 }}>
                    Source
                  </div>
                  <code style={{ fontSize: 11, color: 'var(--text-secondary)', wordBreak: 'break-all' }}>{detail.source}</code>
                </section>
              )}

              {/* Agents referencing this skill */}
              <section>
                <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '.05em', marginBottom: 8 }}>
                  Agents referencing this skill ({detail.used_by.length})
                </div>
                {detail.used_by.length > 0 ? (
                  <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                    {detail.used_by.map(a => (
                      <span key={a} className="badge badge-dim" style={{ fontSize: 11 }}>{a}</span>
                    ))}
                  </div>
                ) : (
                  <p style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)' }}>No agents reference this skill.</p>
                )}
              </section>
            </>
          )}
        </div>

        {/* Footer — toggle + warning */}
        {detail && (
          <div style={{
            borderTop: '1px solid var(--border)', padding: '14px 20px',
            display: 'flex', flexDirection: 'column', gap: 10, flexShrink: 0,
            background: 'var(--bg-elevated)',
          }}>
            {/* Warning when disabling a used skill */}
            {isUsed && detail.enabled && (
              <div
                data-cy="skill-disable-warning"
                style={{
                  fontSize: 12, padding: '8px 12px',
                  background: 'var(--warning, #f59e0b)18',
                  border: '1px solid var(--warning, #f59e0b)44',
                  borderRadius: 'var(--radius-sm)',
                  color: 'var(--warning, #f59e0b)',
                }}
              >
                ⚠ This skill is referenced by {detail.used_by.length} agent{detail.used_by.length !== 1 ? 's' : ''}.
                Disabling it will block future runtime invocations.
              </div>
            )}

            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12 }}>
              <span style={{ fontSize: 13, color: 'var(--text-secondary)' }}>
                {detail.enabled ? 'Enabled' : 'Disabled'}
              </span>
              <button
                data-cy="skill-drawer-toggle"
                className={`btn btn-sm ${detail.enabled ? 'btn-ghost' : 'btn-primary'}`}
                onClick={() => onToggle(skillName, detail.enabled, detail.used_by.length)}
                disabled={togglePending}
                style={{ minWidth: 96 }}
              >
                {togglePending
                  ? <span style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                      <div className="spinner" style={{ width: 12, height: 12 }} /> Saving…
                    </span>
                  : detail.enabled ? 'Disable' : 'Enable'
                }
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
