'use client';

import { BUSINESS_MODE_LABELS, BUSINESS_MODE_DESCRIPTIONS, GOAL_LABELS } from '../lib/finance-copy';

const MODES = ['agency', 'growth', 'school', 'mixed'];
const GOALS = Object.keys(GOAL_LABELS);

export default function FinanceWizardStepBusiness({ value, onChange, onNext }) {
  const { businessMode, mainGoal } = value;
  const canNext = Boolean(businessMode && mainGoal);

  return (
    <div data-cy="wizard-step-business">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        Tell us about your business
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 24px', lineHeight: 1.55 }}>
        We use this to choose the right starting setup and surface what matters most.
      </p>

      {/* Business mode */}
      <div style={{ marginBottom: 22 }}>
        <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
          What best describes your business?
        </div>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 8 }}>
          {MODES.map((mode) => (
            <button
              key={mode}
              onClick={() => onChange({ businessMode: mode })}
              data-cy={`mode-${mode}`}
              style={{
                padding: '11px 14px',
                borderRadius: 9,
                border: `2px solid ${businessMode === mode ? 'var(--accent)' : 'var(--border)'}`,
                background: businessMode === mode ? 'var(--accent-subtle)' : 'var(--bg-elevated)',
                color: businessMode === mode ? 'var(--accent)' : 'var(--text)',
                cursor: 'pointer',
                textAlign: 'left',
              }}
            >
              <div style={{ fontWeight: 700, fontSize: 13 }}>{BUSINESS_MODE_LABELS[mode]}</div>
              <div style={{ fontSize: 11, color: businessMode === mode ? 'var(--accent)' : 'var(--text-dim)', marginTop: 3, lineHeight: 1.4 }}>
                {BUSINESS_MODE_DESCRIPTIONS[mode]}
              </div>
            </button>
          ))}
        </div>
      </div>

      {/* Main goal */}
      <div style={{ marginBottom: 24 }}>
        <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
          What is your main finance goal right now?
        </div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
          {GOALS.map((goal) => (
            <label
              key={goal}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                padding: '9px 12px',
                borderRadius: 8,
                border: `1px solid ${mainGoal === goal ? 'var(--accent)' : 'var(--border)'}`,
                background: mainGoal === goal ? 'var(--accent-subtle)' : 'transparent',
                cursor: 'pointer',
              }}
              data-cy={`goal-${goal}`}
            >
              <input
                type="radio"
                checked={mainGoal === goal}
                onChange={() => onChange({ mainGoal: goal })}
                style={{ accentColor: 'var(--accent)' }}
              />
              <span style={{ fontSize: 13, color: mainGoal === goal ? 'var(--accent)' : 'var(--text-secondary)', fontWeight: mainGoal === goal ? 600 : 400 }}>
                {GOAL_LABELS[goal]}
              </span>
            </label>
          ))}
        </div>
      </div>

      <button
        className="btn btn-primary"
        onClick={onNext}
        disabled={!canNext}
        style={{ width: '100%' }}
        data-cy="wizard-next-step1"
      >
        Continue →
      </button>
    </div>
  );
}
