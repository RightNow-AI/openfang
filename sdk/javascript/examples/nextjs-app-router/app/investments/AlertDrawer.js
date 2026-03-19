'use client';

import { ALERT_TYPE_LABELS, APPROVAL_STATUS_LABELS } from './lib/investments-copy';
import { severityBadgeClass, approvalStatusBadgeClass } from './lib/investments-ui';

const APPROVAL_COPY = {
  alert_before_trade: 'Before any position is opened, you review the signal and say yes.',
  confirm_new_position: 'A new position is ready. Review the thesis and confirm.',
  confirm_exit: 'An exit signal fired. Review and confirm the trade closes.',
  weekly_review: 'Your weekly review is ready. Approve to mark it complete.',
  thesis_update: 'Something changed. Review the thesis update and decide if it still holds.',
};

export default function AlertDrawer({ alert, onApprove, onReject, onRequestChanges, onClose }) {
  if (!alert) return null;

  const needsApproval = alert.approval_required && alert.approval_status === 'pending';
  const approvalCopy = APPROVAL_COPY[alert.approval_type] || 'Review this alert and decide how to proceed.';

  return (
    <div
      style={{
        position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.4)', zIndex: 8000,
        display: 'flex', justifyContent: 'flex-end',
      }}
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        style={{
          width: '100%', maxWidth: 440, height: '100%',
          background: 'var(--background)', borderLeft: '1.5px solid var(--border)',
          overflowY: 'auto', padding: '24px 22px',
          display: 'flex', flexDirection: 'column', gap: 16,
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div style={{ display: 'flex', gap: 7, alignItems: 'center', flexWrap: 'wrap' }}>
            <span className="badge badge-dim" style={{ fontSize: 10 }}>
              {ALERT_TYPE_LABELS[alert.type] || alert.type}
            </span>
            <span className={`badge ${severityBadgeClass(alert.severity)}`} style={{ fontSize: 10 }}>
              {alert.severity?.toUpperCase()}
            </span>
            {alert.symbol && (
              <span style={{ fontFamily: 'monospace', fontWeight: 800, fontSize: 14, color: 'var(--text)' }}>
                {alert.symbol}
              </span>
            )}
          </div>
          <button className="btn btn-ghost btn-xs" onClick={onClose}>✕</button>
        </div>

        <Section title="What changed">
          <p style={{ fontSize: 13, color: 'var(--text)' }}>{alert.description}</p>
        </Section>

        {alert.why_it_matters && (
          <Section title="Why it matters">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{alert.why_it_matters}</p>
          </Section>
        )}

        {alert.pattern && (
          <Section title="Pattern detected">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{alert.pattern}</p>
          </Section>
        )}

        {alert.suggested_action && (
          <Section title="Suggested action">
            <p style={{ fontSize: 13, color: 'var(--text-dim)' }}>{alert.suggested_action}</p>
          </Section>
        )}

        {alert.approval_status && (
          <Section title="Approval status">
            <span className={`badge ${approvalStatusBadgeClass(alert.approval_status)}`} style={{ fontSize: 11 }}>
              {APPROVAL_STATUS_LABELS[alert.approval_status] || alert.approval_status}
            </span>
          </Section>
        )}

        {needsApproval && (
          <div
            style={{
              background: 'rgba(255,106,26,0.07)',
              border: '1.5px solid rgba(255,106,26,0.25)',
              borderRadius: 8,
              padding: '12px 14px',
              fontSize: 12,
              color: 'var(--text)',
            }}
          >
            <strong style={{ color: 'var(--accent)' }}>Your approval needed:</strong>{' '}
            {approvalCopy}
          </div>
        )}

        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', marginTop: 'auto', paddingTop: 16, borderTop: '1px solid var(--border)' }}>
          {needsApproval && (
            <>
              <button className="btn btn-primary btn-sm" onClick={() => onApprove(alert.id)}>Approve</button>
              <button className="btn btn-ghost btn-sm" onClick={() => onRequestChanges(alert.id)}>Request changes</button>
              <button className="btn btn-ghost btn-sm" style={{ color: '#ef4444' }} onClick={() => onReject(alert.id)}>Reject</button>
            </>
          )}
          <button className="btn btn-ghost btn-sm" onClick={onClose}>Close</button>
        </div>
      </div>
    </div>
  );
}

function Section({ title, children }) {
  return (
    <div>
      <div style={{ fontSize: 10, fontWeight: 700, color: 'var(--text-dim)', textTransform: 'uppercase', letterSpacing: 0.5, marginBottom: 5 }}>
        {title}
      </div>
      {children}
    </div>
  );
}
