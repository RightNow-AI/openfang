'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';
import { track } from '../../lib/telemetry';

// ─── Starter bundles ──────────────────────────────────────────────────────────

const HAND_STARTERS = [
  {
    id: 'agency_research',
    title: 'Agency research bundle',
    description: 'Set up hands for client research, summaries, and follow-up prep.',
    bestFor: 'Agency Mode',
    includesHands: ['Researcher Hand', 'Browser Hand', 'Lead Hand'],
    requires: ['web', 'files', 'email'],
    category: 'agency',
    setupPayload: { hand_ids: ['researcher_hand', 'browser_hand', 'lead_hand'], required_integrations: ['web', 'files', 'email'] },
  },
  {
    id: 'agency_followup',
    title: 'Agency follow-up bundle',
    description: 'Set up hands for drafts, reminders, and task assignment.',
    bestFor: 'Agency Mode',
    includesHands: ['Lead Hand', 'Browser Hand'],
    requires: ['email', 'calendar'],
    category: 'agency',
    setupPayload: { hand_ids: ['lead_hand', 'browser_hand'], required_integrations: ['email', 'calendar'] },
  },
  {
    id: 'growth_creative',
    title: 'Growth creative bundle',
    description: 'Set up hands for clips, trend collection, and campaign ideas.',
    bestFor: 'Growth Mode',
    includesHands: ['Clip Hand', 'Collector Hand', 'Predictor Hand'],
    requires: ['files', 'social', 'web'],
    category: 'growth',
    setupPayload: { hand_ids: ['clip_hand', 'collector_hand', 'predictor_hand'], required_integrations: ['files', 'social', 'web'] },
  },
  {
    id: 'growth_leads',
    title: 'Growth leads bundle',
    description: 'Set up hands for lead discovery, scoring, and outreach prep.',
    bestFor: 'Growth Mode',
    includesHands: ['Lead Hand', 'Collector Hand'],
    requires: ['email', 'web'],
    category: 'growth',
    setupPayload: { hand_ids: ['lead_hand', 'collector_hand'], required_integrations: ['email', 'web'] },
  },
  {
    id: 'school_curriculum',
    title: 'School curriculum bundle',
    description: 'Set up hands for lessons, research, and asset collection.',
    bestFor: 'School Mode',
    includesHands: ['Researcher Hand', 'Collector Hand'],
    requires: ['files', 'web'],
    category: 'school',
    setupPayload: { hand_ids: ['researcher_hand', 'collector_hand'], required_integrations: ['files', 'web'] },
  },
  {
    id: 'school_student_success',
    title: 'Student success bundle',
    description: 'Set up hands for reminders, summaries, and follow-up help.',
    bestFor: 'School Mode',
    includesHands: ['Lead Hand', 'Predictor Hand'],
    requires: ['email', 'calendar'],
    category: 'school',
    setupPayload: { hand_ids: ['lead_hand', 'predictor_hand'], required_integrations: ['email', 'calendar'] },
  },
];

const GOALS = [
  { value: 'client-work',       label: 'Client work',          icon: '💼' },
  { value: 'lead-gen',          label: 'Lead generation',      icon: '🎯' },
  { value: 'research',          label: 'Research',             icon: '🔍' },
  { value: 'video-ads',         label: 'Video & ads',          icon: '🎬' },
  { value: 'course-building',   label: 'Course building',      icon: '🎓' },
  { value: 'student-support',   label: 'Student support',      icon: '🧑‍🎓' },
  { value: 'not-sure',          label: "I'm not sure yet",     icon: '💡' },
];

const MODES = [
  { value: 'agency',  label: 'Agency Mode',  icon: '🏢' },
  { value: 'growth',  label: 'Growth Mode',  icon: '📈' },
  { value: 'school',  label: 'School Mode',  icon: '🏫' },
  { value: 'general', label: 'General',      icon: '⚡' },
];

