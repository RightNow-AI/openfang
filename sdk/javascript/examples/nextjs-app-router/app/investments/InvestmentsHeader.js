'use client';

export default function InvestmentsHeader({
  watchCount,
  alertCount,
  onRefresh,
  onOpenWizard,
  onOpenTemplates,
  refreshing,
}) {
  return (
    <div style={{ marginBottom: 24 }}>
      <div style={{ display: 'flex', alignItems: 'flex-start', justifyContent: 'space-between', gap: 16, flexWrap: 'wrap' }}>
        <div>
          <h1 style={{ fontSize: 22, fontWeight: 900, color: 'var(--text)', marginBottom: 5 }}>
            Investment Intelligence
          </h1>
          <p style={{ fontSize: 13, color: 'var(--text-dim)', maxWidth: 540 }}>
            Track markets, watch patterns, update your thesis, and approve important decisions before anything moves.
          </p>
          {(watchCount > 0 || alertCount > 0) && (
            <div style={{ display: 'flex', gap: 10, marginTop: 8 }}>
              {watchCount > 0 && (
                <span className="badge badge-dim" style={{ fontSize: 11 }}>
                  {watchCount} watching
                </span>
              )}
              {alertCount > 0 && (
                <span className="badge badge-error" style={{ fontSize: 11 }}>
                  {alertCount} alert{alertCount !== 1 ? 's' : ''}
                </span>
              )}
            </div>
          )}
        </div>
        <div style={{ display: 'flex', gap: 8, flexShrink: 0, flexWrap: 'wrap' }}>
          <button
            className="btn btn-ghost btn-sm"
            onClick={onRefresh}
            disabled={refreshing}
            title="Refresh data"
            style={{ minWidth: 36 }}
          >
            {refreshing ? (
              <span className="spinner" style={{ width: 13, height: 13 }} />
            ) : '↻'}
          </button>
          <button className="btn btn-ghost btn-sm" onClick={onOpenTemplates}>
            Use a starter template
          </button>
          <button className="btn btn-primary btn-sm" onClick={onOpenWizard}>
            Set up investment research for me
          </button>
        </div>
      </div>
    </div>
  );
}
