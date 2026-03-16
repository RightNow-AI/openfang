'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../lib/work-api';

function statusBadgeClass(s) {
  if (s === 'completed') return 'success';
  if (s === 'running') return 'info';
  if (s === 'failed' || s === 'rejected') return 'error';
  if (s === 'waiting_approval') return 'warn';
  if (s === 'blocked') return 'error';
  if (s === 'retry_scheduled') return 'warn';
  if (s === 'delegated_to_subagent') return 'info';
  if (s === 'pending' || s === 'ready') return 'created';
  return 'dim';
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
            <span style={{ fontSize: 28, opacity: 0.35 }}>∅</span>
            <div>
              <div style={{ fontWeight: 600, color: 'var(--text-secondary)', marginBottom: 4 }}>Inbox is empty</div>
              <div className="text-dim text-sm">Pending work items will appear here when created.</div>
            </div>
          </div>
        )}
        {items.length > 0 && (
          <div data-cy="inbox-list" className="card" style={{ padding: 0, overflow: 'hidden' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th style={{ width: '25%' }}>Title</th>
                  <th>Summary</th>
                  <th style={{ width: 120 }}>Status</th>
                  <th style={{ width: 140 }}>Agent</th>
                  <th style={{ width: 140 }}>Created</th>
                  <th style={{ width: 60 }}></th>
                </tr>
              </thead>
              <tbody>
                {items.map(item => (
                  <tr key={item.id} data-cy="inbox-item">
                    <td style={{ fontWeight: 600, color: 'var(--text)', maxWidth: 200 }} className="truncate">{item.title}</td>
                    <td style={{ fontSize: 12, color: 'var(--text-dim)', maxWidth: 280 }} className="truncate">
                      {item.description || <span className="text-muted">—</span>}
                    </td>
                    <td>
                      <span className={`badge badge-${statusBadgeClass(item.status)}`}>
                        {item.status?.replace(/_/g, ' ')}
                      </span>
                    </td>
                    <td style={{ fontSize: 12, color: 'var(--text-secondary)' }}>{item.assigned_agent_name || <span className="text-muted">—</span>}</td>
                    <td style={{ fontSize: 11, color: 'var(--text-muted)', whiteSpace: 'nowrap' }}>
                      {fmtDate(item.created_at)}
                    </td>
                    <td style={{ textAlign: 'right' }}>
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
