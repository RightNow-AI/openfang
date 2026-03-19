'use client';

import { FIRST_HELP_LABELS, FIRST_HELP_DESCRIPTIONS } from '../lib/finance-copy';

const FIRST_HELP_ICONS = {
  cash_flow: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <polyline points="22 7 13.5 15.5 8.5 10.5 2 17" />
      <polyline points="16 7 22 7 22 13" />
    </svg>
  ),
  invoice_queue: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
      <polyline points="14 2 14 8 20 8" />
      <line x1="16" y1="13" x2="8" y2="13" />
      <line x1="16" y1="17" x2="8" y2="17" />
      <polyline points="10 9 9 9 8 9" />
    </svg>
  ),
  expense_review: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="10" />
      <line x1="12" y1="8" x2="12" y2="12" />
      <line x1="12" y1="16" x2="12.01" y2="16" />
    </svg>
  ),
  ad_roi: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <line x1="8" y1="21" x2="16" y2="21" />
      <line x1="12" y1="17" x2="12" y2="21" />
    </svg>
  ),
  profit_margins: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <line x1="12" y1="1" x2="12" y2="23" />
      <path d="M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6" />
    </svg>
  ),
  server_costs: (
    <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <rect x="2" y="2" width="20" height="8" rx="2" ry="2" />
      <rect x="2" y="14" width="20" height="8" rx="2" ry="2" />
      <line x1="6" y1="6" x2="6.01" y2="6" />
      <line x1="6" y1="18" x2="6.01" y2="18" />
    </svg>
  ),
};

export default function FinanceWizardStepFirstHelp({ value, onSelect, onBack, onNext }) {
  return (
    <div data-cy="wizard-step-first-help">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        What should we tackle first?
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 20px', lineHeight: 1.55 }}>
        Pick your top priority. Your dashboard will highlight this first.
      </p>

      <div className="grid grid-2" style={{ gap: 10, marginBottom: 24 }}>
        {Object.entries(FIRST_HELP_LABELS).map(([key, label]) => {
          const selected = value === key;
          return (
            <button
              key={key}
              onClick={() => onSelect(key)}
              style={{
                padding: '14px 12px',
                borderRadius: 10,
                border: `2px solid ${selected ? 'var(--accent)' : 'var(--border)'}`,
                background: selected ? 'rgba(var(--accent-rgb, 255,106,26), 0.08)' : 'var(--bg-elevated)',
                cursor: 'pointer',
                textAlign: 'left',
                display: 'flex',
                flexDirection: 'column',
                gap: 8,
                transition: 'border-color 0.15s, background 0.15s',
              }}
              data-cy={`first-help-${key}`}
            >
              <span style={{ color: selected ? 'var(--accent)' : 'var(--text-dim)' }}>
                {FIRST_HELP_ICONS[key]}
              </span>
              <span style={{ fontSize: 13, fontWeight: 700, color: selected ? 'var(--accent)' : 'var(--text)', lineHeight: 1.2 }}>
                {label}
              </span>
              {FIRST_HELP_DESCRIPTIONS[key] && (
                <span style={{ fontSize: 11, color: 'var(--text-dim)', lineHeight: 1.4 }}>
                  {FIRST_HELP_DESCRIPTIONS[key]}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div style={{ display: 'flex', gap: 10 }}>
        <button className="btn btn-ghost" onClick={onBack} style={{ flex: 1 }}>← Back</button>
        <button
          className="btn btn-primary"
          onClick={onNext}
          disabled={!value}
          style={{ flex: 2, opacity: value ? 1 : 0.5 }}
          data-cy="wizard-next-step5"
        >
          Review →
        </button>
      </div>
    </div>
  );
}
