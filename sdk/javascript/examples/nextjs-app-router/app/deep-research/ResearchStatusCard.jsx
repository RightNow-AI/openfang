'use client';

export default function ResearchStatusCard({ phase, title, message, detail, actions = [] }) {
  const tone = phase === 'error' ? '#ef4444' : phase === 'running' ? 'var(--accent)' : '#f59e0b';

  return (
    <div style={{
      marginBottom: 18,
      padding: '16px 18px',
      borderRadius: 14,
      border: `1px solid ${phase === 'error' ? 'rgba(239,68,68,0.28)' : 'rgba(37,99,235,0.24)'}`,
      background: phase === 'error' ? 'rgba(127,29,29,0.18)' : 'rgba(37,99,235,0.08)',
    }}>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 6 }}>
        {phase === 'error' ? 'Needs attention' : phase === 'running' ? 'Research in progress' : 'Research started'}
      </div>
      <div style={{ fontSize: 18, fontWeight: 800, color: tone, marginBottom: 6 }}>{title}</div>
      <div style={{ fontSize: 14, lineHeight: 1.6, color: 'var(--text)' }}>{message}</div>
      {detail ? <div style={{ marginTop: 8, fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.6 }}>{detail}</div> : null}
      {actions.length > 0 ? (
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 12 }}>
          {actions.map((action) => (
            <button
              key={action.label}
              onClick={action.onClick}
              style={{
                padding: '8px 12px',
                borderRadius: 8,
                border: action.primary ? 'none' : '1px solid var(--border-light)',
                background: action.primary ? 'var(--accent)' : 'var(--surface)',
                color: action.primary ? '#fff' : 'var(--text)',
                fontSize: 12,
                fontWeight: 700,
                cursor: 'pointer',
              }}
            >
              {action.label}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}