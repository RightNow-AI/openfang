'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../../lib/work-api';

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

export default function WorkDetailClient({ initialItem, initialEvents, initialChildren, id }) {
  const [item, setItem] = useState(initialItem);
  const [events, setEvents] = useState(initialEvents ?? []);
  const [children, setChildren] = useState(initialChildren ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [acting, setActing] = useState('');
  const [actionError, setActionError] = useState('');

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
  }, [id]);

  const doAction = useCallback(async (action) => {
    setActing(action);
    setActionError('');
    try {
      let result;
      if (action === 'run') result = await workApi.runWork(id);
      else if (action === 'approve') result = await workApi.approveWork(id, '');
      else if (action === 'reject') result = await workApi.rejectWork(id, '');
      else if (action === 'cancel') result = await workApi.cancelWork(id);
      else if (action === 'retry') result = await workApi.retryWork(id);
      if (result) setItem(result);
      await refresh();
    } catch (e) {
      setActionError(e.message || `Action "${action}" failed.`);
    }
    setActing('');
  }, [id, refresh]);

  const canRun = item && ['pending', 'ready'].includes(item.status);
  const canApprove = item?.status === 'waiting_approval';
  const canReject = item?.status === 'waiting_approval';
  const canRetry = item?.status === 'failed';
  const canCancel = item && !['completed', 'cancelled', 'rejected'].includes(item.status);

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
            </div>

            {actionError && (
              <div className="error-state" style={{ marginBottom: 16, fontSize: 13 }}>⚠ {actionError}</div>
            )}

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
                        <span className="badge badge-dim" style={{ fontSize: 11, marginRight: 6 }}>{ev.event_type}</span>
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
