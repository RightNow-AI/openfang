'use client';
import { useState, useCallback, useRef } from 'react';
import { apiClient } from '../../lib/api-client';

// ── Icon map ──────────────────────────────────────────────────────────────────
const ICONS = {
  telegram: (
    <svg viewBox="0 0 24 24" fill="currentColor" width="28" height="28">
      <path d="M12 0C5.373 0 0 5.373 0 12s5.373 12 12 12 12-5.373 12-12S18.627 0 12 0zm5.562 8.248-1.97 9.289c-.145.658-.537.818-1.084.508l-3-2.21-1.447 1.394c-.16.16-.295.295-.605.295l.213-3.053 5.56-5.023c.242-.213-.054-.333-.373-.12l-6.871 4.326-2.962-.924c-.643-.204-.657-.643.136-.953l11.57-4.461c.537-.194 1.006.131.833.932z"/>
    </svg>
  ),
  email: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" width="28" height="28">
      <rect x="2" y="4" width="20" height="16" rx="2"/>
      <path d="m2 7 10 7 10-7"/>
    </svg>
  ),
  slack: (
    <svg viewBox="0 0 24 24" fill="currentColor" width="28" height="28">
      <path d="M5.042 15.165a2.528 2.528 0 0 1-2.52 2.523A2.528 2.528 0 0 1 0 15.165a2.527 2.527 0 0 1 2.522-2.52h2.52v2.52zM6.313 15.165a2.527 2.527 0 0 1 2.521-2.52 2.527 2.527 0 0 1 2.521 2.52v6.313A2.528 2.528 0 0 1 8.834 24a2.528 2.528 0 0 1-2.521-2.522v-6.313zM8.834 5.042a2.528 2.528 0 0 1-2.521-2.52A2.528 2.528 0 0 1 8.834 0a2.528 2.528 0 0 1 2.521 2.522v2.52H8.834zM8.834 6.313a2.528 2.528 0 0 1 2.521 2.521 2.528 2.528 0 0 1-2.521 2.521H2.522A2.528 2.528 0 0 1 0 8.834a2.528 2.528 0 0 1 2.522-2.521h6.312zM18.956 8.834a2.528 2.528 0 0 1 2.522-2.521A2.528 2.528 0 0 1 24 8.834a2.528 2.528 0 0 1-2.522 2.521h-2.522V8.834zM17.688 8.834a2.528 2.528 0 0 1-2.523 2.521 2.527 2.527 0 0 1-2.52-2.521V2.522A2.527 2.527 0 0 1 15.165 0a2.528 2.528 0 0 1 2.523 2.522v6.312zM15.165 18.956a2.528 2.528 0 0 1 2.523 2.522A2.528 2.528 0 0 1 15.165 24a2.527 2.527 0 0 1-2.52-2.522v-2.522h2.52zM15.165 17.688a2.527 2.527 0 0 1-2.52-2.523 2.526 2.526 0 0 1 2.52-2.52h6.313A2.527 2.527 0 0 1 24 15.165a2.528 2.528 0 0 1-2.522 2.523h-6.313z"/>
    </svg>
  ),
  discord: (
    <svg viewBox="0 0 24 24" fill="currentColor" width="28" height="28">
      <path d="M20.317 4.37a19.791 19.791 0 0 0-4.885-1.515.074.074 0 0 0-.079.037c-.21.375-.444.864-.608 1.25a18.27 18.27 0 0 0-5.487 0 12.64 12.64 0 0 0-.617-1.25.077.077 0 0 0-.079-.037A19.736 19.736 0 0 0 3.677 4.37a.07.07 0 0 0-.032.027C.533 9.046-.32 13.58.099 18.057c.002.022.015.04.036.052A19.95 19.95 0 0 0 6.143 21.5a.077.077 0 0 0 .084-.028 14.09 14.09 0 0 0 1.226-1.994.076.076 0 0 0-.041-.106 13.107 13.107 0 0 1-1.872-.892.077.077 0 0 1-.008-.128 10.2 10.2 0 0 0 .372-.292.074.074 0 0 1 .077-.01c3.928 1.793 8.18 1.793 12.062 0a.074.074 0 0 1 .078.01c.12.098.246.198.373.292a.077.077 0 0 1-.006.127 12.299 12.299 0 0 1-1.873.892.077.077 0 0 0-.041.107c.36.698.772 1.362 1.225 1.993a.076.076 0 0 0 .084.028 19.839 19.839 0 0 0 6.002-3.388.077.077 0 0 0 .032-.054c.5-5.177-.838-9.674-3.549-13.66a.061.061 0 0 0-.031-.03zM8.02 15.33c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.956-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.956 2.418-2.157 2.418zm7.975 0c-1.183 0-2.157-1.085-2.157-2.419 0-1.333.955-2.419 2.157-2.419 1.21 0 2.176 1.096 2.157 2.42 0 1.333-.946 2.418-2.157 2.418z"/>
    </svg>
  ),
  whatsapp: (
    <svg viewBox="0 0 24 24" fill="currentColor" width="28" height="28">
      <path d="M17.472 14.382c-.297-.149-1.758-.867-2.03-.967-.273-.099-.471-.148-.67.15-.197.297-.767.966-.94 1.164-.173.199-.347.223-.644.075-.297-.15-1.255-.463-2.39-1.475-.883-.788-1.48-1.761-1.653-2.059-.173-.297-.018-.458.13-.606.134-.133.298-.347.446-.52.149-.174.198-.298.298-.497.099-.198.05-.371-.025-.52-.075-.149-.669-1.612-.916-2.207-.242-.579-.487-.5-.669-.51-.173-.008-.371-.01-.57-.01-.198 0-.52.074-.792.372-.272.297-1.04 1.016-1.04 2.479 0 1.462 1.065 2.875 1.213 3.074.149.198 2.096 3.2 5.077 4.487.709.306 1.262.489 1.694.625.712.227 1.36.195 1.871.118.571-.085 1.758-.719 2.006-1.413.248-.694.248-1.289.173-1.413-.074-.124-.272-.198-.57-.347m-5.421 7.403h-.004a9.87 9.87 0 01-5.031-1.378l-.361-.214-3.741.982.998-3.648-.235-.374a9.86 9.86 0 01-1.51-5.26c.001-5.45 4.436-9.884 9.888-9.884 2.64 0 5.122 1.03 6.988 2.898a9.825 9.825 0 012.893 6.994c-.003 5.45-4.437 9.884-9.885 9.884m8.413-18.297A11.815 11.815 0 0012.05 0C5.495 0 .16 5.335.157 11.892c0 2.096.547 4.142 1.588 5.945L.057 24l6.305-1.654a11.882 11.882 0 005.683 1.448h.005c6.554 0 11.89-5.335 11.893-11.893a11.821 11.821 0 00-3.48-8.413z"/>
    </svg>
  ),
  sms: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" width="28" height="28">
      <path d="M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07A19.5 19.5 0 0 1 4.69 12 19.79 19.79 0 0 1 1.63 3.41 2 2 0 0 1 3.6 1.22h3a2 2 0 0 1 2 1.72c.127.96.361 1.903.7 2.81a2 2 0 0 1-.45 2.11L7.91 8.85A16 16 0 0 0 15.1 16l.95-.95a2 2 0 0 1 2.11-.45c.907.339 1.85.573 2.81.7A2 2 0 0 1 22 16.92z"/>
    </svg>
  ),
};

