'use client';

import { SIGNAL_LABELS, SIGNAL_DESCRIPTIONS } from '../lib/investments-copy';

const SIGNALS = ['news', 'earnings', 'filings', 'price', 'volume', 'macro', 'sector'];

export default function InvestmentsWizardStepSignals({ value, onToggle, onBack, onNext }) {
  const canNext = value && value.length > 0;

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        What should we watch for?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 20 }}>
        Pick the signals that matter to your strategy. When these fire, you get notified — nothing moves until you say so.
      </p>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 24 }}>
        {SIGNALS.map((sig) => {
          const checked = value && value.includes(sig);
          return (
            <label
              key={sig}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 10,
                cursor: 'pointer',
                padding: '9px 12px',
                borderRadius: 8,
                border: `1.5px solid ${checked ? 'var(--accent)' : 'var(--border)'}`,
                background: checked ? 'rgba(255,106,26,0.05)' : 'var(--surface)',
                transition: 'border-color 0.15s',
              }}
            >
              <input
                type="checkbox"
                checked={!!checked}
                onChange={() => onToggle(sig)}
                style={{ marginTop: 2, accentColor: 'var(--accent)', flexShrink: 0 }}
              />
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{SIGNAL_LABELS[sig]}</div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 1 }}>{SIGNAL_DESCRIPTIONS[sig]}</div>
              </div>
            </label>
          );
        })}
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between' }}>
        <button className="btn btn-ghost" onClick={onBack}>← Back</button>
        <button className="btn btn-primary" onClick={onNext} disabled={!canNext}>Next →</button>
      </div>
    </div>
  );
}
