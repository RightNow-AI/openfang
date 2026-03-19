'use client';

function CurrencyInput({ label, value, onChange, placeholder, hint }) {
  return (
    <div style={{ marginBottom: 16 }}>
      <label style={{ display: 'block', fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)', marginBottom: 6 }}>
        {label}
      </label>
      <div style={{ position: 'relative' }}>
        <span style={{ position: 'absolute', left: 10, top: '50%', transform: 'translateY(-50%)', color: 'var(--text-dim)', fontSize: 13 }}>$</span>
        <input
          type="number"
          min={0}
          value={value ?? ''}
          onChange={(e) => onChange(e.target.value === '' ? null : Number(e.target.value))}
          placeholder={placeholder}
          style={{
            width: '100%',
            padding: '9px 10px 9px 22px',
            borderRadius: 8,
            border: '1px solid var(--border)',
            background: 'var(--bg-elevated)',
            color: 'var(--text)',
            fontSize: 13,
            boxSizing: 'border-box',
            outline: 'none',
          }}
        />
      </div>
      {hint && <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 4 }}>{hint}</div>}
    </div>
  );
}

function Toggle({ label, description, checked, onChange, cy }) {
  return (
    <label
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: 10,
        padding: '9px 0',
        cursor: 'pointer',
        borderBottom: '1px solid var(--border)',
      }}
      data-cy={cy}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        style={{ accentColor: 'var(--accent)', marginTop: 2, flexShrink: 0 }}
      />
      <div>
        <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)' }}>{label}</div>
        {description && <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 2 }}>{description}</div>}
      </div>
    </label>
  );
}

export default function FinanceWizardStepMoneySources({ value, onChange, onBack, onNext }) {
  return (
    <div data-cy="wizard-step-money-sources">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        Where does money come from?
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 24px', lineHeight: 1.55 }}>
        Estimates are fine — you can update these any time.
      </p>

      <CurrencyInput
        label="Approximate monthly revenue"
        value={value.monthlyRevenue}
        onChange={(v) => onChange({ monthlyRevenue: v })}
        placeholder="e.g. 15000"
        hint="Total money coming in each month on average"
      />
      <CurrencyInput
        label="Cash on hand right now"
        value={value.cashOnHand}
        onChange={(v) => onChange({ cashOnHand: v })}
        placeholder="e.g. 42000"
        hint="What is currently in your business bank accounts"
      />

      <div style={{ marginBottom: 20 }}>
        <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
          What do you want to track?
        </div>
        <Toggle
          label="Client invoices"
          description="Track which invoices are overdue and need follow-up"
          checked={value.tracksInvoices}
          onChange={(v) => onChange({ tracksInvoices: v })}
          cy="toggle-invoices"
        />
        <Toggle
          label="Subscriptions and recurring revenue"
          description="Track subscription renewals, churn, and recurring income"
          checked={value.tracksSubscriptions}
          onChange={(v) => onChange({ tracksSubscriptions: v })}
          cy="toggle-subscriptions"
        />
      </div>

      <div style={{ display: 'flex', gap: 10 }}>
        <button className="btn btn-ghost" onClick={onBack} style={{ flex: 1 }}>← Back</button>
        <button className="btn btn-primary" onClick={onNext} style={{ flex: 2 }} data-cy="wizard-next-step2">
          Continue →
        </button>
      </div>
    </div>
  );
}
