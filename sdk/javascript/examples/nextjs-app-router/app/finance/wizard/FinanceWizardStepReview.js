'use client';

function ReviewRow({ label, value, missing }) {
  return (
    <div style={{
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'flex-start',
      padding: '7px 0',
      borderBottom: '1px solid var(--border)',
      gap: 12,
    }}>
      <span style={{ fontSize: 12, color: 'var(--text-dim)', flexShrink: 0 }}>{label}</span>
      <span style={{
        fontSize: 12,
        fontWeight: 600,
        color: missing ? 'var(--text-warn, #e5a00d)' : 'var(--text)',
        textAlign: 'right',
      }}>
        {value || <em style={{ fontWeight: 400, opacity: 0.6 }}>not set</em>}
      </span>
    </div>
  );
}

function ReviewChips({ label, items }) {
  return (
    <div style={{ padding: '7px 0', borderBottom: '1px solid var(--border)' }}>
      <div style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 6 }}>{label}</div>
      {items && items.length > 0 ? (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
          {items.map((item, i) => (
            <span key={i} className="badge badge-dim" style={{ fontSize: 11 }}>{item}</span>
          ))}
        </div>
      ) : (
        <span style={{ fontSize: 12, color: 'var(--text-warn, #e5a00d)' }}>None selected</span>
      )}
    </div>
  );
}

export default function FinanceWizardStepReview({ summary, creating, onBack, onCreate }) {
  const {
    businessModeLabel,
    goalLabel,
    monthlyRevenue,
    monthlyExpenses,
    trackersEnabled = [],
    approvalLabels = [],
    firstHelpLabel,
  } = summary || {};

  return (
    <div data-cy="wizard-step-review">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        Review your finance setup
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 20px', lineHeight: 1.55 }}>
        Everything looks right? Hit create to activate your finance layer.
      </p>

      <div style={{ marginBottom: 24 }}>
        <ReviewRow label="Business mode" value={businessModeLabel} />
        <ReviewRow label="Main goal" value={goalLabel} />
        <ReviewRow
          label="Monthly revenue"
          value={monthlyRevenue != null ? `$${monthlyRevenue.toLocaleString()}` : null}
        />
        <ReviewRow
          label="Monthly expenses"
          value={monthlyExpenses != null ? `$${monthlyExpenses.toLocaleString()}` : null}
        />
        <ReviewChips label="Trackers enabled" items={trackersEnabled} />
        <ReviewChips label="Approval rules" items={approvalLabels} />
        <ReviewRow label="Focus area" value={firstHelpLabel} />
      </div>

      <div
        style={{
          padding: '10px 12px',
          borderRadius: 8,
          background: 'rgba(var(--accent-rgb, 255,106,26), 0.08)',
          border: '1px solid rgba(var(--accent-rgb, 255,106,26), 0.2)',
          fontSize: 12,
          color: 'var(--text-dim)',
          marginBottom: 20,
          lineHeight: 1.55,
        }}
      >
        Your finance layer activates immediately. You can edit any of this from the Finance settings page.
      </div>

      <div style={{ display: 'flex', gap: 10 }}>
        <button className="btn btn-ghost" onClick={onBack} disabled={creating} style={{ flex: 1 }}>
          ← Back
        </button>
        <button
          className="btn btn-primary"
          onClick={onCreate}
          disabled={creating}
          style={{ flex: 2 }}
          data-cy="wizard-create"
        >
          {creating ? (
            <span style={{ display: 'flex', alignItems: 'center', gap: 6, justifyContent: 'center' }}>
              <span className="spinner" style={{ width: 14, height: 14 }} />
              Creating…
            </span>
          ) : (
            'Create my finance setup'
          )}
        </button>
      </div>
    </div>
  );
}
