'use client';

import { useState } from 'react';

const SUGGESTED_TRAITS = [
  'direct', 'confident', 'warm', 'authoritative', 'playful', 'sharp',
  'premium', 'calm', 'bold', 'clear', 'human', 'expert', 'candid',
];

const SUGGESTED_AVOID = [
  'hypey', 'robotic', 'generic', 'corporate', 'salesy', 'passive',
  'vague', 'jargon-heavy', 'formal', 'dramatic',
];

const fieldStyle = {
  width: '100%', padding: '7px 10px', borderRadius: 6,
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

// ── Tag input ──────────────────────────────────────────────────────────────
// Suggestions are shown as quick-add chips; user can also type custom values.

function TagInput({ tags, onChange, suggestions, placeholder }) {
  const [input, setInput] = useState('');

  function addTag(val) {
    const trimmed = val.trim().toLowerCase();
    if (trimmed && !tags.includes(trimmed)) {
      onChange([...tags, trimmed]);
    }
    setInput('');
  }

  function removeTag(tag) {
    onChange(tags.filter(t => t !== tag));
  }

  function handleKeyDown(e) {
    if ((e.key === 'Enter' || e.key === ',') && input.trim()) {
      e.preventDefault();
      addTag(input);
    }
    if (e.key === 'Backspace' && !input && tags.length > 0) {
      onChange(tags.slice(0, -1));
    }
  }

  const unusedSuggestions = (suggestions || []).filter(s => !tags.includes(s));

  return (
    <div>
      {/* Active tags */}
      {tags.length > 0 && (
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 5, marginBottom: 7 }}>
          {tags.map(tag => (
            <span key={tag} style={{
              display: 'flex', alignItems: 'center', gap: 4,
              padding: '3px 8px', borderRadius: 5,
              background: 'var(--accent-subtle)', border: '1px solid var(--accent)',
              color: 'var(--accent)', fontSize: 12, fontWeight: 500,
            }}>
              {tag}
              <button
                onClick={() => removeTag(tag)}
                style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent)', padding: 0, fontSize: 13, lineHeight: 1 }}
              >×</button>
            </span>
          ))}
        </div>
      )}

      {/* Custom input */}
      <input
        style={fieldStyle}
        value={input}
        onChange={e => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        onBlur={() => { if (input.trim()) addTag(input); }}
        placeholder={placeholder || 'Type and press Enter…'}
      />

      {/* Suggestions */}
      {unusedSuggestions.length > 0 && (
        <div style={{ marginTop: 7 }}>
          <div style={{ fontSize: 10, color: 'var(--text-muted)', marginBottom: 4, textTransform: 'uppercase', letterSpacing: '0.4px' }}>
            Quick add
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
            {unusedSuggestions.map(s => (
              <button
                key={s}
                onClick={() => addTag(s)}
                style={{
                  padding: '3px 8px', borderRadius: 5, fontSize: 11,
                  border: '1px solid var(--border)', background: 'var(--surface2)',
                  color: 'var(--text-dim)', cursor: 'pointer',
                }}
              >+ {s}</button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Examples repeater ──────────────────────────────────────────────────────

function ExamplesRepeater({ items, onChange, reasonKey, reasonPlaceholder }) {
  const list = items?.length ? items : [{ type: 'url', value: '', [reasonKey]: '' }];

  function setField(i, field, val) {
    const next = list.map((x, idx) => idx === i ? { ...x, [field]: val } : x);
    onChange(next);
  }

  function addItem() {
    onChange([...list, { type: 'url', value: '', [reasonKey]: '' }]);
  }

  function removeItem(i) {
    const next = list.filter((_, idx) => idx !== i);
    onChange(next.length ? next : [{ type: 'url', value: '', [reasonKey]: '' }]);
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
      {list.map((item, i) => (
        <div key={i} style={{ padding: 10, borderRadius: 7, border: '1px solid var(--border)', background: 'var(--surface2)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
            <div style={{ display: 'flex', gap: 6 }}>
              {['url', 'copy', 'brand'].map(t => (
                <label key={t} style={{ display: 'flex', alignItems: 'center', gap: 3, fontSize: 11, color: 'var(--text-dim)', cursor: 'pointer' }}>
                  <input
                    type="radio" name={`type-${i}-${reasonKey}`} value={t}
                    checked={item.type === t}
                    onChange={() => setField(i, 'type', t)}
                    style={{ accentColor: 'var(--accent)' }}
                  />
                  {t}
                </label>
              ))}
            </div>
            {list.length > 1 && (
              <button
                onClick={() => removeItem(i)}
                style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 12 }}
              >Remove</button>
            )}
          </div>
          <input
            style={{ ...fieldStyle, marginBottom: 5, background: 'var(--bg-elevated)' }}
            value={item.value}
            onChange={e => setField(i, 'value', e.target.value)}
            placeholder={item.type === 'url' ? 'https://example.com/example-copy' : 'Paste example text or brand name…'}
          />
          <input
            style={{ ...fieldStyle, background: 'var(--bg-elevated)' }}
            value={item[reasonKey] || ''}
            onChange={e => setField(i, reasonKey, e.target.value)}
            placeholder={reasonPlaceholder}
          />
        </div>
      ))}
      <button
        onClick={addItem}
        style={{
          alignSelf: 'flex-start',
          fontSize: 11, color: 'var(--accent)', background: 'none',
          border: 'none', cursor: 'pointer', padding: '2px 0', fontWeight: 600,
        }}
      >+ Add example</button>
    </div>
  );
}

// ── BrandVoiceSection ──────────────────────────────────────────────────────

export default function BrandVoiceSection({ profile, onChange, wizardMode }) {
  return (
    <SectionWrapper title="Brand Voice" icon="🎤" defaultOpen={wizardMode || false} wizardMode={wizardMode}>
      <Field label="Brand Traits" required hint="Words that describe how your brand speaks — press Enter after each">
        <TagInput
          tags={profile.brand_traits || []}
          onChange={v => onChange({ brand_traits: v })}
          suggestions={SUGGESTED_TRAITS}
          placeholder="e.g. direct, confident, warm…"
        />
      </Field>

      <Field label="Traits to Avoid" required hint="The tone your brand deliberately doesn't use">
        <TagInput
          tags={profile.traits_to_avoid || []}
          onChange={v => onChange({ traits_to_avoid: v })}
          suggestions={SUGGESTED_AVOID}
          placeholder="e.g. hypey, robotic, vague…"
        />
      </Field>

      <Field label="Brand Promise" required hint="One sentence: what you guarantee the client will experience or achieve">
        <textarea
          style={{ ...fieldStyle, minHeight: 64, resize: 'vertical' }}
          value={profile.brand_promise}
          onChange={e => onChange({ brand_promise: e.target.value })}
          placeholder="e.g. We make complex financial decisions feel simple and obvious."
        />
      </Field>

      <Field label="Examples You Like" required hint="Brands, ads, emails, or copy that feels right to you">
        <ExamplesRepeater
          items={profile.liked_examples}
          onChange={v => onChange({ liked_examples: v })}
          reasonKey="reason_liked"
          reasonPlaceholder="Why do you like this? (e.g. direct, no fluff, confident)"
        />
      </Field>

      <Field label="Examples You Dislike" required hint="Brands or copy that feels wrong or off-brand">
        <ExamplesRepeater
          items={profile.disliked_examples}
          onChange={v => onChange({ disliked_examples: v })}
          reasonKey="reason_disliked"
          reasonPlaceholder="Why do you dislike this? (e.g. too salesy, sounds robotic)"
        />
      </Field>

      <Field label="Taboo Words" hint="Words never to use in any copy">
        <TagInput
          tags={profile.taboo_words || []}
          onChange={v => onChange({ taboo_words: v })}
          placeholder="e.g. synergy, leverage, disruptive…"
        />
      </Field>

      <Field label="Approved Words / Phrases">
        <TagInput
          tags={profile.approved_words || []}
          onChange={v => onChange({ approved_words: v })}
          placeholder="e.g. clear, proven, straightforward…"
        />
      </Field>

      <Field label="Voice Notes">
        <textarea
          style={{ ...fieldStyle, minHeight: 64, resize: 'vertical' }}
          value={profile.voice_notes}
          onChange={e => onChange({ voice_notes: e.target.value })}
          placeholder="Anything else about how your brand sounds — regional quirks, humor level, formality…"
        />
      </Field>
    </SectionWrapper>
  );
}