function getIcon(name) {
  const k = (name ?? '').toLowerCase();
  return ICONS[k] ?? (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" width="28" height="28">
      <path d="M4 9h16M4 15h16M10 3l-2 18M16 3l-2 18"/>
    </svg>
  );
}

// Brand colours
const BRAND = {
  telegram:  { bg: '#229ED9', glow: 'rgba(34,158,217,.22)',   text: '#fff' },
  email:     { bg: '#e65c00', glow: 'rgba(230,92,0,.22)',     text: '#fff' },
  slack:     { bg: '#4A154B', glow: 'rgba(74,21,75,.22)',     text: '#fff' },
  discord:   { bg: '#5865F2', glow: 'rgba(88,101,242,.22)',   text: '#fff' },
  whatsapp:  { bg: '#25D366', glow: 'rgba(37,211,102,.22)',   text: '#fff' },
  sms:       { bg: '#ff6a1a', glow: 'rgba(255,106,26,.22)',   text: '#fff' },
  signal:    { bg: '#3A76F0', glow: 'rgba(58,118,240,.22)',   text: '#fff' },
  matrix:    { bg: '#0DBD8B', glow: 'rgba(13,189,139,.22)',   text: '#fff' },
  mastodon:  { bg: '#6364FF', glow: 'rgba(99,100,255,.22)',   text: '#fff' },
  bluesky:   { bg: '#0085ff', glow: 'rgba(0,133,255,.22)',    text: '#fff' },
};
function brand(name) {
  return BRAND[(name ?? '').toLowerCase()] ?? { bg: '#64748b', glow: 'rgba(100,116,139,.18)', text: '#fff' };
}

