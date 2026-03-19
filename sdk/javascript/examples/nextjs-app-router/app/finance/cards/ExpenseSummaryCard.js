'use client';

import { fmtCurrency, topExpenseCategories } from '../lib/finance-ui';
import { EXPENSE_CATEGORY_LABELS } from '../lib/finance-copy';

export default function ExpenseSummaryCard({ expenseLines, onOpenDetail }) {
  const total = expenseLines.reduce((s, l) => s + (l.amount || 0), 0);
  const top = topExpenseCategories(expenseLines, 5);

  return (
    <div className="card" data-cy="expense-summary-card">
      <div className="card-header">
        <span>Expenses</span>
        <span style={{ fontFamily: 'var(--font-mono)', fontWeight: 700, color: 'var(--error)', fontSize: 15 }}>
          {fmtCurrency(total)}
        </span>
      </div>

      {expenseLines.length === 0 ? (
        <div style={{ fontSize: 13, color: 'var(--text-dim)', padding: '8px 0' }}>
          No expense lines connected yet.
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {top.map(([category, amount]) => {
            const pct = total > 0 ? (amount / total) * 100 : 0;
            return (
              <div key={category}>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 3 }}>
                  <span style={{ fontSize: 12, color: 'var(--text-secondary)' }}>
                    {EXPENSE_CATEGORY_LABELS[category] || category}
                  </span>
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text)', fontWeight: 600 }}>
                    {fmtCurrency(amount)}
                  </span>
                </div>
                <div style={{ height: 4, borderRadius: 2, background: 'var(--border)', overflow: 'hidden' }}>
                  <div
                    style={{
                      height: '100%',
                      width: `${Math.min(100, pct)}%`,
                      background: 'var(--error)',
                      borderRadius: 2,
                      opacity: 0.7,
                    }}
                  />
                </div>
              </div>
            );
          })}
          {expenseLines.length > 5 && (
            <div style={{ fontSize: 12, color: 'var(--text-muted)', paddingTop: 4 }}>
              +more categories
            </div>
          )}
        </div>
      )}

      <button
        className="btn btn-ghost btn-sm"
        onClick={onOpenDetail}
        style={{ marginTop: 14, width: '100%' }}
        data-cy="expenses-detail-btn"
      >
        View all expenses →
      </button>
    </div>
  );
}
