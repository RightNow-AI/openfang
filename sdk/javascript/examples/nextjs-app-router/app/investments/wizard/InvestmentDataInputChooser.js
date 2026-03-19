'use client';

const OPTIONS = [
  { value: 'manual', title: 'Type it in by hand', description: 'Add symbols and details manually. Good for a focused watchlist you control.' },
  { value: 'csv', title: 'Upload a CSV file', description: 'Import from a spreadsheet or export from your brokerage.' },
  { value: 'provider', title: 'Connect a market data source', description: 'Link Alpha Vantage, Finnhub, or another provider for live data.' },
];

export default function InvestmentDataInputChooser({ value, onChange }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {OPTIONS.map((opt) => {
        const active = value === opt.value;
        return (
          <button
            key={opt.value}
            type="button"
            onClick={() => onChange(opt.value)}
            style={{
              textAlign: 'left',
              padding: '11px 14px',
              borderRadius: 8,
              border: `2px solid ${active ? 'var(--accent)' : 'var(--border)'}`,
              background: active ? 'rgba(255,106,26,0.06)' : 'var(--surface)',
              cursor: 'pointer',
              transition: 'border-color 0.15s',
            }}
          >
            <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text)', marginBottom: 2 }}>{opt.title}</div>
            <div style={{ fontSize: 11, color: 'var(--text-dim)' }}>{opt.description}</div>
          </button>
        );
      })}
    </div>
  );
}
