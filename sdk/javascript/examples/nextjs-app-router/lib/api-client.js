// Browser-side OpenFang API client for Next.js Client Components.
// Reads NEXT_PUBLIC_OPENFANG_BASE_URL from env; falls back to http://127.0.0.1:50051.
//
// Uses localStorage to persist auth tokens (same key names as the Alpine app).

const BASE_URL =
  typeof window !== 'undefined'
    ? (process.env.NEXT_PUBLIC_OPENFANG_BASE_URL || 'http://127.0.0.1:50051')
    : 'http://127.0.0.1:50051';

function getToken() {
  if (typeof localStorage === 'undefined') return '';
  return localStorage.getItem('openfang-auth-token') || localStorage.getItem('openfang-api-key') || '';
}

function buildHeaders() {
  const h = { 'Content-Type': 'application/json' };
  const t = getToken();
  if (t) h['Authorization'] = `Bearer ${t}`;
  return h;
}

async function request(method, path, body) {
  const opts = { method, headers: buildHeaders() };
  if (body !== undefined) opts.body = JSON.stringify(body);
  let res;
  try {
    res = await fetch(`${BASE_URL}${path}`, opts);
  } catch (networkErr) {
    // TypeError: Failed to fetch means CORS preflight failed, daemon is down,
    // or the requested origin is not in the allowed list.
    const hint = networkErr instanceof TypeError
      ? `Network error — ensure the OpenFang daemon is running on ${BASE_URL} and CORS allows this origin`
      : networkErr.message;
    const err = new Error(hint);
    err.status = 0;
    throw err;
  }
  if (!res.ok) {
    if (res.status === 401) {
      // Clear stale token and notify listeners
      localStorage.removeItem('openfang-auth-token');
      localStorage.removeItem('openfang-api-key');
      document.dispatchEvent(new CustomEvent('openfang:auth-required'));
    }
    const text = await res.text().catch(() => res.statusText);
    let msg;
    try {
      const parsed = JSON.parse(text);
      msg = parsed.error || parsed.message || res.statusText;
    } catch { msg = text || res.statusText; }
    const err = new Error(msg || `HTTP ${res.status}`);
    err.status = res.status;
    throw err;
  }
  const ct = res.headers.get('content-type') || '';
  if (ct.includes('application/json')) return res.json();
  const t = await res.text();
  try { return JSON.parse(t); } catch { return { text: t }; }
}

export const apiClient = {
  baseUrl: BASE_URL,
  getToken,
  setToken(token) {
    if (token) {
      localStorage.setItem('openfang-auth-token', token);
    } else {
      localStorage.removeItem('openfang-auth-token');
      localStorage.removeItem('openfang-api-key');
    }
  },
  get: (path) => request('GET', path),
  post: (path, body) => request('POST', path, body),
  put: (path, body) => request('PUT', path, body),
  patch: (path, body) => request('PATCH', path, body),
  del: (path, body) => request('DELETE', path, body),
};
