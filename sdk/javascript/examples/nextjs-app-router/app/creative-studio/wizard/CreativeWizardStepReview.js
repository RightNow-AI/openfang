'use client';
import { getChoiceById } from '../config/ai-choice-catalog';
import { creationTypeLabel } from '../lib/creative-ui';

function PlanStep({ step }) {
  return (
    <div style={{
      display: 'flex',
      alignItems: 'flex-start',
      gap: 12,
      padding: '10px 14px',
      borderRadius: 'var(--radius-sm)',
      background: 'var(--surface2)',
      border: '1px solid var(--border-subtle)',
    }}>
      <span style={{ fontSize: 18, flexShrink: 0 }}>{step.requires_approval ? '🔒' : '▶️'}</span>
      <div>
        <div style={{ fontWeight: 600, fontSize: 13 }}>{step.label}</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>{step.description}</div>
      </div>
    </div>
  );
}

export default function CreativeWizardStepReview({ state, planSteps }) {
  const choiceLabels = Object.entries(state.ai_choices)
    .map(([cat, id]) => {
      const c = getChoiceById(id);
      return c ? `${cap(cat)}: ${c.label}` : null;
    })
    .filter(Boolean);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 24 }}>
      <div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 12 }}>
          Review the plan — here is what will happen
        </div>

        {/* Summary card */}
        <div style={{
          padding: '16px 18px',
          borderRadius: 'var(--radius)',
          border: '1px solid var(--border-light)',
          background: 'var(--bg-elevated)',
          display: 'flex',
          flexDirection: 'column',
          gap: 10,
          marginBottom: 20,
        }}>
          <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <Chip label="Creating" value={creationTypeLabel(state.creation_type)} />
            {state.topic && <Chip label="Topic" value={state.topic} />}
            {state.platform && <Chip label="Platform" value={state.platform} />}
          </div>
          {choiceLabels.length > 0 && (
            <div style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
              <strong>AI tools:</strong> {choiceLabels.join(' · ')}
            </div>
          )}
        </div>

        {/* Steps */}
        <div style={{ fontWeight: 600, fontSize: 13, color: 'var(--text-secondary)', marginBottom: 8 }}>
          Steps that will run
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {planSteps.map(step => (
            <PlanStep key={step.id} step={step} />
          ))}
        </div>
      </div>

      {/* Approval note */}
      <div style={{
        padding: '12px 16px',
        borderRadius: 'var(--radius)',
        background: 'var(--warning-subtle)',
        border: '1px solid var(--warning-muted)',
        fontSize: 13,
        color: 'var(--warning-dim)',
        lineHeight: 1.5,
      }}>
        🔒 Steps marked with a lock <strong>pause and wait for your approval</strong> before running.
        Nothing expensive happens without you saying ok.
      </div>
    </div>
  );
}

function Chip({ label, value }) {
  return (
    <span style={{ fontSize: 12, display: 'flex', gap: 4 }}>
      <span style={{ color: 'var(--text-muted)' }}>{label}:</span>
      <span style={{ fontWeight: 600, color: 'var(--text)' }}>{value}</span>
    </span>
  );
}

function cap(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
