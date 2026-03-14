/**
 * lib/task-runner/openfang-agent-runner.js
 *
 * OpenFangAgentRunner — TaskRunner backend that routes AI jobs through the
 * OpenFang daemon's agent message API instead of worker_threads.
 *
 * Daemon API contract (current):
 *   POST /api/agents/:agentId/message
 *   Body: { message: string }
 *   Response: { response, input_tokens, output_tokens, iterations, cost_usd }
 *
 * This is a synchronous request/response API (no SSE streaming).
 * The runner wraps it in the TaskEvent protocol so callers see a uniform interface.
 * If the daemon gains a streaming runs API later, swap streamEvents() below.
 *
 * Usage rule:
 *   AI reasoning / multi-step subagents -> OpenFangAgentRunner
 *   CPU-heavy local execution           -> worker_threads (see heartbeat.js)
 *
 * @typedef {import('./types.js')} Types   (for JSDoc only — no runtime import needed)
 */

'use strict';

const http = require('http');
const crypto = require('crypto');

const DEFAULT_TIMEOUT_MS = 120_000; // 2 minutes per AI call
const DEFAULT_MAX_RETRIES = 3;
const RETRY_BASE_DELAY_MS = 1_500;

// ─── OpenFangAgentRunner ──────────────────────────────────────────────────────

/**
 * TaskRunner implementation backed by the OpenFang daemon.
 *
 * @implements TaskRunner
 */
class OpenFangAgentRunner {
  /**
   * @param {string} baseUrl  e.g. "http://127.0.0.1:50051"
   * @param {string} [apiKey] Optional Bearer token
   * @param {number} [timeoutMs]
   * @param {number} [maxRetries]
   */
  constructor(baseUrl, apiKey, timeoutMs = DEFAULT_TIMEOUT_MS, maxRetries = DEFAULT_MAX_RETRIES) {
    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.apiKey = apiKey ?? null;
    this.timeoutMs = timeoutMs;
    this.maxRetries = maxRetries;
  }

  /**
   * Start an agent task.
   *
   * @param {import('./types.js').StartTaskInput} input
   * @param {(event: import('./types.js').TaskEvent) => void} onEvent
   * @returns {Promise<import('./types.js').RunningTask>}
   */
  async start(input, onEvent) {
    const remoteRunId = crypto.randomUUID(); // Local UUID — daemon has no run IDs yet
    let cancelled = false;
    let abortFn = null;

    onEvent({ type: 'status', status: 'queued', message: `Submitting to agent "${input.agent}"` });

    const runWithRetry = async () => {
      for (let attempt = 1; attempt <= this.maxRetries; attempt++) {
        if (cancelled) {
          onEvent({ type: 'status', status: 'cancelled' });
          return;
        }

        if (attempt > 1) {
          const delay = RETRY_BASE_DELAY_MS * Math.pow(2, attempt - 2);
          onEvent({
            type: 'status',
            status: 'running',
            message: `Retry ${attempt}/${this.maxRetries} after ${delay}ms`,
          });
          await sleep(delay);
        }

        onEvent({
          type: 'status',
          status: 'running',
          message: attempt === 1
            ? `Agent run ${remoteRunId} started`
            : `Agent run ${remoteRunId} retry ${attempt}`,
        });

        try {
          // Build the prompt: task context embedded into the message
          const prompt = buildPrompt(input);

          const { result, abort } = await this._postMessage(input.agent, prompt);
          abortFn = abort;

          if (cancelled) {
            onEvent({ type: 'status', status: 'cancelled' });
            return;
          }

          // Emit tokens (whole response as one chunk — daemon is non-streaming)
          if (result.response) {
            onEvent({ type: 'token', content: result.response });
          }

          onEvent({
            type: 'result',
            output: {
              response: result.response,
              input_tokens: result.input_tokens,
              output_tokens: result.output_tokens,
              iterations: result.iterations,
              cost_usd: result.cost_usd,
              remoteRunId,
              agent: input.agent,
            },
          });
          onEvent({ type: 'status', status: 'completed' });
          return; // Success — done

        } catch (err) {
          const isLast = attempt === this.maxRetries;
          onEvent({
            type: 'heartbeat',
            at: new Date().toISOString(),
          });

          if (isLast || cancelled) {
            onEvent({ type: 'error', error: err.message });
            onEvent({ type: 'status', status: 'failed' });
            return;
          }
          // else: loop to retry
        }
      }
    };

    // Fire async — do not block caller (matches the non-blocking long-task pattern)
    void runWithRetry().catch((err) => {
      onEvent({ type: 'error', error: err.message });
      onEvent({ type: 'status', status: 'failed' });
    });

    return {
      taskId: input.taskId,
      remoteRunId,
      cancel: async () => {
        cancelled = true;
        if (abortFn) abortFn();
      },
    };
  }

  /**
   * POST /api/agents/:agentId/message with timeout + abort support.
   *
   * @param {string} agentId
   * @param {string} message
   * @returns {Promise<{ result: import('./types.js').DaemonMessageResponse; abort: () => void }>}
   */
  _postMessage(agentId, message) {
    const url = new URL(`/api/agents/${encodeURIComponent(agentId)}/message`, this.baseUrl);
    const body = JSON.stringify({ message });

    const options = {
      hostname: url.hostname,
      port: url.port || 80,
      path: url.pathname,
      method: 'POST',
      headers: {
        'content-type': 'application/json',
        'content-length': Buffer.byteLength(body),
        ...(this.apiKey ? { authorization: `Bearer ${this.apiKey}` } : {}),
      },
      timeout: this.timeoutMs,
    };

    return new Promise((resolve, reject) => {
      const req = http.request(options, (res) => {
        let raw = '';
        res.on('data', (chunk) => (raw += chunk));
        res.on('end', () => {
          if (res.statusCode < 200 || res.statusCode >= 300) {
            reject(new Error(`Daemon returned HTTP ${res.statusCode}: ${raw.slice(0, 200)}`));
            return;
          }
          let parsed;
          try {
            parsed = JSON.parse(raw);
          } catch {
            reject(new Error(`Non-JSON daemon response: ${raw.slice(0, 200)}`));
            return;
          }
          if (parsed.error) {
            reject(new Error(`Daemon error: ${parsed.error}`));
            return;
          }
          resolve({ result: parsed, abort: () => req.destroy() });
        });
      });

      req.on('timeout', () => {
        req.destroy();
        reject(new Error(`Agent call timed out after ${this.timeoutMs}ms`));
      });

      req.on('error', (err) => reject(err));

      req.write(body);
      req.end();
    });
  }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/**
 * Build the prompt string from a StartTaskInput.
 * Embeds context and metadata so the agent has full task visibility.
 *
 * @param {import('./types.js').StartTaskInput} input
 * @returns {string}
 */
function buildPrompt(input) {
  const contextBlock = input.context && Object.keys(input.context).length > 0
    ? `\n\nContext:\n${JSON.stringify(input.context, null, 2)}`
    : '';

  const metaBlock = input.metadata && Object.keys(input.metadata).length > 0
    ? `\n\nMetadata:\n${JSON.stringify(input.metadata, null, 2)}`
    : '';

  return `${input.prompt}${contextBlock}${metaBlock}`;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

// ─── Exports ──────────────────────────────────────────────────────────────────

module.exports = { OpenFangAgentRunner };
