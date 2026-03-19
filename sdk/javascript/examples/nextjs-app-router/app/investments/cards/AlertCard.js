'use client';

import { ALERT_TYPE_LABELS, APPROVAL_STATUS_LABELS } from '../lib/investments-copy';
import { severityBadgeClass, approvalStatusBadgeClass } from '../lib/investments-ui';

export default function AlertCard({ alert, onOpenAlert, onApprove }) {
  const needsApproval = alert.approval_required && alert.approval_status === 'pending';

  return (
    <div
      className="card"
      style={{ cursor: 'pointer', borderLeft: `3px solid ${needsApproval ? 'var(--accent)' : 'transparent'}` }}
      onClick={() => onOpenAlert(alert.id)}
      data-cy={`alert-${alert.id}`}
    >
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 8 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 5, flexWrap: 'wrap', marginBottom: 4 }}>
            <span className="badge badge-dim" style={{ fontSize: 10 }}>
              {ALERT_TYPE_LABELS[alert.type] || alert.type}
            </span>
            <span className={`badge ${severityBadgeClass(alert.severity)}`} style={{ fontSize: 10 }}>
              {alert.severity?.toUpperCase()}
            </span>
            {alert.symbol && (
              <span style={{ fontFamily: 'monospace', fontWeight: 700, fontSize: 12, color: 'var(--text)' }}>
                {alert.symbol}
              </span>
            )}
          </div>
          <p style={{ fontSize: 12, color: 'var(--text)', marginBottom: 4 }}>{alert.description}</p>
          {alert.approval_status && (
            <span className={`badge ${approvalStatusBadgeClass(alert.approval_status)}`} style={{ fontSize: 10 }}>
              {APPROVAL_STATUS_LABELS[alert.approval_status] || alert.approval_status}
            </span>
          )}
        </div>
      </div>

      {needsApproval && (
        <div style={{ marginTop: 10 }}>
          <button
            className="btn btn-primary btn-sm"
            onClick={(e) => { e.stopPropagation(); onApprove(alert.id); }}
          >
            Approve action
          </button>
        </div>
      )}
    </div>
  );
}
