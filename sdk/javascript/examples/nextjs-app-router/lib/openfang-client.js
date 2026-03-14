/**
 * lib/openfang-client.js
 *
 * Thin adapter over the OpenFang daemon's REST API.
 * All direct HTTP contact with the daemon goes through here.
 *
 * Current daemon contract (synchronous request/response):
 *   GET  /api/agents                       → AgentDescriptor[]
 *   POST /api/agents/:agentId/message      → { response, input_tokens, output_tokens, iterations, cost_usd }
 *
 * When the daemon gains a streaming runs API, only this file needs to change.
 */

'use strict';

const { env } = require('./env');

const BASE_URL = env.OPENFANG_BASE_URL;
const TIMEOUT_MS = env.OPENFANG_TIMEOUT_MS;

function authHeaders() {
  const headers = { 'Content-Type': 'application/json' };
  if (env.OPENFANG_API_KEY) headers['Authorization'] = `Bearer ${env.OPENFANG_API_KEY}`;
  return headers;
}

/**
 * GET with timeout.
 * @param {string} path
 * @returns {Promise<unknown>}
 */
async function getJSON(path) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const res = await fetch(`${BASE_URL}${path}`, {
      method: 'GET',
      headers: authHeaders(),
      signal: controller.signal,
      cache: 'no-store',
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw Object.assign(new Error(`Daemon GET ${path} failed: ${res.status} ${text}`), { status: res.status });
    }
    return res.json();
  } catch (err) {
    if (err.name === 'AbortError') throw new Error(`Daemon GET ${path} timed out after ${TIMEOUT_MS}ms`);
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

/**
 * POST with timeout.
 * @param {string} path
 * @param {unknown} body
 * @param {number} [timeoutMs]
 * @returns {Promise<unknown>}
 */
async function postJSON(path, body, timeoutMs = TIMEOUT_MS) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(body),
      signal: controller.signal,
      cache: 'no-store',
    });
    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw Object.assign(new Error(`Daemon POST ${path} failed: ${res.status} ${text}`), { status: res.status });
    }
    return res.json();
  } catch (err) {
    if (err.name === 'AbortError') throw new Error(`Daemon POST ${path} timed out after ${timeoutMs}ms`);
    throw err;
  } finally {
    clearTimeout(timer);
  }
}

const openfangClient = {
  /**
   * List all registered agents.
   * @returns {Promise<unknown[]>}
   */
  async listAgents() {
    const raw = await getJSON('/api/agents');
    return Array.isArray(raw) ? raw : raw?.agents ?? [];
  },

  /**
   * Send a message to a specific agent and return its response.
   * This is the primary execution call.
   *
   * @param {string} agentId
   * @param {string} message
   * @param {string} [sessionId]
   * @returns {Promise<{ response: string, input_tokens: number, output_tokens: number, iterations: number, cost_usd: number }>}
   */
  async sendMessage(agentId, message, sessionId) {
    const body = { message };
    if (sessionId) body.session_id = sessionId;
    return /** @type {any} */ (await postJSON(`/api/agents/${agentId}/message`, body));
  },
};

module.exports = { openfangClient };
