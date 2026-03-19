'use client';

import { riskSeverityBadgeClass } from '../lib/finance-ui';

export default function RiskAlertsCard({ risks, onOpenDetail }) {
  const high = risks.filter((r) => r.severity === 'high');
  const med = risks.filter((r) => r.severity === 'medium');
  const low = risks.filter((r) => r.severity === 'low');
  const sorted = [...high, ...med, ...low];

  return (
    <div
      className="card"
      data-cy="risk-alerts-card"
      style={
        high.length > 0
          ? { borderColor: 'var(--error)', background: 'var(--error-subtle)' }
          : med.length > 0
          ? { borderColor: 'var(--warning)', background: 'var(--warning-subtle)' }
          : {}
      }
    >
      <div className="card-header">
        <span>Risk Alerts</span>
        {risks.length > 0 && (
          <span className={high.length > 0 ? 'badge badge-error' : med.length > 0 ? 'badge badge-warn' : 'badge badge-dim'}>
            {risks.length} alert{risks.length !== 1 ? 's' : ''}
          </span>
        )}
      </div>

      {risks.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '6px 0' }}>
          No risks detected. Finance looks healthy.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
          {sorted.slice(0, 4).map((risk) => (
            <div key={risk.id} style={{ display: 'flex', gap: 10 }}>
              <span className={riskSeverityBadgeClass(risk.severity)} style={{ flexShrink: 0, alignSelf: 'flex-start' }}>
                {risk.severity}
              </span>
              <div>
                <div style={{ fontSize: 13, fontWeight: 600, color: 'var(--text)', marginBottom: 2 }}>
                  {risk.title}
                </div>
                <div style={{ fontSize: 12, color: 'var(--text-dim)', lineHeight: 1.4 }}>
                  {risk.description}
                </div>
              </div>
            </div>
          ))}
          {risks.length > 4 && (
            <button className="btn btn-ghost btn-sm" onClick={onOpenDetail} data-cy="more-risks-btn">
              View all {risks.length} risks →
            </button>
          )}
        </div>
      )}
    </div>
  );
}
