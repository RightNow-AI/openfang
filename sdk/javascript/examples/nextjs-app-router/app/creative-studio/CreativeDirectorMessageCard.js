'use client';

const QUICK_ACTION_LABELS = {
  generate_moodboard_directions: 'Build moodboard directions',
  generate_prompt_pack:          'Draft image prompts',
  generate_script_strategy:      'Draft script strategy',
  generate_image_drafts:         'Generate image drafts',
  generate_video_plan:           'Draft video plan',
  generate_voice_drafts:         'Generate voice drafts',
  generate_full_creative_pack:   'Run full creative pack',
};

function StructuredBlock({ title, content, items, color, highlight }) {
  return (
    <div style={{ padding: '8px 12px', borderRadius: 8, background: highlight ? 'rgba(124,58,237,.12)' : 'rgba(255,255,255,.04)', border: `1px solid ${highlight ? 'rgba(124,58,237,.3)' : 'rgba(255,255,255,.06)'}` }}>
      <div style={{ fontSize: 10, fontWeight: 700, color: color ?? 'var(--text-dim,#888)', letterSpacing: 0.5, marginBottom: 4, textTransform: 'uppercase' }}>{title}</div>
      {content && <div style={{ fontSize: 13, lineHeight: 1.55 }}>{content}</div>}
      {items && (
        <ul style={{ margin: 0, padding: '0 0 0 16px', listStyle: 'disc' }}>
          {items.map((item, i) => (
            <li key={i} style={{ fontSize: 13, lineHeight: 1.5, color: color ?? 'inherit' }}>{item}</li>
          ))}
        </ul>
      )}
    </div>
  );
}

export default function CreativeDirectorMessageCard({ message, onUseNextAction }) {
  const isUser   = message.role === 'user';
  const isSystem = message.role === 'system';
  const s = message.structured;

  return (
    <div
      data-cy="director-message"
      style={{
        alignSelf: isUser ? 'flex-end' : 'flex-start',
        maxWidth: '82%',
        padding: '12px 16px',
        borderRadius: isUser ? '12px 12px 4px 12px' : '12px 12px 12px 4px',
        background: isUser ? 'var(--accent,#7c3aed)' : isSystem ? 'rgba(239,68,68,.1)' : 'var(--surface2,#1a1a2e)',
        border: isUser ? 'none' : `1px solid ${isSystem ? 'rgba(239,68,68,.3)' : 'var(--border,#333)'}`,
        color: isUser ? '#fff' : 'var(--text-primary,#f1f1f1)',
        display: 'flex',
        flexDirection: 'column',
        gap: 6,
      }}
    >
      {!isUser && !isSystem && (
        <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--accent,#7c3aed)', letterSpacing: 1, textTransform: 'uppercase' }}>Creative Director</div>
      )}
      <div style={{ fontSize: 14, lineHeight: 1.6, whiteSpace: 'pre-wrap' }}>{message.text}</div>

      {s && (
        <div style={{ marginTop: 8, display: 'flex', flexDirection: 'column', gap: 8 }}>
          {s.creative_read       && <StructuredBlock title="Creative read"           content={s.creative_read} />}
          {s.strengths?.length > 0 && <StructuredBlock title="Strengths"            items={s.strengths}            color="var(--success,#10b981)" />}
          {s.weaknesses?.length > 0 && <StructuredBlock title="To tighten"          items={s.weaknesses}           color="var(--warning,#f59e0b)" />}
          {s.recommended_direction   && <StructuredBlock title="Recommended direction" content={s.recommended_direction} highlight />}
          {s.visual_direction?.length > 0 && <StructuredBlock title="Visual direction" items={s.visual_direction} />}
          {s.test_angles?.length > 0 && <StructuredBlock title="Test angles"        items={s.test_angles} />}
          {s.tool_plan?.length > 0   && <StructuredBlock title="Tool plan"           items={s.tool_plan} />}
          {s.approval_points?.length > 0 && <StructuredBlock title="Approval gates" items={s.approval_points} color="var(--warning,#f59e0b)" />}
        </div>
      )}

      {s?.next_action && onUseNextAction && (
        <div style={{ marginTop: 8, paddingTop: 10, borderTop: '1px solid rgba(255,255,255,.08)' }}>
          <button
            data-cy="director-next-action"
            onClick={() => onUseNextAction(s.next_action)}
            style={{ padding: '6px 14px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontSize: 12, fontWeight: 700 }}
          >
            → {QUICK_ACTION_LABELS[s.next_action] ?? s.next_action}
          </button>
        </div>
      )}

      <div style={{ fontSize: 10, color: isUser ? 'rgba(255,255,255,.5)' : 'var(--text-dim,#888)', textAlign: isUser ? 'right' : 'left' }}>
        {message.created_at ? new Date(message.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : ''}
      </div>
    </div>
  );
}
