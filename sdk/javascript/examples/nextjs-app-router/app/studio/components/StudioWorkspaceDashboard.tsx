import Link from 'next/link';

import type { StudioWorkspaceDashboardPayload } from '../lib/studio-types';

function formatRelative(timestamp: string) {
  const delta = Math.max(1, Math.round((Date.now() - new Date(timestamp).getTime()) / 60_000));
  if (delta < 60) return `${delta}m ago`;
  const hours = Math.round(delta / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

function stageLabel(stage: string) {
  return stage.charAt(0).toUpperCase() + stage.slice(1);
}

export default function StudioWorkspaceDashboard({ payload }: { payload: StudioWorkspaceDashboardPayload }) {
  const { workspace, drafts, alerts } = payload;

  return (
    <main style={{ maxWidth: 1280, margin: '0 auto', padding: '28px 32px 56px' }}>
      <section style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.25fr) minmax(320px, 0.75fr)', gap: 20, marginBottom: 24 }}>
        <div style={{ borderRadius: 28, padding: 26, border: '1px solid rgba(249,115,22,0.16)', background: 'radial-gradient(circle at top left, rgba(249,115,22,0.18), rgba(15,23,42,0.96) 48%)', boxShadow: 'var(--shadow-sm)' }}>
          <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 10 }}>Workspace dashboard</div>
          <h1 style={{ margin: 0, fontSize: 34, fontWeight: 900 }}>{workspace.name}</h1>
          <p style={{ margin: '10px 0 18px', color: 'rgba(255,255,255,0.68)', fontSize: 15, maxWidth: 720, lineHeight: 1.55 }}>
            {workspace.niche}. Build new shorts, watch the action queue, and keep every draft moving through the same creation pipeline.
          </p>
          <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
            <Link href={`/studio/${workspace.id}/drafts/new`} style={{ textDecoration: 'none', padding: '12px 18px', borderRadius: 999, background: '#f97316', color: '#fff', fontWeight: 800 }}>
              New short
            </Link>
            <Link href="/studio/new" style={{ textDecoration: 'none', padding: '12px 18px', borderRadius: 999, border: '1px solid rgba(255,255,255,0.12)', color: 'inherit', fontWeight: 700 }}>
              New workspace
            </Link>
          </div>
        </div>

        <div style={{ display: 'grid', gap: 14 }}>
          {[
            ['Platform', workspace.platform.toUpperCase()],
            ['Language', workspace.language.toUpperCase()],
            ['Daily goal', `${workspace.publishGoalPerDay} shorts/day`],
            ['Published 7d', `${workspace.stats.publishedLast7Days}`],
          ].map(([label, value]) => (
            <div key={label} style={{ borderRadius: 22, padding: 18, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(255,255,255,0.03)' }}>
              <div style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.8, color: 'rgba(255,255,255,0.52)', marginBottom: 8 }}>{label}</div>
              <div style={{ fontSize: 22, fontWeight: 800 }}>{value}</div>
            </div>
          ))}
        </div>
      </section>

      <section style={{ display: 'grid', gridTemplateColumns: 'minmax(0, 1.1fr) minmax(320px, 0.9fr)', gap: 20, marginBottom: 24 }}>
        <div style={{ borderRadius: 24, padding: 22, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(255,255,255,0.03)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap', marginBottom: 16 }}>
            <div>
              <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 6 }}>Draft backlog</div>
              <div style={{ fontSize: 22, fontWeight: 800 }}>{drafts.length} active drafts</div>
            </div>
            <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>Most recent first</div>
          </div>
          <div style={{ display: 'grid', gap: 14 }}>
            {drafts.map((draft) => (
              <article key={draft.id} style={{ borderRadius: 18, padding: 18, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(10,14,22,0.76)' }}>
                <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap', marginBottom: 12 }}>
                  <div>
                    <div style={{ fontSize: 18, fontWeight: 800, marginBottom: 6 }}>{draft.topic}</div>
                    <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)' }}>{draft.playbook.replace(/_/g, ' ')} · {draft.targetDurationSec}s · updated {formatRelative(draft.updatedAt)}</div>
                  </div>
                  <div style={{ display: 'grid', justifyItems: 'end', gap: 8 }}>
                    <span style={{ padding: '6px 10px', borderRadius: 999, background: 'rgba(249,115,22,0.12)', color: '#fdba74', fontSize: 12, fontWeight: 800 }}>{draft.status}</span>
                    <span style={{ fontSize: 12, color: 'rgba(255,255,255,0.58)' }}>{stageLabel(draft.stage)}</span>
                  </div>
                </div>
                <div style={{ display: 'flex', gap: 10, flexWrap: 'wrap' }}>
                  <Link href={`/studio/${workspace.id}/drafts/${draft.id}/${draft.stage}`} style={{ textDecoration: 'none', padding: '10px 14px', borderRadius: 999, background: '#f97316', color: '#fff', fontWeight: 800 }}>
                    Open pipeline
                  </Link>
                  <Link href={`/studio/${workspace.id}/drafts/${draft.id}`} style={{ textDecoration: 'none', padding: '10px 14px', borderRadius: 999, border: '1px solid rgba(255,255,255,0.12)', color: 'inherit', fontWeight: 700 }}>
                    Resume current stage
                  </Link>
                </div>
              </article>
            ))}
          </div>
        </div>

        <div style={{ display: 'grid', gap: 20 }}>
          <section style={{ borderRadius: 24, padding: 22, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(255,255,255,0.03)' }}>
            <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 8 }}>Action queue</div>
            <div style={{ display: 'grid', gap: 12 }}>
              {drafts.slice(0, 4).map((draft) => (
                <div key={draft.id} style={{ padding: 14, borderRadius: 18, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(10,14,22,0.76)' }}>
                  <div style={{ fontSize: 15, fontWeight: 700, marginBottom: 6 }}>{stageLabel(draft.stage)} next</div>
                  <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.58)', marginBottom: 10 }}>{draft.topic}</div>
                  <Link href={`/studio/${workspace.id}/drafts/${draft.id}/${draft.stage}`} style={{ textDecoration: 'none', color: '#fdba74', fontWeight: 700 }}>
                    Continue {stageLabel(draft.stage).toLowerCase()} →
                  </Link>
                </div>
              ))}
            </div>
          </section>

          <section style={{ borderRadius: 24, padding: 22, border: '1px solid rgba(255,255,255,0.08)', background: 'rgba(255,255,255,0.03)' }}>
            <div style={{ fontSize: 12, textTransform: 'uppercase', letterSpacing: 1, color: 'rgba(255,255,255,0.58)', marginBottom: 8 }}>Policy alerts</div>
            <div style={{ display: 'grid', gap: 12 }}>
              {alerts.length === 0 ? (
                <div style={{ padding: 14, borderRadius: 18, border: '1px solid rgba(34,197,94,0.16)', background: 'rgba(34,197,94,0.08)', color: '#bbf7d0', fontSize: 14 }}>
                  No active policy alerts in this workspace.
                </div>
              ) : alerts.map((alert) => (
                <div key={alert.id} style={{ padding: 14, borderRadius: 18, border: '1px solid rgba(249,115,22,0.22)', background: 'rgba(249,115,22,0.08)' }}>
                  <div style={{ fontSize: 14, fontWeight: 700, marginBottom: 6 }}>{alert.severity === 'danger' ? 'Needs intervention' : 'Warning'}</div>
                  <div style={{ fontSize: 13, color: 'rgba(255,255,255,0.7)', lineHeight: 1.5 }}>{alert.message}</div>
                </div>
              ))}
            </div>
          </section>
        </div>
      </section>
    </main>
  );
}