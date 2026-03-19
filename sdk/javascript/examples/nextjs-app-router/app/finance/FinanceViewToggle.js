'use client';

const VIEWS = [
  { id: 'simple', label: 'Simple' },
  { id: 'detailed', label: 'Detailed' },
  { id: 'advanced', label: 'Advanced' },
];

export default function FinanceViewToggle({ view, onChange }) {
  return (
    <div
      style={{
        display: 'inline-flex',
        gap: 0,
        borderRadius: 8,
        border: '1px solid var(--border)',
        overflow: 'hidden',
        background: 'var(--bg-elevated)',
      }}
      data-cy="finance-view-toggle"
    >
      {VIEWS.map((v) => {
        const active = view === v.id;
        return (
          <button
            key={v.id}
            onClick={() => onChange(v.id)}
            style={{
              padding: '5px 13px',
              fontSize: 12,
              fontWeight: active ? 700 : 500,
              color: active ? '#fff' : 'var(--text-dim)',
              background: active ? 'var(--accent)' : 'transparent',
              border: 'none',
              cursor: 'pointer',
              transition: 'background 0.12s, color 0.12s',
            }}
            data-cy={`view-${v.id}`}
          >
            {v.label}
          </button>
        );
      })}
    </div>
  );
}
