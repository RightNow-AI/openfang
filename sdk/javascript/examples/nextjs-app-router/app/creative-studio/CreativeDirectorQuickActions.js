'use client';

const ACTIONS = [
  { id: 'generate_moodboard_directions', label: 'Build moodboard', icon: '🎨' },
  { id: 'generate_prompt_pack',          label: 'Draft prompts',   icon: '✏️' },
  { id: 'generate_script_strategy',      label: 'Draft script',    icon: '📝' },
  { id: 'generate_image_drafts',         label: 'Generate images', icon: '🖼' },
  { id: 'generate_video_plan',           label: 'Video plan',      icon: '🎬' },
];

export default function CreativeDirectorQuickActions({ disabled, onRunAction }) {
  return (
    <div data-cy="director-quick-actions" style={{ display: 'flex', gap: 7, flexWrap: 'wrap', marginBottom: 16 }}>
      {ACTIONS.map(a => (
        <button
          key={a.id}
          data-cy={`quick-action-${a.id}`}
          onClick={() => onRunAction(a.id)}
          disabled={disabled}
          style={{
            padding: '5px 12px',
            borderRadius: 20,
            background: 'transparent',
            border: '1px solid var(--border)',
            color: disabled ? 'var(--text-dim)' : 'var(--text-primary)',
            cursor: disabled ? 'not-allowed' : 'pointer',
            fontSize: 12,
            fontWeight: 500,
            display: 'flex',
            alignItems: 'center',
            gap: 5,
            opacity: disabled ? 0.5 : 1,
            transition: 'border-color .15s, background .15s',
          }}
          onMouseEnter={e => { if (!disabled) { e.currentTarget.style.borderColor = 'var(--accent)'; e.currentTarget.style.background = 'rgba(124,58,237,.08)'; }}}
          onMouseLeave={e => { e.currentTarget.style.borderColor = 'var(--border)'; e.currentTarget.style.background = 'transparent'; }}
        >
          <span>{a.icon}</span> {a.label}
        </button>
      ))}
    </div>
  );
}
