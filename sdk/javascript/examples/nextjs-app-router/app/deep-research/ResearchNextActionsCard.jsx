'use client';

export default function ResearchNextActionsCard({ actions = [], onCopy }) {
  return (
    <div style={{ padding: '16px', border: '1px solid var(--border-light)', borderRadius: 12, background: 'var(--surface2)' }}>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 8 }}>Next steps</div>
      {actions.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.6 }}>
          No next steps were extracted from the result. Re-run with a clearer question or review the full report for follow-up work.
        </div>
      ) : (
        <>
          <div style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)', marginBottom: 12 }}>{actions[0]}</div>
          <div style={{ display: 'grid', gap: 10 }}>
            {actions.slice(1, 5).map((action, index) => (
              <div key={`${action}-${index}`} style={{ display: 'grid', gridTemplateColumns: '24px minmax(0, 1fr)', gap: 10, alignItems: 'start' }}>
                <span style={{ width: 22, height: 22, borderRadius: 999, background: 'rgba(37,99,235,0.18)', color: '#93c5fd', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', fontSize: 11, fontWeight: 800 }}>{index + 2}</span>
                <span style={{ fontSize: 13, lineHeight: 1.6 }}>{action}</span>
              </div>
            ))}
          </div>
          {onCopy ? (
            <button onClick={onCopy} style={{ marginTop: 12, padding: '7px 12px', borderRadius: 8, border: '1px solid var(--border-light)', background: 'transparent', color: 'var(--text)', fontSize: 12, cursor: 'pointer' }}>
              Copy next steps
            </button>
          ) : null}
        </>
      )}
    </div>
  );
}