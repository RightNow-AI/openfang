/**
 * Market data adapter layer.
 *
 * All data calls go through a normalized adapter so business logic is not
 * tied to any single provider. Each adapter proxies to a Next.js API route
 * which handles the actual provider call server-side (keeping API keys safe).
 *
 * The normalized route is:
 *   GET /api/investments/quote?symbol=AAPL&provider=alpha_vantage
 *
 * Provider-specific adapters are listed below. Add new providers by
 * implementing the MarketDataAdapter shape and registering in ADAPTERS.
 */

// ── Base fetch helper ─────────────────────────────────────────────────────────

async function apiFetch(path) {
  const res = await fetch(path);
  if (!res.ok) {
    const err = new Error(`Market data fetch failed: ${res.status}`);
    err.status = res.status;
    throw err;
  }
  return res.json();
}

// ── Adapter factory ───────────────────────────────────────────────────────────

function makeAdapter(id) {
  return {
    id,
    fetchQuote: (symbol) => apiFetch(`/api/investments/quote?symbol=${encodeURIComponent(symbol)}&provider=${id}`),
    fetchNews: (symbol) => apiFetch(`/api/investments/news?symbol=${encodeURIComponent(symbol)}&provider=${id}`),
    fetchFundamentals: (symbol) => apiFetch(`/api/investments/fundamentals?symbol=${encodeURIComponent(symbol)}&provider=${id}`),
    fetchEarningsCalendar: (symbol) => apiFetch(`/api/investments/calendar?symbol=${encodeURIComponent(symbol)}&provider=${id}`),
  };
}

// ── Registered adapters ───────────────────────────────────────────────────────

export const ADAPTERS = {
  yahoo_finance: makeAdapter('yahoo_finance'),
  alpha_vantage: makeAdapter('alpha_vantage'),
  finnhub: makeAdapter('finnhub'),
  manual_csv: {
    id: 'manual_csv',
    fetchQuote: async () => null,
    fetchNews: async () => [],
    fetchFundamentals: async () => null,
    fetchEarningsCalendar: async () => [],
  },
};

// ── Normalized quote fetch ────────────────────────────────────────────────────

/**
 * Fetch a quote using the specified provider (or first available).
 * Falls back to the next provider in the list if one fails.
 */
export async function fetchQuoteWithFallback(symbol, providers = ['alpha_vantage', 'finnhub', 'yahoo_finance']) {
  for (const providerId of providers) {
    const adapter = ADAPTERS[providerId];
    if (!adapter) continue;
    try {
      const data = await adapter.fetchQuote(symbol);
      if (data) return { provider: providerId, data };
    } catch {
      // try next
    }
  }
  return null;
}
