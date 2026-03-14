'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../lib/work-api';

function statusBadgeClass(s) {
  if (s === 'completed') return 'badge-success';
  if (s === 'running') return 'badge-accent';
  if (s === 'failed' || s === 'rejected') return 'badge-error';
  if (s === 'waiting_approval') return 'badge-warn';
  return 'badge-dim';
}

function fmtDate(iso) {
  if (!iso) return '—';
  try { return new Date(iso).toLocaleString(); } catch { return iso; }
}

export default function InboxClient({ initialItems }) {
  const [items, setItems] = useState(initialItems ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await workApi.getWork({ status: 'pending' });
      setItems(Array.isArray(data?.items) ? data.items : []);
    } catch (e) {
      setError(e.message || 'Could not load inbox.');
    }
    setLoading(false);
  }, []);

  return (
    <div data-cy="inbox-page">
      <div className="page-header">
        <h1>Inbox</h1>
        <div className="flex items-center gap-2">
          <span className="text-dim text-sm">{items.length} item{items.length !== 1 ? 's' : ''}</span>
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="inbox-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && items.length === 0 && (
          <div data-cy="inbox-empty" className="empty-state">
            Inbox is empty. Pending work items will appear here.
          </div>
        )}
        {items.length > 0 && (
          <div data-cy="inbox-list" className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Title</th>
                  <th>Summary</th>
                  <th>Status</th>
                  <th>Assigned Agent</th>
                  <th>Created</th>
                  <th></th>
                </tr>
              </thead>
              <tbody>
                {items.map(item => (
                  <tr key={item.id} data-cy="inbox-item">
                    <td style={{ fontWeight: 600, maxWidth: 200 }}>{item.title}</td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 280 }}>
                      {item.description || '—'}
                    </td>
                    <td>
                      <span className={`badge ${statusBadgeClass(item.status)}`}>
                        {item.status}
                      </span>
                    </td>
                    <td style={{ fontSize: 12 }}>{item.assigned_agent_name || '—'}</td>
                    <td style={{ fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>
                      {fmtDate(item.created_at)}
                    </td>
                    <td>
                      <Link
                        data-cy="inbox-item-detail-link"
                        href={`/work/${item.id}`}
                        className="btn btn-ghost btn-xs"
                      >
                        View
                      </Link>
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
