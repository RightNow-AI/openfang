'use client';
import { useState, useCallback } from 'react';
import { emptyWizardState, canAdvance, needsImage, needsVideo, buildPlanPreview } from './lib/creative-ui';
import CreativeWizardStepType     from './wizard/CreativeWizardStepType';
import CreativeWizardStepBrief    from './wizard/CreativeWizardStepBrief';
import CreativeWizardStepStyle    from './wizard/CreativeWizardStepStyle';
import CreativeWizardStepAiChoices from './wizard/CreativeWizardStepAiChoices';
import CreativeWizardStepReview   from './wizard/CreativeWizardStepReview';
import CreativeWizardStepResults  from './wizard/CreativeWizardStepResults';

const STEPS = [
  { number: 1, label: 'What to create' },
  { number: 2, label: 'About the project' },
  { number: 3, label: 'Style & references' },
  { number: 4, label: 'AI tools' },
  { number: 5, label: 'Review plan' },
  { number: 6, label: 'Results' },
];

function StepIndicator({ current, total }) {
  return (
    <div style={{ display: 'flex', gap: 8, alignItems: 'center', marginBottom: 28, flexWrap: 'wrap' }}>
      {STEPS.map((s, i) => {
        const done    = s.number < current;
        const active  = s.number === current;
        return (
          <div key={s.number} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <div style={{
              width: 26, height: 26,
              borderRadius: '50%',
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              fontSize: 11, fontWeight: 700,
              background: done    ? 'var(--success)' :
                          active  ? 'var(--accent)'  : 'var(--surface2)',
              color: (done || active) ? '#fff' : 'var(--text-muted)',
              flexShrink: 0,
              transition: 'background 0.2s',
            }}>
              {done ? '✓' : s.number}
            </div>
            <span style={{
              fontSize: 12,
              fontWeight: active ? 600 : 400,
              color: active ? 'var(--text)' : done ? 'var(--text-secondary)' : 'var(--text-muted)',
              whiteSpace: 'nowrap',
            }}>
              {s.label}
            </span>
            {i < STEPS.length - 1 && (
              <div style={{ width: 20, height: 1, background: 'var(--border)', flexShrink: 0, marginLeft: 4 }} />
            )}
          </div>
        );
      })}
    </div>
  );
}