const TOOLS = [
  { value: 'email',    label: 'Email',    icon: '✉' },
  { value: 'web',      label: 'Web',      icon: '🌐' },
  { value: 'files',    label: 'Files',    icon: '📁' },
  { value: 'calendar', label: 'Calendar', icon: '📅' },
  { value: 'social',   label: 'Social',   icon: '📱' },
  { value: 'none',     label: 'None yet', icon: '—' },
];

// ─── Helpers ──────────────────────────────────────────────────────────────────

function normalizeHand(raw, i) {
  return {
    id:                  String(raw?.id ?? `hand-${i}`),
    name:                String(raw?.name ?? 'Unnamed Hand'),
    description:         String(raw?.description ?? ''),
    icon:                raw?.icon ?? '🤖',
    status:              raw?.requirements_met ? 'ready' : 'needs_setup',
    category:            raw?.category ?? 'general',
    requirements_met:    Boolean(raw?.requirements_met),
    tools:               Array.isArray(raw?.tools) ? raw.tools : [],
    requirements:        Array.isArray(raw?.requirements) ? raw.requirements : [],
    raw,
  };
}

function catColor(c) {
  if (c === 'agency') return '#7c3aed';
  if (c === 'growth')  return '#059669';
  if (c === 'school')  return '#2563eb';
  return '#6b7280';
}

// ─── Shared wizard primitives ─────────────────────────────────────────────────

