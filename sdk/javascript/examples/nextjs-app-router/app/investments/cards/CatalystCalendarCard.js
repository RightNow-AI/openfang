'use client';

const TYPE_BADGE = {
  earnings: 'badge-warn',
  ipo: 'badge-success',
  dividend: 'badge-dim',
  split: 'badge-dim',
  fda: 'badge-error',
  economic: 'badge-dim',
};

export default function CatalystCalendarCard({ catalysts }) {
  return (
    <div className="card" data-cy="catalyst-calendar">
      <div className="card-header" style={{ marginBottom: 10 }}>
        <span style={{ fontWeight: 700, fontSize: 13, color: 'var(--text)' }}>Upcoming catalysts</span>
      </div>

      {!catalysts || catalysts.length === 0 ? (
        <p style={{ fontSize: 12, color: 'var(--text-dim)', textAlign: 'center', padding: '16px 0' }}>
          No upcoming events tracked.
        </p>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
          {catalysts.map((c, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12 }}>
              <div style={{ width: 52, flexShrink: 0, color: 'var(--text-dim)', fontSize: 11 }}>
                {c.date && new Date(c.date).toLocaleDateString(undefined, { month: 'short', day: 'numeric' })}
              </div>
              <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--text)', width: 44, flexShrink: 0 }}>
                {c.symbol}
              </span>
              <span style={{ flex: 1, color: 'var(--text-dim)' }}>{c.label}</span>
              <span className={`badge ${TYPE_BADGE[c.type] || 'badge-dim'}`} style={{ fontSize: 9 }}>
                {c.type}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
