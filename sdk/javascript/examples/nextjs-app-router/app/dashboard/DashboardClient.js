'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../lib/work-api';

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

function StatTile({ label, value, cy, accent }) {
  return (
    <div
      data-cy={cy}
      style={{
        background: 'var(--bg-surface)',
        border: '1px solid var(--border)',
        borderRadius: 8,
        padding: '16px 20px',
        textAlign: 'center',
      }}
    >
      <div style={{ fontSize: 28, fontWeight: 700, color: accent ? `var(--${accent})` : 'var(--text-primary)' }}>
        {value ?? '—'}
      </div>
      <div style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 4 }}>{label}</div>
    </div>
  );
}

function WorkList({ items, cy, emptyLabel }) {
  if (!items.length) {
    return <div data-cy={cy} className="empty-state" style={{ padding: 16, fontSize: 13 }}>{emptyLabel}</div>;
  }
  return (
    <div data-cy={cy}>
      {items.map(item => (
        <div key={item.id} style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '10px 0', borderBottom: '1px solid var(--border)', fontSize: 13 }}>
          <span className={`badge ${statusBadgeClass(item.status)}`}>{item.status}</span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontWeight: 600, whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{item.title}</div>
            <div style={{ fontSize: 11, color: 'var(--text-muted)' }}>{item.assigned_agent_name || item.assigned_agent_id || 'unassigned'}</div>
          </div>
          <Link href={`/work/${item.id}`} className="btn btn-ghost btn-sm">View</Link>
        </div>
      ))}
    </div>
  );
}