function WizardStep({ step, total = 5, title, subtitle, onBack, onNext, nextLabel, nextDisabled, children }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div style={{ fontSize: 11, color: 'var(--text-dim,#888)', letterSpacing: 1, textTransform: 'uppercase' }}>Step {step} of {total}</div>
      <div>
        <h2 style={{ fontSize: 19, fontWeight: 700, margin: 0 }}>{title}</h2>
        {subtitle && <p style={{ fontSize: 13, color: 'var(--text-dim,#888)', margin: '5px 0 0', lineHeight: 1.6 }}>{subtitle}</p>}
      </div>
      <div>{children}</div>
      <div style={{ display: 'flex', gap: 10, justifyContent: 'flex-end', paddingTop: 8, borderTop: '1px solid var(--border,#2a2a3e)' }}>
        {onBack && <button onClick={onBack} style={{ padding: '8px 18px', borderRadius: 6, background: 'transparent', border: '1px solid var(--border,#333)', color: 'var(--text-secondary,#ccc)', cursor: 'pointer', fontSize: 13 }}>← Back</button>}
        {onNext && <button onClick={onNext} disabled={nextDisabled} style={{ padding: '8px 20px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: nextDisabled ? 'not-allowed' : 'pointer', fontWeight: 700, fontSize: 13, opacity: nextDisabled ? 0.5 : 1 }}>{nextLabel ?? 'Next →'}</button>}
      </div>
    </div>
  );
}

function ChoiceBtn({ selected, onClick, icon, label }) {
  return (
    <button onClick={onClick} style={{ display: 'block', width: '100%', textAlign: 'left', padding: '11px 14px', borderRadius: 8, marginBottom: 7, border: `1px solid ${selected ? 'var(--accent,#7c3aed)' : 'var(--border,#333)'}`, background: selected ? 'rgba(124,58,237,0.1)' : 'transparent', cursor: 'pointer' }}>
      {icon && <span style={{ marginRight: 10 }}>{icon}</span>}
      <span style={{ fontWeight: selected ? 700 : 400, fontSize: 14 }}>{label}</span>
    </button>
  );
}

function ToggleBtn({ selected, onClick, icon, label }) {
  return (
    <button onClick={onClick} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 14px', borderRadius: 8, marginBottom: 6, border: `1px solid ${selected ? 'var(--accent,#7c3aed)' : 'var(--border,#333)'}`, background: selected ? 'rgba(124,58,237,0.1)' : 'transparent', cursor: 'pointer', width: '100%' }}>
      {icon && <span style={{ fontSize: 18, width: 24, textAlign: 'center' }}>{icon}</span>}
      <span style={{ fontWeight: selected ? 700 : 400, fontSize: 13 }}>{label}</span>
      {selected && <span style={{ marginLeft: 'auto', color: 'var(--accent,#7c3aed)' }}>✓</span>}
    </button>
  );
}

// ─── Wizard steps ─────────────────────────────────────────────────────────────

function StepGoal({ value, onSelect, onNext }) {
  return (
    <WizardStep step={1} title="What are you trying to do?" subtitle="We'll suggest the right hands for your goal." onNext={onNext} nextDisabled={!value}>
      {GOALS.map(g => <ChoiceBtn key={g.value} selected={value === g.value} onClick={() => onSelect(g.value)} icon={g.icon} label={g.label} />)}
    </WizardStep>
  );
}

function StepMode({ value, onSelect, onBack, onNext }) {
  return (
    <WizardStep step={2} title="What mode are you in?" subtitle="This helps us pick the bundle that fits best." onBack={onBack} onNext={onNext} nextDisabled={!value}>
      {MODES.map(m => <ChoiceBtn key={m.value} selected={value === m.value} onClick={() => onSelect(m.value)} icon={m.icon} label={m.label} />)}
    </WizardStep>
  );
}

function StepTools({ value, onToggle, onBack, onNext }) {
  return (
    <WizardStep step={3} title="What tools do you already have?" subtitle="This is optional — helps us skip things you don't need." onBack={onBack} onNext={onNext}>
      {TOOLS.map(t => <ToggleBtn key={t.value} selected={value.includes(t.value)} onClick={() => onToggle(t.value)} icon={t.icon} label={t.label} />)}
    </WizardStep>
  );
}

function StepRecommendations({ goal, mode, tools, bundles, hands, configuringId, onConfigureBundle, onConfigureHand, onBack, onNext }) {
  const modeFilter = mode && mode !== 'general' ? mode : null;
  const rec = modeFilter
    ? bundles.filter(b => b.category === modeFilter)
    : bundles;
  const displayed = rec.length > 0 ? rec : bundles.slice(0, 3);

  return (
    <WizardStep step={4} title="Here's what we'd recommend" subtitle="You can set these up now or come back later." onBack={onBack} onNext={onNext} nextLabel="Looks good →">
      {displayed.map(b => {
        const color = catColor(b.category);
        return (
          <div key={b.id} style={{ border: `1px solid ${color}44`, borderRadius: 10, padding: '14px 16px', marginBottom: 8 }}>
            <div style={{ fontWeight: 700, fontSize: 14, marginBottom: 4 }}>{b.title}</div>
            <div style={{ fontSize: 12, color: 'var(--text-dim,#888)', marginBottom: 8 }}>{b.description}</div>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', marginBottom: 8 }}>
              {b.includesHands.map(h => <span key={h} style={{ fontSize: 11, padding: '2px 7px', borderRadius: 999, background: `${color}18`, color, border: `1px solid ${color}33` }}>{h}</span>)}
            </div>
            <button onClick={() => onConfigureBundle(b)} disabled={configuringId === b.id} style={{ padding: '6px 14px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 12 }}>
              {configuringId === b.id ? 'Setting up…' : 'Set up this bundle'}
            </button>
          </div>
        );
      })}
    </WizardStep>
  );
}

function StepFinish({ onClose }) {
  return (
    <WizardStep step={5} title="You're all set!" subtitle="Your hands are being configured. You can check the status in My Hands." onNext={onClose} nextLabel="Done">
      <div style={{ textAlign: 'center', fontSize: 48, padding: '16px 0' }}>🤝</div>
    </WizardStep>
  );
}

// ─── HandSetupWizard ──────────────────────────────────────────────────────────

function HandSetupWizard({ open, availableHands, onClose, onConfigureBundle, onConfigureHand, onComplete }) {
  const [step, setStep]           = useState(1);
  const [goal, setGoal]           = useState(null);
  const [mode, setMode]           = useState(null);
  const [tools, setTools]         = useState([]);
  const [configuringId, setConf]  = useState(null);

  const toggleTool = (v) => {
    if (v === 'none') { setTools(['none']); return; }
    setTools(prev => {
      const w = prev.filter(x => x !== 'none');
      return w.includes(v) ? w.filter(x => x !== v) : [...w, v];
    });
  };

  const handleBundle = async (b) => {
    setConf(b.id);
    try {
      await onConfigureBundle(b);
    } catch {}
    setConf(null);
    setStep(5);
  };

  const handleClose = () => {
    setStep(1); setGoal(null); setMode(null); setTools([]);
    onClose();
  };

  if (!open) return null;

  return (
    <div
      data-cy="hands-wizard"
      style={{ position: 'fixed', inset: 0, zIndex: 1100, background: 'rgba(0,0,0,0.65)', backdropFilter: 'blur(3px)', display: 'flex', alignItems: 'center', justifyContent: 'center', padding: 24 }}
      onClick={e => { if (e.target === e.currentTarget) handleClose(); }}
    >
      <div style={{ width: '100%', maxWidth: 540, background: 'var(--bg-elevated,#111)', border: '1px solid var(--border,#333)', borderRadius: 14, padding: '28px 32px', maxHeight: '90vh', overflowY: 'auto' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 24 }}>
          <div style={{ fontWeight: 700, fontSize: 13, color: 'var(--text-dim,#888)' }}>Set up hands for me</div>
          <button onClick={handleClose} style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 20, color: 'var(--text-dim,#888)' }}>✕</button>
        </div>
        {step === 1 && <StepGoal value={goal} onSelect={setGoal} onNext={() => setStep(2)} />}
        {step === 2 && <StepMode value={mode} onSelect={setMode} onBack={() => setStep(1)} onNext={() => setStep(3)} />}
        {step === 3 && <StepTools value={tools} onToggle={toggleTool} onBack={() => setStep(2)} onNext={() => setStep(4)} />}
        {step === 4 && <StepRecommendations goal={goal} mode={mode} tools={tools} bundles={HAND_STARTERS} hands={availableHands} configuringId={configuringId} onConfigureBundle={handleBundle} onConfigureHand={() => {}} onBack={() => setStep(3)} onNext={() => setStep(5)} />}
        {step === 5 && <StepFinish onClose={handleClose} />}
      </div>
    </div>
  );
}

// ─── Cards ────────────────────────────────────────────────────────────────────

function CatBadge({ cat }) {
  const c = catColor(cat);
  return <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 999, background: `${c}18`, color: c, border: `1px solid ${c}33`, textTransform: 'capitalize' }}>{cat}</span>;
}

function StatusBadge({ status }) {
  if (status === 'ready') return <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 999, background: 'rgba(34,197,94,.15)', color: '#22c55e', border: '1px solid rgba(34,197,94,.3)' }}>✓ Ready</span>;
  return <span style={{ fontSize: 11, padding: '2px 7px', borderRadius: 999, background: 'rgba(251,191,36,.15)', color: '#fbbf24', border: '1px solid rgba(251,191,36,.3)' }}>⚙ Needs setup</span>;
}

