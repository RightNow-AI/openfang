/**
 * lib/task-runner/index.js
 *
 * Public barrel export for the task-runner adapter layer.
 *
 * Usage (Next.js API route):
 *
 *   const { createTaskRunner, LongTaskHandler } = require('../lib/task-runner');
 *   const runner = createTaskRunner();
 *   const handler = new LongTaskHandler(runner, (event) => console.log(event));
 *
 *   // Start a real AI subagent loop
 *   const task = await handler.startAgentLoop({
 *     taskId: crypto.randomUUID(),
 *     agent: 'alive',
 *     prompt: 'Investigate and propose next steps.',
 *     context: { goal: 'audit API endpoints', repoPath: '/openfang' },
 *     metadata: { source: 'api-route', mode: 'real-ai-subagent-loop' },
 *   });
 */

'use strict';

const { OpenFangAgentRunner } = require('./openfang-agent-runner');
const { LongTaskHandler } = require('./long-task-handler');

/**
 * Create the default OpenFangAgentRunner using environment variables.
 *
 * @param {object} [opts]
 * @param {string} [opts.baseUrl]
 * @param {string} [opts.apiKey]
 * @param {number} [opts.timeoutMs]
 * @param {number} [opts.maxRetries]
 * @returns {OpenFangAgentRunner}
 */
function createTaskRunner(opts = {}) {
  return new OpenFangAgentRunner(
    opts.baseUrl ?? process.env.OPENFANG_BASE_URL ?? 'http://127.0.0.1:50051',
    opts.apiKey ?? process.env.OPENFANG_API_KEY,
    opts.timeoutMs,
    opts.maxRetries,
  );
}

module.exports = { OpenFangAgentRunner, LongTaskHandler, createTaskRunner };
