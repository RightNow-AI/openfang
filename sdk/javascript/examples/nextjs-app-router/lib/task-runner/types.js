/**
 * lib/task-runner/types.js
 *
 * Shared type definitions for the OpenFang task-runner adapter layer.
 *
 * These JSDoc typedefs mirror the TypeScript interfaces from the architecture spec
 * so VS Code provides full type-checking via checkJs without requiring a tsconfig.
 *
 * Architecture:
 *   longTaskHandler -> TaskRunner interface -> OpenFangAgentRunner
 *
 * The TaskRunner interface is the stable contract. Swap backends without
 * touching any task caller.
 */

'use strict';

/**
 * @typedef {'queued' | 'running' | 'streaming' | 'completed' | 'failed' | 'cancelled'} TaskStatus
 */

/**
 * @typedef {
 *   | { type: 'status'; status: TaskStatus; message?: string }
 *   | { type: 'token'; content: string }
 *   | { type: 'heartbeat'; at: string }
 *   | { type: 'result'; output: unknown }
 *   | { type: 'error'; error: string }
 * } TaskEvent
 */

/**
 * @typedef {
 *   | { type: 'task.accepted'; taskId: string; remoteRunId: string }
 *   | { type: 'task.started'; taskId: string }
 *   | { type: 'task.progress'; taskId: string; message: string }
 *   | { type: 'task.token'; taskId: string; content: string }
 *   | { type: 'task.heartbeat'; taskId: string; at: string }
 *   | { type: 'task.completed'; taskId: string; output: unknown }
 *   | { type: 'task.failed'; taskId: string; error: string }
 *   | { type: 'task.cancelled'; taskId: string }
 * } AgentTaskEvent
 */

/**
 * @typedef {object} StartTaskInput
 * @property {string} taskId
 * @property {string} agent
 * @property {string} prompt
 * @property {Record<string, unknown>} [context]
 * @property {Record<string, unknown>} [metadata]
 */

/**
 * @typedef {object} RunningTask
 * @property {string} taskId
 * @property {string} remoteRunId
 * @property {() => Promise<void>} cancel
 */

/**
 * @typedef {object} TaskRecord
 * @property {string} taskId
 * @property {string} agent
 * @property {string} [remoteRunId]
 * @property {TaskStatus} status
 * @property {string} startedAt
 * @property {string} updatedAt
 * @property {unknown} [output]
 * @property {string} [error]
 * @property {string[]} streamBuffer
 * @property {number} retries
 * @property {() => Promise<void>} [cancel]
 */

/**
 * @typedef {object} DaemonMessageResponse
 * @property {string} response
 * @property {number} input_tokens
 * @property {number} output_tokens
 * @property {number} iterations
 * @property {number} cost_usd
 */

module.exports = {};
