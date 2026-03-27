'use client';
import { useState, useCallback, useEffect } from 'react';
import Link from 'next/link';
import { workApi } from '../../../lib/work-api';
import { getPlanningData, startPlanningRound } from '../../../lib/planning-api';
import PlanningPanel from '../../components/PlanningPanel';

const PLANNING_EVENT_LABEL = {
  // planning events
  scope_classified:       'Scope classified',
  planning_round_opened:  'Review started',
  planning_turn_submitted:'Turn submitted',
  planning_approved:      'Review approved',
  planning_vetoed:        'Review blocked',
  planning_timed_out:     'Review timed out',
  planning_aborted:       'Review aborted',
  // execution events
  planning_required:      'Planning required',
  adapter_selected:       'Adapter selected',
  approval_required:      'Approval required',
  dependency_missing:     'Dependency missing',
  permission_denied:      'Permission denied',
  execution_started:      'Execution started',
  execution_finished:     'Execution finished',
  verification_failed:    'Verification failed',
  verified_success:       'Verified',
  retry_scheduled:        'Retry scheduled',
  delegated_to_subagent:  'Delegated to sub-agent',
  completed:              'Completed',
  failed:                 'Failed',
};

const PLANNING_EVENT_BADGE = {
  // planning events
  scope_classified:       'badge-info',
  planning_round_opened:  'badge-info',
  planning_turn_submitted:'badge-dim',
  planning_approved:      'badge-success',
  planning_vetoed:        'badge-error',
  planning_timed_out:     'badge-warn',
  planning_aborted:       'badge-warn',
  // execution events
  planning_required:      'badge-info',
  adapter_selected:       'badge-dim',
  approval_required:      'badge-warn',
  dependency_missing:     'badge-error',
  permission_denied:      'badge-error',
  execution_started:      'badge-accent',
  execution_finished:     'badge-dim',
  verification_failed:    'badge-error',
  verified_success:       'badge-success',
  retry_scheduled:        'badge-warn',
  delegated_to_subagent:  'badge-info',
  completed:              'badge-success',
  failed:                 'badge-error',
};

const EXECUTION_PATH_LABEL = {
  fast_path:     'Fast path',
  planned_swarm: 'Planned swarm',
  review_swarm:  'Review swarm',
};

function blockReasonLabel(br) {
  if (!br) return null;
  if (typeof br === 'string') return br;
  if (br.MissingPermission)  return `Missing permission: ${br.MissingPermission}`;
  if (br.DependencyUnsatisfied) return `Dependency unsatisfied: ${br.DependencyUnsatisfied}`;
  if (br.BudgetExceeded)     return `Budget exceeded`;
  if (br.ForbiddenAction)    return `Forbidden action: ${br.ForbiddenAction}`;
  if (br.VerificationFailed) return `Verification failed: ${br.VerificationFailed}`;
  if (br.ManualHold)         return `Manual hold: ${br.ManualHold || 'operator'}`;
  return JSON.stringify(br);
}