function HandStarterBundleCard({ bundle, configuringId, onSetup }) {
  const color = catColor(bundle.category);
  return (
    <div data-cy="hand-starter-card" style={{ border: `1px solid ${color}33`, borderRadius: 10, padding: '18px 20px', display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>{bundle.title}</div>
          <div style={{ fontSize: 12, color: 'var(--text-dim,#888)', marginTop: 2 }}>{bundle.bestFor}</div>
        </div>
        <CatBadge cat={bundle.category} />
      </div>
      <div style={{ fontSize: 13, color: 'var(--text-secondary,#ccc)', lineHeight: 1.5 }}>{bundle.description}</div>
      <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
        {bundle.includesHands.map(h => <span key={h} style={{ fontSize: 11, padding: '2px 7px', borderRadius: 999, background: 'rgba(255,255,255,.06)', border: '1px solid var(--border,#333)' }}>{h}</span>)}
      </div>
      <div style={{ fontSize: 12, color: 'var(--text-dim,#888)' }}>Needs: {bundle.requires.join(', ')}</div>
      <button
        data-cy="use-hand-bundle-btn"
        onClick={onSetup}
        disabled={configuringId === bundle.id}
        style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13, alignSelf: 'flex-start' }}
      >
        {configuringId === bundle.id ? 'Setting up…' : 'Set up this bundle'}
      </button>
    </div>
  );
}

