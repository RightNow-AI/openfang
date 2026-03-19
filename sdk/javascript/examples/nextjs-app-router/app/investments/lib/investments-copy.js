// ── Scope / Asset class ───────────────────────────────────────────────────────

export const WATCH_SCOPE_LABELS = {
  stocks: 'Stocks',
  etfs: 'ETFs',
  crypto: 'Crypto',
  sectors: 'Sectors',
  themes: 'Themes',
  mixed: 'A mix of these',
};

export const WATCH_SCOPE_DESCRIPTIONS = {
  stocks: 'Individual company shares',
  etfs: 'Funds that track indexes, sectors, or strategies',
  crypto: 'Digital assets and tokens',
  sectors: 'Broad market sectors like tech, energy, or healthcare',
  themes: 'Investment themes like AI, clean energy, or emerging markets',
  mixed: 'A combination of different types',
};

// ── Time horizon ──────────────────────────────────────────────────────────────

export const TIME_HORIZON_LABELS = {
  short: 'Short term',
  medium: 'Medium term',
  long: 'Long term',
};

export const TIME_HORIZON_DESCRIPTIONS = {
  short: 'Days to a few months',
  medium: 'Several months to a year',
  long: 'One year or more',
};

// ── Risk comfort ──────────────────────────────────────────────────────────────

export const RISK_COMFORT_LABELS = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
};

export const RISK_COMFORT_DESCRIPTIONS = {
  low: 'I prefer stable, lower-volatility ideas',
  medium: 'I can handle normal market swings',
  high: 'I am comfortable with high volatility',
};

// ── Signals ───────────────────────────────────────────────────────────────────

export const SIGNAL_LABELS = {
  news: 'News',
  earnings: 'Earnings',
  filings: 'Filings',
  price: 'Price moves',
  volume: 'Volume',
  macro: 'Macro trends',
  sector: 'Sector changes',
};

export const SIGNAL_DESCRIPTIONS = {
  news: 'Watch major updates and headlines.',
  earnings: 'Watch earnings dates, results, and related reactions.',
  filings: 'Watch company filings and major changes.',
  price: 'Watch meaningful price changes.',
  volume: 'Watch unusual trading volume.',
  macro: 'Watch rates, inflation, or larger market signals.',
  sector: 'Watch strength and weakness across sectors.',
};

// ── Patterns ──────────────────────────────────────────────────────────────────

export const PATTERN_LABELS = {
  momentum: 'Momentum',
  mean_reversion: 'Mean reversion',
  earnings_reaction: 'Earnings reaction',
  breakout: 'Breakout',
  sector_rotation: 'Sector rotation',
  valuation_band: 'Valuation band',
  unusual_volume: 'Unusual volume',
};

export const PATTERN_DESCRIPTIONS = {
  momentum: 'Watch for strong trends that keep moving.',
  mean_reversion: 'Watch for moves that stretch too far and may snap back.',
  earnings_reaction: 'Watch how price reacts around earnings.',
  breakout: 'Watch for moves through important levels.',
  sector_rotation: 'Watch money moving from one sector to another.',
  valuation_band: 'Watch for price moving outside a normal valuation range.',
  unusual_volume: 'Watch for volume spikes that may signal interest or stress.',
};

// ── Approval rules ────────────────────────────────────────────────────────────

export const APPROVAL_RULE_LABELS = {
  before_trade_proposal: 'Before trade proposals',
  before_reallocation: 'Before reallocation',
  before_sell_signal: 'Before sell signals',
  before_external_execution: 'Before external execution',
  before_client_alert_send: 'Before sending client alerts',
};

export const APPROVAL_RULE_DESCRIPTIONS = {
  before_trade_proposal: 'Pause before a new idea becomes an action proposal.',
  before_reallocation: 'Pause before portfolio weights change.',
  before_sell_signal: 'Pause before reduce or exit suggestions are finalized.',
  before_external_execution: 'Pause before anything touches a broker or outside system.',
  before_client_alert_send: 'Pause before investment updates go to clients or subscribers.',
};

// ── Thesis status ─────────────────────────────────────────────────────────────

export const THESIS_STATUS_LABELS = {
  intact: 'Intact',
  weakened: 'Weakened',
  broken: 'Broken',
};

// ── Watchlist status ──────────────────────────────────────────────────────────

export const WATCHLIST_STATUS_LABELS = {
  watching: 'Watching',
  candidate: 'Candidate',
  hold: 'Hold',
  reduce: 'Reduce',
  archived: 'Archived',
};

// ── Alert types ───────────────────────────────────────────────────────────────

export const ALERT_TYPE_LABELS = {
  pattern_detected: 'Pattern detected',
  risk_limit: 'Risk limit',
  earnings_soon: 'Earnings soon',
  thesis_broken: 'Thesis broken',
  allocation_breach: 'Allocation breach',
  api_budget_breach: 'API budget breach',
};

// ── Approval status ───────────────────────────────────────────────────────────

export const APPROVAL_STATUS_LABELS = {
  none: 'No approval needed',
  pending: 'Waiting for approval',
  approved: 'Approved',
  rejected: 'Rejected',
};

// ── Severity ──────────────────────────────────────────────────────────────────

export const SEVERITY_LABELS = {
  low: 'Low',
  medium: 'Medium',
  high: 'High',
};

// ── Trackers ──────────────────────────────────────────────────────────────────

export const TRACKER_LABELS = {
  news: 'News',
  filings: 'Filings',
  price: 'Price',
  volume: 'Volume',
  patterns: 'Patterns',
  risk: 'Risk',
  thesis: 'Thesis',
  calendar: 'Calendar',
};

// ── Asset class ───────────────────────────────────────────────────────────────

export const ASSET_CLASS_LABELS = {
  stock: 'Stock',
  etf: 'ETF',
  crypto: 'Crypto',
  sector: 'Sector',
  theme: 'Theme',
  macro: 'Macro',
};

// ── Impact ────────────────────────────────────────────────────────────────────

export const IMPACT_LABELS = {
  low: 'Low impact',
  medium: 'Medium impact',
  high: 'High impact',
};

// ── Horizon ───────────────────────────────────────────────────────────────────

export const HORIZON_LABELS = {
  swing: 'Swing',
  position: 'Position',
  long_term: 'Long term',
};
