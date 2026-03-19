'use client';

import InvestmentStarterCard from './cards/InvestmentStarterCard';
import { INVESTMENT_STARTERS } from './config/investment-starters';

export default function RecommendedInvestmentsTab({ applyingTemplateId, onApplyTemplate, onOpenWizard }) {
  return (
    <div>
      <div style={{ marginBottom: 20 }}>
        <h2 style={{ fontSize: 16, fontWeight: 800, color: 'var(--text)', marginBottom: 4 }}>
          Start the easy way
        </h2>
        <p style={{ fontSize: 12, color: 'var(--text-dim)' }}>
          Choose a simple setup to start tracking markets, ideas, and risks.
        </p>
      </div>

      <div className="grid grid-3" style={{ gap: 14, marginBottom: 28 }}>
        {(INVESTMENT_STARTERS || []).map((tmpl) => (
          <InvestmentStarterCard
            key={tmpl.id}
            template={tmpl}
            applying={applyingTemplateId === tmpl.id}
            onApply={() => onApplyTemplate(tmpl.id)}
          />
        ))}
      </div>

      <div
        className="card"
        style={{
          textAlign: 'center',
          padding: '28px 20px',
          border: '2px dashed var(--border)',
          background: 'transparent',
        }}
      >
        <div style={{ fontSize: 14, fontWeight: 700, color: 'var(--text)', marginBottom: 6 }}>
          Want something more specific?
        </div>
        <p style={{ fontSize: 12, color: 'var(--text-dim)', marginBottom: 14 }}>
          Use the setup wizard to choose exactly what you want to track, what signals matter, and how much approval you want involved.
        </p>
        <button className="btn btn-primary" onClick={onOpenWizard}>
          Set up investment research for me
        </button>
      </div>
    </div>
  );
}
