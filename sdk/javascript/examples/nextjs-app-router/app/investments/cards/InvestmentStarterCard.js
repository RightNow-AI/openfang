'use client';

const CATEGORY_COLORS = {
  daily: 'var(--accent)',
  research: '#3b82f6',
  earnings: '#f59e0b',
  patterns: '#8b5cf6',
  portfolio: '#10b981',
  thesis: '#6366f1',
};

export default function InvestmentStarterCard({ template, applying, onApply }) {
  const color = CATEGORY_COLORS[template.id] || 'var(--accent)';

  return (
    <div
      className="card"
      style={{ display: 'flex', flexDirection: 'column', gap: 10 }}
      data-cy={`starter-card-${template.id}`}
    >
      <div className="card-header" style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>{template.title}</div>
        {template.approvalRequired && (
          <span className="badge badge-warn" style={{ fontSize: 10, flexShrink: 0 }}>Approval required</span>
        )}
      </div>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', margin: 0, lineHeight: 1.55 }}>{template.description}</p>

      <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>
        <span style={{ fontWeight: 600 }}>Best for</span>{' '}
        <span>{template.bestFor}</span>
      </div>

      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
        {template.tracks.map((t) => (
          <span key={t} className="badge badge-dim" style={{ fontSize: 10, background: `${color}18`, color }}>{t}</span>
        ))}
      </div>

      <button
        className="btn btn-primary btn-sm"
        onClick={() => onApply(template)}
        disabled={applying}
        style={{ marginTop: 'auto' }}
        data-cy={`starter-apply-${template.id}`}
      >
        {applying ? (
          <span style={{ display: 'flex', alignItems: 'center', gap: 6, justifyContent: 'center' }}>
            <span className="spinner" style={{ width: 12, height: 12 }} />
            Applying…
          </span>
        ) : 'Use this setup'}
      </button>
    </div>
  );
}
