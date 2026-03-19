'use client';

const VIEWS = [
  { id: 'simple', label: 'Simple' },
  { id: 'detailed', label: 'Detailed' },
  { id: 'advanced', label: 'Advanced' },
];

export default function InvestmentsViewToggle({ view, onChange }) {
  return (
    <div
      style={{
        display: 'inline-flex',
        background: 'var(--surface)',
        border: '1.5px solid var(--border)',
        borderRadius: 8,
        padding: 2,
        gap: 2,
        marginBottom: 18,
      }}
    >
      {VIEWS.map((v) => {
        const active = view === v.id;
        return (
          <button
            key={v.id}
            onClick={() => onChange(v.id)}
            style={{
              padding: '5px 14px',
              borderRadius: 6,
              border: 'none',
              fontSize: 12,
              fontWeight: active ? 700 : 500,
              color: active ? '#fff' : 'var(--text-dim)',
              background: active ? 'var(--accent)' : 'transparent',
              cursor: 'pointer',
              transition: 'background 0.15s, color 0.15s',
            }}
          >
            {v.label}
          </button>
        );
      })}
    </div>
  );
}
