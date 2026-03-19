'use client';

const APPROVAL_LABELS = {
  before_image_generation: 'Before image generation',
  before_video_generation:  'Before video generation',
  before_voice_generation:  'Before voice generation',
  before_external_tool_use: 'Before external tool use',
  before_publish_or_send:   'Before publish / send',
};

export default function CreativeApprovalChecklist({ approvalTypes, approved, onApproveType }) {
  if (!approvalTypes?.length) return null;

  return (
    <div data-cy="approval-checklist">
      <div style={{ fontWeight: 700, fontSize: 11, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 10 }}>Approval gates</div>
      {approvalTypes.map(type => {
        const done = approved?.includes(type);
        return (
          <div key={type} style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 9 }}>
            <div
              onClick={() => !done && onApproveType(type)}
              style={{ width: 18, height: 18, borderRadius: 4, border: `1.5px solid ${done ? 'var(--success,#10b981)' : 'var(--border,#555)'}`, background: done ? 'var(--success,#10b981)' : 'transparent', display: 'flex', alignItems: 'center', justifyContent: 'center', flexShrink: 0, cursor: done ? 'default' : 'pointer', transition: 'background .15s, border-color .15s' }}
            >
              {done && <span style={{ color: '#fff', fontSize: 11, lineHeight: 1 }}>✓</span>}
            </div>
            <div style={{ fontSize: 12, color: done ? 'var(--text-dim,#888)' : 'var(--text-primary,#f1f1f1)', textDecoration: done ? 'line-through' : 'none' }}>
              {APPROVAL_LABELS[type] ?? type.replace(/_/g, ' ')}
            </div>
          </div>
        );
      })}
    </div>
  );
}
