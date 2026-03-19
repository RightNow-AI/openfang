'use client';

import { FINANCE_STARTERS } from './config/finance-starters';
import FinanceStarterCard from './cards/FinanceStarterCard';

export default function RecommendedFinanceTab({ onApplyTemplate, onOpenWizard, applyingTemplateId }) {
  const featured = FINANCE_STARTERS.slice(0, 3);

  return (
    <div data-cy="tab-recommended">
      <div style={{ marginBottom: 28 }}>
        <div style={{ fontSize: 13, fontWeight: 700, color: 'var(--text-secondary)', marginBottom: 14, textTransform: 'uppercase', letterSpacing: '0.06em' }}>
          Quick starters
        </div>
        <div className="grid grid-3" style={{ gap: 14 }}>
          {featured.map((t) => (
            <FinanceStarterCard
              key={t.id}
              template={t}
              applying={applyingTemplateId === t.id}
              onApply={onApplyTemplate}
            />
          ))}
        </div>
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '18px 20px',
          borderRadius: 12,
          border: '1.5px dashed var(--border)',
          background: 'var(--bg-elevated)',
          gap: 20,
        }}
      >
        <div>
          <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 4 }}>
            Want a custom setup?
          </div>
          <div style={{ fontSize: 13, color: 'var(--text-dim)' }}>
            Tell us about your business and we&apos;ll configure the right finance layer for you.
          </div>
        </div>
        <button
          className="btn btn-primary"
          onClick={onOpenWizard}
          style={{ flexShrink: 0 }}
          data-cy="open-wizard-recommended"
        >
          Set up for me →
        </button>
      </div>
    </div>
  );
}
