'use client';

const ASPECT_RATIOS = ['', '1:1', '4:5', '9:16', '16:9', '4:3', '2:3'];
const DURATIONS = ['', 'Under 15 seconds', '15-30 seconds', '30-60 seconds', '1-2 minutes', '2-5 minutes'];

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

export default function CreativeWizardStepStyle({ state, onChange, needsVideo }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
      <div style={{ fontWeight: 700, fontSize: 15, marginBottom: 4 }}>
        Pick your style and references
      </div>

      <div style={{
        padding: '14px 16px',
        borderRadius: 'var(--radius)',
        border: '1px dashed var(--border-light)',
        background: 'var(--surface2)',
        display: 'flex',
        flexDirection: 'column',
        gap: 8,
        cursor: 'pointer',
      }}>
        <div style={{ fontWeight: 600, fontSize: 13 }}>📎 Upload inspiration images</div>
        <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          Drag files here or click to browse — JPG, PNG, PDF (optional)
        </div>
        <input type="file" multiple accept="image/*,.pdf" style={{ fontSize: 12, color: 'var(--text-dim)' }} />
      </div>

      <Field label="Paste reference links" hint="(optional)">
        <textarea
          data-cy="style-ref-links"
          rows={2}
          style={{ ...inputStyle, resize: 'vertical' }}
          placeholder="One URL per line — ads, competitor sites, brands you like"
          value={state.reference_links_raw}
          onChange={e => onChange('reference_links_raw', e.target.value)}
        />
      </Field>

      <Field label="Mood / style description" hint="(optional)">
        <textarea
          data-cy="style-mood"
          rows={2}
          style={{ ...inputStyle, resize: 'vertical' }}
          placeholder="e.g. Clean studio feel, bold typography, warm orange tones"
          value={state.style_description}
          onChange={e => onChange('style_description', e.target.value)}
        />
      </Field>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
        <Field label="Visual keywords" hint="(optional)">
          <input
            data-cy="style-keywords"
            style={inputStyle}
            placeholder="e.g. minimal, bright, energetic"
            value={state.visual_keywords_raw}
            onChange={e => onChange('visual_keywords_raw', e.target.value)}
          />
        </Field>
        <Field label="Words or styles to avoid" hint="(optional)">
          <input
            data-cy="style-avoid"
            style={inputStyle}
            placeholder="e.g. dark, cluttered, retro"
            value={state.words_to_avoid_raw}
            onChange={e => onChange('words_to_avoid_raw', e.target.value)}
          />
        </Field>
      </div>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>
        <Field label="Aspect ratio" hint="(optional)">
          <select
            data-cy="style-ratio"
            style={inputStyle}
            value={state.aspect_ratio}
            onChange={e => onChange('aspect_ratio', e.target.value)}
          >
            {ASPECT_RATIOS.map(r => (
              <option key={r} value={r}>{r || 'Auto'}</option>
            ))}
          </select>
        </Field>
        {needsVideo && (
          <Field label="Video length" hint="(optional)">
            <select
              data-cy="style-duration"
              style={inputStyle}
              value={state.duration}
              onChange={e => onChange('duration', e.target.value)}
            >
              {DURATIONS.map(d => (
                <option key={d} value={d}>{d || 'Not sure yet'}</option>
              ))}
            </select>
          </Field>
        )}
      </div>

      {needsVideo && (
        <Field label="Voice / tone notes" hint="(optional)">
          <input
            data-cy="style-voice-tone"
            style={inputStyle}
            placeholder="e.g. Energetic female voice, calm and professional male"
            value={state.voice_tone}
            onChange={e => onChange('voice_tone', e.target.value)}
          />
        </Field>
      )}
    </div>
  );
}
