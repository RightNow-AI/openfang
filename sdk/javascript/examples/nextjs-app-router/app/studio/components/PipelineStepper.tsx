'use client';

import Link from 'next/link';

import { useDraftEvents } from '../../../hooks/useDraftEvents';

const STAGES = ['research', 'script', 'voice', 'visuals', 'edit', 'publish'] as const;

function stageTitle(stage: string) {
  return stage.charAt(0).toUpperCase() + stage.slice(1);
}

function stageState(currentStage: string, stage: string) {
  const currentIndex = STAGES.indexOf(currentStage as (typeof STAGES)[number]);
  const stageIndex = STAGES.indexOf(stage as (typeof STAGES)[number]);
  if (stageIndex < currentIndex) return 'complete';
  if (stageIndex === currentIndex) return 'active';
  return 'queued';
}

type Props = {
  draftId: string;
  workspaceId: string;
  initialStage: string;
  initialStatus: string;
};

export default function PipelineStepper({ draftId, workspaceId, initialStage, initialStatus }: Props) {
  const { currentStage, currentStatus, isConnected } = useDraftEvents(draftId, {
    initialStage,
    initialStatus,
  });

  return (
    <section style={{ padding: 20, borderRadius: 22, border: '1px solid rgba(249,115,22,0.16)', background: 'linear-gradient(180deg, rgba(17,24,39,0.92), rgba(10,14,22,0.94))', boxShadow: 'var(--shadow-sm)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap', marginBottom: 14 }}>
        <div>
          <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 6 }}>Pipeline</div>
          <div style={{ fontSize: 18, fontWeight: 800 }}>{stageTitle(currentStage)} stage</div>
        </div>
        <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
          <span style={{ padding: '6px 10px', borderRadius: 999, background: 'rgba(249,115,22,0.16)', color: '#fdba74', fontSize: 12, fontWeight: 700 }}>{currentStatus}</span>
          <span style={{ fontSize: 12, color: isConnected ? '#86efac' : 'rgba(255,255,255,0.56)' }}>{isConnected ? 'Live events connected' : 'Polling fallback'}</span>
        </div>
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(112px, 1fr))', gap: 12 }}>
        {STAGES.map((stage, index) => {
          const status = stageState(currentStage, stage);
          const isActive = status === 'active';
          const isComplete = status === 'complete';
          return (
            <Link
              key={stage}
              href={`/studio/${workspaceId}/drafts/${draftId}/${stage}`}
              style={{
                textDecoration: 'none',
                color: 'inherit',
                padding: 14,
                borderRadius: 18,
                border: isActive ? '1px solid rgba(249,115,22,0.5)' : '1px solid rgba(255,255,255,0.08)',
                background: isActive ? 'rgba(249,115,22,0.1)' : isComplete ? 'rgba(34,197,94,0.08)' : 'rgba(255,255,255,0.03)',
              }}
            >
              <div style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.8, color: 'rgba(255,255,255,0.52)', marginBottom: 8 }}>Step {index + 1}</div>
              <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 6 }}>{stageTitle(stage)}</div>
              <div style={{ fontSize: 12, color: isActive ? '#fdba74' : isComplete ? '#86efac' : 'rgba(255,255,255,0.58)' }}>
                {isActive ? 'In progress' : isComplete ? 'Complete' : 'Queued'}
              </div>
            </Link>
          );
        })}
      </div>
    </section>
  );
}