function ExecutionPanel({ report }) {
  if (!report) return null;
  const {
    execution_path, adapter_selection, action_result,
    verification, status, block_reason, result_summary, artifact_refs,
    retry_count, retry_scheduled, delegated_to, cost_usd, warnings,
    events_emitted, started_at, finished_at,
  } = report;

  const pathLabel    = EXECUTION_PATH_LABEL[execution_path] ?? execution_path;
  const statusBadge  = status === 'completed'              ? 'badge-success'
                     : status === 'failed'                 ? 'badge-error'
                     : status === 'blocked'                ? 'badge-error'
                     : status === 'waiting_approval'       ? 'badge-warn'
                     : status === 'retry_scheduled'        ? 'badge-warn'
                     : status === 'delegated_to_subagent'  ? 'badge-info'
                     : status === 'running'                ? 'badge-accent'
                     : 'badge-dim';

  return (
    <div data-cy="execution-panel" style={{ marginBottom: 20 }}>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 16 }}>

        {/* Left column: execution details */}
        <div className="card" data-cy="execution-details-card">
          <h3 style={{ marginTop: 0 }}>Execution Details</h3>

          <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 10 }}>
            <span style={{ color: 'var(--text-muted)', fontSize: 13, minWidth: 120 }}>Status</span>
            <span className={`badge ${statusBadge}`} data-cy="execution-status">
              {status?.replace(/_/g, ' ')}
            </span>
          </div>

          <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
            <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Path</span>
            <span data-cy="execution-path" style={{ color: 'var(--text-primary)', fontWeight: 500 }}>{pathLabel}</span>
          </div>

          {adapter_selection && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }} data-cy="execution-adapter">
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Adapter</span>
              <span style={{ color: 'var(--text-primary)' }}>
                <strong>{adapter_selection.chosen}</strong>
                {adapter_selection.rejected?.length > 0 && (
                  <span style={{ color: 'var(--text-muted)', marginLeft: 6 }}>
                    (skipped: {adapter_selection.rejected.join(', ')})
                  </span>
                )}
              </span>
            </div>
          )}

          {adapter_selection?.rationale && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Rationale</span>
              <span style={{ color: 'var(--text-secondary)', fontStyle: 'italic' }}>{adapter_selection.rationale}</span>
            </div>
          )}

          {cost_usd != null && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Cost</span>
              <span style={{ color: 'var(--text-primary)' }}>${cost_usd.toFixed(5)}</span>
            </div>
          )}

          {started_at && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Started</span>
              <span style={{ color: 'var(--text-secondary)' }}>{fmtDate(started_at)}</span>
            </div>
          )}

          {finished_at && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Finished</span>
              <span style={{ color: 'var(--text-secondary)' }}>{fmtDate(finished_at)}</span>
            </div>
          )}

          {retry_count > 0 && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }} data-cy="execution-retry">
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Retries</span>
              <span style={{ color: retry_scheduled ? 'var(--warn)' : 'var(--text-secondary)' }}>
                {retry_count}{retry_scheduled ? ' (scheduled)' : ''}
              </span>
            </div>
          )}

          {delegated_to && (
            <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }} data-cy="execution-delegated">
              <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Delegated to</span>
              <Link href={`/work/${delegated_to}`} style={{ color: 'var(--accent)', fontFamily: 'monospace', fontSize: 12 }}>
                {delegated_to.slice(0, 8)}…
              </Link>
            </div>
          )}
        </div>

        {/* Right column: verification + block */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
          {verification && (
            <div className="card" data-cy="execution-verification"
              style={{ borderColor: verification.passed ? 'var(--success-muted)' : 'var(--error-muted)' }}>
              <h3 style={{ marginTop: 0, color: verification.passed ? 'var(--success)' : 'var(--error)' }}>
                {verification.passed ? '✓ Verified' : '✗ Verification failed'}
              </h3>
              {verification.evidence && (
                <p style={{ margin: 0, fontSize: 13, color: 'var(--text-secondary)' }}>{verification.evidence}</p>
              )}
            </div>
          )}

          {block_reason && (
            <div className="card" style={{ borderColor: 'var(--error-muted)' }} data-cy="execution-block-reason">
              <h3 style={{ marginTop: 0, color: 'var(--error)' }}>Blocked</h3>
              <p style={{ margin: 0, fontSize: 13 }}>{blockReasonLabel(block_reason)}</p>
            </div>
          )}

          {action_result && (
            <div className="card">
              <h3 style={{ marginTop: 0 }}>Action Result</h3>
              {action_result.output && (
                <pre style={{ fontSize: 11, overflowX: 'auto', margin: '0 0 8px', color: 'var(--text-secondary)', maxHeight: 160 }}>
                  {action_result.output}
                </pre>
              )}
              {action_result.error && (
                <pre style={{ fontSize: 11, color: 'var(--error)', margin: 0 }}>{action_result.error}</pre>
              )}
              <div style={{ display: 'flex', gap: 16, marginTop: 6, fontSize: 11, color: 'var(--text-muted)' }}>
                {action_result.tokens_in  > 0 && <span>in: {action_result.tokens_in} tok</span>}
                {action_result.tokens_out > 0 && <span>out: {action_result.tokens_out} tok</span>}
                {action_result.iterations > 1 && <span>iter: {action_result.iterations}</span>}
              </div>
            </div>
          )}
        </div>
      </div>

      {result_summary && (
        <div className="card" style={{ marginTop: 12 }}>
          <h3 style={{ marginTop: 0 }}>Summary</h3>
          <p style={{ margin: 0, fontSize: 13 }}>{result_summary}</p>
        </div>
      )}

      {artifact_refs?.length > 0 && (
        <div className="card" style={{ marginTop: 12 }}>
          <h3 style={{ marginTop: 0 }}>Artifacts</h3>
          <ul style={{ margin: 0, paddingLeft: 16, fontSize: 12 }}>
            {artifact_refs.map((ref, i) => <li key={i} style={{ color: 'var(--text-secondary)' }}>{ref}</li>)}
          </ul>
        </div>
      )}

      {warnings?.length > 0 && (
        <div className="card" style={{ marginTop: 12, borderColor: 'var(--warn-muted)' }}>
          <h3 style={{ marginTop: 0, color: 'var(--warn)' }}>Warnings</h3>
          <ul style={{ margin: 0, paddingLeft: 16, fontSize: 13 }}>
            {warnings.map((w, i) => <li key={i}>{w}</li>)}
          </ul>
        </div>
      )}

      {events_emitted?.length > 0 && (
        <div className="card" style={{ marginTop: 12 }} data-cy="execution-events">
          <h3 style={{ marginTop: 0 }}>Events emitted</h3>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 6 }}>
            {events_emitted.map((ev, i) => (
              <span key={i} className={`badge ${PLANNING_EVENT_BADGE[ev] ?? 'badge-dim'}`} style={{ fontSize: 11 }}>
                {PLANNING_EVENT_LABEL[ev] ?? ev}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function statusBadgeClass(s) {
  if (s === 'completed') return 'badge-success';
  if (s === 'running') return 'badge-accent';
  if (s === 'failed' || s === 'rejected') return 'badge-error';
  if (s === 'waiting_approval') return 'badge-warn';
  if (s === 'approved') return 'badge-success';
  if (s === 'cancelled') return 'badge-dim';
  return 'badge-dim';
}

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

function Field({ label, value }) {
  if (!value && value !== 0) return null;
  return (
    <div style={{ display: 'flex', gap: 8, marginBottom: 6, fontSize: 13 }}>
      <span style={{ color: 'var(--text-muted)', minWidth: 120, flexShrink: 0 }}>{label}</span>
      <span style={{ color: 'var(--text-primary)', wordBreak: 'break-word' }}>{String(value)}</span>
    </div>
  );
}

function formatByteSize(byteSize) {
  if (!Number.isFinite(byteSize) || byteSize <= 0) return null;
  if (byteSize < 1024) return `${byteSize} B`;
  if (byteSize < 1024 * 1024) return `${(byteSize / 1024).toFixed(1)} KB`;
  return `${(byteSize / (1024 * 1024)).toFixed(1)} MB`;
}

function normalizeArtifactRecord(artifact) {
  if (!artifact || typeof artifact !== 'object') return null;

  return {
    artifactId: artifact.artifactId ?? artifact.artifact_id ?? null,
    title: artifact.title ?? artifact.filename ?? 'Artifact',
    kind: artifact.kind ?? null,
    contentType: artifact.contentType ?? artifact.content_type ?? null,
    byteSize: artifact.byteSize ?? artifact.byte_size ?? null,
    createdAt: artifact.createdAt ?? artifact.created_at ?? null,
    downloadPath: artifact.downloadPath ?? artifact.download_path ?? null,
  };
}

function RunArtifactsPanel({ artifacts, loading, error }) {
  if (loading) {
    return (
      <div className="card" style={{ marginTop: 12 }} data-cy="work-detail-artifacts">
        <h3 style={{ marginTop: 0 }}>Run artifacts</h3>
        <div className="text-dim text-sm">Loading durable artifacts…</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="card" style={{ marginTop: 12, borderColor: 'var(--warn-muted)' }} data-cy="work-detail-artifacts-error">
        <h3 style={{ marginTop: 0, color: 'var(--warn)' }}>Run artifacts</h3>
        <div style={{ fontSize: 13, color: 'var(--text-secondary)' }}>{error}</div>
      </div>
    );
  }

  if (!artifacts?.length) {
    return null;
  }

  return (
    <div className="card" style={{ marginTop: 12 }} data-cy="work-detail-artifacts">
      <h3 style={{ marginTop: 0 }}>Run artifacts</h3>
      <p style={{ marginTop: 0, fontSize: 12, color: 'var(--text-muted)' }}>
        Durable files captured for this run.
      </p>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
        {artifacts.map((artifact, index) => {
          const href = artifact.artifactId
            ? `/api/artifacts/${encodeURIComponent(artifact.artifactId)}`
            : artifact.downloadPath || null;
          const createdLabel = artifact.createdAt ? fmtDate(artifact.createdAt) : null;
          const byteSizeLabel = formatByteSize(artifact.byteSize);

          return (
            <div
              key={artifact.artifactId || `${artifact.title}-${index}`}
              style={{ border: '1px solid var(--border-light)', borderRadius: 10, padding: '12px 14px' }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', gap: 12, alignItems: 'flex-start' }}>
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-primary)', wordBreak: 'break-word' }}>
                    {artifact.title}
                  </div>
                  {artifact.kind && (
                    <div style={{ marginTop: 4, fontSize: 11, color: 'var(--text-muted)', textTransform: 'uppercase', letterSpacing: '0.05em' }}>
                      {artifact.kind}
                    </div>
                  )}
                </div>
                {href && (
                  <a href={href} className="btn btn-ghost btn-sm">Download</a>
                )}
              </div>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: 8, marginTop: 8, fontSize: 11, color: 'var(--text-muted)' }}>
                {artifact.contentType ? <span>{artifact.contentType}</span> : null}
                {byteSizeLabel ? <span>{byteSizeLabel}</span> : null}
                {createdLabel ? <span>{createdLabel}</span> : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export default function WorkDetailClient({ initialItem, initialEvents, initialChildren, id }) {
  const [item, setItem] = useState(initialItem);
  const [events, setEvents] = useState(initialEvents ?? []);
  const [children, setChildren] = useState(initialChildren ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [acting, setActing] = useState('');
  const [actionError, setActionError] = useState('');
  const [executionReport, setExecutionReport] = useState(null);
  const [planningScope, setPlanningScope] = useState(null);
  const [planningRound, setPlanningRound] = useState(null);
  const [planningLoading, setPlanningLoading] = useState(false);
  const [planningError, setPlanningError] = useState('');
  const [runArtifacts, setRunArtifacts] = useState([]);
  const [artifactLoading, setArtifactLoading] = useState(false);
  const [artifactError, setArtifactError] = useState('');

  useEffect(() => {
    const workspaceId = item?.workspace_id;
    const runId = item?.run_id;

    if (!workspaceId || !runId) {
      setRunArtifacts([]);
      setArtifactError('');
      setArtifactLoading(false);
      return;
    }

    let cancelled = false;

    async function loadArtifacts() {
      setArtifactLoading(true);
      setArtifactError('');
      try {
        const response = await fetch(
          `/api/workspaces/${encodeURIComponent(workspaceId)}/runs/${encodeURIComponent(runId)}/artifacts`,
          { cache: 'no-store' }
        );
        const data = await response.json().catch(() => ({}));
        if (!response.ok) {
          throw new Error(data?.error || 'Could not load run artifacts.');
        }

        if (!cancelled) {
          const artifacts = Array.isArray(data?.artifacts)
            ? data.artifacts.map(normalizeArtifactRecord).filter(Boolean)
            : [];
          setRunArtifacts(artifacts);
        }
      } catch (e) {
        if (!cancelled) {
          setRunArtifacts([]);
          setArtifactError(e.message || 'Could not load run artifacts.');
        }
      }

      if (!cancelled) {
        setArtifactLoading(false);
      }
    }

    loadArtifacts();

    return () => {
      cancelled = true;
    };
  }, [item?.run_id, item?.workspace_id]);

  const refreshPlanning = useCallback(async () => {
    setPlanningLoading(true);
    setPlanningError('');
    try {
      const data = await getPlanningData(id);
      if (data) {
        setPlanningScope(data.scope ?? null);
        setPlanningRound(data.planning_round ?? null);
      }
    } catch (e) {
      setPlanningError(e.message || 'Could not load planning data.');
    }
    setPlanningLoading(false);
  }, [id]);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const [itemData, eventsData, childData] = await Promise.all([
        workApi.getWorkById(id),
        workApi.getWorkEvents(id),
        workApi.getWork({ parent_id: id, limit: 50 }),
      ]);
      setItem(itemData);
      setEvents(Array.isArray(eventsData?.events) ? eventsData.events : []);
      setChildren(Array.isArray(childData?.items) ? childData.items : []);
    } catch (e) {
      setError(e.message || 'Could not load work item.');
    }
    setLoading(false);
    // Refresh planning data in parallel (soft — errors don't block the page)
    refreshPlanning();
  }, [id, refreshPlanning]);

  const doAction = useCallback(async (action) => {
    if (action === 'start_planning') {
      setPlanningLoading(true);
      setPlanningError('');
      try {
        const round = await startPlanningRound(id);
        if (round) setPlanningRound(round);
        await refreshPlanning();
      } catch (e) {
        setPlanningError(e.message || 'Could not start team review.');
      }
      setPlanningLoading(false);
      return;
    }
    if (action === 'refresh') { await refreshPlanning(); return; }
    if (action === 'scroll_to_veto' || action === 'scroll_to_conditions') {
      document.querySelector('[data-cy="planning-panel"]')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      return;
    }
    if (action === 'revise') {
      // Future: open an edit modal. For now scroll to description area.
      document.querySelector('[data-cy="work-detail-page"]')?.scrollIntoView({ behavior: 'smooth', block: 'start' });
      return;
    }
    setActing(action);
    setActionError('');
    try {
      let result;
      if (action === 'run') result = await workApi.runWork(id);
      else if (action === 'approve') result = await workApi.approveWork(id, '');
      else if (action === 'reject') result = await workApi.rejectWork(id, '');
      else if (action === 'cancel') result = await workApi.cancelWork(id);
      else if (action === 'retry') result = await workApi.retryWork(id);
      // runWork returns an ExecutionReport (has work_item_id), not a WorkItem
      if (result?.work_item_id !== undefined) {
        setExecutionReport(result);
      } else if (result) {
        setItem(result);
      }
      await refresh();
    } catch (e) {
      setActionError(e.message || `Action "${action}" failed.`);
    }
    setActing('');
  }, [id, refresh, refreshPlanning]);

  // Planning-aware run gate:
  // If the work item is in planned_swarm / review_swarm path but the planning
  // round is not yet approved, block the raw run button — NextActionPanel
  // shows the correct planning action instead.
  const planningBlocksRun =
    planningScope?.path &&
    planningScope.path !== 'fast_path' &&
    planningRound?.status !== 'approved';

  const canRun = item && ['pending', 'ready'].includes(item.status) && !planningBlocksRun;
  const canApprove = item?.status === 'waiting_approval';
  const canReject = item?.status === 'waiting_approval';
  const canRetry = item?.status === 'failed';
  const canCancel = item && !['completed', 'cancelled', 'rejected'].includes(item.status);
  const delegatedChildId = executionReport?.delegated_to ?? null;

  return (
    <div data-cy="work-detail-page">
      <div className="page-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
          <Link href="/inbox" className="btn btn-ghost btn-sm">← Back</Link>
          <h1 style={{ margin: 0 }}>{item ? item.title : 'Work Item'}</h1>
          {item && (
            <span className={`badge ${statusBadgeClass(item.status)}`}>{item.status}</span>
          )}
        </div>
        <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
          {loading ? 'Refreshing…' : 'Refresh'}
        </button>
      </div>

      <div className="page-body">
        {error && (
          <div data-cy="work-detail-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}

        {item && (
          <>
            {/* Actions */}
            <div data-cy="work-detail-actions" style={{ display: 'flex', gap: 8, marginBottom: 20, flexWrap: 'wrap' }}>
              {canRun && (
                <button data-cy="action-run" className="btn btn-primary btn-sm" onClick={() => doAction('run')} disabled={!!acting}>
                  {acting === 'run' ? '…' : '▶ Run'}
                </button>
              )}
              {canApprove && (
                <button data-cy="action-approve" className="btn btn-primary btn-sm" onClick={() => doAction('approve')} disabled={!!acting}>
                  {acting === 'approve' ? '…' : '✓ Approve'}
                </button>
              )}
              {canReject && (
                <button data-cy="action-reject" className="btn btn-ghost btn-sm" style={{ color: 'var(--error)' }} onClick={() => doAction('reject')} disabled={!!acting}>
                  {acting === 'reject' ? '…' : '✗ Reject'}
                </button>
              )}
              {canRetry && (
                <button data-cy="action-retry" className="btn btn-ghost btn-sm" onClick={() => doAction('retry')} disabled={!!acting}>
                  {acting === 'retry' ? '…' : '↺ Retry'}
                </button>
              )}
              {canCancel && (
                <button data-cy="action-cancel" className="btn btn-ghost btn-sm" style={{ color: 'var(--error)' }} onClick={() => doAction('cancel')} disabled={!!acting}>
                  {acting === 'cancel' ? '…' : '✕ Cancel'}
                </button>
              )}
              {delegatedChildId && (
                <Link
                  data-cy="action-view-delegated"
                  href={`/work/${delegatedChildId}`}
                  className="btn btn-ghost btn-sm"
                >
                  ↗ View delegated child
                </Link>
              )}
            </div>

            {actionError && (
              <div className="error-state" style={{ marginBottom: 16, fontSize: 13 }}>⚠ {actionError}</div>
            )}

            {/* Planning section (only shown when scope or round is present) */}
            {(planningScope || planningRound) && (
              <div style={{ marginBottom: 20 }}>
                <div style={{ fontSize: 11, fontWeight: 700, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', marginBottom: 10 }}
                  data-cy="planning-section-label">
                  Planning
                </div>
                <PlanningPanel
                  scope={planningScope}
                  planningRound={planningRound}
                  loading={planningLoading}
                  error={planningError}
                  onAction={doAction}
                  onRefresh={refreshPlanning}
                  itemStatus={item?.status ?? ''}
                />
              </div>
            )}

            <div
              data-cy="execution-section-label"
              style={{ fontSize: 11, fontWeight: 700, textTransform: 'uppercase', letterSpacing: '0.08em', color: 'var(--text-muted)', marginBottom: 10 }}
            >
              Execution
            </div>

            {executionReport && (
              <ExecutionPanel report={executionReport} />
            )}

            <RunArtifactsPanel
              artifacts={runArtifacts}
              loading={artifactLoading}
              error={artifactError}
            />

            <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 20 }}>
              {/* Left: details */}
              <div className="card">
                <h3 style={{ marginTop: 0 }}>Details</h3>
                <Field label="ID"           value={item.id} />
                <Field label="Type"         value={item.work_type} />
                <Field label="Source"       value={item.source} />
                <Field label="Priority"     value={item.priority} />
                <Field label="Assigned To"  value={item.assigned_agent_name || item.assigned_agent_id} />
                <Field label="Created By"   value={item.created_by} />
                <Field label="Run ID"       value={item.run_id} />
                <Field label="Workspace"    value={item.workspace_id} />
                <Field label="Created"      value={fmtDate(item.created_at)} />
                <Field label="Started"      value={fmtDate(item.started_at)} />
                <Field label="Completed"    value={fmtDate(item.completed_at)} />
                <Field label="Deadline"     value={fmtDate(item.deadline)} />
                <Field label="Retries"      value={item.max_retries > 0 ? `${item.retry_count}/${item.max_retries}` : null} />
                {item.parent_id && (
                  <div style={{ display: 'flex', gap: 8, marginTop: 8, fontSize: 13 }}>
                    <span style={{ color: 'var(--text-muted)', minWidth: 120 }}>Parent</span>
                    <Link data-cy="work-detail-parent-link" href={`/work/${item.parent_id}`} style={{ color: 'var(--accent)' }}>
                      {item.parent_id.slice(0, 8)}…
                    </Link>
                  </div>
                )}
              </div>

              {/* Right: description / payload / result / error */}
              <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
                {item.description && (
                  <div className="card">
                    <h3 style={{ marginTop: 0 }}>Description</h3>
                    <p style={{ margin: 0, fontSize: 13 }}>{item.description}</p>
                  </div>
                )}
                {item.payload && Object.keys(item.payload).length > 0 && (
                  <div className="card">
                    <h3 style={{ marginTop: 0 }}>Payload</h3>
                    <pre style={{ fontSize: 11, overflowX: 'auto', margin: 0, color: 'var(--text-secondary)' }}>
                      {JSON.stringify(item.payload, null, 2)}
                    </pre>
                  </div>
                )}
                {item.result && (
                  <div className="card">
                    <h3 style={{ marginTop: 0 }}>Output</h3>
                    <pre style={{ fontSize: 11, overflowX: 'auto', margin: 0, color: 'var(--text-secondary)' }}>{item.result}</pre>
                  </div>
                )}
                {item.error && (
                  <div className="card" style={{ borderColor: 'var(--error-muted)' }}>
                    <h3 style={{ marginTop: 0, color: 'var(--error)' }}>Error</h3>
                    <pre style={{ fontSize: 11, overflowX: 'auto', margin: 0, color: 'var(--error)' }}>{item.error}</pre>
                  </div>
                )}
              </div>
            </div>

            {/* Child work items */}
            {children.length > 0 && (
              <div className="card" style={{ marginTop: 20 }} data-cy="work-detail-children">
                <h3 style={{ marginTop: 0 }}>Delegated Sub-tasks ({children.length})</h3>
                <table className="data-table">
                  <thead>
                    <tr><th>Title</th><th>Status</th><th>Agent</th><th></th></tr>
                  </thead>
                  <tbody>
                    {children.map(child => (
                      <tr key={child.id}>
                        <td style={{ fontWeight: 600 }}>{child.title}</td>
                        <td><span className={`badge ${statusBadgeClass(child.status)}`}>{child.status}</span></td>
                        <td style={{ fontSize: 12 }}>{child.assigned_agent_name || child.assigned_agent_id || '—'}</td>
                        <td><Link href={`/work/${child.id}`} className="btn btn-ghost btn-sm">View</Link></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}

            {/* Timeline */}
            <div className="card" style={{ marginTop: 20 }} data-cy="work-detail-timeline">
              <h3 style={{ marginTop: 0 }}>Timeline</h3>
              {events.length === 0 && (
                <div className="text-dim text-sm">No events recorded yet.</div>
              )}
              {events.length > 0 && (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {events.map((ev, i) => (
                    <div key={ev.id || i} data-cy="timeline-event" style={{ display: 'flex', gap: 12, alignItems: 'flex-start', fontSize: 13 }}>
                      <div style={{ width: 140, flexShrink: 0, fontSize: 11, color: 'var(--text-muted)', paddingTop: 1 }}>
                        {fmtDate(ev.created_at)}
                      </div>
                      <div>
                        <span
                          className={`badge ${PLANNING_EVENT_BADGE[ev.event_type] ?? 'badge-dim'}`}
                          style={{ fontSize: 11, marginRight: 6 }}
                          data-cy="timeline-event-type"
                        >
                          {PLANNING_EVENT_LABEL[ev.event_type] ?? ev.event_type}
                        </span>
                        {ev.from_status && ev.to_status && (
                          <span style={{ color: 'var(--text-dim)' }}>{ev.from_status} → {ev.to_status}</span>
                        )}
                        {ev.detail && <div style={{ color: 'var(--text-secondary)', marginTop: 2 }}>{ev.detail}</div>}
                        {ev.actor && <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>by {ev.actor}</div>}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
