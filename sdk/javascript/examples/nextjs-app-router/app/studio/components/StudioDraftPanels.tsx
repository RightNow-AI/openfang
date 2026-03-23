'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState } from 'react';

import ProviderCredentialManager from '../../agent-catalog/ProviderCredentialManager';
import { getProvidersByIds, STUDIO_VISUAL_PROVIDER_IDS, STUDIO_VOICE_PROVIDER_IDS } from '../../lib/provider-directory';
import { useDraftEvents } from '../../../hooks/useDraftEvents';
import type { StudioArtifact, StudioDraftRecord, StudioWorkspaceRecord } from '../lib/studio-types';

type PanelProps = {
  workspace: StudioWorkspaceRecord;
  draft: StudioDraftRecord;
};

const voiceProviderOptions = getProvidersByIds(STUDIO_VOICE_PROVIDER_IDS);
const visualProviderOptions = getProvidersByIds(STUDIO_VISUAL_PROVIDER_IDS);

function stageHref(workspaceId: string, draftId: string, stage: string) {
  return `/studio/${workspaceId}/drafts/${draftId}/${stage}`;
}

function cardStyle() {
  return {
    borderRadius: 22,
    padding: 22,
    border: '1px solid rgba(255,255,255,0.08)',
    background: 'rgba(255,255,255,0.03)',
  } as const;
}

function primaryButtonStyle(enabled = true) {
  return {
    padding: '12px 18px',
    borderRadius: 16,
    border: 'none',
    background: '#f97316',
    color: '#fff',
    fontWeight: 800,
    cursor: enabled ? 'pointer' : 'not-allowed',
    opacity: enabled ? 1 : 0.6,
  } as const;
}

function secondaryButtonStyle() {
  return {
    padding: '12px 18px',
    borderRadius: 16,
    border: '1px solid rgba(255,255,255,0.12)',
    background: 'transparent',
    color: 'inherit',
    fontWeight: 700,
    cursor: 'pointer',
  } as const;
}

function initialArtifacts(draft: StudioDraftRecord) {
  return [
    draft.artifacts.research,
    draft.artifacts.script,
    draft.artifacts.voice,
    ...(draft.artifacts.visuals ?? []),
    draft.artifacts.previewRender,
    draft.artifacts.finalRender,
  ].filter(Boolean) as StudioArtifact[];
}

