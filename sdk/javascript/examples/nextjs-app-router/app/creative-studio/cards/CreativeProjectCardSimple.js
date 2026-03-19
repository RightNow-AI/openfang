'use client';

const STATUS_BADGES = {
  draft:       { label: 'Draft',            bg: 'var(--surface2)',        color: 'var(--text-dim)' },
  plan_ready:  { label: 'Plan ready',       bg: 'var(--warning-subtle)',  color: 'var(--warning)' },
  approved:    { label: 'Approved',         bg: 'var(--success-subtle)',  color: 'var(--success)' },
  running:     { label: 'Running…',         bg: 'var(--accent-subtle)',   color: 'var(--accent)' },
  done:        { label: 'Done',             bg: 'var(--success-muted)',   color: 'var(--success-dim)' },
  error:       { label: 'Error',            bg: 'var(--error-subtle)',    color: 'var(--error)' },
};

const TYPE_ICONS = { image: '🖼️', video: '🎬', 'image+video': '✨' };

export default function CreativeProjectCardSimple({ project, onOpen }) {
  const badge = STATUS_BADGES[project.status] ?? STATUS_BADGES.draft;
  return (
    <div
      data-cy="creative-project-card"
      style={{
        padding: '14px 16px',
        borderRadius: 'var(--radius)',
        border: '1px solid var(--border)',
        background: 'var(--bg-elevated)',
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        cursor: 'pointer',
      }}
      onClick={() => onOpen && onOpen(project)}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          <span style={{ fontSize: 20 }}>{TYPE_ICONS[project.creation_type] ?? '🎨'}</span>
          <span style={{ fontWeight: 600, fontSize: 13, color: 'var(--text)' }}>
            {project.name || project.topic || 'Untitled'}
          </span>
        </div>
        <span style={{
          fontSize: 10, padding: '2px 8px', borderRadius: 999,
          background: badge.bg, color: badge.color, fontWeight: 600,
        }}>
          {badge.label}
        </span>
      </div>
      {project.topic && (
        <div style={{ fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.4 }}>
          {project.topic}
        </div>
      )}
      {project.platform && (
        <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{project.platform}</div>
      )}
    </div>
  );
}
