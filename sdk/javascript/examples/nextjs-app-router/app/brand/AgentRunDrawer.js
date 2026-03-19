'use client';

const TASK_LABELS = {
  analyze_business: 'Analyze My Business',
  research_competitors: 'Research Competitors',
  build_voice_guide: 'Build Voice Guide',
  create_customer_avatar: 'Create Customer Avatar',
  draft_outreach_email_sequence: 'Draft Outreach Emails',
};

// ── Spinner animation ─────────────────────────────────────────────────────

function Spinner({ size = 24, color = 'var(--accent)' }) {
  return (
    <>
      <div style={{
        width: size, height: size,
        border: `3px solid var(--border)`,
        borderTopColor: color,
        borderRadius: '50%',
        animation: 'bcc-spin 0.7s linear infinite',
        flexShrink: 0,
      }} />
      <style>{`@keyframes bcc-spin { to { transform: rotate(360deg); } }`}</style>
    </>
  );
}

// ── Elapsed timer ─────────────────────────────────────────────────────────

function ElapsedDisplay({ duration_ms }) {
  if (!duration_ms) return null;
  const sec = (duration_ms / 1000).toFixed(1);
  return (
    <span style={{ fontSize: 12, color: 'var(--text-muted)' }}>
      Completed in {sec}s
    </span>
  );
}

// ── AgentRunDrawer ─────────────────────────────────────────────────────────

export default function AgentRunDrawer({ open, runState, onClose, onViewOutput }) {
  if (!open) return null;

  const taskLabel = TASK_LABELS[runState?.task_type] || runState?.task_type || 'Unknown task';
  const status = runState?.status;
  const hasOutput = status === 'completed' && runState?.output;
  const hasFailed = status === 'failed';

  return (
    <>
      {/* Backdrop */}
      <div
        onClick={onClose}
        style={{
          position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.35)',
          zIndex: 100, backdropFilter: 'blur(2px)',
        }}
      />

      {/* Drawer */}
      <div style={{
        position: 'fixed',
        bottom: 0, right: 0,
        width: '100%', maxWidth: 480,
        background: 'var(--bg-elevated)',
        borderTop: '1px solid var(--border)',
        borderLeft: '1px solid var(--border)',
        borderRadius: '14px 0 0 0',
        zIndex: 101,
        padding: '20px 24px 28px',
        boxShadow: 'var(--shadow-md)',
        animation: 'slideUp 0.22s ease',
      }}>
        <style>{`
          @keyframes slideUp {
            from { transform: translateY(40px); opacity: 0; }
            to   { transform: translateY(0); opacity: 1; }
          }
        `}</style>

        {/* Header */}
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', marginBottom: 20 }}>
          <div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)', fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.4px', marginBottom: 3 }}>
              Agent Run
            </div>
            <div style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)' }}>
              {taskLabel}
            </div>
          </div>
          <button
            onClick={onClose}
            style={{
              background: 'none', border: 'none', cursor: 'pointer',
              fontSize: 20, color: 'var(--text-muted)', padding: 0,
              lineHeight: 1,
            }}
          >×</button>
        </div>

        {/* Status body */}
        {status === 'running' && (
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', padding: '24px 0', gap: 16 }}>
            <Spinner size={36} />
            <div>
              <div style={{ fontSize: 14, fontWeight: 600, color: 'var(--text)', textAlign: 'center', marginBottom: 6 }}>
                Running…
              </div>
              <div style={{ fontSize: 12, color: 'var(--text-dim)', textAlign: 'center', lineHeight: 1.6, maxWidth: 320 }}>
                The agent is processing your brand context. This usually takes 15–45 seconds depending on complexity.
              </div>
            </div>

            {/* Step indicators */}
            <div style={{ width: '100%', marginTop: 8 }}>
              {['Reading brand context', 'Building task prompt', 'Agent thinking…'].map((step, i) => (
                <div key={step} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '5px 0' }}>
                  <div style={{
                    width: 6, height: 6, borderRadius: '50%',
                    background: i === 2 ? 'var(--accent)' : 'var(--success)',
                    flexShrink: 0,
                  }} />
                  <span style={{ fontSize: 12, color: i === 2 ? 'var(--text)' : 'var(--text-dim)' }}>
                    {step}
                  </span>
                  {i === 2 && <Spinner size={12} />}
                </div>
              ))}
            </div>
          </div>
        )}

        {hasOutput && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            {/* Success indicator */}
            <div style={{
              display: 'flex', alignItems: 'center', gap: 10,
              padding: '12px 14px', borderRadius: 8,
              background: 'var(--success-subtle)', border: '1px solid var(--success)',
            }}>
              <span style={{ fontSize: 20 }}>✓</span>
              <div>
                <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--success)' }}>
                  {runState.output.title} ready
                </div>
                <ElapsedDisplay duration_ms={runState.output.duration_ms} />
              </div>
            </div>

            {/* Content preview */}
            <div style={{
              borderRadius: 8, border: '1px solid var(--border)',
              background: 'var(--surface2)',
              padding: '12px 14px',
              maxHeight: 180, overflowY: 'auto',
            }}>
              <pre style={{
                margin: 0, whiteSpace: 'pre-wrap', wordBreak: 'break-word',
                fontSize: 11, lineHeight: 1.6, color: 'var(--text-secondary)',
                fontFamily: 'var(--font-sans)',
              }}>
                {(runState.output.content || '').slice(0, 600)}{runState.output.content?.length > 600 ? '…' : ''}
              </pre>
            </div>

            {/* Action buttons */}
            <div style={{ display: 'flex', gap: 8 }}>
              <button
                onClick={onViewOutput}
                style={{
                  flex: 1, padding: '9px 14px', borderRadius: 7, fontWeight: 700,
                  background: 'var(--accent)', color: '#fff', border: 'none',
                  cursor: 'pointer', fontSize: 13,
                }}
              >
                View Full Output
              </button>
              <button
                onClick={onClose}
                style={{
                  padding: '9px 14px', borderRadius: 7, fontWeight: 600,
                  background: 'var(--surface2)', color: 'var(--text-dim)',
                  border: '1px solid var(--border)', cursor: 'pointer', fontSize: 13,
                }}
              >
                Dismiss
              </button>
            </div>
          </div>
        )}

        {hasFailed && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            <div style={{
              display: 'flex', alignItems: 'flex-start', gap: 10,
              padding: '12px 14px', borderRadius: 8,
              background: 'var(--error-subtle)', border: '1px solid var(--error)',
            }}>
              <span style={{ fontSize: 18 }}>✗</span>
              <div>
                <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--error)', marginBottom: 4 }}>
                  Run failed
                </div>
                <div style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
                  {runState.error || 'An unknown error occurred.'}
                </div>
              </div>
            </div>
            <div style={{ fontSize: 12, color: 'var(--text-muted)', lineHeight: 1.6 }}>
              Tip: Make sure the daemon is running and an agent is configured, then try again.
            </div>
            <button
              onClick={onClose}
              style={{
                padding: '8px 14px', borderRadius: 7, fontWeight: 600,
                background: 'var(--surface2)', color: 'var(--text-dim)',
                border: '1px solid var(--border)', cursor: 'pointer', fontSize: 13,
              }}
            >
              Close
            </button>
          </div>
        )}
      </div>
    </>
  );
}
