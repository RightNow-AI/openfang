'use client';

function Field({ label, hint, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
      <label style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-secondary)' }}>
        {label}
        {hint && <span style={{ fontWeight: 400, color: 'var(--text-muted)', marginLeft: 6 }}>{hint}</span>}
      </label>
      {children}
    </div>
  );
}

const inputStyle = {
  padding: '9px 12px',
  borderRadius: 'var(--radius-sm)',
  border: '1px solid var(--border)',
  background: 'var(--bg-elevated)',
  color: 'var(--text)',
  fontSize: 13,
  outline: 'none',
  width: '100%',
};

export default function CreativeWizardStepBrief({ state, onChange }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4 }}>
        Tell us what it is about
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
        <Field label="Project name" hint="(optional)">
          <input
            data-cy="brief-name"
            style={inputStyle}
            placeholder="e.g. Summer Sale Campaign"
            value={state.name}
            onChange={e => onChange('name', e.target.value)}
          />
        </Field>

        <Field label="Platform" hint="(optional)">
          <input
            data-cy="brief-platform"
            style={inputStyle}
            placeholder="e.g. Instagram, TikTok, YouTube"
            value={state.platform}
            onChange={e => onChange('platform', e.target.value)}
          />
        </Field>
      </div>

      <Field label="Topic *" hint="What is this about?">
        <input
          data-cy="brief-topic"
          style={inputStyle}
          placeholder="e.g. Our new protein powder for busy moms"
          value={state.topic}
          onChange={e => onChange('topic', e.target.value)}
        />
      </Field>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
        <Field label="Offer or product" hint="(optional)">
          <input
            data-cy="brief-offer"
            style={inputStyle}
            placeholder="e.g. 30% off launch deal, Premium plan"
            value={state.offer}
            onChange={e => onChange('offer', e.target.value)}
          />
        </Field>

        <Field label="Who is this for?" hint="(optional)">
          <input
            data-cy="brief-audience"
            style={inputStyle}
            placeholder="e.g. Women 25-40, small business owners"
            value={state.audience}
            onChange={e => onChange('audience', e.target.value)}
          />
        </Field>
      </div>

      <Field label="What should happen after someone sees this?" hint="(optional)">
        <input
          data-cy="brief-outcome"
          style={inputStyle}
          placeholder="e.g. Click to buy, Follow the account, Sign up"
          value={state.desired_outcome}
          onChange={e => onChange('desired_outcome', e.target.value)}
        />
      </Field>

      <Field label="Anything else we should know?" hint="(optional)">
        <textarea
          data-cy="brief-notes"
          rows={3}
          style={{ ...inputStyle, resize: 'vertical' }}
          placeholder="Tone, competitors to avoid, legal restrictions, etc."
          value={state.notes}
          onChange={e => onChange('notes', e.target.value)}
        />
      </Field>
    </div>
  );
}
