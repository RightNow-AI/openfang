'use client';

const TABS = [
  { id: 'recommended', label: 'Recommended' },
  { id: 'watchlist', label: 'Watchlist' },
  { id: 'research', label: 'Research' },
  { id: 'portfolio', label: 'Portfolio' },
  { id: 'alerts', label: 'Alerts' },
  { id: 'advanced', label: 'Advanced' },
];

export default function InvestmentsTabs({ activeTab, onChange, alertCount }) {
  return (
    <div style={{ display: 'flex', gap: 0, borderBottom: '2px solid var(--border)', marginBottom: 24, overflowX: 'auto' }}>
      {TABS.map((tab) => {
        const active = activeTab === tab.id;
        return (
          <button
            key={tab.id}
            onClick={() => onChange(tab.id)}
            style={{
              padding: '9px 16px',
              fontSize: 13,
              fontWeight: active ? 700 : 500,
              color: active ? 'var(--accent)' : 'var(--text-dim)',
              background: 'transparent',
              border: 'none',
              borderBottom: `2px solid ${active ? 'var(--accent)' : 'transparent'}`,
              cursor: 'pointer',
              marginBottom: -2,
              display: 'flex',
              alignItems: 'center',
              gap: 5,
              whiteSpace: 'nowrap',
              transition: 'color 0.15s',
            }}
          >
            {tab.label}
            {tab.id === 'alerts' && alertCount > 0 && (
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 800,
                  color: '#fff',
                  background: '#ef4444',
                  borderRadius: 10,
                  padding: '1px 6px',
                  minWidth: 16,
                  textAlign: 'center',
                }}
              >
                {alertCount}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}