// ── Spinner ───────────────────────────────────────────────────────────────────
function Spinner({ color = 'currentColor', size = 14 }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2.5"
      style={{ animation: 'spin .7s linear infinite', flexShrink: 0 }}>
      <path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"/>
    </svg>
  );
}

// ── Status badge ──────────────────────────────────────────────────────────────
function StatusBadge({ configured, has_token, status }) {
  const active  = ['connected', 'active', 'enabled'].includes((status ?? '').toLowerCase());
  const errored = ['error', 'failed'].includes((status ?? '').toLowerCase());
  if (active) return <Pill color="var(--success)" bg="var(--success-subtle)" border="var(--success-muted)" dot>Live</Pill>;
  if (errored) return <Pill color="var(--error)" bg="var(--error-subtle)" border="var(--error-muted)" dot>Error</Pill>;
  if (configured || has_token) return <Pill color="var(--warning)" bg="var(--warning-subtle)" border="var(--warning-muted)" dot>Configured</Pill>;
  return <Pill color="var(--text-muted)" bg="var(--surface2)" border="var(--border)" dot>Not set up</Pill>;
}

function Pill({ color, bg, border, dot, children }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 5, fontSize: 11, fontWeight: 600, color, background: bg, border: `1px solid ${border}`, borderRadius: 20, padding: '2px 9px' }}>
      {dot && <span style={{ width: 6, height: 6, borderRadius: '50%', background: color, display: 'inline-block' }} />}
      {children}
    </span>
  );
}

// ── Wizard steps ──────────────────────────────────────────────────────────────
const STEPS = ['Welcome', 'Credentials', 'Test', 'Done'];

function FieldRow({ field, value, shown, onChange, onToggleShow }) {
  const isSecret = ['secret', 'Secret'].includes(field.field_type ?? '');
  const isNumber = ['number', 'Number'].includes(field.field_type ?? '');
  const isArea   = ['text_area', 'TextArea'].includes(field.field_type ?? '');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
      <label style={{ fontSize: 13, fontWeight: 600, color: 'var(--text-secondary)', display: 'flex', alignItems: 'center', gap: 6 }}>
        {field.label}
        {field.required && <sup style={{ color: 'var(--error)', fontSize: 11 }}>*</sup>}
        {field.env_var && (
          <code style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--text-muted)', background: 'var(--surface2)', padding: '1px 6px', borderRadius: 4 }}>
            {field.env_var}
          </code>
        )}
      </label>
      <div style={{ position: 'relative' }}>
        {isArea ? (
          <textarea rows={3} value={value} onChange={e => onChange(e.target.value)} placeholder={field.placeholder}
            style={{ width: '100%', padding: '9px 12px', fontSize: 13, background: 'var(--surface2)', border: '1px solid var(--border-light)', borderRadius: 8, color: 'var(--text)', resize: 'vertical', boxSizing: 'border-box' }} />
        ) : (
          <input
            type={isSecret && !shown ? 'password' : isNumber ? 'number' : 'text'}
            value={value}
            onChange={e => onChange(e.target.value)}
            placeholder={field.placeholder}
            style={{ width: '100%', padding: '9px 12px', paddingRight: isSecret ? 40 : 12, fontSize: 13, background: 'var(--surface2)', border: '1px solid var(--border-light)', borderRadius: 8, color: 'var(--text)', boxSizing: 'border-box' }}
          />
        )}
        {isSecret && (
          <button type="button" onClick={onToggleShow}
            style={{ position: 'absolute', right: 8, top: '50%', transform: 'translateY(-50%)', background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', padding: 4, display: 'flex', alignItems: 'center', lineHeight: 1 }}>
            {shown
              ? <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/><path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
              : <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg>
            }
          </button>
        )}
      </div>
    </div>
  );
}

