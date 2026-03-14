'use client';
import { useState, useCallback } from 'react';
import Link from 'next/link';
import { workApi } from '../../lib/work-api';

function approvalStatusBadge(s) {
  if (s === 'approved') return <span className="badge badge-success">{s}</span>;
  if (s === 'rejected') return <span className="badge badge-error">{s}</span>;
  if (s === 'pending') return <span className="badge badge-warn">{s}</span>;
  return <span className="badge badge-dim">{s || 'unknown'}</span>;
}

export default function ApprovalsClient({ initialApprovals }) {
  const [approvals, setApprovals] = useState(initialApprovals ?? []);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [acting, setActing] = useState({});
  const [feedback, setFeedback] = useState({});

  const refresh = useCallback(async () => {
    setLoading(true);
    setError('');
    try {
      const data = await workApi.getWork({ approval_status: 'pending' });
      setApprovals(Array.isArray(data?.items) ? data.items : []);
    } catch (e) {
      setError(e.message || 'Could not load approvals.');
    }
    setLoading(false);
  }, []);

  const act = useCallback(async (id, action) => {
    setActing(prev => ({ ...prev, [id]: true }));
    setFeedback(prev => ({ ...prev, [id]: null }));
    try {
      if (action === 'approve') {
        await workApi.approveWork(id);
      } else {
        await workApi.rejectWork(id);
      }
      setFeedback(prev => ({ ...prev, [id]: { ok: true, msg: action === 'approve' ? 'Approved' : 'Rejected' } }));
      // Remove from list after short delay so user sees the feedback
      setTimeout(() => setApprovals(prev => prev.filter(a => a.id !== id)), 800);
    } catch (e) {
      setFeedback(prev => ({ ...prev, [id]: { ok: false, msg: e.message || `Could not ${action}.` } }));
    }
    setActing(prev => ({ ...prev, [id]: false }));
  }, []);

  return (
    <div data-cy="approvals-page">
      <div className="page-header">
        <h1>Approvals</h1>
        <div className="flex items-center gap-2">
          {approvals.length > 0 && (
            <span className="badge badge-warn">{approvals.length} pending</span>
          )}
          <button className="btn btn-ghost btn-sm" onClick={refresh} disabled={loading}>
            {loading ? 'Refreshing…' : 'Refresh'}
          </button>
        </div>
      </div>
      <div className="page-body">
        {error && (
          <div data-cy="approvals-error" className="error-state">
            ⚠ {error}
            <button className="btn btn-ghost btn-sm" onClick={refresh}>Retry</button>
          </div>
        )}
        {!error && approvals.length === 0 && (
          <div data-cy="approvals-empty" className="empty-state">
            <div style={{ textAlign: 'center' }}>
              <div style={{ fontSize: 32, marginBottom: 8 }}>✓</div>
              <div>No pending approvals. Work items requiring sign-off will appear here.</div>
            </div>
          </div>
        )}
        {approvals.length > 0 && (
          <div data-cy="approvals-list" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
            {approvals.map(item => (
              <div key={item.id} data-cy="approval-card" className="card" style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
                <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 12 }}>
                  <div style={{ flex: 1, minWidth: 0 }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 6, flexWrap: 'wrap' }}>
                      {approvalStatusBadge(item.approval_status)}
                      <span style={{ fontWeight: 600, fontSize: 14 }}>{item.title}</span>
                      {item.assigned_agent_name && (
                        <span className="text-dim text-sm">by {item.assigned_agent_name}</span>
                      )}
                    </div>
                    {item.description && (
                      <p style={{ margin: '0 0 4px', fontSize: 13, color: 'var(--text-secondary)', lineHeight: 1.5 }}>
                        {item.description}
                      </p>
                    )}
                    {item.approval_note && (
                      <div style={{
                        marginTop: 8,
                        padding: '7px 10px',
                        background: 'var(--surface2)',
                        borderRadius: 'var(--radius-sm)',
                        fontSize: 12,
                        color: 'var(--text-secondary)',
                      }}>
                        {item.approval_note}
                      </div>
                    )}
                  </div>
                  <Link href={`/work/${item.id}`} className="btn btn-ghost btn-xs" style={{ flexShrink: 0 }}>
                    Detail
                  </Link>
                </div>

                {feedback[item.id] && (
                  <div className={feedback[item.id].ok ? 'success-state' : 'error-state'} style={{ fontSize: 13, padding: '6px 10px' }}>
                    {feedback[item.id].msg}
                  </div>
                )}

                <div style={{ display: 'flex', gap: 8 }}>
                  <button
                    data-cy="approval-approve-btn"
                    className="btn btn-primary btn-sm"
                    onClick={() => act(item.id, 'approve')}
                    disabled={!!acting[item.id]}
                  >
                    {acting[item.id] ? '…' : '✓ Approve'}
                  </button>
                  <button
                    data-cy="approval-reject-btn"
                    className="btn btn-ghost btn-sm"
                    style={{ color: 'var(--error)', borderColor: 'var(--error-muted)' }}
                    onClick={() => act(item.id, 'reject')}
                    disabled={!!acting[item.id]}
                  >
                    {acting[item.id] ? '…' : '✕ Reject'}
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
