'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../lib/work-api';

function statusBadgeClass(s) {
  if (s === 'completed') return 'badge-success';
  if (s === 'running') return 'badge-accent';
  if (s === 'failed' || s === 'cancelled') return 'badge-error';
  if (s === 'waiting_approval') return 'badge-warn';
  return 'badge-dim';
}

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

export default function SchedulerClient({ initialItems }) {
  const [items, setItems] = useState(initialItems ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [acting, setActing] = useState({});

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await workApi.getWork({ scheduled: 'true' });
      setItems(Array.isArray(data?.items) ? data.items : []);
    } catch (e) {
      setError(e.message || 'Could not load scheduled work items.');
    }
    setLoading(false);
  }, []);

  const cancel = useCallback(async (id) => {
    if (!window.confirm('Cancel this work item?')) return;
    setActing(prev => ({ ...prev, [id]: 'cancel' }));
    try {
      await workApi.cancelWork(id);
      await refresh();
    } catch (e) {
      setError(e.message || 'Could not cancel item.');
    }
    setActing(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  const retry = useCallback(async (id) => {
    setActing(prev => ({ ...prev, [id]: 'retry' }));
    try {
      await workApi.retryWork(id);
      await refresh();
    } catch (e) {
      setError(e.message || 'Could not retry item.');
    }
    setActing(prev => ({ ...prev, [id]: null }));
  }, [refresh]);

  return (
    <div data-cy="scheduler-page">
      <div className="page-header">
        <h1>Scheduler</h1>
        <div className="flex items-center gap-2">
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="scheduler-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && items.length === 0 && (
          <div data-cy="scheduler-empty" className="empty-state">
            No scheduled work items. Create work items with a scheduled_at time to see them here.
          </div>
        )}
        {items.length > 0 && (
          <div data-cy="scheduled-list" className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Title</th>
                  <th>Scheduled For</th>
                  <th>Status</th>
                  <th>Retries</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {items.map(item => (
                  <tr key={item.id} data-cy="scheduled-item">
                    <td style={{ fontWeight: 600, maxWidth: 220 }}>
                      <Link href={`/work/${item.id}`} style={{ color: 'var(--accent)' }}>
                        {item.title}
                      </Link>
                    </td>
                    <td style={{ fontSize: 12, whiteSpace: 'nowrap' }}>{fmtDate(item.scheduled_at)}</td>
                    <td>
                      <span className={`badge ${statusBadgeClass(item.status)}`}>
                        {item.status}
                      </span>
                    </td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)' }}>
                      {item.retry_count ?? 0} / {item.max_retries ?? 0}
                    </td>
                    <td style={{ display: 'flex', gap: 6 }}>
                      {item.status !== 'cancelled' && item.status !== 'completed' && (
                        <button
                          data-cy="schedule-cancel-btn"
                          className="btn btn-ghost btn-xs"
                          style={{ color: 'var(--error)' }}
                          onClick={() => cancel(item.id)}
                          disabled={acting[item.id] === 'cancel'}
                        >
                          {acting[item.id] === 'cancel' ? '…' : 'Cancel'}
                        </button>
                      )}
                      {item.status === 'failed' && (
                        <button
                          data-cy="schedule-retry-btn"
                          className="btn btn-ghost btn-xs"
                          onClick={() => retry(item.id)}
                          disabled={acting[item.id] === 'retry'}
                        >
                          {acting[item.id] === 'retry' ? '…' : 'Retry'}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
