'use client';

import { useState, useEffect } from 'react';
import FinanceKpiStrip from './FinanceKpiStrip';
import RevenueSummaryCard from './cards/RevenueSummaryCard';
import ExpenseSummaryCard from './cards/ExpenseSummaryCard';
import CashFlowCard from './cards/CashFlowCard';
import MarginByModeCard from './cards/MarginByModeCard';
import ApprovalQueueCard from './cards/ApprovalQueueCard';
import RiskAlertsCard from './cards/RiskAlertsCard';
import ServerApiCostCard from './cards/ServerApiCostCard';
import SalesRevenueCard from './cards/SalesRevenueCard';
import InvestmentIntelligenceCard from './cards/InvestmentIntelligenceCard';
import MarketRiskCard from './cards/MarketRiskCard';
import PortfolioExposureCard from './cards/PortfolioExposureCard';

export default function FinanceOverviewTab({ summary, view, onOpenDetail }) {
  const [investmentSummary, setInvestmentSummary] = useState(null);
  const detailed = view === 'detailed' || view === 'advanced';
  const investmentLoading = detailed && investmentSummary === null;

  useEffect(() => {
    if (!detailed) return;
    let cancelled = false;
    fetch('/api/investments/finance-summary')
      .then((r) => r.json())
      .then((d) => { if (!cancelled) setInvestmentSummary(d); })
      .catch(() => { if (!cancelled) setInvestmentSummary({}); });
    return () => { cancelled = true; };
  }, [detailed]);

  return (
    <div data-cy="tab-overview">
      <FinanceKpiStrip summary={summary} />

      <div className="grid grid-2" style={{ gap: 14, marginTop: 20 }}>
        <CashFlowCard kpis={summary?.kpis} onOpenDetail={() => onOpenDetail('cash_flow')} />
        <ApprovalQueueCard
          approvalsWaiting={summary?.approvals_waiting ?? 0}
          onOpenDetail={() => onOpenDetail('approvals')}
        />
        <RevenueSummaryCard
          revenueLines={summary?.revenue_lines ?? []}
          onOpenDetail={() => onOpenDetail('revenue')}
        />
        <ExpenseSummaryCard
          expenseLines={summary?.expense_lines ?? []}
          onOpenDetail={() => onOpenDetail('expenses')}
        />
        {detailed && (
          <>
            <MarginByModeCard summary={summary} onOpenDetail={() => onOpenDetail('margins')} />
            <RiskAlertsCard
              risks={summary?.risks ?? []}
              onOpenDetail={() => onOpenDetail('risks')}
            />
            <ServerApiCostCard
              serverCostMonthly={summary?.server_cost_monthly ?? 0}
              apiCostMonthly={summary?.api_cost_monthly ?? 0}
              onOpenDetail={() => onOpenDetail('server_api_costs')}
            />
            <SalesRevenueCard
              revenueLines={summary?.revenue_lines ?? []}
              onOpenDetail={() => onOpenDetail('sales_revenue')}
            />
            <InvestmentIntelligenceCard
              summary={investmentSummary}
              loading={investmentLoading}
            />
            <MarketRiskCard summary={investmentSummary} />
            <PortfolioExposureCard summary={investmentSummary} />
          </>
        )}
      </div>
    </div>
  );
}