function HandCardSimple({ hand, configuringId, onSetup }) {
  return (
    <div data-cy="hand-card-simple" style={{ border: '1px solid var(--border,#333)', borderRadius: 8, padding: '12px 16px', display: 'flex', gap: 12, alignItems: 'center' }}>
      <span style={{ fontSize: 24, flexShrink: 0 }}>{hand.icon}</span>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ fontWeight: 600, fontSize: 14 }}>{hand.name}</div>
        <div style={{ display: 'flex', gap: 6, marginTop: 4, flexWrap: 'wrap', alignItems: 'center' }}>
          <StatusBadge status={hand.status} />
          <CatBadge cat={hand.category} />
        </div>
      </div>
      {hand.status === 'needs_setup' && (
        <button
          data-cy="hand-setup-btn"
          onClick={onSetup}
          disabled={configuringId === hand.id}
          style={{ padding: '6px 12px', borderRadius: 6, background: 'transparent', border: '1px solid var(--accent,#7c3aed)', color: 'var(--accent,#7c3aed)', cursor: 'pointer', fontWeight: 600, fontSize: 12, flexShrink: 0 }}
        >
          {configuringId === hand.id ? '…' : 'Set up'}
        </button>
      )}
    </div>
  );
}

// ─── Tabs ─────────────────────────────────────────────────────────────────────

function RecommendedHandsTab({ hands, configuringId, onOpenWizard, onSetupBundle }) {
  const ready = hands.filter(h => h.status === 'ready');
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 28 }}>
      <div style={{ padding: '20px 24px', background: 'rgba(124,58,237,.07)', border: '1px solid rgba(124,58,237,.28)', borderRadius: 10, display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12 }}>
        <div>
          <div style={{ fontWeight: 700, fontSize: 15 }}>Not sure where to start?</div>
          <div style={{ fontSize: 13, color: 'var(--text-dim,#888)', marginTop: 3 }}>{"Tell us your goal and we'll choose the right hands for you."}</div>
        </div>
        <button data-cy="open-wizard-from-rec" onClick={onOpenWizard} style={{ padding: '8px 18px', borderRadius: 8, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
          Set up for me
        </button>
      </div>
      <div>
        <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 14 }}>Starter bundles</div>
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
          {HAND_STARTERS.map(b => (
            <HandStarterBundleCard key={b.id} bundle={b} configuringId={configuringId} onSetup={() => onSetupBundle(b)} />
          ))}
        </div>
      </div>
      {ready.length > 0 && (
        <div>
          <div style={{ fontSize: 13, fontWeight: 700, marginBottom: 10 }}>Your hands that are ready</div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {ready.map(h => <HandCardSimple key={h.id} hand={h} configuringId={configuringId} onSetup={() => {}} />)}
          </div>
        </div>
      )}
    </div>
  );
}

function MyHandsTab({ hands, configuringId, onSetup, onOpenWizard }) {
  if (hands.length === 0) {
    return (
      <div data-cy="hands-empty" style={{ padding: '48px 24px', textAlign: 'center', border: '1px dashed var(--border,#333)', borderRadius: 10 }}>
        <div style={{ fontSize: 36, marginBottom: 12 }}>🤝</div>
        <div style={{ fontSize: 17, fontWeight: 700, marginBottom: 6 }}>No hands configured yet</div>
        <div style={{ fontSize: 13, color: 'var(--text-dim,#888)', maxWidth: 360, margin: '0 auto 24px', lineHeight: 1.6 }}>Hands are ready-made operator bundles that combine an agent role, skills, tools, and settings.</div>
        <button data-cy="empty-open-wizard" onClick={onOpenWizard} style={{ padding: '9px 20px', borderRadius: 8, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 14 }}>
          Set up for me
        </button>
      </div>
    );
  }
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      {hands.map(h => <HandCardSimple key={h.id} hand={h} configuringId={configuringId} onSetup={() => onSetup(h.id)} />)}
    </div>
  );
}

