'use client';

import { APPROVAL_RULE_LABELS, APPROVAL_RULE_DESCRIPTIONS } from '../lib/investments-copy';

const RULES = ['alert_before_trade', 'confirm_new_position', 'confirm_exit', 'weekly_review', 'thesis_update'];

export default function InvestmentsWizardStepApprovals({ value, onToggle, onBack, onNext }) {
  const noApproval = value && value.includes('none');

  function toggleNone() {
    if (noApproval) {
      onToggle('none');
    } else {
      RULES.forEach((r) => { if ((value || []).includes(r)) onToggle(r); });
      onToggle('none');
    }
  }

  return (
    <div>
      <h3 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
        When should we pause and wait for you?
      </h3>
      <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 12 }}>
        Approval means: the system prepares the idea — you review it — then you decide whether it moves forward. Nothing acts on your behalf without a green light.
      </p>

      <div
        style={{
          background: 'rgba(255,106,26,0.06)',
          border: '1px solid rgba(255,106,26,0.2)',
          borderRadius: 8,
          padding: '10px 14px',
          marginBottom: 16,
          fontSize: 12,
          color: 'var(--text-dim)',
        }}
      >
        <strong style={{ color: 'var(--accent)' }}>How approvals work:</strong> The system prepares the idea. You review it. Then you decide whether it moves forward. This keeps you in control without slowing you down too much.
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 8, marginBottom: 12 }}>
        {RULES.map((rule) => {
          const checked = !noApproval && value && value.includes(rule);
          return (
            <label
              key={rule}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 10,
                cursor: noApproval ? 'not-allowed' : 'pointer',
                padding: '9px 12px',
                borderRadius: 8,
                border: `1.5px solid ${checked ? 'var(--accent)' : 'var(--border)'}`,
                background: checked ? 'rgba(255,106,26,0.05)' : 'var(--surface)',
                opacity: noApproval ? 0.45 : 1,
              }}
            >
              <input
                type="checkbox"
                checked={!!checked}
                disabled={noApproval}
                onChange={() => onToggle(rule)}
                style={{ marginTop: 2, accentColor: 'var(--accent)', flexShrink: 0 }}
              />
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{APPROVAL_RULE_LABELS[rule]}</div>
                <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 1 }}>{APPROVAL_RULE_DESCRIPTIONS[rule]}</div>
              </div>
            </label>
          );
        })}
      </div>

      <label style={{
        display: 'flex',
        alignItems: 'center',
        gap: 10,
        cursor: 'pointer',
        padding: '9px 12px',
        borderRadius: 8,
        border: `1.5px solid ${noApproval ? '#94a3b8' : 'var(--border)'}`,
        background: noApproval ? 'rgba(148,163,184,0.07)' : 'var(--surface)',
        marginBottom: 24,
      }}>
        <input
          type="checkbox"
          checked={!!noApproval}
          onChange={toggleNone}
          style={{ accentColor: '#94a3b8', flexShrink: 0 }}
        />
        <div>
          <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>No approval needed</div>
          <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 1 }}>
            The agent acts on signals directly. You can still review everything in the alerts tab.
          </div>
        </div>
      </label>

      <div style={{ display: 'flex', justifyContent: 'space-between' }}>
        <button className="btn btn-ghost" onClick={onBack}>← Back</button>
        <button className="btn btn-primary" onClick={onNext}>Next →</button>
      </div>
    </div>
  );
}