function WizardModal({ channel, onClose, onSaved }) {
  const [step, setStep]           = useState(0);
  const [values, setValues]       = useState({});
  const [show, setShow]           = useState({});
  const [showAdv, setShowAdv]     = useState(false);
  const [saving, setSaving]       = useState(false);
  const [testing, setTesting]     = useState(false);
  const [testResult, setTestResult] = useState(null);
  const [saveError, setSaveError] = useState('');
  const overlayRef = useRef(null);

  const b      = brand(channel.name);
  const icon   = getIcon(channel.name);
  const fields = channel.fields ?? [];
  const primaryFields  = fields.filter(f => !f.advanced);
  const advancedFields = fields.filter(f => f.advanced);
  const requiredFilled = primaryFields.filter(f => f.required).every(f => (values[f.key] ?? '').trim());

  function handleOverlayClick(e) { if (e.target === overlayRef.current) onClose(); }

  async function handleSave() {
    setSaving(true); setSaveError('');
    try {
      await apiClient.post(`/api/channels/${channel.name}/configure`, { fields: values });
      setStep(2);
    } catch (e) { setSaveError(e.message || 'Save failed.'); }
    setSaving(false);
  }

  async function handleTest() {
    setTesting(true); setTestResult(null);
    try {
      const res = await apiClient.post(`/api/channels/${channel.name}/test`, {});
      setTestResult({ ok: true, message: res?.message ?? res?.note ?? 'Connection successful!' });
    } catch (e) { setTestResult({ ok: false, message: e.message || 'Test failed.' }); }
    setTesting(false);
  }

  function primaryBtnStyle(disabled = false) {
    return {
      display: 'inline-flex', alignItems: 'center', gap: 7,
      background: disabled ? 'var(--surface2)' : b.bg,
      color: disabled ? 'var(--text-muted)' : b.text,
      border: 'none', borderRadius: 9, padding: '9px 20px',
      fontWeight: 600, fontSize: 13,
      cursor: disabled ? 'not-allowed' : 'pointer',
      boxShadow: disabled ? 'none' : `0 4px 14px ${b.glow}`,
      opacity: disabled ? .6 : 1, transition: 'opacity .15s',
    };
  }

  const stepContent = [
    /* ─ Step 0: Welcome ─ */
    <div key="welcome" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', textAlign: 'center', gap: 16, padding: '8px 0 12px' }}>
      <div style={{ width: 72, height: 72, borderRadius: 20, background: b.bg, boxShadow: `0 12px 32px ${b.glow}`, display: 'flex', alignItems: 'center', justifyContent: 'center', color: b.text }}>
        {icon}
      </div>
      <div>
        <div style={{ fontSize: 22, fontWeight: 700, marginBottom: 6 }}>{channel.display_name ?? channel.name}</div>
        <div style={{ color: 'var(--text-dim)', fontSize: 14, maxWidth: 360, lineHeight: 1.65 }}>{channel.description}</div>
      </div>
      <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap', justifyContent: 'center' }}>
        {channel.difficulty && <InfoChip label="Difficulty" value={channel.difficulty} />}
        {channel.setup_time && <InfoChip label="Time" value={channel.setup_time} />}
        {channel.category   && <InfoChip label="Category" value={channel.category} />}
      </div>
      {channel.setup_steps?.length > 0 && (
        <div style={{ width: '100%', background: 'var(--surface2)', borderRadius: 10, padding: '14px 18px', textAlign: 'left' }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '.06em', marginBottom: 8 }}>What you&apos;ll do</div>
          <ol style={{ margin: 0, paddingLeft: 18, display: 'flex', flexDirection: 'column', gap: 6 }}>
            {channel.setup_steps.map((s, i) => <li key={i} style={{ color: 'var(--text-secondary)', fontSize: 13, lineHeight: 1.5 }}>{s}</li>)}
          </ol>
        </div>
      )}
    </div>,

    /* ─ Step 1: Credentials ─ */
    <div key="creds" style={{ display: 'flex', flexDirection: 'column', gap: 14, padding: '4px 0 8px' }}>
      <p style={{ margin: 0, fontSize: 13, color: 'var(--text-dim)', lineHeight: 1.65 }}>
        Secrets are saved to <code style={{ fontSize: 12 }}>~/.openfang/secrets.env</code> on your machine and never transmitted.
      </p>
      {primaryFields.map(f => (
        <FieldRow key={f.key} field={f} value={values[f.key] ?? ''} shown={!!show[f.key]}
          onChange={v => setValues(p => ({ ...p, [f.key]: v }))}
          onToggleShow={() => setShow(p => ({ ...p, [f.key]: !p[f.key] }))} />
      ))}
      {advancedFields.length > 0 && (
        <>
          <button type="button" onClick={() => setShowAdv(v => !v)}
            style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--accent)', fontSize: 13, textAlign: 'left', padding: 0, display: 'flex', alignItems: 'center', gap: 5 }}>
            <span style={{ display: 'inline-block', transform: showAdv ? 'rotate(90deg)' : '', transition: 'transform .15s' }}>›</span>
            {showAdv ? 'Hide' : 'Show'} advanced options
          </button>
          {showAdv && advancedFields.map(f => (
            <FieldRow key={f.key} field={f} value={values[f.key] ?? ''} shown={!!show[f.key]}
              onChange={v => setValues(p => ({ ...p, [f.key]: v }))}
              onToggleShow={() => setShow(p => ({ ...p, [f.key]: !p[f.key] }))} />
          ))}
        </>
      )}
      {saveError && (
        <div style={{ background: 'var(--error-subtle)', border: '1px solid var(--error-muted)', borderRadius: 8, padding: '10px 14px', color: 'var(--error)', fontSize: 13 }}>
          ⚠ {saveError}
        </div>
      )}
    </div>,

    /* ─ Step 2: Test ─ */
    <div key="test" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 18, padding: '8px 0', textAlign: 'center' }}>
      <div style={{ fontSize: 15, fontWeight: 600 }}>Verify connection</div>
      <div style={{ color: 'var(--text-dim)', fontSize: 13, maxWidth: 340, lineHeight: 1.65 }}>
        Ping {channel.display_name ?? channel.name} to confirm your credentials work before activating.
      </div>
      <button onClick={handleTest} disabled={testing}
        style={{ display: 'inline-flex', alignItems: 'center', gap: 8, background: b.bg, color: b.text, border: 'none', borderRadius: 10, padding: '10px 24px', fontWeight: 600, fontSize: 14, cursor: testing ? 'wait' : 'pointer', boxShadow: `0 4px 14px ${b.glow}`, opacity: testing ? .7 : 1 }}>
        {testing && <Spinner color={b.text} />}
        {testing ? 'Testing…' : 'Run connection test'}
      </button>
      {testResult && (
        <div style={{ width: '100%', borderRadius: 10, padding: '14px 16px', background: testResult.ok ? 'var(--success-subtle)' : 'var(--error-subtle)', border: `1px solid ${testResult.ok ? 'var(--success-muted)' : 'var(--error-muted)'}`, color: testResult.ok ? 'var(--success)' : 'var(--error)', fontSize: 13 }}>
          {testResult.ok ? '✓ ' : '✗ '}{testResult.message}
        </div>
      )}
      <div style={{ color: 'var(--text-muted)', fontSize: 12 }}>You can skip the test and activate anyway.</div>
    </div>,

    /* ─ Step 3: Done ─ */
    <div key="done" style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 16, padding: '10px 0 8px', textAlign: 'center' }}>
      <div style={{ width: 64, height: 64, borderRadius: '50%', background: 'var(--success-subtle)', border: '2px solid var(--success-muted)', display: 'flex', alignItems: 'center', justifyContent: 'center', color: 'var(--success)', fontSize: 26 }}>✓</div>
      <div style={{ fontSize: 20, fontWeight: 700 }}>{channel.display_name ?? channel.name} ready!</div>
      <div style={{ color: 'var(--text-dim)', fontSize: 13, maxWidth: 340, lineHeight: 1.65 }}>
        Credentials saved. Agents with <code style={{ fontSize: 12 }}>ToolProfile::Messaging</code> or an explicit <strong>{channel.name}_send</strong> allowlist can now route messages through this channel.
      </div>
      {channel.config_template && (
        <div style={{ width: '100%', background: 'var(--surface3)', borderRadius: 8, padding: '12px 14px', textAlign: 'left' }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '.06em', marginBottom: 6 }}>Config snippet (applied)</div>
          <pre style={{ margin: 0, fontSize: 12, color: 'var(--text-dim)', overflow: 'auto' }}>{channel.config_template}</pre>
        </div>
      )}
    </div>,
  ];

  return (
    <div ref={overlayRef} onClick={handleOverlayClick}
      style={{ position: 'fixed', inset: 0, zIndex: 1000, background: 'rgba(7,17,31,.64)', backdropFilter: 'blur(4px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 16 }}>
      <div style={{ background: 'var(--bg-elevated)', borderRadius: 18, boxShadow: 'var(--shadow-md)', border: '1px solid var(--border-light)', width: '100%', maxWidth: 510, maxHeight: '90dvh', display: 'flex', flexDirection: 'column', overflow: 'hidden', animation: 'slideUp .2s ease' }}>

        {/* ─ Modal header ─ */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '18px 22px 14px', borderBottom: '1px solid var(--border-subtle)', flexShrink: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <div style={{ width: 36, height: 36, borderRadius: 10, background: b.bg, display: 'flex', alignItems: 'center', justifyContent: 'center', color: b.text, flexShrink: 0 }}>
              {getIcon(channel.name)}
            </div>
            <div>
              <div style={{ fontSize: 15, fontWeight: 700 }}>Set up {channel.display_name ?? channel.name}</div>
              <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>Step {step + 1} of {STEPS.length}</div>
            </div>
          </div>
          <button onClick={onClose} style={{ background: 'none', border: 'none', cursor: 'pointer', color: 'var(--text-muted)', fontSize: 20, lineHeight: 1, padding: 4 }}>✕</button>
        </div>

        {/* ─ Stepper ─ */}
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'center', padding: '14px 22px 0', flexShrink: 0 }}>
          {STEPS.map((label, i) => (
            <div key={i} style={{ display: 'flex', alignItems: 'center', flex: i < STEPS.length - 1 ? 1 : 'none' }}>
              <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4, minWidth: 56 }}>
                <div style={{ width: 27, height: 27, borderRadius: '50%', background: i <= step ? b.bg : 'var(--surface2)', border: `2px solid ${i <= step ? b.bg : 'var(--border)'}`, display: 'flex', alignItems: 'center', justifyContent: 'center', color: i <= step ? b.text : 'var(--text-muted)', fontSize: 12, fontWeight: 700, transition: 'all .2s' }}>
                  {i < step ? '✓' : i + 1}
                </div>
                <span style={{ fontSize: 11, color: i === step ? 'var(--text)' : 'var(--text-muted)', fontWeight: i === step ? 600 : 400, whiteSpace: 'nowrap' }}>{label}</span>
              </div>
              {i < STEPS.length - 1 && (
                <div style={{ flex: 1, height: 2, background: i < step ? b.bg : 'var(--border)', transition: 'background .3s', marginBottom: 18, marginLeft: -4, marginRight: -4 }} />
              )}
            </div>
          ))}
        </div>

        {/* ─ Body ─ */}
        <div style={{ padding: '18px 24px', overflowY: 'auto', flex: 1 }}>{stepContent[step]}</div>

        {/* ─ Footer ─ */}
        <div style={{ padding: '13px 24px 18px', borderTop: '1px solid var(--border-subtle)', display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexShrink: 0 }}>
          <button onClick={() => step === 0 ? onClose() : setStep(s => s - 1)}
            style={{ background: 'none', border: '1px solid var(--border)', borderRadius: 8, padding: '8px 16px', cursor: 'pointer', color: 'var(--text-dim)', fontSize: 13, fontWeight: 500 }}>
            {step === 0 ? 'Cancel' : '← Back'}
          </button>

          {step === 0 && <button onClick={() => setStep(1)} style={primaryBtnStyle()}>Continue →</button>}
          {step === 1 && (
            <button onClick={handleSave} disabled={!requiredFilled || saving} style={primaryBtnStyle(!requiredFilled || saving)}>
              {saving && <Spinner color={b.text} />}
              {saving ? 'Saving…' : 'Save & continue →'}
            </button>
          )}
          {step === 2 && <button onClick={() => setStep(3)} style={primaryBtnStyle()}>{testResult?.ok ? 'Activate →' : 'Skip test →'}</button>}
          {step === 3 && <button onClick={() => { onSaved(channel.name); onClose(); }} style={primaryBtnStyle()}>Done ✓</button>}
        </div>
      </div>
    </div>
  );
}