function progressPanel(label: string, progress: number) {
  return (
    <div style={{ ...cardStyle(), border: '1px solid rgba(249,115,22,0.24)', background: 'rgba(249,115,22,0.08)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 10 }}>
        <div style={{ fontWeight: 800 }}>{label}</div>
        <div style={{ color: '#fdba74', fontWeight: 800 }}>{progress}%</div>
      </div>
      <div style={{ height: 10, borderRadius: 999, background: 'rgba(255,255,255,0.08)', overflow: 'hidden' }}>
        <div style={{ width: `${progress}%`, height: '100%', background: 'linear-gradient(90deg, #f97316, #fb923c)' }} />
      </div>
    </div>
  );
}

function messageBox(message: string, tone: 'error' | 'info' = 'info') {
  return (
    <div style={{ ...cardStyle(), border: tone === 'error' ? '1px solid rgba(249,115,22,0.28)' : '1px solid rgba(255,255,255,0.08)', background: tone === 'error' ? 'rgba(249,115,22,0.1)' : 'rgba(255,255,255,0.03)', color: tone === 'error' ? '#fdba74' : 'inherit' }}>
      {message}
    </div>
  );
}

function workspaceProviderOverridePanel(workspaceId: string, providerId: string, label: string) {
  return (
    <div style={{ padding: 16, borderRadius: 18, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(255,255,255,0.02)' }}>
      <ProviderCredentialManager
        workspaceId={workspaceId}
        providerId={providerId}
        title={`${label} override`}
        description="Save a workspace-scoped provider key here to override the global Agent Catalog vault for this studio workspace only."
        connectedLabel="Using workspace override"
        compact
      />
    </div>
  );
}

function artifactFromList(artifacts: StudioArtifact[], type: string) {
  return artifacts.find((artifact) => artifact.artifactType === type) ?? null;
}

function scriptData(artifact: StudioArtifact | null | undefined) {
  return (artifact?.json ?? {}) as {
    hook?: string;
    body?: string;
    cta?: string;
    wordCount?: number;
    tone?: string;
    selectedAngle?: string;
  };
}

export function ResearchStagePanel({ workspace, draft }: PanelProps) {
  const router = useRouter();
  const [selectedAngle, setSelectedAngle] = useState<number | null>(null);
  const [isRequesting, setIsRequesting] = useState(false);
  const [isApproving, setIsApproving] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });

  const researchArtifact = artifactFromList(artifacts, 'ResearchPack') ?? draft.artifacts.research ?? null;
  const researchJson = (researchArtifact?.json ?? {}) as { candidates?: Array<{ title?: string; rationale?: string; hook?: string }>; selectedAngleIndex?: number };
  const selected = selectedAngle ?? (typeof researchJson.selectedAngleIndex === 'number' ? researchJson.selectedAngleIndex : null);
  const isGenerating = activeJob?.stage === 'research' || isRequesting;

  async function startResearch() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/research/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ sources: ['youtube_trends', 'internal_library'], maxCandidates: 3 }),
      });
      if (!response.ok) throw new Error('Failed to wake up the research engine.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to wake up the research engine.');
      setIsRequesting(false);
    }
  }

  async function approveResearch() {
    if (selected == null) return;
    setIsApproving(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ stage: 'research', selectedAngleIndex: selected }),
      });
      if (!response.ok) throw new Error('Failed to approve the research angle.');
      router.push(stageHref(workspace.id, draft.id, 'script'));
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to approve the research angle.');
      setIsApproving(false);
    }
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'minmax(280px, 0.82fr) minmax(0, 1.18fr)', gap: 20 }}>
      <section style={cardStyle()}>
        <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 10 }}>Original idea</div>
        <div style={{ fontSize: 22, fontWeight: 800, marginBottom: 12 }}>{draft.topic}</div>
        <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', lineHeight: 1.6, marginBottom: 18 }}>
          Playbook: {draft.playbook.replace(/_/g, ' ')}. Target runtime: {draft.targetDurationSec} seconds.
        </div>
        <button onClick={startResearch} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
          {isGenerating ? 'Research running…' : researchArtifact ? 'Rerun research' : 'Start deep research'}
        </button>
      </section>

      <section style={{ display: 'grid', gap: 16 }}>
        {isGenerating ? progressPanel('Research engine', progress) : null}
        {!researchArtifact && !isGenerating ? messageBox('No research pack yet. Start the engine to generate three angles and hooks.') : null}
        {researchArtifact ? (
          <div style={{ ...cardStyle(), display: 'grid', gap: 14 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
              <div>
                <div style={{ fontSize: 20, fontWeight: 800 }}>Select an angle</div>
                <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', marginTop: 6 }}>Choose the frame that should flow into the script stage.</div>
              </div>
              <button onClick={startResearch} style={secondaryButtonStyle()}>Rerun</button>
            </div>
            {(researchJson.candidates ?? []).map((candidate, index) => {
              const active = selected === index;
              return (
                <button key={`${candidate.title}-${index}`} type="button" onClick={() => setSelectedAngle(index)} style={{ textAlign: 'left', padding: 18, borderRadius: 18, border: active ? '1px solid rgba(249,115,22,0.48)' : '1px solid rgba(255,255,255,0.08)', background: active ? 'rgba(249,115,22,0.1)' : 'rgba(255,255,255,0.03)', color: 'inherit', cursor: 'pointer' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 8 }}>
                    <div style={{ fontSize: 17, fontWeight: 800 }}>{candidate.title}</div>
                    {active ? <span style={{ fontSize: 12, fontWeight: 800, color: '#fdba74' }}>Selected</span> : null}
                  </div>
                  <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.7)', lineHeight: 1.55, marginBottom: 10 }}>{candidate.rationale}</div>
                  <div style={{ padding: 12, borderRadius: 14, background: 'rgba(10,14,22,0.62)', fontSize: 14, fontStyle: 'italic' }}>
                    “{candidate.hook}”
                  </div>
                </button>
              );
            })}
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button onClick={approveResearch} style={primaryButtonStyle(selected != null && !isApproving)} disabled={selected == null || isApproving}>
                {isApproving ? 'Locking choice…' : 'Approve and write script →'}
              </button>
            </div>
          </div>
        ) : null}
        {localError || error ? messageBox(localError || error || '', 'error') : null}
      </section>
    </div>
  );
}

