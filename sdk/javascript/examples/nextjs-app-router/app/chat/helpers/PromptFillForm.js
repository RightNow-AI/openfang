'use client';

// ── PromptFillForm ─────────────────────────────────────────────────────────────
// Shows a simple form with one input per template field.
// When submitted, replaces {fieldKey} tokens in templateText and calls onSubmit.

import { useState } from 'react';

function fillTemplate(templateText, values) {
  let result = templateText;
  for (const [key, value] of Object.entries(values)) {
    if (value?.trim()) {
      result = result.replaceAll(`{${key}}`, value.trim());
    } else {
      // Remove unfilled optional placeholders gracefully
      result = result.replaceAll(`{${key}}`, '(not specified)');
    }
  }
  return result;
}

export default function PromptFillForm({ template, onSubmit, onCancel }) {
  const [values, setValues] = useState(() =>
    Object.fromEntries(template.fields.map((f) => [f.key, ''])),
  );
  const [submitting, setSubmitting] = useState(false);

  const requiredFilled = template.fields
    .filter((f) => f.required)
    .every((f) => values[f.key]?.trim());

  async function handleSubmit(e) {
    e.preventDefault();
    if (!requiredFilled || submitting) return;
    setSubmitting(true);
    const prompt = fillTemplate(template.templateText, values);
    await onSubmit(prompt);
    setSubmitting(false);
  }

  return (
    <div data-cy="prompt-fill-form" style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12, marginBottom: 20 }}>
        <span style={{ fontSize: 28, flexShrink: 0 }}>{template.icon}</span>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15, color: 'var(--text-primary)', lineHeight: 1.3 }}>
            {template.title}
          </div>
          <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 3, lineHeight: 1.4 }}>
            {template.description}
          </div>
        </div>
      </div>

      {/* Instructions */}
      <div style={{
        padding: '10px 14px',
        borderRadius: 8,
        background: 'rgba(124,58,237,.08)',
        border: '1px solid rgba(124,58,237,.25)',
        fontSize: 12,
        color: 'var(--text-secondary, #bbb)',
        marginBottom: 18,
        lineHeight: 1.5,
      }}>
        Answer these questions and we will build your prompt — no editing brackets needed.
      </div>

      {/* Fields */}
      <form onSubmit={handleSubmit} style={{ display: 'flex', flexDirection: 'column', gap: 14, flex: 1, overflowY: 'auto', paddingRight: 2 }}>
        {template.fields.map((field) => (
          <div key={field.key}>
            <label style={{ display: 'block', fontSize: 12, fontWeight: 600, color: 'var(--text-secondary, #bbb)', marginBottom: 5 }}>
              {field.label}
              {field.required && <span style={{ color: 'var(--accent)', marginLeft: 3 }}>*</span>}
            </label>
            <input
              type="text"
              value={values[field.key]}
              onChange={(e) => setValues((prev) => ({ ...prev, [field.key]: e.target.value }))}
              placeholder={field.placeholder}
              autoComplete="off"
              style={{
                width: '100%',
                padding: '9px 12px',
                borderRadius: 8,
                border: '1px solid var(--border)',
                background: 'var(--bg-elevated)',
                color: 'var(--text-primary)',
                fontSize: 13,
                outline: 'none',
                transition: 'border-color .15s',
                boxSizing: 'border-box',
              }}
              onFocus={(e) => (e.target.style.borderColor = 'var(--accent)')}
              onBlur={(e) => (e.target.style.borderColor = 'var(--border)')}
            />
            {field.help && (
              <div style={{ fontSize: 11, color: 'var(--text-dim)', marginTop: 4 }}>{field.help}</div>
            )}
          </div>
        ))}

        {/* Actions */}
        <div style={{ display: 'flex', gap: 10, paddingTop: 8, marginTop: 'auto', flexShrink: 0 }}>
          <button
            type="submit"
            disabled={!requiredFilled || submitting}
            data-cy="build-prompt-btn"
            style={{
              flex: 1,
              padding: '11px 16px',
              borderRadius: 9,
              background: (!requiredFilled || submitting) ? 'var(--surface3, #2a2a3e)' : 'var(--accent)',
              color: (!requiredFilled || submitting) ? 'var(--text-dim)' : '#fff',
              border: 'none',
              fontWeight: 700,
              fontSize: 13,
              cursor: (!requiredFilled || submitting) ? 'not-allowed' : 'pointer',
              transition: 'background .15s',
            }}
          >
            {submitting ? 'Sending…' : 'Build my prompt'}
          </button>
          <button
            type="button"
            onClick={onCancel}
            style={{
              padding: '11px 16px',
              borderRadius: 9,
              background: 'transparent',
              border: '1px solid var(--border)',
              color: 'var(--text-dim)',
              fontWeight: 600,
              fontSize: 13,
              cursor: 'pointer',
            }}
          >
            Back
          </button>
        </div>
      </form>
    </div>
  );
}
