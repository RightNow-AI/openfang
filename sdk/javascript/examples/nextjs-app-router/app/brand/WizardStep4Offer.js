'use client';

/**
 * WizardStep4Offer — "What do you sell" step.
 *
 * Fields:
 *   core_offer        (textarea, required)
 *   pricing_model     (select, required)
 *   primary_cta       (text, required)
 *   sales_process     (textarea, required)
 *   lead_magnet       (text, optional)
 *   proof_assets      (textarea → stored as string, optional)
 */

const PRICING_MODELS = [
  { value: '',                label: 'How do you charge…' },
  { value: 'one-time',        label: 'One-time payment' },
  { value: 'retainer',        label: 'Monthly retainer' },
  { value: 'subscription',    label: 'Subscription' },
  { value: 'project-based',   label: 'Project-based' },
  { value: 'high-ticket',     label: 'High-ticket / premium' },
  { value: 'custom',          label: 'Custom / negotiated' },
];

const fieldStyle = {
  width: '100%', padding: '8px 10px', borderRadius: 6,
  border: '1px solid var(--border)', background: 'var(--surface2)',
  color: 'var(--text)', fontSize: 13, outline: 'none', boxSizing: 'border-box',
};

const labelStyle = {
  display: 'block', fontSize: 11, fontWeight: 600, color: 'var(--text-dim)',
  marginBottom: 4, textTransform: 'uppercase', letterSpacing: '0.4px',
};

const requiredDot = <span style={{ color: 'var(--accent)', marginLeft: 2 }}>*</span>;

function Field({ label, required, hint, children }) {
  return (
    <div style={{ marginBottom: 16 }}>
      <label style={labelStyle}>{label}{required && requiredDot}</label>
      {children}
      {hint && <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 3 }}>{hint}</div>}
    </div>
  );
}

export default function WizardStep4Offer({ profile, onChange }) {
  function set(field) {
    return (e) => onChange({ [field]: e.target.value });
  }

  return (
    <div>
      <Field label="Main Offer" required hint="What exactly do you sell? Be specific.">
        <textarea
          style={{ ...fieldStyle, minHeight: 80, resize: 'vertical' }}
          value={profile.core_offer}
          onChange={set('core_offer')}
          placeholder="e.g. Done-for-you email funnels for service businesses who want more booked calls without paid ads…"
        />
      </Field>

      <Field label="How Do You Charge" required>
        <select
          style={{ ...fieldStyle, cursor: 'pointer' }}
          value={profile.pricing_model}
          onChange={set('pricing_model')}
        >
          {PRICING_MODELS.map(m => (
            <option key={m.value} value={m.value}>{m.label}</option>
          ))}
        </select>
      </Field>

      <Field label="What Should People Do Next" required hint="The single action you want visitors to take — your primary CTA.">
        <input
          style={fieldStyle}
          value={profile.primary_cta}
          onChange={set('primary_cta')}
          placeholder="e.g. Book a free 30-minute strategy call"
        />
      </Field>

      <Field label="How Does Your Sales Process Work" required hint="Walk us through the steps from first contact to paying client.">
        <textarea
          style={{ ...fieldStyle, minHeight: 80, resize: 'vertical' }}
          value={profile.sales_process}
          onChange={set('sales_process')}
          placeholder="e.g. 1. Book a call → 2. Strategy session → 3. Proposal sent → 4. Onboarding call → 5. Work begins"
        />
      </Field>

      {/* ── Optional fields ── */}
      <div style={{
        marginTop: 8, paddingTop: 16, borderTop: '1px solid var(--border)',
      }}>
        <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.5px', marginBottom: 14 }}>
          Optional — but helpful for email sequences
        </div>

        <Field label="Lead Magnet" hint="Free resource that gets people into your funnel.">
          <input
            style={fieldStyle}
            value={profile.lead_magnet}
            onChange={set('lead_magnet')}
            placeholder="e.g. Free website audit, 5-day email course, resource guide…"
          />
        </Field>

        <Field label="Social Proof You Already Have" hint="Testimonials, case studies, client wins, results — paste or list them.">
          <textarea
            style={{ ...fieldStyle, minHeight: 72, resize: 'vertical' }}
            value={Array.isArray(profile.proof_assets) ? profile.proof_assets.join('\n') : profile.proof_assets || ''}
            onChange={(e) => onChange({ proof_assets: e.target.value.split('\n').filter(Boolean) })}
            placeholder="e.g. Helped 3 clients book 15+ calls/month within 60 days. 4.9★ rating on Google…"
          />
        </Field>
      </div>
    </div>
  );
}
