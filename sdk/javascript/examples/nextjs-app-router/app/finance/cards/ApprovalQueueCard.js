'use client';

export default function ApprovalQueueCard({ approvalsWaiting, onOpenDetail }) {
  const hasItems = approvalsWaiting > 0;

  return (
    <div
      className="card"
      data-cy="approval-queue-card"
      style={hasItems ? { borderColor: 'var(--warning)', background: 'var(--warning-subtle)' } : {}}
    >
      <div className="card-header">
        <span>Approval Queue</span>
        {hasItems && (
          <span className="badge badge-warn">{approvalsWaiting} waiting</span>
        )}
      </div>

      {hasItems ? (
        <>
          <p style={{ fontSize: 13, color: 'var(--text-secondary)', marginBottom: 14 }}>
            {approvalsWaiting} finance action{approvalsWaiting !== 1 ? 's' : ''} need your review before
            they can proceed.
          </p>
          <button
            className="btn btn-primary btn-sm"
            onClick={onOpenDetail}
            data-cy="review-approvals-btn"
          >
            Review approvals
          </button>
        </>
      ) : (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '6px 0' }}>
          No pending approvals. You are all caught up.
        </div>
      )}
    </div>
  );
}