// ── Channel card ──────────────────────────────────────────────────────────────
function ChannelCard({ ch, onSetup }) {
  const b    = brand(ch.name);
  const icon = getIcon(ch.name);
  const configured = ch.configured || ch.has_token;
  return (
    <div style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: 14, overflow: 'hidden', boxShadow: 'var(--shadow-xs)', transition: 'box-shadow .15s, border-color .15s' }}
      onMouseEnter={e => { e.currentTarget.style.boxShadow = 'var(--shadow-sm)'; e.currentTarget.style.borderColor = 'var(--border-light)'; }}
      onMouseLeave={e => { e.currentTarget.style.boxShadow = 'var(--shadow-xs)'; e.currentTarget.style.borderColor = 'var(--border)'; }}>
      <div style={{ height: 4, background: b.bg }} />
      <div style={{ padding: '16px 18px' }}>
        <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', marginBottom: 12 }}>
          <div style={{ width: 44, height: 44, borderRadius: 12, background: b.bg, boxShadow: `0 6px 18px ${b.glow}`, display: 'flex', alignItems: 'center', justifyContent: 'center', color: b.text }}>
            {icon}
          </div>
          <StatusBadge configured={ch.configured} has_token={ch.has_token} status={ch.status} />
        </div>
        <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4 }}>{ch.display_name ?? ch.name}</div>
        <div style={{ fontSize: 12, color: 'var(--text-muted)', marginBottom: 12, lineHeight: 1.5, minHeight: 34 }}>{ch.description}</div>
        <div style={{ display: 'flex', gap: 5, flexWrap: 'wrap', marginBottom: 14 }}>
          {ch.difficulty && <Tag>{ch.difficulty}</Tag>}
          {ch.setup_time && <Tag>{ch.setup_time}</Tag>}
        </div>
        <button onClick={() => onSetup(ch)}
          style={{ width: '100%', padding: '9px 0', borderRadius: 9, background: configured ? 'var(--surface2)' : b.bg, color: configured ? 'var(--text)' : b.text, border: configured ? '1px solid var(--border-light)' : 'none', fontWeight: 600, fontSize: 13, cursor: 'pointer', boxShadow: configured ? 'none' : `0 4px 12px ${b.glow}` }}>
          {configured ? 'Edit credentials' : 'Set up →'}
        </button>
      </div>
    </div>
  );
}

