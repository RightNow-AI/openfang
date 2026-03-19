'use client';

const TABS = [
  { id: 'recommended', label: 'Recommended' },
  { id: 'overview', label: 'Overview' },
  { id: 'templates', label: 'Templates' },
  { id: 'advanced', label: 'Advanced' },
];

export default function FinanceTabs({ activeTab, onChange }) {
  return (
    <div
      style={{ display: 'flex', gap: 2, borderBottom: '1px solid var(--border)', marginBottom: 22 }}
      role="tablist"
      data-cy="finance-tabs"
    >
      {TABS.map((tab) => {
        const active = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            role="tab"
            aria-selected={active}
            onClick={() => onChange(tab.id)}
            style={{
              padding: '8px 16px',
              fontSize: 13,
              fontWeight: active ? 700 : 500,
              color: active ? 'var(--accent)' : 'var(--text-dim)',
              background: 'transparent',
              border: 'none',
              borderBottom: `2px solid ${active ? 'var(--accent)' : 'transparent'}`,
              cursor: 'pointer',
              marginBottom: -1,
              borderRadius: 0,
              transition: 'color 0.12s, border-color 0.12s',
            }}
            data-cy={`tab-${tab.id}`}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
}
