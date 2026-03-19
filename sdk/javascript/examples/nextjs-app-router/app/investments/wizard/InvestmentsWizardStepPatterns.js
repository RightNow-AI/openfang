'use client';

import { PATTERN_LABELS, PATTERN_DESCRIPTIONS } from '../lib/investments-copy';

const PATTERNS = ['momentum', 'mean_reversion', 'earnings_reaction', 'breakout', 'sector_rotation', 'valuation_band', 'unusual_volume'];
const BASICS = ['momentum', 'earnings_reaction', 'unusual_volume'];

export default function InvestmentsWizardStepPatterns({ value, onToggle, onBack, onNext }) {
  function chooseBasics(e) {
    e.preventDefault();
    BASICS.forEach((p) => {
      if (!(value || []).includes(p)) onToggle(p);
    });
  }

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        What patterns should we flag?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 12 }}>
        These are market behaviors we look for on your behalf. You review before anything happens.
      </p>

      <button
        className="btn btn-ghost btn-sm"
        style={{ marginBottom: 16 }}
        onClick={chooseBasics}
      >
        Choose the basics for me
      </button>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 24 }}>
        {PATTERNS.map((pat) => {
          const checked = value && value.includes(pat);
          return (
            <label
              key={pat}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 10,
                cursor: 'pointer',
                padding: '9px 12px',
                borderRadius: 8,
                border: `1.5px solid ${checked ? 'var(--accent)' : 'var(--border)'}`,
                background: checked ? 'rgba(255,106,26,0.05)' : 'var(--surface)',
              }}
            >
              <input
                type="checkbox"
                checked={!!checked}
                onChange={() => onToggle(pat)}
                style={{ marginTop: 2, accentColor: 'var(--accent)', flexShrink: 0 }}
              />
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{PATTERN_LABELS[pat]}</div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 1 }}>{PATTERN_DESCRIPTIONS[pat]}</div>
              </div>
            </label>
          );
        })}
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between' }}>
        <button className="btn btn-ghost" onClick={onBack}>← Back</button>
        <button className="btn btn-primary" onClick={onNext}>Next →</button>
      </div>
    </div>
  );
}
