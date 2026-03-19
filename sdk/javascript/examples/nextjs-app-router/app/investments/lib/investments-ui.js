import {
  WATCH_SCOPE_LABELS,
  TIME_HORIZON_LABELS,
  RISK_COMFORT_LABELS,
  SIGNAL_LABELS,
  PATTERN_LABELS,
  APPROVAL_RULE_LABELS,
} from './investments-copy';
import { MARKET_DATA_PROVIDERS } from '../config/market-data-providers';

// ── Payload builder ───────────────────────────────────────────────────────────

export function buildInvestmentsPayload(state) {
  return {
    watch_scope: state.watchScope,
    symbols: state.symbols,
    time_horizon: state.timeHorizon,
    risk_comfort: state.riskComfort,
    enabled_signals: state.signals,
    enabled_patterns: state.patterns,
    approval_rules: state.approvalRules,
    providers: state.providers,
    input_method: state.inputMethod || 'manual',
  };
}

// ── Review summary builder ────────────────────────────────────────────────────

export function getInvestmentsReviewSummary(state) {
  return {
    scopeLabel: WATCH_SCOPE_LABELS[state.watchScope] || state.watchScope || '—',
    symbolsLabel: state.symbols.length > 0 ? state.symbols.join(', ') : 'None added',
    horizonLabel: TIME_HORIZON_LABELS[state.timeHorizon] || '—',
    riskLabel: RISK_COMFORT_LABELS[state.riskComfort] || '—',
    signalLabels: state.signals.map((s) => SIGNAL_LABELS[s] || s),
    patternLabels: state.patterns.map((p) => PATTERN_LABELS[p] || p),
    approvalLabels: state.approvalRules.map((r) => APPROVAL_RULE_LABELS[r] || r),
    providerLabels: state.providers.map((id) => {
      const p = MARKET_DATA_PROVIDERS.find((x) => x.id === id);
      return p ? p.label : id;
    }),
  };
}

// ── Formatters ────────────────────────────────────────────────────────────────

export function fmtPrice(n) {
  if (n == null || isNaN(n)) return '—';
  return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD', minimumFractionDigits: 2 }).format(n);
}

export function fmtPercent(n) {
  if (n == null || isNaN(n)) return '—';
  const sign = n >= 0 ? '+' : '';
  return `${sign}${(n * 100).toFixed(2)}%`;
}

export function fmtPctRaw(n) {
  if (n == null || isNaN(n)) return '—';
  const sign = n >= 0 ? '+' : '';
  return `${sign}${n.toFixed(2)}%`;
}

export function fmtCompact(n) {
  if (n == null || isNaN(n)) return '—';
  if (Math.abs(n) >= 1_000_000_000) return `$${(n / 1_000_000_000).toFixed(1)}B`;
  if (Math.abs(n) >= 1_000_000) return `$${(n / 1_000_000).toFixed(1)}M`;
  if (Math.abs(n) >= 1_000) return `$${(n / 1_000).toFixed(1)}K`;
  return `$${n.toFixed(0)}`;
}

export function changeColor(n) {
  if (n == null) return 'var(--text-dim)';
  return n >= 0 ? 'var(--color-success, #22c55e)' : 'var(--color-error, #ef4444)';
}

export function severityColor(severity) {
  if (severity === 'high') return 'var(--color-error, #ef4444)';
  if (severity === 'medium') return 'var(--text-warn, #e5a00d)';
  return 'var(--text-dim)';
}

export function severityBadgeClass(severity) {
  if (severity === 'high') return 'badge badge-error';
  if (severity === 'medium') return 'badge badge-warn';
  return 'badge badge-dim';
}

export function thesisStatusColor(status) {
  if (status === 'intact') return 'var(--color-success, #22c55e)';
  if (status === 'weakened') return 'var(--text-warn, #e5a00d)';
  if (status === 'broken') return 'var(--color-error, #ef4444)';
  return 'var(--text-dim)';
}

export function impactBadgeClass(impact) {
  if (impact === 'high') return 'badge badge-error';
  if (impact === 'medium') return 'badge badge-warn';
  return 'badge badge-dim';
}

export function approvalStatusBadgeClass(status) {
  if (status === 'approved') return 'badge badge-success';
  if (status === 'rejected') return 'badge badge-error';
  if (status === 'pending') return 'badge badge-warn';
  return 'badge badge-dim';
}

// ── Empty/default data ────────────────────────────────────────────────────────

export function emptyInvestmentsSummary() {
  return {
    watchlist: [],
    research: [],
    theses: [],
    portfolio: [],
    alerts: [],
    finance_summary: {
      watchlist_count: 0,
      high_severity_alerts: 0,
      portfolio_value: null,
      unrealized_pnl_percent: null,
      concentration_risk_flag: false,
      approvals_waiting: 0,
    },
  };
}

// ── Watchlist helpers ─────────────────────────────────────────────────────────

export function sortWatchlistByStatus(items) {
  const ORDER = { candidate: 0, watching: 1, hold: 2, reduce: 3, archived: 4 };
  return [...items].sort((a, b) => (ORDER[a.status] ?? 9) - (ORDER[b.status] ?? 9));
}

export function sortAlertsBySeverity(alerts) {
  const ORDER = { high: 0, medium: 1, low: 2 };
  return [...alerts].sort((a, b) => (ORDER[a.severity] ?? 9) - (ORDER[b.severity] ?? 9));
}

export function pendingApprovalAlerts(alerts) {
  return alerts.filter((a) => a.approval_required && a.approval_status === 'pending');
}

export function highSeverityAlerts(alerts) {
  return alerts.filter((a) => a.severity === 'high');
}
