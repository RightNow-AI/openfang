'use client';

/**
 * WizardProgress — 6-step top progress bar for the Brand Wizard.
 * Completed steps show a check. Current step is highlighted. Future steps are dim.
 * Clicking a completed step navigates back (caller handles guard).
 */
export default function WizardProgress({ steps, currentStep, onStepClick }) {
  return (
    <div style={{
      display: 'flex', alignItems: 'center', justifyContent: 'center',
      gap: 0, paddingBottom: 0, overflowX: 'auto',
    }}>
      {steps.map((step, i) => {
        const done    = i < currentStep;
        const active  = i === currentStep;
        const future  = i > currentStep;

        return (
          <div key={step.key} style={{ display: 'flex', alignItems: 'center' }}>
            {/* Step pill */}
            <button
              onClick={() => onStepClick?.(i)}
              disabled={future}
              style={{
                display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4,
                padding: '8px 12px 10px',
                background: 'none', border: 'none', cursor: done ? 'pointer' : 'default',
                borderBottom: active ? '2px solid var(--accent)' : '2px solid transparent',
                transition: 'border-color 0.15s',
                minWidth: 80,
              }}
            >
              {/* Circle */}
              <div style={{
                width: 22, height: 22, borderRadius: '50%',
                display: 'flex', alignItems: 'center', justifyContent: 'center',
                fontSize: 11, fontWeight: 700,
                background: done
                  ? 'var(--success)'
                  : active
                    ? 'var(--accent)'
                    : 'var(--surface3)',
                color: done || active ? '#fff' : 'var(--text-muted)',
                flexShrink: 0,
                transition: 'background 0.2s',
              }}>
                {done ? '✓' : i + 1}
              </div>
              {/* Label */}
              <span style={{
                fontSize: 11, fontWeight: active ? 700 : 500, whiteSpace: 'nowrap',
                color: done
                  ? 'var(--text-dim)'
                  : active
                    ? 'var(--accent)'
                    : 'var(--text-muted)',
              }}>
                {step.label}
              </span>
            </button>

            {/* Connector line between steps */}
            {i < steps.length - 1 && (
              <div style={{
                width: 24, height: 1,
                background: i < currentStep ? 'var(--success)' : 'var(--border)',
                flexShrink: 0, marginBottom: 12,
                transition: 'background 0.3s',
              }} />
            )}
          </div>
        );
      })}
    </div>
  );
}
