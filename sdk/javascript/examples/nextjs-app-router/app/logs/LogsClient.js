'use client';
import { useState, useCallback } from 'react';
import { apiClient } from '../../lib/api-client';

const ACTION_CLASS = {
  create: 'badge-success',
  delete: 'badge-error',
  update: 'badge-warn',
  start: 'badge-success',
  stop: 'badge-muted',
  error: 'badge-error',
};

function normalizeEntry(raw, i) {
  return {
    id: raw?.id ?? i,
    timestamp: raw?.timestamp ?? raw?.created_at ?? '',
    action: raw?.action ?? raw?.event_type ?? '',
    subject: raw?.subject ?? raw?.resource ?? raw?.target ?? '',
    detail: raw?.detail ?? raw?.message ?? raw?.description ?? '',
    actor: raw?.actor ?? raw?.user ?? raw?.source ?? '',
  };
}

function actionBadge(action) {
  const key = (action || '').toLowerCase().split('_')[0];
  const cls = ACTION_CLASS[key] || 'badge-dim';
  return <span className={`badge ${cls}`}>{action || '—'}</span>;
}

function fmt(ts) {
  if (!ts) return '—';
  try { return new Date(ts).toLocaleString(); } catch { return ts; }
}

export default function LogsClient({ initialEntries }) {
  const [entries, setEntries] = useState(initialEntries ?? []);
  const [limit, setLimit] = useState(50);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const load = useCallback(async (n = limit) => {
    setLoading(true);
    setError('');
    try {
      const data = await apiClient.get(`/api/audit/recent?n=${n}`);
      const arr = Array.isArray(data) ? data : data?.entries ?? data?.events ?? [];
      setEntries(arr.map(normalizeEntry));
    } catch (e) {
      setError(e.message || 'Could not load audit log.');
    }
    setLoading(false);
  }, [limit]);

  function loadMore() {
    const next = limit + 50;
    setLimit(next);
    load(next);
  }

  return (
    <div>
      <div className="page-header">
        <h1>Audit Logs</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{entries.length} entries</span>
          <button className="btn btn-ghost btn-sm" onClick={() => load()} disabled={loading}>
            {loading ? 'Loading…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={() => load()}>Retry</button>
          </div>
        )}
        {entries.length === 0 && !error && (
          <div className="empty-state">No audit entries recorded yet.</div>
        )}
        {entries.length > 0 && (
          <>
            <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Time</th>
                    <th>Action</th>
                    <th>Subject</th>
                    <th>Detail</th>
                    <th>Actor</th>
                  </tr>
                </thead>
                <tbody>
                  {entries.map(e => (
                    <tr key={e.id}>
                      <td style={{ whiteSpace: 'nowrap', fontSize: 12, color: 'var(--text-dim)' }}>{fmt(e.timestamp)}</td>
                      <td>{actionBadge(e.action)}</td>
                      <td style={{ maxWidth: 240, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                        {e.subject || <span className="text-muted">—</span>}
                      </td>
                      <td style={{ maxWidth: 340, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: 12, color: 'var(--text-dim)' }}>
                        {e.detail || <span className="text-muted">—</span>}
                      </td>
                      <td style={{ fontSize: 12 }}>{e.actor || <span className="text-muted">system</span>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            {entries.length >= limit && (
              <button className="btn btn-ghost btn-sm" style={{ marginTop: 12 }} onClick={loadMore} disabled={loading}>
                Load more
              </button>
            )}
          </>
        )}
      </div>
    </div>
  );
}
