import Link from 'next/link';
import type { StudioIndexPayload, StudioJob, StudioWorkspace } from '../lib/studio-types';
import styles from './studio-surfaces.module.css';

function formatRelative(timestamp: string) {
  const delta = Math.max(1, Math.round((Date.now() - new Date(timestamp).getTime()) / 60_000));
  if (delta < 60) return `${delta}m ago`;
  const hours = Math.round(delta / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

function statusColor(status: string) {
  switch (status) {
    case 'approved':
    case 'completed':
      return styles.statusApproved;
    case 'running':
    case 'active':
      return styles.statusRunning;
    case 'pending':
    case 'queued':
      return styles.statusPending;
    case 'changes_requested':
    case 'failed':
    case 'blocked':
      return styles.statusFailed;
    default:
      return styles.statusDefault;
  }
}

function WorkspaceCard({ workspace }: { workspace: StudioWorkspace }) {
  return (
    <Link href={`/studio/${workspace.id}`} className={`${styles.card} ${styles.workspaceCard}`}>
      <div className={styles.workspaceCardHeader}>
        <div>
          <div className={styles.workspaceClient}>{workspace.client_name}</div>
          <h2 className={styles.workspaceName}>{workspace.title}</h2>
        </div>
        <span className={`${styles.stageBadge} ${statusColor(workspace.status)}`}>
          {workspace.current_stage.replace(/_/g, ' ')}
        </span>
      </div>
      <p className={styles.workspaceSummary}>{workspace.summary}</p>
      <div className={styles.workspaceFacts}>
        <div>
          <div className={styles.metaLabel}>Channel</div>
          <div className={styles.metaValue}>{workspace.primary_channel.replace(/_/g, ' ')}</div>
        </div>
        <div>
          <div className={styles.metaLabel}>Format</div>
          <div className={styles.metaValue}>{workspace.output_format.replace(/_/g, ' ')}</div>
        </div>
        <div>
          <div className={styles.metaLabel}>Updated</div>
          <div className={styles.metaValue}>{formatRelative(workspace.updated_at)}</div>
        </div>
      </div>
    </Link>
  );
}

function JobRow({ job }: { job: StudioJob }) {
  return (
    <div className={styles.jobRow}>
      <div>
        <div className={styles.jobLabel}>{job.label}</div>
        <div className={styles.jobProvider}>{job.provider}</div>
      </div>
      <div className={styles.jobType}>{job.job_type}</div>
      <div className={`${styles.jobStatus} ${statusColor(job.status)}`}>{job.status}</div>
      <div className={styles.jobProgress}>{job.progress}%</div>
    </div>
  );
}

export default function StudioLandingPage({ payload }: { payload: StudioIndexPayload }) {
  return (
    <main className={styles.page}>
      <section className={styles.heroGrid}>
        <div className={`${styles.card} ${styles.heroCard}`}>
          <div className={styles.eyebrow}>Creator Studio</div>
          <h1 className={styles.heroTitle}>Stage-driven production for briefs, drafts, renders, and approvals.</h1>
          <p className={styles.heroCopy}>
            Use Next.js for the operator surface and keep job execution in the Rust pipeline. Each workspace exposes the current draft, active stage, job queue, and approval gate without hiding the underlying state transitions.
          </p>
          <div className={styles.actionRow}>
            <Link href="/studio/new" className={styles.primaryAction}>
              Create workspace
            </Link>
            <Link href="/creative-studio" className={styles.secondaryAction}>
              Open legacy creative surface
            </Link>
          </div>
        </div>

        <div className={styles.metricsGrid}>
          {[
            ['Live workspaces', payload.summary.live_workspaces],
            ['Active jobs', payload.summary.active_jobs],
            ['Approval backlog', payload.summary.approval_backlog],
          ].map(([label, value]) => (
            <div key={label} className={styles.card}>
              <div className={styles.metricLabel}>{label}</div>
              <div className={styles.metricValue}>{value}</div>
            </div>
          ))}
        </div>
      </section>

      <section className={styles.contentGrid}>
        <div>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}>Workspaces</h2>
            <Link href="/studio/new" className={styles.sectionLink}>New workspace</Link>
          </div>
          <div className={styles.workspaceList}>
            {payload.workspaces.map((workspace) => <WorkspaceCard key={workspace.id} workspace={workspace} />)}
          </div>
        </div>

        <div className={styles.card}>
          <div className={styles.sectionHeader}>
            <h2 className={styles.sectionTitle}>Pipeline jobs</h2>
            <span className={styles.sectionMeta}>{payload.jobs.length} visible</span>
          </div>
          {payload.jobs.length === 0 ? (
            <div className={styles.emptyState}>No jobs queued.</div>
          ) : (
            payload.jobs.map((job) => <JobRow key={job.id} job={job} />)
          )}
        </div>
      </section>
    </main>
  );
}
