'use client';

import { useState } from 'react';

const ACQUISITION_CHANNELS = [
  'organic social', 'paid social', 'seo', 'email', 'referrals',
  'cold outreach', 'events', 'partnerships', 'content', 'community',
];

const AWARENESS_LEVELS = [
  { value: '', label: 'Select level…' },
  { value: 'unaware', label: 'Unaware — doesn\'t know they have the problem' },
  { value: 'problem aware', label: 'Problem Aware — knows the pain, not the solution' },
  { value: 'solution aware', label: 'Solution Aware — knows solutions exist' },
  { value: 'product aware', label: 'Product Aware — knows about you, not convinced' },
  { value: 'most aware', label: 'Most Aware — ready to buy' },
];

const fieldStyle = {
  width: '100%',
  padding: '7px 10px',
  borderRadius: 6,
  border: '1px solid var(--border)',
  background: 'var(--surface2)',
  color: 'var(--text)',
  fontSize: 13,
  outline: 'none',
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

const requiredDot = <span style={{ color: 'var(--accent)', marginLeft: 2 }}>*</span>;

function Field({ label, required, hint, children }) {
  return (
    <div style={{ marginBottom: 14 }}>
      <label style={labelStyle}>{label}{required && requiredDot}</label>
      {children}
      {hint && <div style={{ fontSize: 11, color: 'var(--text-muted)', marginTop: 3 }}>{hint}</div>}
    </div>
  );
}

function SectionWrapper({ title, icon, children, defaultOpen = false, wizardMode = false }) {
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

// ── Array text input —– each row is a text input, + to add, × to remove ──

function ArrayTextInput({ values, onChange, placeholder, minRows = 3 }) {
  const items = values?.length ? values : [''];

  function setItem(i, val) {
    const next = [...items];
    next[i] = val;
    onChange(next);
  }

  function addItem() {
    onChange([...items, '']);
  }

  function removeItem(i) {
    const next = items.filter((_, idx) => idx !== i);
    onChange(next.length ? next : ['']);
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
      {items.map((val, i) => (
        <div key={i} style={{ display: 'flex', gap: 5, alignItems: 'center' }}>
          <input
            style={{ ...fieldStyle, flex: 1 }}
            value={val}
            onChange={e => setItem(i, e.target.value)}
            placeholder={`${placeholder} ${i + 1}`}
          />
          {items.length > 1 && (
            <button
              onClick={() => removeItem(i)}
              style={{
                width: 24, height: 24, borderRadius: 4, border: '1px solid var(--border)',
                background: 'var(--surface2)', color: 'var(--text-muted)',
                cursor: 'pointer', fontSize: 14, display: 'flex', alignItems: 'center', justifyContent: 'center',
                flexShrink: 0,
              }}
              title="Remove"
            >×</button>
          )}
        </div>
      ))}
      <button
        onClick={addItem}
        style={{
          alignSelf: 'flex-start',
          fontSize: 11, color: 'var(--accent)', background: 'none', border: 'none',
          cursor: 'pointer', padding: '2px 0', fontWeight: 600,
        }}
      >+ Add another</button>
    </div>
  );
}

// ── Competitor repeater ───────────────────────────────────────────────────

function CompetitorRepeater({ competitors, onChange }) {
  const items = competitors?.length ? competitors : [{ name: '', url: '', notes: '' }];

  function setField(i, field, val) {
    const next = items.map((c, idx) => idx === i ? { ...c, [field]: val } : c);
    onChange(next);
  }

  function addItem() {
    onChange([...items, { name: '', url: '', notes: '' }]);
  }

  function removeItem(i) {
    const next = items.filter((_, idx) => idx !== i);
    onChange(next.length ? next : [{ name: '', url: '', notes: '' }]);
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
      {items.map((comp, i) => (
        <div
          key={i}
          style={{ padding: 10, borderRadius: 7, border: '1px solid var(--border)', background: 'var(--surface2)' }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
            <span style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)' }}>
              Competitor {i + 1}
            </span>
            {items.length > 1 && (
              <button
                onClick={() => removeItem(i)}
                style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 12 }}
              >Remove</button>
            )}
          </div>
          <input
            style={{ ...fieldStyle, marginBottom: 5, background: 'var(--bg-elevated)' }}
            value={comp.name}
            onChange={e => setField(i, 'name', e.target.value)}
            placeholder="Competitor name"
          />
          <input
            style={{ ...fieldStyle, marginBottom: 5, background: 'var(--bg-elevated)' }}
            type="url"
            value={comp.url}
            onChange={e => setField(i, 'url', e.target.value)}
            placeholder="Website URL (optional)"
          />
          <input
            style={{ ...fieldStyle, background: 'var(--bg-elevated)' }}
            value={comp.notes}
            onChange={e => setField(i, 'notes', e.target.value)}
            placeholder="Notes (optional)"
          />
        </div>
      ))}
      <button
        onClick={addItem}
        style={{
          alignSelf: 'flex-start',
          fontSize: 11, color: 'var(--accent)', background: 'none', border: 'none',
          cursor: 'pointer', padding: '2px 0', fontWeight: 600,
        }}
      >+ Add competitor</button>
    </div>
  );
}