export default function CreativeWizard({ initialState, onClose, onSave }) {
  const [state, setState]     = useState(() => initialState ?? emptyWizardState());
  const [project, setProject] = useState(null);
  const [assets, setAssets]   = useState([]);
  const [running, setRunning] = useState(false);
  const [saving,  setSaving]  = useState(false);
  const [error,   setError]   = useState('');

  function setField(key, value) {
    setState(prev => ({ ...prev, [key]: value }));
  }

  const planSteps = buildPlanPreview(state);
  const ni = needsImage(state.creation_type);
  const nv = needsVideo(state.creation_type);

  const goNext = useCallback(() => {
    if (!canAdvance(state)) return;
    setState(prev => ({ ...prev, step: Math.min(prev.step + 1, 6) }));
  }, [state]);

  const goBack = useCallback(() => {
    setState(prev => ({ ...prev, step: Math.max(prev.step - 1, 1) }));
  }, []);

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError('');
    try {
      const body = {
        name: state.name || state.topic || 'Untitled creative project',
        creation_type: state.creation_type,
        goal: state.goal,
        topic: state.topic,
        offer: state.offer,
        audience: state.audience,
        platform: state.platform,
        desired_outcome: state.desired_outcome,
        notes: state.notes,
        style_description: state.style_description,
        visual_keywords: state.visual_keywords_raw.split(',').map(k => k.trim()).filter(Boolean),
        words_to_avoid:  state.words_to_avoid_raw.split(',').map(k => k.trim()).filter(Boolean),
        reference_links: state.reference_links_raw.split('\n').map(l => l.trim()).filter(Boolean),
        aspect_ratio: state.aspect_ratio || null,
        duration:     state.duration || null,
        voice_tone:   state.voice_tone || null,
        ai_choices:   state.ai_choices,
      };
      const res = await fetch('/api/creative-projects', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      });
      const data = await res.json().catch(() => ({}));
      if (!res.ok) throw new Error(data.error || 'Could not save project.');
      setProject(data);
      if (onSave) onSave(data);
    } catch (e) {
      setError(e.message || 'Save failed.');
    }
    setSaving(false);
  }, [state, onSave]);

  const handleApprovePlan = useCallback(async () => {
    if (!project?.id) {
      // Save first, then approve
      await handleSave();
      return;
    }
    setRunning(true);
    setError('');
    try {
      const res = await fetch(`/api/creative-projects/${project.id}/approve`, { method: 'POST' });
      if (!res.ok) throw new Error('Could not approve plan.');
      setState(prev => ({ ...prev, step: 6 }));
      // Poll for results
      pollResults(project.id);
    } catch (e) {
      setError(e.message);
      setRunning(false);
    }
  }, [project, handleSave]);

  const handleGenerateDraft = useCallback(async () => {
    let proj = project;
    if (!proj) {
      setSaving(true);
      try {
        const body = {
          name: state.name || state.topic || 'Untitled creative project',
          creation_type: state.creation_type,
          goal: state.goal,
          topic: state.topic,
          offer: state.offer,
          audience: state.audience,
          platform: state.platform,
          desired_outcome: state.desired_outcome,
          notes: state.notes,
          style_description: state.style_description,
          visual_keywords: state.visual_keywords_raw.split(',').map(k => k.trim()).filter(Boolean),
          words_to_avoid:  state.words_to_avoid_raw.split(',').map(k => k.trim()).filter(Boolean),
          reference_links: state.reference_links_raw.split('\n').map(l => l.trim()).filter(Boolean),
          aspect_ratio: state.aspect_ratio || null,
          duration:     state.duration || null,
          voice_tone:   state.voice_tone || null,
          ai_choices:   state.ai_choices,
        };
        const res = await fetch('/api/creative-projects', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(body),
        });
        const data = await res.json().catch(() => ({}));
        if (!res.ok) throw new Error(data.error || 'Could not create project.');
        proj = data;
        setProject(data);
      } catch (e) {
        setError(e.message);
        setSaving(false);
        return;
      }
      setSaving(false);
    }

    setRunning(true);
    setError('');
    setState(prev => ({ ...prev, step: 6 }));
    try {
      const runRes = await fetch(`/api/creative-projects/${proj.id}/run`, { method: 'POST' });
      if (!runRes.ok) throw new Error('Could not start run.');
      pollResults(proj.id);
    } catch (e) {
      setError(e.message);
      setRunning(false);
    }
  }, [project, state]);

  function pollResults(id) {
    let attempts = 0;
    const MAX = 30;
    const iv = setInterval(async () => {
      attempts++;
      if (attempts > MAX) { clearInterval(iv); setRunning(false); return; }
      try {
        const r = await fetch(`/api/creative-projects/${id}/results`);
        const d = await r.json().catch(() => ({}));
        if (Array.isArray(d.assets)) setAssets(d.assets);
        if (d.status === 'done' || d.status === 'error') {
          setRunning(false);
          if (d.status === 'error') setError(d.error || 'Generation encountered an error.');
          clearInterval(iv);
        }
      } catch { /* keep polling */ }
    }, 3000);
  }

  async function handleApproveAsset(assetId) {
    if (!project?.id) return;
    try {
      await fetch(`/api/creative-projects/${project.id}/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ asset_id: assetId }),
      });
      setAssets(prev => prev.map(a => a.id === assetId ? { ...a, approval_state: 'approved' } : a));
    } catch (e) { setError(e.message); }
  }

  function handleReviseAsset(assetId) {
    setAssets(prev => prev.map(a => a.id === assetId ? { ...a, approval_state: 'rejected' } : a));
  }

  async function handleExport() {
    if (!project?.id) return;
    window.open(`/api/creative-projects/${project.id}/results?format=zip`, '_blank');
  }

  function handleSendToWorkflow() {
    if (project?.id) {
      window.location.href = `/command-center/new?source=creative-project&id=${project.id}`;
    }
  }

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* Header */}
      <div style={{
        padding: '20px 28px 0',
        borderBottom: '1px solid var(--border)',
        paddingBottom: 20,
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'flex-start',
        flexWrap: 'wrap',
        gap: 12,
      }}>
        <div>
          <div style={{ fontWeight: 800, fontSize: 18 }}>
            {state.step < 6 ? '🎨 New creative project' : `🎨 ${state.name || state.topic || 'Creative project'}`}
          </div>
          {state.step < 6 && (
            <div style={{ fontSize: 12, color: 'var(--text-dim)', marginTop: 2 }}>
              Step {state.step} of {STEPS.length} — {STEPS[state.step - 1].label}
            </div>
          )}
        </div>
        {onClose && (
          <button className="btn btn-ghost btn-sm" onClick={onClose}>✕ Close</button>
        )}
      </div>

      {/* Body */}
      <div style={{ flex: 1, overflow: 'auto', padding: '24px 28px' }}>
        <StepIndicator current={state.step} total={STEPS.length} />

        {error && (
          <div className="error-state" style={{ marginBottom: 16 }}>⚠ {error}</div>
        )}

        {state.step === 1 && (
          <CreativeWizardStepType state={state} onChange={setField} />
        )}
        {state.step === 2 && (
          <CreativeWizardStepBrief state={state} onChange={setField} />
        )}
        {state.step === 3 && (
          <CreativeWizardStepStyle state={state} onChange={setField} needsVideo={nv} />
        )}
        {state.step === 4 && (
          <CreativeWizardStepAiChoices state={state} onChange={setField} needsImage={ni} needsVideo={nv} />
        )}
        {state.step === 5 && (
          <CreativeWizardStepReview state={state} planSteps={planSteps} />
        )}
        {state.step === 6 && (
          <CreativeWizardStepResults
            project={project}
            assets={assets}
            running={running}
            error={null}
            onApprove={handleApproveAsset}
            onRevise={handleReviseAsset}
            onExport={handleExport}
            onSendToWorkflow={handleSendToWorkflow}
          />
        )}
      </div>

      {/* Footer nav */}
      {state.step < 6 && (
        <div style={{
          padding: '16px 28px',
          borderTop: '1px solid var(--border)',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          gap: 12,
          flexWrap: 'wrap',
        }}>
          <div>
            {state.step > 1 && (
              <button className="btn btn-ghost btn-sm" onClick={goBack}>
                ← Back
              </button>
            )}
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            {state.step < 5 && (
              <button
                className="btn btn-ghost btn-sm"
                onClick={handleSave}
                disabled={saving || !state.topic}
              >
                {saving ? 'Saving…' : 'Save for later'}
              </button>
            )}
            {state.step < 5 && (
              <button
                data-cy="wizard-next"
                className="btn btn-sm"
                style={{ background: 'var(--accent)', color: '#fff', border: 'none' }}
                onClick={goNext}
                disabled={!canAdvance(state)}
              >
                Next →
              </button>
            )}
            {state.step === 5 && (
              <>
                <button
                  className="btn btn-ghost btn-sm"
                  onClick={handleSave}
                  disabled={saving}
                >
                  {saving ? 'Saving…' : 'Save for later'}
                </button>
                <button
                  data-cy="approve-plan"
                  className="btn btn-sm"
                  style={{ background: 'var(--success)', color: '#fff', border: 'none' }}
                  onClick={handleApprovePlan}
                  disabled={running || saving}
                >
                  Approve plan
                </button>
                <button
                  data-cy="generate-draft"
                  className="btn btn-sm"
                  style={{ background: 'var(--accent)', color: '#fff', border: 'none' }}
                  onClick={handleGenerateDraft}
                  disabled={running || saving}
                >
                  {running ? 'Running…' : 'Generate draft'}
                </button>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
