'use client';

const CREATION_ICONS = { image: '🖼️', video: '🎬', 'image+video': '✨' };

export default function CreativeStarterCard({ starter, onStart }) {
  return (
    <div
      data-cy="creative-starter-card"
      style={{
        border: '1px solid var(--border)',
        borderRadius: 'var(--radius)',
        padding: '18px 20px',
        display: 'flex',
        flexDirection: 'column',
        gap: 10,
        background: 'var(--bg-elevated)',
        cursor: 'pointer',
        transition: 'border-color 0.15s, box-shadow 0.15s',
      }}
      onMouseEnter={e => { e.currentTarget.style.borderColor = 'var(--accent)'; e.currentTarget.style.boxShadow = 'var(--shadow-sm)'; }}
      onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.boxShadow = 'none'; }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <span style={{ fontSize: 28 }}>{CREATION_ICONS[starter.creation_type] ?? '🎨'}</span>
        <span style={{
          fontSize: 10, padding: '2px 8px', borderRadius: 999,
          background: 'var(--surface2)', color: 'var(--text-dim)',
          fontWeight: 600, textTransform: 'uppercase', letterSpacing: '0.04em',
          alignSelf: 'flex-start',
        }}>
          {starter.creation_type === 'image+video' ? 'Images + Video' :
           starter.creation_type === 'image' ? 'Images' : 'Video'}
        </span>
      </div>
      <div style={{ fontWeight: 700, fontSize: 15, color: 'var(--text)' }}>{starter.title}</div>
      <div style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.5 }}>{starter.tagline}</div>
      <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>
        Best for: {starter.best_for}
      </div>
      <button
        className="btn btn-sm"
        style={{ marginTop: 6, background: 'var(--accent)', color: '#fff', border: 'none', alignSelf: 'flex-start' }}
        onClick={() => onStart(starter)}
      >
        Start this project
      </button>
    </div>
  );
}
