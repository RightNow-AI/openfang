import { FINANCE_STARTERS } from '../config/finance-starters';
import {
  BUSINESS_MODE_LABELS,
  GOAL_LABELS,
  APPROVAL_RULE_LABELS,
  FIRST_HELP_LABELS,
  TRACKER_LABELS,
} from './finance-copy';

// ── Payload builder ───────────────────────────────────────────────────────────

export function buildFinancePayloadFromWizard(state) {
  return {
    business_name: state.businessName || '',
    business_mode: state.businessMode,
    main_finance_goal: state.mainGoal,
    monthly_revenue_estimate: state.monthlyRevenue,
    monthly_expense_estimate: state.monthlyExpenses,
    cash_on_hand: state.cashOnHand,
    tracks_invoices: state.tracksInvoices,
    tracks_payroll: state.tracksPayroll,
    tracks_subscriptions: state.tracksSubscriptions,
    tracks_ad_spend: state.tracksAdSpend,
    tracks_server_costs: state.tracksServerCosts,
    tracks_api_costs: state.tracksApiCosts,
    approvers: [],
    approval_rules: state.approvalRules,
    first_help: state.firstHelp,
  };
}

// ── Review summary ────────────────────────────────────────────────────────────

export function getFinanceReviewSummary(state) {
  const enabledTrackers = [
    state.tracksInvoices && 'invoices',
    state.tracksPayroll && 'payroll',
    state.tracksSubscriptions && 'subscriptions',
    state.tracksAdSpend && 'ad_spend',
    state.tracksServerCosts && 'server_costs',
    state.tracksApiCosts && 'api_costs',
  ].filter(Boolean);

  return {
    businessModeLabel: BUSINESS_MODE_LABELS[state.businessMode] || state.businessMode || '—',
    goalLabel: GOAL_LABELS[state.mainGoal] || state.mainGoal || '—',
    monthlyRevenue: state.monthlyRevenue,
    monthlyExpenses: state.monthlyExpenses,
    trackersEnabled: enabledTrackers.map((t) => TRACKER_LABELS[t] || t),
    approvalLabels: state.approvalRules.map((r) => APPROVAL_RULE_LABELS[r] || r),
    firstHelpLabel: FIRST_HELP_LABELS[state.firstHelp] || state.firstHelp || '—',
  };
}

// ── Template recommendation ───────────────────────────────────────────────────

export function recommendFinanceTemplates(mode) {
  return FINANCE_STARTERS.filter((t) => t.category === mode || t.category === 'general');
}

// ── Line mappers ──────────────────────────────────────────────────────────────

export function mapRevenueLinesByMode(lines) {
  const result = { agency: 0, growth: 0, school: 0, other: 0 };
  for (const line of lines) {
    const page = line.source_page || 'other';
    if (page in result) result[page] += line.amount;
    else result.other += line.amount;
  }
  return result;
}

export function mapExpenseLinesByCategory(lines) {
  const result = {};
  for (const line of lines) {
    result[line.category] = (result[line.category] || 0) + line.amount;
  }
  return result;
}

// ── Formatters ────────────────────────────────────────────────────────────────

export function fmtCurrency(n, compact = false) {
  if (n == null || Number.isNaN(n)) return '—';
  const num = Number(n);
  if (compact && Math.abs(num) >= 1000) {
    return '$' + (num / 1000).toFixed(1) + 'k';
  }
  return num.toLocaleString('en-US', { style: 'currency', currency: 'USD', maximumFractionDigits: 0 });
}

export function fmtPercent(n, decimals = 1) {
  if (n == null || Number.isNaN(n)) return '—';
  return Number(n).toFixed(decimals) + '%';
}

export function fmtNumber(n) {
  if (n == null || Number.isNaN(n)) return '—';
  return Number(n).toLocaleString('en-US');
}

// ── Risk colors ───────────────────────────────────────────────────────────────

export function riskSeverityColor(severity) {
  if (severity === 'high') return 'var(--error)';
  if (severity === 'medium') return 'var(--warning)';
  return 'var(--text-dim)';
}

export function riskSeverityBadgeClass(severity) {
  if (severity === 'high') return 'badge badge-error';
  if (severity === 'medium') return 'badge badge-warn';
  return 'badge badge-dim';
}

// ── Category ordering ─────────────────────────────────────────────────────────

export function topExpenseCategories(lines, topN = 5) {
  const map = mapExpenseLinesByCategory(lines);
  return Object.entries(map)
    .sort((a, b) => b[1] - a[1])
    .slice(0, topN);
}

export function topRevenueLines(lines, topN = 5) {
  return [...lines].sort((a, b) => b.amount - a.amount).slice(0, topN);
}

// ── Empty summary ─────────────────────────────────────────────────────────────

export function emptyFinanceSummary() {
  return {
    profile: null,
    kpis: {
      cash_on_hand: 0,
      monthly_revenue: 0,
      monthly_expenses: 0,
      net_profit: 0,
      runway_months: null,
      overdue_invoices_count: 0,
      average_invoice_age_days: null,
      ad_spend_monthly: 0,
      server_cost_monthly: 0,
      api_cost_monthly: 0,
      margin_percent: null,
    },
    revenue_lines: [],
    expense_lines: [],
    risks: [],
    approvals_waiting: 0,
  };
}
