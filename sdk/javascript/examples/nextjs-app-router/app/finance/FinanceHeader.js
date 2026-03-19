'use client';

export default function FinanceHeader({ hasProfile, onOpenWizard, onSwitchTab, onRefresh, refreshing }) {
  return (
    <div className="page-header" data-cy="finance-header">
      <div>
        <h1 style={{ margin: 0 }}>Finance</h1>
        <p style={{ margin: '4px 0 0', fontSize: 13, color: 'var(--text-dim)' }}>
          {hasProfile
            ? 'Approvals, decisions, and money signals — all in one place.'
            : 'Set up your finance layer to start tracking income, expenses, and approvals.'}
        </p>
      </div>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        {!hasProfile && (
          <button
            className="btn btn-primary"
            onClick={onOpenWizard}
            data-cy="header-open-wizard"
          >
            Set up finance for me
          </button>
        )}
        <button
          className="btn btn-ghost btn-sm"
          onClick={() => onSwitchTab('templates')}
          data-cy="header-templates"
        >
          Use a starter
        </button>
        {hasProfile && (
          <button
            className="btn btn-ghost btn-sm"
            onClick={onRefresh}
            disabled={refreshing}
            title="Refresh finance data"
            data-cy="header-refresh"
          >
            {refreshing ? (
              <span className="spinner" style={{ width: 13, height: 13 }} />
            ) : (
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <polyline points="23 4 23 10 17 10" />
                <polyline points="1 20 1 14 7 14" />
                <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
              </svg>
            )}
          </button>
        )}
      </div>
    </div>
  );
}