function HandTemplatesTab({ configuringId, onSetupBundle }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <div style={{ fontSize: 13, color: 'var(--text-dim,#888)' }}>{"Pick a bundle and we'll configure the hands for you. You can customise after."}</div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(280px, 1fr))', gap: 12 }}>
        {HAND_STARTERS.map(b => (
          <HandStarterBundleCard key={b.id} bundle={b} configuringId={configuringId} onSetup={() => onSetupBundle(b)} />
        ))}
      </div>
    </div>
  );
}

function AdvancedHandsTab({ hands, loading, onOpenWizard }) {
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
        <button onClick={onOpenWizard} style={{ padding: '6px 14px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
          + Set up hands (guided)
        </button>
      </div>
      {hands.length === 0 && !loading && (
        <div data-cy="hands-empty" className="empty-state">No hands available.</div>
      )}
      {hands.length > 0 && (
        <div data-cy="hands-grid" className="grid grid-auto" style={{ gap: 16 }}>
          {hands.map(hand => (
            <div key={hand.id} data-cy="hand-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 10, opacity: hand.requirements_met ? 1 : 0.85 }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 10 }}>
                <span style={{ fontSize: 28, lineHeight: 1, flexShrink: 0 }}>{hand.icon}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6, flexWrap: 'wrap' }}>
                    <span style={{ fontWeight: 700, fontSize: 14 }}>{hand.name}</span>
                    {hand.requirements_met
                      ? <span className="badge badge-success" style={{ fontSize: 11 }}>Ready</span>
                      : <span className="badge badge-warn" style={{ fontSize: 11 }}>Needs setup</span>}
                  </div>
                  {hand.description && <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 3 }}>{hand.description}</div>}
                </div>
              </div>
              {hand.raw?.requirements?.length > 0 && (
                <div>
                  <div data-cy="hand-requirements-section" style={{ fontSize: 11 }}>
                    <ul data-cy="hand-requirements-section" style={{ margin: 0, padding: '0 0 0 16px', listStyle: 'disc' }}>
                      {hand.raw.requirements.map((r, ri) => <li key={ri} style={{ fontSize: 11, color: 'var(--text-dim)' }}>{r}</li>)}
                    </ul>
                  </div>
                </div>
              )}
              {hand.raw?.tools?.length > 0 && (
                <div data-cy="hand-tools-section" style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                  Tools: {hand.raw.tools.join(', ')}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── HandsPageV2 — main export ────────────────────────────────────────────────

export default function HandsPageV2({ initialHands }) {
  const [activeTab,     setActiveTab]     = useState('recommended');
  const [hands,         setHands]         = useState((initialHands ?? []).map(normalizeHand));
  const [loading,       setLoading]       = useState(false);
  const [error,         setError]         = useState('');
  const [configuringId, setConfiguringId] = useState(null);
  const [wizardOpen,    setWizardOpen]    = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get('/api/hands');
      const raw = Array.isArray(data?.hands) ? data.hands : Array.isArray(data) ? data : [];
      setHands(raw.map(normalizeHand));
    } catch (e) {
      setError(e.message || 'Could not load hands.');
    }
    setLoading(false);
  }, []);

  const setupHand = useCallback(async (handId) => {
    setConfiguringId(handId);
    setError('');
    try {
      await apiClient.post(`/api/hands/${handId}/setup`, {});
      track('hand_configured', { handId });
      await refresh();
    } catch (e) {
      setError(e.message || 'Could not configure hand.');
    }
    setConfiguringId(null);
  }, [refresh]);

  const setupBundle = useCallback(async (bundle) => {
    setConfiguringId(bundle.id);
    setError('');
    try {
      for (const handId of bundle.setupPayload.hand_ids) {
        await apiClient.post(`/api/hands/${handId}/setup`, {}).catch(() => {});
      }
      track('hand_bundle_configured', { bundleId: bundle.id });
      await refresh();
      setActiveTab('my');
    } catch (e) {
      setError(e.message || `Could not set up "${bundle.title}". The backend may not support hands setup yet.`);
    }
    setConfiguringId(null);
  }, [refresh]);

  const ready = hands.filter(h => h.status === 'ready');
  const needsSetup = hands.filter(h => h.status === 'needs_setup');

  const TABS = [
    { key: 'recommended', label: 'Recommended' },
    { key: 'my',          label: `My hands${hands.length > 0 ? ` (${hands.length})` : ''}` },
    { key: 'templates',   label: 'Templates' },
    { key: 'advanced',    label: 'Advanced' },
  ];

  return (
    <div data-cy="hands-page">
      <HandSetupWizard
        open={wizardOpen}
        availableHands={hands}
        onClose={() => setWizardOpen(false)}
        onConfigureBundle={setupBundle}
        onConfigureHand={setupHand}
        onComplete={refresh}
      />

      <div className="page-header">
        <div>
          <h1 style={{ margin: 0 }}>Hands</h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim,#888)', margin: '4px 0 0' }}>
            Ready-made operator bundles — agent role, skills, tools, and settings in one.
          </p>
        </div>
        <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
          {ready.length > 0 && <span className="badge badge-success">{ready.length} ready</span>}
          {needsSetup.length > 0 && <span className="badge badge-muted">{needsSetup.length} needs setup</span>}
          <button data-cy="open-wizard-btn" onClick={() => setWizardOpen(true)} style={{ padding: '7px 14px', borderRadius: 6, background: 'var(--accent,#7c3aed)', color: '#fff', border: 'none', cursor: 'pointer', fontWeight: 600, fontSize: 13 }}>
            Set up for me
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>{loading ? 'Refreshing…' : 'Refresh'}</button>
        </div>
      </div>

      {error && (
        <div data-cy="hands-error" className="error-state" style={{ margin: '0 0 16px' }}>
          ⚠ {error}
          <button className="btn btn-ghost btn-sm" onClick={() => setError('')} style={{ marginLeft: 8 }}>Dismiss</button>
        </div>
      )}

      <div style={{ display: 'flex', borderBottom: '1px solid var(--border,#333)', marginBottom: 20 }}>
        {TABS.map(tab => (
          <button
            key={tab.key}
            data-cy={`hands-tab-${tab.key}`}
            onClick={() => setActiveTab(tab.key)}
            style={{ padding: '10px 14px', background: 'transparent', border: 'none', borderBottom: `2px solid ${activeTab === tab.key ? 'var(--accent,#7c3aed)' : 'transparent'}`, color: activeTab === tab.key ? 'var(--text-primary,#fff)' : 'var(--text-dim,#888)', cursor: 'pointer', fontSize: 13, fontWeight: activeTab === tab.key ? 700 : 400 }}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="page-body">
        {activeTab === 'recommended' && <RecommendedHandsTab hands={hands} configuringId={configuringId} onOpenWizard={() => setWizardOpen(true)} onSetupBundle={setupBundle} />}
        {activeTab === 'my'          && <MyHandsTab hands={hands} configuringId={configuringId} onSetup={setupHand} onOpenWizard={() => setWizardOpen(true)} />}
        {activeTab === 'templates'   && <HandTemplatesTab configuringId={configuringId} onSetupBundle={setupBundle} />}
        {activeTab === 'advanced'    && <AdvancedHandsTab hands={hands} loading={loading} onOpenWizard={() => setWizardOpen(true)} />}
      </div>
    </div>
  );
}