function Tag({ children }) {
  return <span style={{ fontSize: 11, padding: '2px 8px', borderRadius: 20, background: 'var(--surface2)', color: 'var(--text-dim)', border: '1px solid var(--border)' }}>{children}</span>;
}

function InfoChip({ label, value }) {
  return (
    <div style={{ background: 'var(--surface2)', borderRadius: 8, padding: '5px 12px', fontSize: 12, color: 'var(--text-dim)' }}>
      <span style={{ color: 'var(--text-muted)', marginRight: 4 }}>{label}:</span>
      <strong style={{ color: 'var(--text)' }}>{value}</strong>
    </div>
  );
}

function SummaryTile({ label, value, accent }) {
  return (
    <div style={{ background: 'var(--surface)', border: '1px solid var(--border)', borderRadius: 10, padding: '10px 18px', display: 'flex', flexDirection: 'column', gap: 2, minWidth: 90 }}>
      <span style={{ fontSize: 11, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '.06em' }}>{label}</span>
      <span style={{ fontSize: 22, fontWeight: 700, color: accent ?? 'var(--text)' }}>{value}</span>
    </div>
  );
}

// ── Category filter ───────────────────────────────────────────────────────────
const CATEGORIES = ['All', 'messaging', 'social', 'email', 'other'];

