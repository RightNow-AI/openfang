'use client';

import { useState, useEffect, useCallback, useRef } from 'react';
import BusinessSnapshotSection from './BusinessSnapshotSection';
import AudienceMarketSection from './AudienceMarketSection';
import BrandVoiceSection from './BrandVoiceSection';
import AgentLaunchpadPanel from './AgentLaunchpadPanel';
import OutputsWorkspace from './OutputsWorkspace';
import AgentRunDrawer from './AgentRunDrawer';
import WizardProgress from './WizardProgress';
import WizardStep4Offer from './WizardStep4Offer';
import AutofillReview from './AutofillReview';

const STORAGE_KEY = 'openfang-brand-profile';

export const INITIAL_PROFILE = {
  // Business Snapshot
  business_name: '',
  website_url: '',
  industry: '',
  business_model: '',
  location: '',
  primary_offer: '',
  main_goal_90_days: '',
  // Audience & Market
  ideal_customer: '',
  top_pain_points: ['', '', ''],
  desired_outcomes: ['', '', ''],
  top_objections: ['', '', ''],
  current_acquisition_channels: [],
  top_competitors: [{ name: '', url: '', notes: '' }],
  customer_awareness_level: '',
  // Brand Voice
  brand_traits: [],
  traits_to_avoid: [],
  brand_promise: '',
  liked_examples: [{ type: 'url', value: '', reason_liked: '' }],
  disliked_examples: [{ type: 'url', value: '', reason_disliked: '' }],
  taboo_words: [],
  approved_words: [],
  voice_notes: '',
  // Offers & Funnel
  core_offer: '',
  pricing_model: '',
  primary_cta: '',
  sales_process: '',
  lead_magnet: '',
  proof_assets: [],
};

export function calcCompletion(p) {
  const checks = [
    !!p.business_name,
    !!p.website_url,
    !!p.industry,
    !!p.business_model,
    !!p.primary_offer,
    !!p.main_goal_90_days,
    !!p.ideal_customer,
    (p.top_pain_points || []).some(Boolean),
    (p.desired_outcomes || []).some(Boolean),
    (p.top_objections || []).some(Boolean),
    (p.current_acquisition_channels || []).length > 0,
    (p.top_competitors || []).some(c => c.name),
    (p.brand_traits || []).some(Boolean),
    (p.traits_to_avoid || []).some(Boolean),
    !!p.brand_promise,
    (p.liked_examples || []).some(e => e.value),
    (p.disliked_examples || []).some(e => e.value),
  ];
  const filled = checks.filter(Boolean).length;
  return { score: Math.round((filled / checks.length) * 100), filled, total: checks.length };
}

// ── Wizard config ──────────────────────────────────────────────────────────

const WIZARD_STEPS = [
  { key: 'business',  label: 'Business',    title: 'Tell us about your business',           copy: "Start with the basics. You don't need perfect answers — we'll help fill gaps later." },
  { key: 'customer',  label: 'Customer',    title: 'Who is your customer',                  copy: 'The more specific you are, the better the results.' },
  { key: 'voice',     label: 'Voice',       title: 'How should your brand sound',           copy: 'Pick words that feel like you. Tell us what to avoid.' },
  { key: 'offer',     label: 'Offer',       title: 'What do you sell',                      copy: 'Keep it simple. Say what you sell and what the next step should be.' },
  { key: 'launch',    label: 'Choose Help', title: 'What would you like help with first',   copy: "Great. We have enough to get started. Pick one thing and we'll build it for you." },
  { key: 'results',   label: 'Results',     title: 'Here is what we made',                  copy: 'You can review, approve, edit, or generate the next item.' },
];

