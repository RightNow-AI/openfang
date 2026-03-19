export const MARKET_DATA_PROVIDERS = [
  {
    id: 'yahoo_finance',
    label: 'Yahoo Finance research feed',
    hint: 'Good for broad watchlists and quick research.',
    requiresApiKey: false,
    official: false,
    note: 'Research-source adapter — treat as secondary, not sole source of truth.',
  },
  {
    id: 'alpha_vantage',
    label: 'Alpha Vantage',
    hint: 'Good for official API access and indicators.',
    requiresApiKey: true,
    official: true,
    docsUrl: 'https://www.alphavantage.co/documentation/',
    note: 'Official REST API with MCP server support. Free tier is rate-limited.',
  },
  {
    id: 'finnhub',
    label: 'Finnhub',
    hint: 'Good for research, filings, transcripts, and deeper updates.',
    requiresApiKey: true,
    official: true,
    docsUrl: 'https://finnhub.io/docs/api',
    note: 'Official REST API with fundamentals, filings, and earnings transcripts.',
  },
  {
    id: 'manual_csv',
    label: 'Manual CSV',
    hint: 'Good if you already have a file to upload.',
    requiresApiKey: false,
    official: false,
  },
  {
    id: 'other',
    label: 'Other',
    hint: 'Use this if you plan to add another source later.',
    requiresApiKey: false,
    official: false,
  },
];

export const POLLING_CADENCE_OPTIONS = [
  { value: 'manual', label: 'Manual only' },
  { value: 'daily', label: 'Daily' },
  { value: '4h', label: 'Every 4 hours' },
  { value: '1h', label: 'Hourly' },
];
