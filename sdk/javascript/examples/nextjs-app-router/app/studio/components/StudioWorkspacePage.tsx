'use client';

import Link from 'next/link';
import { useRouter } from 'next/navigation';
import { useState, useTransition } from 'react';
import type { StudioWorkspaceDetailPayload } from '../lib/studio-types';

function pillColor(status: string) {
  switch (status) {
    case 'approved':
    case 'completed':
    case 'complete':
      return '#22c55e';
    case 'running':
    case 'active':
      return '#38bdf8';
    case 'pending':
    case 'queued':
      return '#f59e0b';
    case 'changes_requested':
    case 'blocked':
    case 'failed':
      return '#f97316';
    default:
      return 'var(--text-dim)';
  }
}

function formatRelative(timestamp: string) {
  const delta = Math.max(1, Math.round((Date.now() - new Date(timestamp).getTime()) / 60_000));
  if (delta < 60) return `${delta}m ago`;
  const hours = Math.round(delta / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export default function StudioWorkspacePage({ payload }: { payload: StudioWorkspaceDetailPayload }) {
  const router = useRouter();
  const [error, setError] = useState('');
  const [isPending, startTransition] = useTransition();
  const { workspace, drafts, stages, jobs, events, approval } = payload;
  const activeDraft = drafts.find((draft) => draft.id === workspace.active_draft_id) ?? drafts[0] ?? null;

  async function queueRenderJob() {
    setError('');
    const response = await fetch('/api/studio/jobs', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        workspace_id: workspace.id,
        label: `Render ${workspace.title}`,
        job_type: 'render',
        provider: 'runway',
      }),
    });
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
      setError(body.error || 'Could not queue the render job.');
      return;
    }
    startTransition(() => router.refresh());
  }

  async function approveDraft() {
    if (!activeDraft) return;
    setError('');
    const response = await fetch('/api/studio/approvals', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        workspace_id: workspace.id,
        target_type: 'draft',
        target_id: activeDraft.id,
        status: 'approved',
      }),
    });
    const body = await response.json().catch(() => ({}));
    if (!response.ok) {
      setError(body.error || 'Could not approve the active draft.');
      return;
    }
    startTransition(() => router.refresh());
  }

  return (
    <main style={{ padding: '28px 32px 56px', maxWidth: 1240, margin: '0 auto' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 20, flexWrap: 'wrap', marginBottom: 24 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 8 }}>
            <Link href="/studio" style={{ fontSize: 13, color: 'var(--text-dim)', textDecoration: 'none' }}>Studio</Link>
            <span style={{ color: 'var(--text-dim)' }}>/</span>
            <span style={{ fontSize: 13, color: 'var(--text-dim)' }}>{workspace.client_name}</span>
          </div>
          <h1 style={{ margin: 0, fontSize: 28, fontWeight: 800 }}>{workspace.title}</h1>
          <p style={{ margin: '10px 0 0', color: 'var(--text-dim)', fontSize: 15, maxWidth: 760 }}>{workspace.objective}</p>
        </div>

        <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
          <button onClick={queueRenderJob} disabled={isPending} style={{ padding: '11px 16px', borderRadius: 999, border: '1px solid var(--border)', background: 'transparent', color: 'inherit', fontWeight: 600, cursor: isPending ? 'progress' : 'pointer' }}>
            Queue render job
          </button>
          <button onClick={approveDraft} disabled={isPending || approval?.status === 'approved' || !activeDraft} style={{ padding: '11px 16px', borderRadius: 999, border: 'none', background: 'var(--accent)', color: '#fff', fontWeight: 700, cursor: isPending ? 'progress' : 'pointer', opacity: approval?.status === 'approved' ? 0.7 : 1 }}>
            Approve active draft
          </button>
        </div>
      </div>

      {error ? <div style={{ marginBottom: 16, color: '#f97316', fontSize: 13 }}>{error}</div> : null}

      <section style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.4fr) minmax(320px, 1fr)', gap: 20, marginBottom: 24 }}>
        <div style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 14 }}>
            <div>
              <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 0.8, color: 'var(--text-dim)' }}>Workspace summary</div>
              <div style={{ fontSize: 16, fontWeight: 700, marginTop: 4 }}>{workspace.summary}</div>
            </div>
            <span style={{ padding: '6px 10px', borderRadius: 999, background: 'rgba(56,189,248,0.12)', color: pillColor(workspace.current_stage), fontSize: 12, fontWeight: 700, alignSelf: 'flex-start' }}>
              {workspace.current_stage.replace(/_/g, ' ')}
            </span>
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))', gap: 14 }}>
            {[
              ['Channel', workspace.primary_channel.replace(/_/g, ' ')],
              ['Format', workspace.output_format.replace(/_/g, ' ')],
              ['Status', workspace.status],
              ['Updated', formatRelative(workspace.updated_at)],
            ].map(([label, value]) => (
              <div key={label} style={{ padding: 14, border: '1px solid rgba(255,255,255,0.06)', borderRadius: 14 }}>
                <div style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.7, color: 'var(--text-dim)', marginBottom: 6 }}>{label}</div>
                <div style={{ fontSize: 14, fontWeight: 700 }}>{value}</div>
              </div>
            ))}
          </div>
        </div>

        <div style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
          <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 0.8, color: 'var(--text-dim)', marginBottom: 8 }}>Approval lane</div>
          <div style={{ fontSize: 18, fontWeight: 700, marginBottom: 6 }}>{approval?.status.replace(/_/g, ' ') ?? workspace.approval_status.replace(/_/g, ' ')}</div>
          <p style={{ margin: '0 0 12px', fontSize: 14, color: 'var(--text-dim)', lineHeight: 1.5 }}>{approval?.summary ?? 'No pending approval request for this workspace.'}</p>
          <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>
            Requested by {approval?.requested_by ?? 'studio-director'} on {approval ? new Date(approval.requested_at).toLocaleString() : 'n/a'}
          </div>
        </div>
      </section>

      <section style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.15fr) minmax(0, 0.85fr)', gap: 20, marginBottom: 24 }}>
        <div style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
          <div style={{ fontSize: 18, fontWeight: 700, marginBottom: 14 }}>Stage pipeline</div>
          <div style={{ display: 'grid', gap: 12 }}>
            {stages.map((stage) => (
              <div key={stage.id} style={{ display: 'grid', gridTemplateColumns: '96px minmax(0, 1fr)', gap: 14, padding: 14, borderRadius: 14, background: stage.status === 'active' ? 'rgba(56,189,248,0.08)' : 'rgba(255,255,255,0.02)', border: '1px solid rgba(255,255,255,0.06)' }}>
                <div>
                  <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 0.7, color: 'var(--text-dim)' }}>{stage.key}</div>
                  <div style={{ marginTop: 8, fontSize: 13, color: pillColor(stage.status), fontWeight: 700 }}>{stage.status}</div>
                </div>
                <div>
                  <div style={{ fontSize: 16, fontWeight: 700 }}>{stage.label}</div>
                  <div style={{ marginTop: 4, fontSize: 13, color: 'var(--text-dim)' }}>{stage.owner}</div>
                  <p style={{ margin: '10px 0 6px', fontSize: 14, lineHeight: 1.5 }}>{stage.notes}</p>
                  <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>Next: {stage.next_action}</div>
                </div>
              </div>
            ))}
          </div>
        </div>

        <div style={{ display: 'grid', gap: 20 }}>
          <div style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
            <div style={{ fontSize: 18, fontWeight: 700, marginBottom: 12 }}>Drafts</div>
            <div style={{ display: 'grid', gap: 12 }}>
              {drafts.map((draft) => (
                <div key={draft.id} style={{ padding: 14, borderRadius: 14, border: '1px solid rgba(255,255,255,0.06)', background: draft.id === workspace.active_draft_id ? 'rgba(56,189,248,0.08)' : 'rgba(255,255,255,0.02)' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 8 }}>
                    <div style={{ fontSize: 15, fontWeight: 700 }}>{draft.title}</div>
                    <span style={{ fontSize: 12, color: pillColor(draft.status), fontWeight: 700 }}>{draft.status}</span>
                  </div>
                  <div style={{ fontSize: 13, color: 'var(--text-dim)', marginBottom: 8 }}>{draft.format.replace(/_/g, ' ')} · {draft.owner}</div>
                  <p style={{ margin: '0 0 10px', fontSize: 14, lineHeight: 1.5 }}>{draft.summary}</p>
                  <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
                    {draft.assets_required.map((asset) => (
                      <span key={asset} style={{ padding: '4px 8px', borderRadius: 999, background: 'rgba(255,255,255,0.06)', fontSize: 12 }}>{asset}</span>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
            <div style={{ fontSize: 18, fontWeight: 700, marginBottom: 12 }}>Events</div>
            <div style={{ display: 'grid', gap: 12 }}>
              {events.map((event) => (
                <div key={event.id} style={{ paddingBottom: 12, borderBottom: '1px solid rgba(255,255,255,0.06)' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12 }}>
                    <div style={{ fontSize: 14, fontWeight: 700 }}>{event.title}</div>
                    <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>{formatRelative(event.created_at)}</div>
                  </div>
                  <div style={{ marginTop: 6, fontSize: 14, color: 'var(--text-dim)' }}>{event.message}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section style={{ border: '1px solid var(--border)', borderRadius: 18, padding: 22, boxShadow: 'var(--shadow-sm)' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, marginBottom: 12, flexWrap: 'wrap' }}>
          <h2 style={{ margin: 0, fontSize: 18, fontWeight: 700 }}>Jobs</h2>
          <div style={{ fontSize: 12, color: 'var(--text-dim)' }}>{jobs.length} visible in the queue</div>
        </div>
        <div style={{ display: 'grid', gap: 10 }}>
          {jobs.map((job) => (
            <div key={job.id} style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.8fr) 1fr 100px', gap: 14, alignItems: 'center', padding: 14, borderRadius: 14, border: '1px solid rgba(255,255,255,0.06)' }}>
              <div>
                <div style={{ fontSize: 15, fontWeight: 700 }}>{job.label}</div>
                <div style={{ marginTop: 4, fontSize: 13, color: 'var(--text-dim)' }}>{job.provider} · {job.job_type}</div>
              </div>
              <div style={{ fontSize: 13, color: pillColor(job.status), fontWeight: 700 }}>{job.status}</div>
              <div style={{ fontSize: 13, textAlign: 'right' }}>{job.progress}%</div>
            </div>
          ))}
        </div>
      </section>
    </main>
  );
}
