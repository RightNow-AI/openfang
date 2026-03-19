'use client';

import { STARTER_CATEGORY_LABELS } from '../config/finance-starters';

const CATEGORY_COLORS = {
  agency: { bg: 'var(--accent-subtle)', color: 'var(--accent)' },
  growth: { bg: 'var(--success-subtle)', color: 'var(--success)' },
  school: { bg: 'rgba(59,130,246,0.1)', color: '#3b82f6' },
  general: { bg: 'rgba(148,163,184,0.1)', color: 'var(--text-dim)' },
};

export default function FinanceStarterCard({ template, applying, onApply }) {
  const catStyle = CATEGORY_COLORS[template.category] || CATEGORY_COLORS.general;

  return (
    <div
      className="card"
      data-cy={`starter-card-${template.id}`}
      style={{ display: 'flex', flexDirection: 'column', gap: 0 }}
    >
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12, marginBottom: 10 }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 14, color: 'var(--text)', marginBottom: 4 }}>
            {template.title}
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.5 }}>
            {template.description}
          </div>
        </div>
        <span
          style={{
            flexShrink: 0,
            fontSize: 10,
            fontWeight: 700,
            padding: '2px 8px',
            borderRadius: 20,
            background: catStyle.bg,
            color: catStyle.color,
          }}
        >
          {STARTER_CATEGORY_LABELS[template.category] || template.category}
        </span>
      </div>

      {/* Best for */}
      <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 10 }}>
        Best for: {template.bestFor}
      </div>

      {/* Tracks */}
      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4, marginBottom: 14 }}>
        {template.tracks.map((t) => (
          <span key={t} className="badge badge-dim">{t}</span>
        ))}
      </div>

      {/* Footer */}
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 8 }}>
        {template.needsApproval ? (
          <span style={{ fontSize: 11, color: 'var(--text-dim)', display: 'flex', alignItems: 'center', gap: 4 }}>
            <span>🔐</span> Requires approval
          </span>
        ) : (
          <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>No approvals needed</span>
        )}
        <button
          className="btn btn-primary btn-sm"
          onClick={onApply}
          disabled={applying}
          data-cy={`apply-starter-${template.id}`}
        >
          {applying ? (
            <><span className="spinner" style={{ width: 12, height: 12 }} /> Applying…</>
          ) : (
            'Use this template'
          )}
        </button>
      </div>
    </div>
  );
}