function validateStep(stepIndex, profile) {
  switch (stepIndex) {
    case 0: return [
      !profile.business_name    && 'Business name',
      !profile.business_model   && 'Business type',
      !profile.industry         && 'Industry',
      !profile.main_goal_90_days && '90-day goal',
    ].filter(Boolean);
    case 1: return [
      !profile.ideal_customer                                 && 'Ideal customer',
      !(profile.top_pain_points || []).some(Boolean)          && 'At least one problem',
      !(profile.desired_outcomes || []).some(Boolean)         && 'At least one desired result',
      !(profile.top_objections || []).some(Boolean)           && 'At least one objection',
    ].filter(Boolean);
    case 2: return [
      (profile.brand_traits || []).length < 3                 && 'At least 3 brand traits',
      (profile.traits_to_avoid || []).length < 3              && 'At least 3 traits to avoid',
      !profile.brand_promise                                  && 'Brand promise',
      !(profile.liked_examples || []).some(e => e.value)      && '1 liked example',
      !(profile.disliked_examples || []).some(e => e.value)   && '1 disliked example',
    ].filter(Boolean);
    case 3: return [
      !profile.core_offer      && 'Main offer',
      !profile.pricing_model   && 'Pricing model',
      !profile.primary_cta     && 'What people should do next (CTA)',
      !profile.sales_process   && 'Sales process',
    ].filter(Boolean);
    default: return [];
  }
}

// ── Main Component ─────────────────────────────────────────────────────────

