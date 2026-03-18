'use client';

import { useState } from 'react';

const BUSINESS_MODELS = [
  'service', 'agency', 'consulting', 'ecommerce', 'saas',
  'info-product', 'local-business', 'creator-business', 'other',
];

// ── Shared field styles ────────────────────────────────────────────────────

const fieldStyle = {
  width: '100%',
  padding: '7px 10px',
  borderRadius: 6,
  border: '1px solid var(--border)',
  background: 'var(--surface2)',
  color: 'var(--text)',
  fontSize: 13,
  outline: 'none',
  transition: 'border-color 0.15s',
  boxSizing: 'border-box',
};

const labelStyle = {
  display: 'block',
  fontSize: 11,
  fontWeight: 600,
  color: 'var(--text-dim)',
  marginBottom: 4,
  textTransform: 'uppercase',
  letterSpacing: '0.4px',
};

const requiredDot = (
  <span style={{ color: 'var(--accent)', marginLeft: 2 }}>*</span>
);

function Field({ label, required, hint, children }) {
  return (
    <div style={{ marginBottom: 14 }}>
      <label style={labelStyle}>
        {label}{required && requiredDot}
      </label>
      {children}
      {hint && <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 3 }}>{hint}</div>}
    </div>
  );
}

// ── Section wrapper ────────────────────────────────────────────────────────

function SectionWrapper({ title, icon, children, defaultOpen = true, wizardMode = false }) {
  const [open, setOpen] = useState(defaultOpen);
  if (wizardMode) return <div>{children}</div>;
  return (
    <div style={{ borderBottom: '1px solid var(--border)' }}>
      <button
        onClick={() => setOpen(v => !v)}
        style={{
          width: '100%', display: 'flex', alignItems: 'center', justifyContent: 'space-between',
          padding: '12px 16px', background: 'none', border: 'none', cursor: 'pointer',
          color: 'var(--text)', fontWeight: 700, fontSize: 13,
        }}
      >
        <span style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
          <span style={{ fontSize: 14 }}>{icon}</span>
          {title}
        </span>
        <span style={{ fontSize: 16, color: 'var(--text-muted)', transform: open ? 'rotate(90deg)' : 'none', transition: 'transform 0.15s' }}>›</span>
      </button>
      {open && (
        <div style={{ padding: '0 16px 16px' }}>
          {children}
        </div>
      )}
    </div>
  );
}

// ── BusinessSnapshotSection ────────────────────────────────────────────────

export default function BusinessSnapshotSection({ profile, onChange, wizardMode }) {
  function set(field) {
    return (e) => onChange({ [field]: e.target.value });
  }

  return (
    <SectionWrapper title="Business Snapshot" icon="🏢" defaultOpen wizardMode={wizardMode}>
      <Field label="Business Name" required>
        <input
          style={fieldStyle}
          value={profile.business_name}
          onChange={set('business_name')}
          placeholder="e.g. Apex Marketing Agency"
          maxLength={120}
        />
      </Field>

      <Field label="Website URL" hint="Used to auto-research your business">
        <input
          style={fieldStyle}
          type="url"
          value={profile.website_url}
          onChange={set('website_url')}
          placeholder="https://yourdomain.com"
        />
      </Field>

      <Field label="Industry" required>
        <input
          style={fieldStyle}
          value={profile.industry}
          onChange={set('industry')}
          placeholder="e.g. B2B SaaS, Financial Services, Health & Wellness"
        />
      </Field>

      <Field label="Business Model" required>
        <select
          style={{ ...fieldStyle, cursor: 'pointer' }}
          value={profile.business_model}
          onChange={set('business_model')}
        >
          <option value="">Select model…</option>
          {BUSINESS_MODELS.map(m => (
            <option key={m} value={m}>
              {m.charAt(0).toUpperCase() + m.slice(1).replace(/-/g, ' ')}
            </option>
          ))}
        </select>
      </Field>

      <Field label="Location" hint="Optional — relevant for local businesses">
        <input
          style={fieldStyle}
          value={profile.location}
          onChange={set('location')}
          placeholder="e.g. Austin, TX or Remote"
        />
      </Field>

      <Field label="Primary Offer" required hint="What do you sell?">
        <textarea
          style={{ ...fieldStyle, minHeight: 72, resize: 'vertical' }}
          value={profile.primary_offer}
          onChange={set('primary_offer')}
          placeholder="e.g. We help B2B SaaS companies add $50k–$200k ARR through LinkedIn outbound…"
        />
      </Field>

      <Field label="90-Day Goal" required hint="What's the #1 outcome you're working toward right now?">
        <textarea
          style={{ ...fieldStyle, minHeight: 64, resize: 'vertical' }}
          value={profile.main_goal_90_days}
          onChange={set('main_goal_90_days')}
          placeholder="e.g. Book 20 qualified discovery calls per month"
        />
      </Field>
    </SectionWrapper>
  );
}