function FilterBar({ active, onChange }) {
  return (
    <div style={{ display: 'flex', gap: 7, flexWrap: 'wrap', marginBottom: 18 }}>
      {CATEGORIES.map(cat => (
        <button key={cat} onClick={() => onChange(cat)}
          style={{ fontSize: 12, fontWeight: 600, padding: '5px 14px', borderRadius: 20, cursor: 'pointer', border: '1px solid', transition: 'all .15s', background: active === cat ? 'var(--accent)' : 'var(--surface2)', color: active === cat ? '#fff' : 'var(--text-dim)', borderColor: active === cat ? 'var(--accent)' : 'var(--border)' }}>
          {cat === 'All' ? 'All' : cat.charAt(0).toUpperCase() + cat.slice(1)}
        </button>
      ))}
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────
export default function ChannelsClient({ initialChannels }) {
  const [channels, setChannels] = useState(initialChannels ?? []);
  const [loading,  setLoading]  = useState(false);
  const [error,    setError]    = useState('');
  const [wizard,   setWizard]   = useState(null);
  const [category, setCategory] = useState('All');

  const refresh = useCallback(async () => {
    setLoading(true); setError('');
    try {
      const data = await apiClient.get('/api/channels');
      setChannels(Array.isArray(data) ? data : (data?.channels ?? []));
    } catch (e) { setError(e.message || 'Could not load channels.'); }
    setLoading(false);
  }, []);

  function handleSaved(name) {
    setChannels(prev => prev.map(c => c.name === name ? { ...c, configured: true, has_token: true, status: 'active' } : c));
  }

  const filtered = category === 'All' ? channels : channels.filter(c => (c.category ?? '').toLowerCase() === category);
  const configuredCount = channels.filter(c => c.configured || c.has_token).length;

  return (
    <>
      <style>{`
        @keyframes slideUp { from { opacity:0; transform:translateY(18px); } to { opacity:1; transform:translateY(0); } }
        @keyframes spin { to { transform:rotate(360deg); } }
      `}</style>

      {wizard && <WizardModal channel={wizard} onClose={() => setWizard(null)} onSaved={handleSaved} />}

      <div>
        <div className="page-header">
          <div>
            <h1 style={{ marginBottom: 2 }}>Integrations</h1>
            <div style={{ fontSize: 13, color: 'var(--text-muted)' }}>{configuredCount} of {channels.length} channels configured</div>
          </div>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading} style={{ display: 'inline-flex', alignItems: 'center', gap: 5 }}>
            {loading && <Spinner size={12} />}
            {loading ? 'Loading…' : 'Refresh'}
          </button>
        </div>

        <div className="page-body">
          {error && (
            <div style={{ background: 'var(--error-subtle)', border: '1px solid var(--error-muted)', borderRadius: 10, padding: '12px 16px', color: 'var(--error)', fontSize: 13, marginBottom: 16, display: 'flex', gap: 10, alignItems: 'center' }}>
              ⚠ {error}
              <button className="btn btn-ghost btn-sm" onClick={refresh} style={{ marginLeft: 'auto' }}>Retry</button>
            </div>
          )}

          {channels.length > 0 && (
            <div style={{ display: 'flex', gap: 10, marginBottom: 18, flexWrap: 'wrap' }}>
              <SummaryTile label="Total"      value={channels.length} />
              <SummaryTile label="Configured" value={configuredCount}                 accent="var(--success)" />
              <SummaryTile label="Pending"    value={channels.length - configuredCount} accent="var(--warning)" />
            </div>
          )}

          <FilterBar active={category} onChange={setCategory} />

          {filtered.length === 0 && !error && (
            <div style={{ textAlign: 'center', padding: '48px 0', color: 'var(--text-muted)' }}>
              {channels.length === 0 ? 'No channels — make sure the daemon is running.' : 'No channels in this category.'}
            </div>
          )}

          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(270px, 1fr))', gap: 14 }}>
            {filtered.map(ch => <ChannelCard key={ch.name ?? ch.id} ch={ch} onSetup={setWizard} />)}
          </div>
        </div>
      </div>
    </>
  );
}
