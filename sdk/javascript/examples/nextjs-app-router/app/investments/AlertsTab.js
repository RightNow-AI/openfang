'use client';

import AlertCard from './cards/AlertCard';
import { sortAlertsBySeverity } from './lib/investments-ui';

export default function AlertsTab({ alerts, onOpenAlert, onApproveAlert }) {
  const sorted = sortAlertsBySeverity(alerts || []);
  const pending = sorted.filter((a) => a.approval_required && a.approval_status === 'pending');

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'baseline', gap: 10, marginBottom: 18 }}>
        <h2 style={{ fontSize: 15, fontWeight: 800, color: 'var(--text)' }}>What needs attention</h2>
        {pending.length > 0 && (
          <span className="badge badge-error" style={{ fontSize: 11 }}>
            {pending.length} waiting for approval
          </span>
        )}
      </div>

      {sorted.length === 0 ? (
        <div style={{ textAlign: 'center', padding: '44px 20px', color: 'var(--text-dim)' }}>
          <div style={{ fontSize: 14, fontWeight: 700, marginBottom: 6 }}>No alerts right now</div>
          <p style={{ fontSize: 12 }}>That is a good sign.</p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {sorted.map((alert) => (
            <AlertCard
              key={alert.id}
              alert={alert}
              onOpenAlert={onOpenAlert}
              onApprove={onApproveAlert}
            />
          ))}
        </div>
      )}
    </div>
  );
}