export default function DashboardClient({
  initialSummary,
  initialOrchestratorStatus,
  initialRunningItems,
  initialApprovalItems,
  initialFailedItems,
}) {
  const [summary, setSummary] = useState(initialSummary);
  const [orchStatus, setOrchStatus] = useState(initialOrchestratorStatus);
  const [runningItems, setRunningItems] = useState(initialRunningItems ?? []);
  const [approvalItems, setApprovalItems] = useState(initialApprovalItems ?? []);
  const [failedItems, setFailedItems] = useState(initialFailedItems ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [heartbeatRunning, setHeartbeatRunning] = useState(false);
  const [heartbeatResult, setHeartbeatResult] = useState(null);
  const [heartbeatError, setHeartbeatError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const [summaryData, statusData, runningData, approvalData, failedData] = await Promise.allSettled([
        workApi.getSummary(),
        workApi.getOrchestratorStatus(),
        workApi.getWork({ status: 'running', limit: 20 }),
        workApi.getWork({ status: 'waiting_approval', limit: 20 }),
        workApi.getWork({ status: 'failed', limit: 20 }),
      ]);
      if (summaryData.status === 'fulfilled') setSummary(summaryData.value);
      if (statusData.status === 'fulfilled') setOrchStatus(statusData.value);
      if (runningData.status === 'fulfilled') setRunningItems(runningData.value?.items ?? []);
      if (approvalData.status === 'fulfilled') setApprovalItems(approvalData.value?.items ?? []);
      if (failedData.status === 'fulfilled') setFailedItems(failedData.value?.items ?? []);
    } catch (e) {
      setError(e.message || 'Could not refresh dashboard.');
    }
    setLoading(false);
  }, []);

  const triggerHeartbeat = useCallback(async () => {
    setHeartbeatRunning(true);
    setHeartbeatError('');
    setHeartbeatResult(null);
    try {
      const result = await workApi.triggerHeartbeat();
      setHeartbeatResult(result);
      await refresh();
    } catch (e) {
      setHeartbeatError(e.message || 'Heartbeat failed.');
    }
    setHeartbeatRunning(false);
  }, [refresh]);

  return (
    <div data-cy="dashboard-page">
      <div className="page-header">
        <h1 style={{ margin: 0 }}>Operator Dashboard</h1>
        <div style={{ display: 'flex', gap: 8 }}>
          <button
            data-cy="dashboard-heartbeat-btn"
            className="btn btn-primary btn-sm"
            onClick={triggerHeartbeat}
            disabled={heartbeatRunning || loading}
          >
            {heartbeatRunning ? 'Running…' : '⚡ Trigger Heartbeat'}
          </button>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>

      <div className="page-body">
        {error && <div className="error-state" style={{ marginBottom: 16 }}>⚠ {error}</div>}

        {heartbeatError && (
          <div className="error-state" style={{ marginBottom: 16 }}>⚠ {heartbeatError}</div>
        )}
        {heartbeatResult && (
          <div className="card" style={{ marginBottom: 20, fontSize: 13 }}>
            <strong>Heartbeat completed</strong> — claimed {heartbeatResult.items_claimed ?? 0}, started {heartbeatResult.items_scheduled_started ?? 0} scheduled, delegated {heartbeatResult.items_delegated ?? 0}
            {heartbeatResult.duration_ms != null && <span style={{ color: 'var(--text-muted)' }}> ({heartbeatResult.duration_ms}ms)</span>}
          </div>
        )}

        {/* Summary tiles */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(120px, 1fr))', gap: 12, marginBottom: 24 }}>
          <StatTile label="Pending"         value={summary?.pending}          cy="dashboard-pending-count" />
          <StatTile label="Ready"           value={summary?.ready}            cy="dashboard-ready-count" />
          <StatTile label="Running"         value={summary?.running}          cy="dashboard-running-count"  accent="accent" />
          <StatTile label="Awaiting Appr."  value={summary?.waiting_approval} cy="dashboard-approval-count" accent="warn" />
          <StatTile label="Failed"          value={summary?.failed}           cy="dashboard-failed-count"   accent="error" />
          <StatTile label="Completed"       value={summary?.completed}        cy="dashboard-completed-count" accent="success" />
          <StatTile label="Scheduled"       value={summary?.scheduled}        cy="dashboard-scheduled-count" />
        </div>

        {/* Orchestrator status */}
        {orchStatus && (
          <div className="card" style={{ marginBottom: 24 }} data-cy="dashboard-orchestrator-status">
            <h3 style={{ marginTop: 0 }}>Orchestrator</h3>
            <div style={{ display: 'flex', gap: 32, flexWrap: 'wrap', fontSize: 13 }}>
              <div>
                <span style={{ color: 'var(--text-muted)' }}>Status </span>
                <span className={`badge ${orchStatus.running ? 'badge-success' : 'badge-dim'}`}>
                  {orchStatus.running ? 'Running' : 'Idle'}
                </span>
              </div>
              {orchStatus.last_heartbeat_at && (
                <div>
                  <span style={{ color: 'var(--text-muted)' }}>Last heartbeat </span>
                  <span>{fmtDate(orchStatus.last_heartbeat_at)}</span>
                </div>
              )}
              {orchStatus.uptime_secs != null && (
                <div>
                  <span style={{ color: 'var(--text-muted)' }}>Uptime </span>
                  <span>{orchStatus.uptime_secs}s</span>
                </div>
              )}
              {orchStatus.queued_count != null && (
                <div>
                  <span style={{ color: 'var(--text-muted)' }}>Queued </span>
                  <span>{orchStatus.queued_count}</span>
                </div>
              )}
              {orchStatus.running_count != null && (
                <div>
                  <span style={{ color: 'var(--text-muted)' }}>Running </span>
                  <span>{orchStatus.running_count}</span>
                </div>
              )}
              {orchStatus.pending_approval_count != null && (
                <div>
                  <span style={{ color: 'var(--text-muted)' }}>Awaiting Approval </span>
                  <span>{orchStatus.pending_approval_count}</span>
                </div>
              )}
            </div>
          </div>
        )}

        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(320px, 1fr))', gap: 20 }}>
          {/* Running */}
          <div className="card">
            <h3 style={{ marginTop: 0 }}>Running ({runningItems.length})</h3>
            <WorkList items={runningItems} cy="dashboard-running-list" emptyLabel="No items running." />
          </div>

          {/* Awaiting approval */}
          <div className="card">
            <h3 style={{ marginTop: 0 }}>Awaiting Approval ({approvalItems.length})</h3>
            <WorkList items={approvalItems} cy="dashboard-approval-list" emptyLabel="No items awaiting approval." />
          </div>

          {/* Failed */}
          <div className="card">
            <h3 style={{ marginTop: 0 }}>Failed ({failedItems.length})</h3>
            <WorkList items={failedItems} cy="dashboard-failed-list" emptyLabel="No failed items." />
          </div>
        </div>
      </div>
    </div>
  );
}
