// Server-side OpenFang API fetcher for Next.js Server Components and Route Handlers.
// Uses the daemon base URL directly (server → server, no browser involved).
//
// For client-side pages use lib/api-client.js instead.

const { env } = require('./env');

function withTimeout(promise, ms) {
  return Promise.race([
    promise,
    new Promise((_, reject) =>
      setTimeout(() => reject(new Error(`Request timed out after ${ms}ms`)), ms)
    ),
  ]);
}

function buildHeaders(apiKey) {
  const headers = { 'Content-Type': 'application/json' };
  const key = apiKey || env.OPENFANG_API_KEY;
  if (key) headers['Authorization'] = `Bearer ${key}`;
  return headers;
}

async function fetchJSON(path, options = {}, apiKey) {
  const url = `${env.OPENFANG_BASE_URL}${path}`;
  const res = await withTimeout(
    fetch(url, { ...options, headers: { ...buildHeaders(apiKey), ...(options.headers || {}) } }),
    env.OPENFANG_TIMEOUT_MS
  );
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText);
    let msg;
    try { msg = JSON.parse(text).error || res.statusText; } catch { msg = res.statusText; }
    const err = new Error(msg || `HTTP ${res.status}`);
    err.status = res.status;
    throw err;
  }
  const ct = res.headers.get('content-type') || '';
  if (ct.includes('application/json')) return res.json();
  const t = await res.text();
  try { return JSON.parse(t); } catch { return { text: t }; }
}

const api = {
  get: (path, apiKey) => fetchJSON(path, { method: 'GET' }, apiKey),
  post: (path, body, apiKey) => fetchJSON(path, { method: 'POST', body: JSON.stringify(body) }, apiKey),
  put: (path, body, apiKey) => fetchJSON(path, { method: 'PUT', body: JSON.stringify(body) }, apiKey),
  patch: (path, body, apiKey) => fetchJSON(path, { method: 'PATCH', body: JSON.stringify(body) }, apiKey),
  del: (path, apiKey) => fetchJSON(path, { method: 'DELETE' }, apiKey),

  // Convenience: fetch multiple endpoints in parallel, returning null for failed ones
  async gather(paths, apiKey) {
    const results = await Promise.allSettled(paths.map(p => api.get(p, apiKey)));
    return results.map(r => (r.status === 'fulfilled' ? r.value : null));
  },
};

module.exports = { api };
