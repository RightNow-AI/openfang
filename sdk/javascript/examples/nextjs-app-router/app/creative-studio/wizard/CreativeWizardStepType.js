'use client';

const CREATION_OPTIONS = [
  {
    id: 'image',
    label: 'Images',
    icon: '🖼️',
    tagline: 'Generate ad visuals, product shots, or brand graphics',
  },
  {
    id: 'video',
    label: 'Videos',
    icon: '🎬',
    tagline: 'Create short-form video clips with script, voice, and visuals',
  },
  {
    id: 'image+video',
    label: 'Images + Videos',
    icon: '✨',
    tagline: 'Full creative pipeline: from brief to finished assets',
  },
];

const GOAL_OPTIONS = [
  { id: 'ad',           label: 'Ad creative' },
  { id: 'social',       label: 'Social content' },
  { id: 'product-promo',label: 'Product promo' },
  { id: 'explainer',    label: 'Explainer' },
  { id: 'lesson',       label: 'Lesson content' },
  { id: 'brand',        label: 'Brand visuals' },
  { id: 'other',        label: 'Something else' },
];

export default function CreativeWizardStepType({ state, onChange }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 12 }}>
          What do you want to create?
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(200px, 1fr))', gap: 12 }}>
          {CREATION_OPTIONS.map(opt => (
            <button
              key={opt.id}
              type="button"
              data-cy={`creation-type-${opt.id}`}
              onClick={() => onChange('creation_type', opt.id)}
              style={{
                textAlign: 'left',
                padding: '16px 18px',
                borderRadius: 'var(--radius)',
                border: state.creation_type === opt.id
                  ? '2px solid var(--accent)'
                  : '2px solid var(--border)',
                background: state.creation_type === opt.id ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
                cursor: 'pointer',
                transition: 'border-color 0.15s, background 0.15s',
              }}
            >
              <div style={{ fontSize: 28, marginBottom: 6 }}>{opt.icon}</div>
              <div style={{ fontWeight: 700, fontSize: 14, color: 'var(--text)' }}>{opt.label}</div>
              <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 4, lineHeight: 1.4 }}>{opt.tagline}</div>
            </button>
          ))}
        </div>
      </div>

      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 12 }}>
          What is the goal?
        </div>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8 }}>
          {GOAL_OPTIONS.map(opt => (
            <button
              key={opt.id}
              type="button"
              data-cy={`goal-${opt.id}`}
              onClick={() => onChange('goal', opt.id)}
              style={{
                padding: '8px 16px',
                borderRadius: 999,
                border: state.goal === opt.id
                  ? '2px solid var(--accent)'
                  : '2px solid var(--border)',
                background: state.goal === opt.id ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
                cursor: 'pointer',
                fontSize: 13,
                fontWeight: 500,
                color: 'var(--text)',
                transition: 'border-color 0.15s, background 0.15s',
              }}
            >
              {opt.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
