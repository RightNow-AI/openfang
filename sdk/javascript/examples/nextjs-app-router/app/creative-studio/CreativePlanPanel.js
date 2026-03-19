'use client';
import { useState } from 'react';

const TASK_LABELS = {
  generate_moodboard_directions: 'Build moodboard directions',
  generate_prompt_pack:          'Draft image prompts',
  generate_script_strategy:      'Draft script strategy',
  generate_image_drafts:         'Generate image drafts',
  generate_video_plan:           'Draft video plan',
  generate_voice_drafts:         'Generate voice drafts',
  generate_full_creative_pack:   'Run full creative pack',
};

export default function CreativePlanPanel({ plan, onApprove, onRevise, onLaunchTask }) {
  const [reviseNote, setReviseNote] = useState('');
  const [showRevise, setShowRevise] = useState(false);

  if (!plan) {
    return (
      <div style={{ padding: '56px 0', textAlign: 'center', color: 'var(--text-dim,#888)', fontSize: 14 }}>
        <div style={{ fontSize: 36, marginBottom: 10 }}>📋</div>
        <div>No plan yet. Use the Director tab to build a creative plan.</div>
      </div>
    );
  }

  return (
    <div data-cy="plan-panel" style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      {/* Thesis */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 12, flexWrap: 'wrap' }}>
        <div style={{ flex: 1 }}>
          <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 1, marginBottom: 6 }}>Creative thesis</div>
          <div style={{ fontSize: 16, fontWeight: 600, lineHeight: 1.55 }}>{plan.thesis}</div>
        </div>
        <PlanStatusBadge status={plan.status} />
      </div>

      {/* Sections */}
      {plan.audience_angle && <PlanSection title="Audience angle" content={plan.audience_angle} />}
      {plan.visual_direction?.length > 0 && <PlanSection title="Visual direction" items={plan.visual_direction} />}
      {plan.hook_angles?.length > 0 && <PlanSection title="Hook angles" items={plan.hook_angles} />}
      {plan.prompt_strategy?.length > 0 && <PlanSection title="Prompt strategy" items={plan.prompt_strategy} />}
      {plan.script_strategy?.length > 0 && <PlanSection title="Script strategy" items={plan.script_strategy} />}

      {/* Approval gates */}
      {plan.approval_points?.length > 0 && (
        <div style={{ padding: '12px 16px', borderRadius: 10, background: 'rgba(249,115,22,.08)', border: '1px solid rgba(249,115,22,.3)' }}>
          <div style={{ fontWeight: 700, fontSize: 12, color: '#f97316', marginBottom: 8 }}>⏸ Approval gates required</div>
          {plan.approval_points.map(pt => (
            <div key={pt} style={{ fontSize: 12, color: '#f97316', marginBottom: 3 }}>· {pt.replace(/_/g, ' ')}</div>
          ))}
        </div>
      )}

      {/* Actions */}
      {plan.status !== 'approved' && (
        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          <button
            onClick={onApprove}
            style={{ padding: '9px 22px', borderRadius: 8, background: 'var(--success,#10b981)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 700, fontSize: 14 }}
          >
            ✓ Approve this plan
          </button>
          <button
            onClick={() => setShowRevise(v => !v)}
            style={{ padding: '9px 16px', borderRadius: 8, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13 }}
          >
            Request revision
          </button>
        </div>
      )}

      {showRevise && (
        <div style={{ display: 'flex', gap: 8 }}>
          <input
            value={reviseNote}
            onChange={e => setReviseNote(e.target.value)}
            placeholder="What should change?"
            style={{ flex: 1, padding: '9px 12px', borderRadius: 7, background: 'var(--bg-elevated,#111)', border: '1px solid var(--border,#333)', color: 'var(--text-primary,#f1f1f1)', fontSize: 13, outline: 'none' }}
            onKeyDown={e => e.key === 'Enter' && reviseNote.trim() && (onRevise(reviseNote), setReviseNote(''), setShowRevise(false))}
          />
          <button
            onClick={() => { onRevise(reviseNote); setReviseNote(''); setShowRevise(false); }}
            disabled={!reviseNote.trim()}
            style={{ padding: '9px 16px', borderRadius: 7, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: reviseNote.trim() ? 'pointer' : 'not-allowed', fontWeight: 600, fontSize: 13, opacity: reviseNote.trim() ? 1 : 0.5 }}
          >
            Send
          </button>
        </div>
      )}

      {/* Task launch (only when plan is approved) */}
      {plan.status === 'approved' && (
        <div style={{ borderTop: '1px solid var(--border,#333)', paddingTop: 18 }}>
          <div style={{ fontWeight: 700, fontSize: 13, marginBottom: 10 }}>Launch a task</div>
          <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
            {Object.keys(TASK_LABELS).map(t => (
              <button key={t} onClick={() => onLaunchTask(t)} style={{ padding: '7px 13px', borderRadius: 7, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-primary,#f1f1f1)', cursor: 'pointer', fontSize: 12 }}>
                {TASK_LABELS[t]}
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function PlanSection({ title, content, items }) {
  return (
    <div style={{ padding: '12px 14px', borderRadius: 8, background: 'var(--surface2,#1a1a2e)', border: '1px solid var(--border,#333)' }}>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim,#888)', textTransform: 'uppercase', letterSpacing: 0.5, marginBottom: 6 }}>{title}</div>
      {content && <div style={{ fontSize: 13, lineHeight: 1.55 }}>{content}</div>}
      {items && (
        <ul style={{ margin: 0, padding: '0 0 0 16px' }}>
          {items.map((it, i) => <li key={i} style={{ fontSize: 13, lineHeight: 1.5, marginBottom: 2 }}>{it}</li>)}
        </ul>
      )}
    </div>
  );
}

function PlanStatusBadge({ status }) {
  const MAP = { draft: ['#6b7280', 'Draft'], ready_for_review: ['#f59e0b', 'Ready for review'], approved: ['#10b981', 'Approved'] };
  const [c, label] = MAP[status] ?? ['#6b7280', status ?? 'draft'];
  return <span style={{ fontSize: 11, padding: '3px 10px', borderRadius: 999, background: `${c}22`, color: c, border: `1px solid ${c}44`, flexShrink: 0 }}>{label}</span>;
}