export function ScriptStagePanel({ workspace, draft }: PanelProps) {
  const router = useRouter();
  const [tone, setTone] = useState('curious');
  const [includeCta, setIncludeCta] = useState(true);
  const [isRequesting, setIsRequesting] = useState(false);
  const [isApproving, setIsApproving] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });

  const scriptArtifact = artifactFromList(artifacts, 'ScriptVersion') ?? draft.artifacts.script ?? null;
  const researchArtifact = artifactFromList(artifacts, 'ResearchPack') ?? draft.artifacts.research ?? null;
  const researchJson = (researchArtifact?.json ?? {}) as { selectedAngle?: string; candidates?: Array<{ title?: string }> };
  const script = scriptData(scriptArtifact);
  const isGenerating = activeJob?.stage === 'script' || isRequesting;

  async function generateScript() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/script/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ tone, includeCta, targetDurationSec: draft.targetDurationSec }),
      });
      if (!response.ok) throw new Error('Failed to start the scriptwriter.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to start the scriptwriter.');
      setIsRequesting(false);
    }
  }

  async function approveScript() {
    setIsApproving(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/approve`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ stage: 'script' }),
      });
      if (!response.ok) throw new Error('Failed to approve the script.');
      router.push(stageHref(workspace.id, draft.id, 'voice'));
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to approve the script.');
      setIsApproving(false);
    }
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'minmax(260px, 0.82fr) minmax(0, 1.18fr)', gap: 20 }}>
      <section style={{ ...cardStyle(), display: 'grid', gap: 16, alignContent: 'start' }}>
        <div>
          <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 8 }}>Approved angle</div>
          <div style={{ fontSize: 18, fontWeight: 800 }}>{researchJson.selectedAngle || researchJson.candidates?.[0]?.title || draft.topic}</div>
        </div>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Tone</span>
          <select value={tone} onChange={(event) => setTone(event.target.value)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            <option value="curious">Curious and educational</option>
            <option value="dramatic">Dramatic and intense</option>
            <option value="casual">Casual and conversational</option>
            <option value="urgent">Urgent and punchy</option>
          </select>
        </label>
        <label style={{ display: 'flex', gap: 10, alignItems: 'center', fontSize: 14 }}>
          <input type="checkbox" checked={includeCta} onChange={(event) => setIncludeCta(event.target.checked)} />
          Include call to action
        </label>
        <button onClick={generateScript} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
          {isGenerating ? 'Writing script…' : scriptArtifact ? 'Rewrite script' : 'Generate script'}
        </button>
      </section>

      <section style={{ display: 'grid', gap: 16 }}>
        {isGenerating ? progressPanel('Script engine', progress) : null}
        {!scriptArtifact && !isGenerating ? messageBox('No script version yet. Pick a tone and generate the first pass.') : null}
        {scriptArtifact ? (
          <div style={{ ...cardStyle(), display: 'grid', gap: 16 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
              <div style={{ fontSize: 20, fontWeight: 800 }}>Script version</div>
              <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>{script.wordCount ?? draft.targetDurationSec} words · {script.tone ?? tone}</div>
            </div>
            <div style={{ padding: 16, borderRadius: 18, background: 'rgba(250,204,21,0.08)', border: '1px solid rgba(250,204,21,0.12)' }}>
              <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 0.8, color: '#fde68a', marginBottom: 8 }}>Hook</div>
              <div style={{ fontSize: 18, fontWeight: 800 }}>{script.hook}</div>
            </div>
            <div style={{ padding: 16, borderRadius: 18, background: 'rgba(255,255,255,0.03)', fontSize: 15, lineHeight: 1.7, whiteSpace: 'pre-wrap' }}>
              {script.body}
            </div>
            {script.cta ? (
              <div style={{ padding: 16, borderRadius: 18, background: 'rgba(59,130,246,0.08)', border: '1px solid rgba(59,130,246,0.14)' }}>
                <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 0.8, color: '#93c5fd', marginBottom: 8 }}>CTA</div>
                <div style={{ fontSize: 16, fontWeight: 700 }}>{script.cta}</div>
              </div>
            ) : null}
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button onClick={approveScript} style={primaryButtonStyle(Boolean(scriptArtifact) && !isApproving)} disabled={!scriptArtifact || isApproving}>
                {isApproving ? 'Saving…' : 'Approve and pick voice →'}
              </button>
            </div>
          </div>
        ) : null}
        {localError || error ? messageBox(localError || error || '', 'error') : null}
      </section>
    </div>
  );
}

export function VoiceStagePanel({ workspace, draft }: PanelProps) {
  const [provider, setProvider] = useState('elevenlabs');
  const [voiceId, setVoiceId] = useState('adam');
  const [speed, setSpeed] = useState(1.05);
  const [isRequesting, setIsRequesting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });
  const voiceArtifact = artifactFromList(artifacts, 'VoiceTrack') ?? draft.artifacts.voice ?? null;
  const isGenerating = activeJob?.stage === 'voice' || isRequesting;

  async function generateVoice() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/voice/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, voiceId, speed }),
      });
      if (!response.ok) throw new Error('Failed to generate the voice track.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to generate the voice track.');
      setIsRequesting(false);
    }
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'minmax(260px, 0.78fr) minmax(0, 1.22fr)', gap: 20 }}>
      <section style={{ ...cardStyle(), display: 'grid', gap: 16, alignContent: 'start' }}>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Voice engine</span>
          <select value={provider} onChange={(event) => setProvider(event.target.value)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            {voiceProviderOptions.map((option) => (
              <option key={option.id} value={option.id}>{option.name}</option>
            ))}
          </select>
        </label>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Narrator voice</span>
          <select value={voiceId} onChange={(event) => setVoiceId(event.target.value)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            <option value="adam">Adam</option>
            <option value="bella">Bella</option>
            <option value="onyx">Onyx</option>
          </select>
        </label>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Playback speed</span>
          <input type="range" min="0.9" max="1.2" step="0.01" value={speed} onChange={(event) => setSpeed(Number(event.target.value))} />
          <span style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>{speed.toFixed(2)}x</span>
        </label>
        <button onClick={generateVoice} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
          {isGenerating ? 'Generating voice…' : voiceArtifact ? 'Regenerate voice' : 'Generate voice track'}
        </button>
        {workspaceProviderOverridePanel(workspace.id, provider, 'Voice provider')}
      </section>
      <section style={{ display: 'grid', gap: 16 }}>
        {isGenerating ? progressPanel('Voice engine', progress) : null}
        {voiceArtifact ? (
          <div style={cardStyle()}>
            <div style={{ fontSize: 20, fontWeight: 800, marginBottom: 8 }}>Voice track ready</div>
            <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', marginBottom: 16 }}>Narration metadata is attached to the draft. Rust media jobs can swap this stub for a real waveform later.</div>
            <Link href={stageHref(workspace.id, draft.id, 'visuals')} style={{ textDecoration: 'none', display: 'inline-flex', ...primaryButtonStyle(true) }}>
              Continue to visuals →
            </Link>
          </div>
        ) : !isGenerating ? messageBox('No narration yet. Generate a voice track to unlock the visuals stage.') : null}
        {localError || error ? messageBox(localError || error || '', 'error') : null}
      </section>
    </div>
  );
}

export function VisualsStagePanel({ workspace, draft }: PanelProps) {
  const [provider, setProvider] = useState('runway');
  const [style, setStyle] = useState('documentary');
  const [sceneCount, setSceneCount] = useState(4);
  const [isRequesting, setIsRequesting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });
  const visualArtifacts = artifacts.filter((artifact) => artifact.artifactType === 'ImageAsset' || artifact.artifactType === 'ScenePlan');
  const visuals = visualArtifacts.length > 0 ? visualArtifacts : draft.artifacts.visuals ?? [];
  const isGenerating = activeJob?.stage === 'visuals' || isRequesting;

  async function generateVisuals() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/visuals/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ provider, style, sceneCount }),
      });
      if (!response.ok) throw new Error('Failed to generate visuals.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to generate visuals.');
      setIsRequesting(false);
    }
  }

  return (
    <div style={{ display: 'grid', gap: 20 }}>
      <section style={{ ...cardStyle(), display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(190px, 1fr))', gap: 16 }}>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Visual engine</span>
          <select value={provider} onChange={(event) => setProvider(event.target.value)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            {visualProviderOptions.map((option) => (
              <option key={option.id} value={option.id}>{option.name}</option>
            ))}
          </select>
        </label>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Visual style</span>
          <select value={style} onChange={(event) => setStyle(event.target.value)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            <option value="documentary">Documentary</option>
            <option value="cinematic">Cinematic</option>
            <option value="anime">Anime</option>
            <option value="3d_animation">3D animation</option>
          </select>
        </label>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Scene count</span>
          <input type="number" min={3} max={8} value={sceneCount} onChange={(event) => setSceneCount(Number(event.target.value) || 4)} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }} />
        </label>
        <div style={{ display: 'grid', alignItems: 'end' }}>
          <button onClick={generateVisuals} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
            {isGenerating ? 'Generating visuals…' : visuals.length > 0 ? 'Regenerate visuals' : 'Generate visuals'}
          </button>
        </div>
      </section>
      {workspaceProviderOverridePanel(workspace.id, provider, 'Visual provider')}
      {isGenerating ? progressPanel('Visual generation', progress) : null}
      {visuals.length > 0 ? (
        <section style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(240px, 1fr))', gap: 16 }}>
          {visuals.map((artifact) => (
            <article key={artifact.id} style={{ ...cardStyle(), padding: 14 }}>
              <div style={{ aspectRatio: '9 / 16', borderRadius: 18, overflow: 'hidden', background: 'rgba(255,255,255,0.04)', marginBottom: 12 }}>
                {artifact.posterUrl || artifact.url ? <img src={artifact.posterUrl || artifact.url || ''} alt={artifact.label || 'Draft visual'} style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : null}
              </div>
              <div style={{ fontSize: 15, fontWeight: 800, marginBottom: 6 }}>{artifact.label || 'Scene'}</div>
              <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>{String((artifact.json as { style?: string })?.style || style).replace(/_/g, ' ')}</div>
            </article>
          ))}
        </section>
      ) : !isGenerating ? messageBox('No visuals yet. Generate scene art to move into edit.') : null}
      {visuals.length > 0 ? (
        <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
          <Link href={stageHref(workspace.id, draft.id, 'edit')} style={{ textDecoration: 'none', display: 'inline-flex', ...primaryButtonStyle(true) }}>
            Continue to edit →
          </Link>
        </div>
      ) : null}
      {localError || error ? messageBox(localError || error || '', 'error') : null}
    </div>
  );
}

export function EditStagePanel({ workspace, draft }: PanelProps) {
  const [renderMode, setRenderMode] = useState<'preview' | 'final'>('preview');
  const [burnSubtitles, setBurnSubtitles] = useState(true);
  const [isRequesting, setIsRequesting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });
  const previewArtifact = artifactFromList(artifacts, 'PreviewRender') ?? draft.artifacts.previewRender ?? null;
  const finalArtifact = artifactFromList(artifacts, 'FinalRender') ?? draft.artifacts.finalRender ?? null;
  const activeRender = renderMode === 'final' ? finalArtifact : previewArtifact;
  const isGenerating = activeJob?.stage === 'edit' || isRequesting;

  async function generateEdit() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/edit/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ renderMode, burnSubtitles }),
      });
      if (!response.ok) throw new Error('Failed to render the edit.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to render the edit.');
      setIsRequesting(false);
    }
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'minmax(260px, 0.78fr) minmax(0, 1.22fr)', gap: 20 }}>
      <section style={{ ...cardStyle(), display: 'grid', gap: 16 }}>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Render mode</span>
          <select value={renderMode} onChange={(event) => setRenderMode(event.target.value === 'final' ? 'final' : 'preview')} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            <option value="preview">Preview</option>
            <option value="final">Final</option>
          </select>
        </label>
        <label style={{ display: 'flex', gap: 10, alignItems: 'center', fontSize: 14 }}>
          <input type="checkbox" checked={burnSubtitles} onChange={(event) => setBurnSubtitles(event.target.checked)} />
          Burn subtitles into the render
        </label>
        <button onClick={generateEdit} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
          {isGenerating ? 'Rendering…' : activeRender ? 'Rerender cut' : 'Generate preview render'}
        </button>
      </section>
      <section style={{ display: 'grid', gap: 16 }}>
        {isGenerating ? progressPanel('Edit engine', progress) : null}
        {activeRender ? (
          <div style={cardStyle()}>
            <div style={{ aspectRatio: '9 / 16', borderRadius: 20, overflow: 'hidden', background: 'rgba(255,255,255,0.04)', marginBottom: 14 }}>
              {activeRender.posterUrl ? <img src={activeRender.posterUrl} alt={activeRender.label || 'Rendered preview'} style={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : null}
            </div>
            <div style={{ fontSize: 20, fontWeight: 800, marginBottom: 8 }}>{activeRender.label}</div>
            <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', marginBottom: 14 }}>Preview the vertical framing now, then move directly into publish.</div>
            <Link href={stageHref(workspace.id, draft.id, 'publish')} style={{ textDecoration: 'none', display: 'inline-flex', ...primaryButtonStyle(true) }}>
              Continue to publish →
            </Link>
          </div>
        ) : !isGenerating ? messageBox('No render yet. Generate a preview or final cut before publishing.') : null}
        {localError || error ? messageBox(localError || error || '', 'error') : null}
      </section>
    </div>
  );
}

export function PublishStagePanel({ draft }: PanelProps) {
  const [mode, setMode] = useState<'immediate' | 'schedule'>('immediate');
  const [isRequesting, setIsRequesting] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);
  const { activeJob, progress, artifacts, error, currentStatus } = useDraftEvents(draft.id, {
    initialArtifacts: initialArtifacts(draft),
    initialStage: draft.stage,
    initialStatus: draft.status,
  });
  const finalArtifact = artifactFromList(artifacts, 'FinalRender') ?? draft.artifacts.finalRender ?? draft.artifacts.previewRender ?? null;
  const isGenerating = activeJob?.stage === 'publish' || isRequesting;

  async function publish() {
    setIsRequesting(true);
    setLocalError(null);
    try {
      const response = await fetch(`/api/studio/drafts/${draft.id}/publish/run`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode }),
      });
      if (!response.ok) throw new Error('Failed to publish the draft.');
    } catch (caughtError) {
      setLocalError(caughtError instanceof Error ? caughtError.message : 'Failed to publish the draft.');
      setIsRequesting(false);
    }
  }

  return (
    <div style={{ display: 'grid', gridTemplateColumns: 'minmax(260px, 0.78fr) minmax(0, 1.22fr)', gap: 20 }}>
      <section style={{ ...cardStyle(), display: 'grid', gap: 16 }}>
        <label style={{ display: 'grid', gap: 8 }}>
          <span style={{ fontSize: 13, fontWeight: 700 }}>Publish mode</span>
          <select value={mode} onChange={(event) => setMode(event.target.value === 'schedule' ? 'schedule' : 'immediate')} style={{ padding: '12px 14px', borderRadius: 14, border: '1px solid rgba(255,255,255,0.12)', background: 'rgba(255,255,255,0.04)', color: 'inherit' }}>
            <option value="immediate">Publish now</option>
            <option value="schedule">Schedule for queue</option>
          </select>
        </label>
        <button onClick={publish} style={primaryButtonStyle(!isGenerating)} disabled={isGenerating}>
          {isGenerating ? 'Publishing…' : mode === 'schedule' ? 'Schedule draft' : 'Publish draft'}
        </button>
      </section>
      <section style={{ display: 'grid', gap: 16 }}>
        {isGenerating ? progressPanel('Publish engine', progress) : null}
        {finalArtifact ? (
          <div style={cardStyle()}>
            <div style={{ fontSize: 20, fontWeight: 800, marginBottom: 8 }}>{currentStatus === 'Published' ? 'Draft published' : currentStatus === 'Queued' ? 'Draft scheduled' : 'Publish package ready'}</div>
            <div style={{ fontSize: 14, color: 'rgba(255,255,255,0.62)', marginBottom: 14 }}>The final render is attached to the draft and the workspace queue has been updated.</div>
            <div style={{ padding: 14, borderRadius: 18, background: 'rgba(255,255,255,0.03)' }}>
              <div style={{ fontSize: 14, fontWeight: 700, marginBottom: 6 }}>{finalArtifact.label}</div>
              <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>Status: {currentStatus}</div>
            </div>
          </div>
        ) : !isGenerating ? messageBox('No publish-ready render yet. Finish the edit stage first.') : null}
        {localError || error ? messageBox(localError || error || '', 'error') : null}
      </section>
    </div>
  );
}