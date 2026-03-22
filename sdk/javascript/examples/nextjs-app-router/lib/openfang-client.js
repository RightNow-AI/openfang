/**
 * lib/openfang-client.js
 *
 * Thin adapter over the OpenFang daemon's REST API.
 * All direct HTTP contact with the daemon goes through here.
 *
 * Current daemon contract:
 *   GET  /api/agents                       → AgentDescriptor[]
 *   POST /api/agents/:agentId/message      → { response, input_tokens, output_tokens, iterations, cost_usd }
 *   POST /api/agents/:agentId/message/stream → SSE events
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

/**
 * POST SSE stream with per-read timeout.
 *
 * @param {string} path
 * @param {unknown} body
 * @param {(event: { event: string, data: any }) => Promise<void> | void} onEvent
 * @param {{ signal?: AbortSignal }} [options]
 * @param {number} [timeoutMs]
 * @returns {Promise<void>}
 */
async function postSSE(path, body, onEvent, options = {}, timeoutMs = TIMEOUT_MS) {
  const controller = new AbortController();
  const externalSignal = options.signal;
  let detachExternalAbort = null;

  if (externalSignal) {
    if (externalSignal.aborted) {
      controller.abort(externalSignal.reason);
    } else {
      const abortFromExternal = () => controller.abort(externalSignal.reason);
      externalSignal.addEventListener('abort', abortFromExternal, { once: true });
      detachExternalAbort = () => externalSignal.removeEventListener('abort', abortFromExternal);
    }
  }
  try {
    const res = await fetch(`${BASE_URL}${path}`, {
      method: 'POST',
      headers: {
        ...authHeaders(),
        Accept: 'text/event-stream',
      },
      body: JSON.stringify(body),
      signal: controller.signal,
      cache: 'no-store',
    });

    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw Object.assign(new Error(`Daemon POST ${path} failed: ${res.status} ${text}`), { status: res.status });
    }

    if (!res.body) {
      throw new Error(`Daemon POST ${path} returned no stream body`);
    }

    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    async function readWithTimeout() {
      let timer;
      try {
        return await Promise.race([
          reader.read(),
          new Promise((_, reject) => {
            timer = setTimeout(() => {
              controller.abort();
              reject(new Error(`Daemon POST ${path} stream timed out after ${timeoutMs}ms`));
            }, timeoutMs);
          }),
        ]);
      } finally {
        clearTimeout(timer);
      }
    }

    try {
      while (true) {
        const { done, value } = await readWithTimeout();
        if (done) {
          break;
        }

        buffer += decoder.decode(value, { stream: true });
        const messages = buffer.split('\n\n');
        buffer = messages.pop() || '';

        for (const message of messages) {
          const lines = message.split('\n');
          let eventName = 'message';
          const dataLines = [];

          for (const rawLine of lines) {
            const line = rawLine.trimEnd();
            if (!line || line.startsWith(':')) {
              continue;
            }
            if (line.startsWith('event:')) {
              eventName = line.slice(6).trim();
              continue;
            }
            if (line.startsWith('data:')) {
              dataLines.push(line.slice(5).trimStart());
            }
          }

          if (dataLines.length === 0) {
            continue;
          }

          const rawData = dataLines.join('\n');
          let data;
          try {
            data = JSON.parse(rawData);
          } catch {
            data = { raw: rawData };
          }

          await onEvent({ event: eventName, data });
        }
      }
    } finally {
      try {
        await reader.cancel();
      } catch {}
      reader.releaseLock();
    }
  } catch (err) {
    if (err?.name === 'AbortError') {
      if (externalSignal?.aborted) {
        const reason = externalSignal.reason;
        throw reason instanceof Error ? reason : new Error(String(reason || `Daemon POST ${path} stream aborted`));
      }
      throw new Error(`Daemon POST ${path} stream timed out after ${timeoutMs}ms`);
    }
    throw err;
  } finally {
    detachExternalAbort?.();
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

  /**
   * Spawn an agent from a TOML manifest.
   *
   * @param {string} manifestToml
   * @returns {Promise<{ agent_id?: string, name?: string }>}
   */
  async spawnAgentFromManifest(manifestToml) {
    return /** @type {any} */ (await postJSON('/api/agents', { manifest_toml: manifestToml }));
  },

  /**
   * Send a message to a specific agent and consume its SSE stream.
   * Timeout is applied per read, not across the full run, so long research jobs can continue.
   *
   * @param {string} agentId
   * @param {string} message
   * @param {(event: { event: string, data: any }) => Promise<void> | void} onEvent
   * @param {{ signal?: AbortSignal }} [options]
   * @returns {Promise<void>}
   */
  async streamMessage(agentId, message, onEvent, options) {
    const body = { message };
    await postSSE(`/api/agents/${agentId}/message/stream`, body, onEvent, options);
  },
};

module.exports = { openfangClient };
