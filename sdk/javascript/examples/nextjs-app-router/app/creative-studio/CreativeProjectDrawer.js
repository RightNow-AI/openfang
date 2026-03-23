'use client';
import { useState } from 'react';

const PLATFORM_OPTIONS = ['Instagram', 'TikTok', 'YouTube', 'LinkedIn', 'Twitter / X', 'Email', 'Website', 'Other'];

export default function CreativeProjectDrawer({ open, project, onClose, onSave }) {
  const [form, setForm] = useState(project ?? {});
  const [saving, setSaving] = useState(false);

  const set = (k, v) => setForm(prev => ({ ...prev, [k]: v }));

  const handleSave = async () => {
    setSaving(true);
    try {
      await onSave(form);
      onClose();
    } catch {}
    setSaving(false);
  };

  if (!open) return null;

  return (
    <div
      style={{ position: 'fixed', inset: 0, zIndex: 1200, background: 'rgba(0,0,0,.65)', backdropFilter: 'blur(3px)', display: 'flex', justifyContent: 'flex-end' }}
      onClick={e => e.target === e.currentTarget && onClose()}
    >
      <div data-cy="project-drawer" style={{ width: 480, background: 'var(--bg-elevated)', borderLeft: '1px solid var(--border)', overflowY: 'auto', padding: '28px 28px', display: 'flex', flexDirection: 'column', gap: 0 }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 }}>
          <div style={{ fontWeight: 700, fontSize: 18 }}>Edit brief</div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 22, color: 'var(--text-dim)', lineHeight: 1 }}>✕</button>
        </div>

        <Field label="Project name"     value={form.name            ?? ''} onChange={v => set('name', v)} />
        <Field label="Topic / product"  value={form.topic           ?? ''} onChange={v => set('topic', v)} required />
        <Field label="Offer"            value={form.offer           ?? ''} onChange={v => set('offer', v)} />
        <Field label="Audience"         value={form.audience        ?? ''} onChange={v => set('audience', v)} />
        <Field label="Desired outcome"  value={form.desired_outcome ?? ''} onChange={v => set('desired_outcome', v)} />
        <Field label="Style notes"      value={form.style_notes     ?? ''} onChange={v => set('style_notes', v)} textarea />
        <Field label="Notes"            value={form.notes           ?? ''} onChange={v => set('notes', v)} textarea />

        <div style={{ marginBottom: 16 }}>
          <label style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, display: 'block', marginBottom: 6 }}>Platform</label>
          <select
            value={form.platform ?? ''}
            onChange={e => set('platform', e.target.value)}
            style={{ width: '100%', padding: '9px 12px', borderRadius: 7, background: 'var(--bg-elevated)', border: '1px solid var(--border)', color: 'var(--text-primary)', fontSize: 13 }}
          >
            <option value="">Select platform…</option>
            {PLATFORM_OPTIONS.map(p => <option key={p} value={p}>{p}</option>)}
          </select>
        </div>

        <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', paddingTop: 16, borderTop: '1px solid var(--border)', marginTop: 8 }}>
          <button onClick={onClose} style={{ padding: '9px 18px', borderRadius: 7, background: 'transparent', border: '1px solid var(--border)', color: 'var(--text-dim)', cursor: 'pointer', fontSize: 13 }}>Cancel</button>
          <button onClick={handleSave} disabled={saving} style={{ padding: '9px 22px', borderRadius: 7, background: 'var(--accent)', color: '#fff', border: 'none', cursor: saving ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 13, opacity: saving ? 0.7 : 1 }}>
            {saving ? 'Saving…' : 'Save changes'}
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, value, onChange, required, textarea }) {
  const Tag = textarea ? 'textarea' : 'input';
  return (
    <div style={{ marginBottom: 16 }}>
      <label style={{ fontSize: 11, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 1, display: 'block', marginBottom: 6 }}>
        {label}{required && <span style={{ color: 'var(--error,#ef4444)', marginLeft: 3 }}>*</span>}
      </label>
      <Tag
        value={value}
        onChange={e => onChange(e.target.value)}
        rows={textarea ? 3 : undefined}
        style={{ width: '100%', padding: '9px 12px', borderRadius: 7, background: 'var(--bg-elevated)', border: '1px solid var(--border)', color: 'var(--text-primary)', fontSize: 13, outline: 'none', resize: textarea ? 'vertical' : undefined, boxSizing: 'border-box', fontFamily: 'inherit' }}
      />
    </div>
  );
}
