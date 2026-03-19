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

export default function FinanceWizardStepCosts({ value, onChange, onBack, onNext }) {
  return (
    <div data-cy="wizard-step-costs">
      <h3 style={{ fontSize: 16, fontWeight: 700, color: 'var(--text)', margin: '0 0 6px' }}>
        What do you spend money on?
      </h3>
      <p style={{ fontSize: 13, color: 'var(--text-dim)', margin: '0 0 24px', lineHeight: 1.55 }}>
        Estimates are fine — this helps us flag the right alerts for you.
      </p>

      <CurrencyInput
        label="Approximate monthly expenses"
        value={value.monthlyExpenses}
        onChange={(v) => onChange({ monthlyExpenses: v })}
        placeholder="e.g. 9500"
        hint="Total money going out each month on average"
      />

      <div style={{ marginBottom: 20 }}>
        <div style={{ fontSize: 12, fontWeight: 700, color: 'var(--text-secondary)', textTransform: 'uppercase', letterSpacing: '0.06em', marginBottom: 10 }}>
          Which cost categories should we watch?
        </div>
        <Toggle
          label="Payroll & contractors"
          description="Get alerts when payroll is approaching or overdue"
          checked={value.tracksPayroll}
          onChange={(v) => onChange({ tracksPayroll: v })}
          cy="toggle-payroll"
        />
        <Toggle
          label="Ad spend"
          description="Monitor marketing budgets and flag overspend"
          checked={value.tracksAdSpend}
          onChange={(v) => onChange({ tracksAdSpend: v })}
          cy="toggle-ad-spend"
        />
        <Toggle
          label="Server & infrastructure costs"
          description="Track cloud hosting, VPS, and infrastructure bills"
          checked={value.tracksServerCosts}
          onChange={(v) => onChange({ tracksServerCosts: v })}
          cy="toggle-server-costs"
        />
        <Toggle
          label="API & AI service costs"
          description="Track LLM tokens, API calls, and external service fees"
          checked={value.tracksApiCosts}
          onChange={(v) => onChange({ tracksApiCosts: v })}
          cy="toggle-api-costs"
        />
      </div>

      <div style={{ display: 'flex', gap: 10 }}>
        <button className="btn btn-ghost" onClick={onBack} style={{ flex: 1 }}>← Back</button>
        <button className="btn btn-primary" onClick={onNext} style={{ flex: 2 }} data-cy="wizard-next-step3">
          Continue →
        </button>
      </div>
    </div>
  );
}
