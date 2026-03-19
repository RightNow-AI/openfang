'use client';

import { APPROVAL_RULE_LABELS } from '../lib/finance-copy';

const APPROVAL_RULE_HINTS = {
  large_expense: 'Any single expense above your threshold needs a sign-off',
  payroll_run: 'Require approval before issuing payroll',
  new_vendor: 'New vendor or contractor relationships need approval',
  ad_spend_increase: 'Flag when ad spend is about to increase significantly',
  refund_over_threshold: 'Client refunds above a set amount need approval',
  invoice_write_off: 'Writing off an unpaid invoice requires approval',
  subscription_cancel: 'Cancelling a paid subscription needs a sign-off',
  budget_reallocation: 'Moving budget between categories needs approval',
};

export default function FinanceWizardStepApprovals({ value, onToggleRule, onBack, onNext }) {
  const rules = value.approvalRules || [];

  return (
    <div data-cy="wizard-step-approvals">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        What needs your approval?
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 20px', lineHeight: 1.55 }}>
        These actions will show up in your Approval Queue and won&apos;t proceed without a sign-off.
        You can adjust this any time.
      </p>

      <div style={{ marginBottom: 24 }}>
        {Object.entries(APPROVAL_RULE_LABELS).map(([key, label]) => {
          const checked = rules.includes(key);
          return (
            <label
              key={key}
              style={{
                display: 'flex',
                alignItems: 'flex-start',
                gap: 10,
                padding: '9px 0',
                cursor: 'pointer',
                borderBottom: '1px solid var(--border)',
                background: checked ? 'rgba(var(--accent-rgb, 255,106,26), 0.04)' : 'transparent',
              }}
            >
              <input
                type="checkbox"
                checked={checked}
                onChange={() => onToggleRule(key)}
                style={{ accentColor: 'var(--accent)', marginTop: 2, flexShrink: 0 }}
              />
              <div>
                <div style={{ fontSize: 13, fontWeight: checked ? 700 : 600, color: 'var(--text)' }}>{label}</div>
                {APPROVAL_RULE_HINTS[key] && (
                  <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>
                    {APPROVAL_RULE_HINTS[key]}
                  </div>
                )}
              </div>
            </label>
          );
        })}
      </div>

      {rules.length === 0 && (
        <p style={{ fontSize: 12, color: 'var(--text-warn, #e5a00d)', marginBottom: 16 }}>
          No approval rules selected — your finance layer will run without guardrails.
        </p>
      )}

      <div style={{ display: 'flex', gap: 10 }}>
        <button className="btn btn-ghost" onClick={onBack} style={{ flex: 1 }}>← Back</button>
        <button className="btn btn-primary" onClick={onNext} style={{ flex: 2 }} data-cy="wizard-next-step4">
          Continue →
        </button>
      </div>
    </div>
  );
}