// ── Multi-select chips ─────────────────────────────────────────────────────

function MultiSelectChips({ options, selected, onChange }) {
  function toggle(opt) {
    if (selected.includes(opt)) {
      onChange(selected.filter(s => s !== opt));
    } else {
      onChange([...selected, opt]);
    }
  }

  return (
    <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5 }}>
      {options.map(opt => {
        const active = selected.includes(opt);
        return (
          <button
            key={opt}
            onClick={() => toggle(opt)}
            style={{
              padding: '4px 9px', borderRadius: 6, fontSize: 11, fontWeight: 500,
              border: `1px solid ${active ? 'var(--accent)' : 'var(--border)'}`,
              background: active ? 'var(--accent-subtle)' : 'var(--surface2)',
              color: active ? 'var(--accent)' : 'var(--text-dim)',
              cursor: 'pointer',
            }}
          >{opt}</button>
        );
      })}
    </div>
  );
}

// ── AudienceMarketSection ─────────────────────────────────────────────────

export default function AudienceMarketSection({ profile, onChange, wizardMode }) {
  return (
    <SectionWrapper title="Audience & Market" icon="🎯" defaultOpen={wizardMode || false} wizardMode={wizardMode}>
      <Field label="Ideal Customer" required hint="Who is the buyer?">
        <textarea
          style={{ ...fieldStyle, minHeight: 72, resize: 'vertical' }}
          value={profile.ideal_customer}
          onChange={e => onChange({ ideal_customer: e.target.value })}
          placeholder="e.g. Founder-led B2B SaaS companies at $500k–$3M ARR who are trying to move upmarket…"
        />
      </Field>

      <Field label="Top Pain Points" required hint="What's keeping them up at night?">
        <ArrayTextInput
          values={profile.top_pain_points}
          onChange={v => onChange({ top_pain_points: v })}
          placeholder="Pain point"
        />
      </Field>

      <Field label="Desired Outcomes" required hint="What does success look like for them?">
        <ArrayTextInput
          values={profile.desired_outcomes}
          onChange={v => onChange({ desired_outcomes: v })}
          placeholder="Desired outcome"
        />
      </Field>

      <Field label="Top Objections" required hint="Why might they not buy?">
        <ArrayTextInput
          values={profile.top_objections}
          onChange={v => onChange({ top_objections: v })}
          placeholder="Objection"
        />
      </Field>

      <Field label="Acquisition Channels" required>
        <MultiSelectChips
          options={ACQUISITION_CHANNELS}
          selected={profile.current_acquisition_channels || []}
          onChange={v => onChange({ current_acquisition_channels: v })}
        />
      </Field>

      <Field label="Competitors" required hint="Add at least 1 for Research Competitors task">
        <CompetitorRepeater
          competitors={profile.top_competitors}
          onChange={v => onChange({ top_competitors: v })}
        />
      </Field>

      <Field label="Customer Awareness Level" hint="Where is your customer in their buying journey?">
        <select
          style={{ ...fieldStyle, cursor: 'pointer' }}
          value={profile.customer_awareness_level}
          onChange={e => onChange({ customer_awareness_level: e.target.value })}
        >
          {AWARENESS_LEVELS.map(l => (
            <option key={l.value} value={l.value}>{l.label}</option>
          ))}
        </select>
      </Field>
    </SectionWrapper>
  );
}