export default function BrandCommandCenterClient() {
  const [step, setStep]                   = useState(0);
  const [profile, setProfile]             = useState(INITIAL_PROFILE);
  const [outputs, setOutputs]             = useState([]);
  const [activeOutputId, setActiveOutputId] = useState(null);
  const [runState, setRunState]           = useState(null);
  const [drawerOpen, setDrawerOpen]       = useState(false);
  const [savedAt, setSavedAt]             = useState(null);
  const [validationErrors, setValidationErrors] = useState([]);
  const [autofillState, setAutofillState] = useState(null); // null | 'loading' | { patch, found, missing, error }
  const saveTimer = useRef(null);

  // Load from localStorage on mount
  useEffect(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored) setProfile(prev => ({ ...INITIAL_PROFILE, ...JSON.parse(stored) }));
    } catch { /* ignore */ }
  }, []);

  const updateProfile = useCallback((updates) => {
    setProfile(prev => {
      const next = typeof updates === 'function' ? updates(prev) : { ...prev, ...updates };
      clearTimeout(saveTimer.current);
      saveTimer.current = setTimeout(() => {
        try { localStorage.setItem(STORAGE_KEY, JSON.stringify(next)); setSavedAt(new Date()); }
        catch { /* ignore */ }
      }, 800);
      return next;
    });
    setValidationErrors([]);
  }, []);

  function saveNow(p) {
    try { localStorage.setItem(STORAGE_KEY, JSON.stringify(p)); setSavedAt(new Date()); }
    catch { /* ignore */ }
  }

  function goNext() {
    const errors = validateStep(step, profile);
    if (errors.length) { setValidationErrors(errors); return; }
    setValidationErrors([]);
    setStep(s => Math.min(s + 1, 5));
    window.scrollTo(0, 0);
  }

  function goBack() {
    setValidationErrors([]);
    setStep(s => Math.max(s - 1, 0));
    window.scrollTo(0, 0);
  }

  async function triggerAutofill() {
    if (!profile.website_url) return;
    setAutofillState('loading');
    try {
      const res = await fetch('/api/brand/autofill', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ website_url: profile.website_url }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error || 'Autofill failed');
      setAutofillState(data);
    } catch (err) {
      setAutofillState({ error: err.message });
    }
  }

  async function runTask(task_type) {
    setRunState({ task_type, status: 'running', error: null, output: null });
    setDrawerOpen(true);
    try {
      const res = await fetch('/api/brand/run', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ task_type, brand_profile: profile }),
      });
      const data = await res.json();
      if (!res.ok) {
        setRunState(s => ({ ...s, status: 'failed', error: data.error || 'Agent run failed.' }));
        return;
      }
      const newOutput = {
        id: `out-${Date.now()}`,
        output_type: data.output_type,
        title: data.title,
        content: data.content,
        task_type,
        agent_id: data.agent_id,
        duration_ms: data.duration_ms,
        status: 'ready_for_review',
        created_at: new Date().toISOString(),
      };
      setRunState(s => ({ ...s, status: 'completed', output: newOutput }));
      setOutputs(prev => [newOutput, ...prev]);
      setActiveOutputId(newOutput.id);
      // Auto-advance to Results after a moment
      setTimeout(() => { setDrawerOpen(false); setStep(5); window.scrollTo(0, 0); }, 1800);
    } catch (err) {
      setRunState(s => ({ ...s, status: 'failed', error: err.message || 'Network error — is the daemon running?' }));
    }
  }

  function approveOutput(id) {
    setOutputs(prev => prev.map(o => o.id === id ? { ...o, status: 'approved' } : o));
  }
  function requestRevision(id) {
    setOutputs(prev => prev.map(o => o.id === id ? { ...o, status: 'draft' } : o));
  }
  function duplicateOutput(id) {
    const orig = outputs.find(o => o.id === id);
    if (!orig) return;
    const copy = { ...orig, id: `out-${Date.now()}`, title: `${orig.title} (copy)`, status: 'draft', created_at: new Date().toISOString() };
    setOutputs(prev => [copy, ...prev]);
    setActiveOutputId(copy.id);
  }
  function exportOutput(id) {
    const output = outputs.find(o => o.id === id);
    if (!output) return;
    const blob = new Blob([output.content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `${output.title.toLowerCase().replace(/\s+/g, '-')}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const cfg              = WIZARD_STEPS[step];
  const isFormStep       = step < 4;
  const showAutofillBtn  = step < 4 && !!profile.website_url;
  const activeOutput     = outputs.find(o => o.id === activeOutputId) ?? null;

  return (
    <div style={{ minHeight: '100vh', display: 'flex', flexDirection: 'column', background: 'var(--bg)' }}>

      {/* ── Wizard header ── */}
      <div style={{
        textAlign: 'center', padding: '20px 24px 0',
        borderBottom: '1px solid var(--border)', background: 'var(--bg-elevated)',
      }}>
        <div style={{ fontSize: 11, fontWeight: 700, color: 'var(--accent)', letterSpacing: '1px', textTransform: 'uppercase', marginBottom: 4 }}>
          Brand Command Center
        </div>
        <div style={{ fontSize: 22, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
          Let&apos;s Build Your Brand
        </div>
        <div style={{ fontSize: 13, color: 'var(--text-muted)', marginBottom: 20 }}>
          Answer a few simple questions. We&apos;ll turn your answers into a brand plan, voice guide, and ready-to-use marketing drafts.
        </div>
        <WizardProgress steps={WIZARD_STEPS} currentStep={step} onStepClick={(i) => { if (i < step) { setValidationErrors([]); setStep(i); } }} />
      </div>

      {/* ── Step content ── */}
      <div style={{ flex: 1, padding: isFormStep ? '32px 24px 24px' : '24px' }}>
        <div style={isFormStep ? { maxWidth: 700, margin: '0 auto' } : {}}>

          {/* Step title row */}
          <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, marginBottom: 24 }}>
            <div>
              <h1 style={{ fontSize: 20, fontWeight: 800, color: 'var(--text)', margin: '0 0 6px' }}>{cfg.title}</h1>
              <p style={{ fontSize: 13, color: 'var(--text-muted)', margin: 0 }}>{cfg.copy}</p>
            </div>

            {/* Autofill helper */}
            {showAutofillBtn && (
              <div style={{
                flexShrink: 0, padding: '10px 14px', borderRadius: 8, maxWidth: 210,
                border: '1px dashed var(--border)', background: 'var(--surface2)', textAlign: 'center',
              }}>
                <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--text-dim)', marginBottom: 4 }}>Need help?</div>
                <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 8 }}>We can pull details from your website.</div>
                <button
                  onClick={triggerAutofill}
                  disabled={autofillState === 'loading'}
                  style={{
                    padding: '5px 12px', borderRadius: 5, border: 'none', cursor: 'pointer',
                    background: 'var(--accent)', color: '#fff', fontSize: 12, fontWeight: 600,
                    opacity: autofillState === 'loading' ? 0.6 : 1,
                  }}
                >
                  {autofillState === 'loading' ? 'Working…' : 'Use my website to help'}
                </button>
              </div>
            )}
          </div>

          {/* Validation errors */}
          {validationErrors.length > 0 && (
            <div style={{
              marginBottom: 20, padding: '10px 14px', borderRadius: 7,
              background: 'rgba(239,68,68,0.08)', border: '1px solid rgba(239,68,68,0.25)',
            }}>
              <div style={{ fontSize: 12, fontWeight: 700, color: '#ef4444', marginBottom: 4 }}>
                Please complete these fields before continuing:
              </div>
              <ul style={{ margin: 0, paddingLeft: 18 }}>
                {validationErrors.map((e, i) => (
                  <li key={i} style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.8 }}>{e}</li>
                ))}
              </ul>
            </div>
          )}

          {/* Step-specific content */}
          {step === 0 && <BusinessSnapshotSection profile={profile} onChange={updateProfile} wizardMode />}
          {step === 1 && <AudienceMarketSection   profile={profile} onChange={updateProfile} wizardMode />}
          {step === 2 && <BrandVoiceSection        profile={profile} onChange={updateProfile} wizardMode />}
          {step === 3 && <WizardStep4Offer         profile={profile} onChange={updateProfile} />}
          {step === 4 && <AgentLaunchpadPanel      profile={profile} runState={runState} onRunTask={runTask} />}
          {step === 5 && (
            <OutputsWorkspace
              outputs={outputs}
              activeOutputId={activeOutputId}
              onSelect={setActiveOutputId}
              onApprove={approveOutput}
              onRequestRevision={requestRevision}
              onDuplicate={duplicateOutput}
              onExport={exportOutput}
              activeOutput={activeOutput}
            />
          )}

          {/* ── Nav buttons ── */}
          <div style={{
            display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap',
            marginTop: 28, paddingTop: 20, borderTop: '1px solid var(--border)',
          }}>
            {step > 0 && (
              <button onClick={goBack} style={{
                padding: '9px 18px', borderRadius: 7, border: '1px solid var(--border)',
                background: 'transparent', color: 'var(--text)', cursor: 'pointer', fontSize: 13,
              }}>← Back</button>
            )}
            <button
              onClick={() => saveNow(profile)}
              style={{
                padding: '9px 14px', borderRadius: 7, border: '1px solid var(--border)',
                background: 'transparent', color: 'var(--text-muted)', cursor: 'pointer', fontSize: 12,
              }}
            >Save for later</button>

            <div style={{ flex: 1 }} />

            {savedAt && (
              <span style={{ fontSize: 11, color: 'var(--text-muted)' }}>
                Saved {savedAt.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
              </span>
            )}

            {/* Continue btn (steps 0-3) */}
            {step < 4 && (
              <button onClick={goNext} style={{
                padding: '9px 22px', borderRadius: 7, border: 'none',
                background: 'var(--accent)', color: '#fff', cursor: 'pointer',
                fontSize: 13, fontWeight: 600, boxShadow: 'var(--shadow-accent)',
              }}>Continue →</button>
            )}

            {/* After launchpad: view what was made */}
            {step === 4 && outputs.length > 0 && (
              <button onClick={() => { setStep(5); window.scrollTo(0, 0); }} style={{
                padding: '9px 18px', borderRadius: 7, border: 'none',
                background: 'var(--accent)', color: '#fff', cursor: 'pointer', fontSize: 13, fontWeight: 600,
              }}>View results →</button>
            )}

            {/* From results: generate more */}
            {step === 5 && outputs.length < 5 && (
              <button onClick={() => { setStep(4); window.scrollTo(0, 0); }} style={{
                padding: '9px 18px', borderRadius: 7, border: 'none',
                background: 'var(--accent)', color: '#fff', cursor: 'pointer', fontSize: 13, fontWeight: 600,
              }}>Generate more →</button>
            )}
          </div>

        </div>
      </div>

      {/* ── Autofill review overlay ── */}
      {autofillState && autofillState !== 'loading' && (
        <AutofillReview
          data={autofillState}
          onAccept={(patch) => { updateProfile(patch); setAutofillState(null); }}
          onDismiss={() => setAutofillState(null)}
        />
      )}

      {/* ── Agent run drawer ── */}
      <AgentRunDrawer
        open={drawerOpen}
        runState={runState}
        onClose={() => setDrawerOpen(false)}
        onViewOutput={() => {
          setDrawerOpen(false);
          if (runState?.output?.id) setActiveOutputId(runState.output.id);
          setStep(5);
          window.scrollTo(0, 0);
        }}
      />
    </div>
  );
}
