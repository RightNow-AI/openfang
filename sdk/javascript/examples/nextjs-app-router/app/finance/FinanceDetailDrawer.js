'use client';

const DETAIL_TITLES = {
  cash_flow: 'Cash Flow',
  approvals: 'Approval Queue',
  revenue: 'Revenue Detail',
  expenses: 'Expense Detail',
  margins: 'Margin by Mode',
  risks: 'Risk Alerts',
  server_api_costs: 'Server & API Costs',
  sales_revenue: 'Sales Revenue',
};

export default function FinanceDetailDrawer({ itemId, detail, loading, error, onApproveAction, onClose }) {
  if (!itemId) return null;

  return (
    <div
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 900,
        display: 'flex',
        justifyContent: 'flex-end',
      }}
      data-cy="finance-detail-drawer"
    >
      <div
        style={{
          position: 'absolute', inset: 0,
          background: 'rgba(0,0,0,0.35)',
        }}
        onClick={onClose}
      />
      <div
        style={{
          position: 'relative',
          width: '100%',
          maxWidth: 400,
          background: 'var(--bg-card)',
          borderLeft: '1px solid var(--border)',
          boxShadow: '-12px 0 40px rgba(0,0,0,0.25)',
          display: 'flex',
          flexDirection: 'column',
          zIndex: 1,
        }}
      >
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '16px 18px',
          borderBottom: '1px solid var(--border)',
        }}>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)' }}>
            {DETAIL_TITLES[itemId] || itemId}
          </div>
          <button
            className="btn btn-ghost btn-xs"
            onClick={onClose}
            aria-label="Close detail panel"
            style={{ padding: '3px 7px' }}
          >
            ✕
          </button>
        </div>

        <div style={{ flex: 1, overflowY: 'auto', padding: '18px' }}>
          {loading && (
            <div style={{ textAlign: 'center', padding: '32px 0' }}>
              <span className="spinner" />
            </div>
          )}
          {error && (
            <div className="badge badge-error" style={{ padding: '8px 12px', fontSize: 12 }}>
              {error}
            </div>
          )}
          {!loading && !error && detail && (
            <pre style={{
              fontSize: 11,
              color: 'var(--text-dim)',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
              background: 'var(--bg-elevated)',
              padding: 12,
              borderRadius: 8,
              border: '1px solid var(--border)',
            }}>
              {JSON.stringify(detail, null, 2)}
            </pre>
          )}
          {!loading && !error && !detail && (
            <p style={{ fontSize: 13, color: 'var(--text-dim)', textAlign: 'center', paddingTop: 24 }}>
              No detail available.
            </p>
          )}
        </div>

        {onApproveAction && detail && (
          <div style={{ padding: '14px 18px', borderTop: '1px solid var(--border)' }}>
            <button
              className="btn btn-primary"
              style={{ width: '100%' }}
              onClick={() => onApproveAction(itemId)}
              data-cy="drawer-approve"
            >
              Approve this action
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
