'use client';

import { FINANCE_STARTERS } from './config/finance-starters';
import { STARTER_CATEGORY_LABELS } from './config/finance-starters';
import FinanceStarterCard from './cards/FinanceStarterCard';

const CATEGORIES = [...new Set(FINANCE_STARTERS.map((t) => t.category))];

export default function FinanceTemplatesTab({ onApplyTemplate, applyingTemplateId }) {
  return (
    <div data-cy="tab-templates">
      {CATEGORIES.map((cat) => {
        const items = FINANCE_STARTERS.filter((t) => t.category === cat);
        return (
          <div key={cat} style={{ marginBottom: 28 }}>
            <div style={{
              fontSize: 12,
              fontWeight: 700,
              color: 'var(--text-secondary)',
              textTransform: 'uppercase',
              letterSpacing: '0.06em',
              marginBottom: 12,
            }}>
              {STARTER_CATEGORY_LABELS[cat] || cat}
            </div>
            <div className="grid grid-3" style={{ gap: 14 }}>
              {items.map((t) => (
                <FinanceStarterCard
                  key={t.id}
                  template={t}
                  applying={applyingTemplateId === t.id}
                  onApply={onApplyTemplate}
                />
              ))}
            </div>
          </div>
        );
      })}
    </div>
  );
}
