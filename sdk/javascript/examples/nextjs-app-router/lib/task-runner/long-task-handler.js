/**
 * lib/task-runner/long-task-handler.js
 *
 * LongTaskHandler — orchestrates AI agent tasks through a TaskRunner.
 *
 * Responsibilities:
 *   - Tracks in-memory task state (status, streamBuffer, output, error)
 *   - Persists state to disk so remoteRunId survives restarts
 *   - Handles cancellation
 *   - Emits AgentTaskEvents to callers (UI, API routes, heartbeat)
 *
 * Does NOT know daemon endpoint details — that lives in OpenFangAgentRunner.
 * Does NOT mix worker lifecycle with agent lifecycle.
 * Does NOT block waiting for completion — callers subscribe via onTaskEvent.
 *
 * @typedef {import('./types.js')} _Types
 */

'use strict';

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const STATE_FILE = path.resolve(__dirname, '..', '..', '..', '..', '.heartbeat-state.json');

// ─── LongTaskHandler ─────────────────────────────────────────────────────────

/**
 * Manages AI long-task lifecycle and persistence.
 */
class LongTaskHandler {
  /**
   * @param {object} runner An object with a `start(input, onEvent)` method (TaskRunner interface)
   * @param {(event: import('./types.js').AgentTaskEvent) => void} [onTaskEvent] Global event subscriber
   */
  constructor(runner, onTaskEvent) {
    /** @type {Map<string, import('./types.js').TaskRecord>} */
    this.tasks = new Map();
    this.runner = runner;
    this.onTaskEvent = onTaskEvent ?? null;

    // Load persisted state on startup
    this._loadState();
  }

  /**
   * Start an AI agent loop for a task.
   *
   * @param {object} args
   * @param {string} args.taskId
   * @param {string} args.prompt
   * @param {Record<string, unknown>} [args.context]
   * @param {Record<string, unknown>} [args.metadata]
   * @param {string} [args.agent]
   * @returns {Promise<{ taskId: string; remoteRunId: string; agent: string; status: string }>}
   */
  async startAgentLoop(args) {
    const taskId = args.taskId;
    const agent = args.agent ?? 'alive';

    /** @type {import('./types.js').TaskRecord} */
    const record = {
      taskId,
      agent,
      status: 'queued',
      startedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
      streamBuffer: [],
      retries: 0,
    };

    this.tasks.set(taskId, record);
    this._saveState();

    const runningTask = await this.runner.start(
      {
        taskId,
        agent,
        prompt: args.prompt,
        context: args.context,
        metadata: {
          source: 'long-task-handler',
          mode: 'real-ai-subagent-loop',
          ...(args.metadata ?? {}),
        },
      },
      (event) => this._handleEvent(taskId, event),
    );

    record.remoteRunId = runningTask.remoteRunId;
    record.cancel = runningTask.cancel;
    record.updatedAt = new Date().toISOString();

    this._emit({ type: 'task.accepted', taskId, remoteRunId: runningTask.remoteRunId });
    this._saveState();

    return {
      taskId,
      remoteRunId: runningTask.remoteRunId,
      agent,
      status: record.status,
    };
  }

  /**
   * Cancel a running task.
   *
   * @param {string} taskId
   */
  async cancel(taskId) {
    const record = this.tasks.get(taskId);
    if (!record) throw new Error(`Task not found: ${taskId}`);
    if (!record.cancel) throw new Error(`Task ${taskId} is not cancellable`);

    await record.cancel();
    record.status = 'cancelled';
    record.updatedAt = new Date().toISOString();

    this._emit({ type: 'task.cancelled', taskId });
    this._saveState();
  }

  /**
   * Get current state of a task.
   *
   * @param {string} taskId
   * @returns {import('./types.js').TaskRecord | null}
   */
  getTask(taskId) {
    return this.tasks.get(taskId) ?? null;
  }

  /**
   * Get all task records as a plain array (safe for JSON serialization).
   *
   * @returns {object[]}
   */
  listTasks() {
    return [...this.tasks.values()].map((r) => ({
      taskId: r.taskId,
      agent: r.agent,
      remoteRunId: r.remoteRunId,
      status: r.status,
      startedAt: r.startedAt,
      updatedAt: r.updatedAt,
      output: r.output,
      error: r.error,
      retries: r.retries,
      streamBufferLength: r.streamBuffer.length,
    }));
  }

  // ── Private ────────────────────────────────────────────────────────────────

  /**
   * @param {string} taskId
   * @param {import('./types.js').TaskEvent} event
   */
  _handleEvent(taskId, event) {
    const record = this.tasks.get(taskId);
    if (!record) return;

    record.updatedAt = new Date().toISOString();

    switch (event.type) {
      case 'status':
        record.status = event.status;
        this._emit({ type: 'task.started', taskId });
        if (event.message) {
          this._emit({ type: 'task.progress', taskId, message: event.message });
        }
        break;

      case 'token':
        record.streamBuffer.push(event.content);
        this._emit({ type: 'task.token', taskId, content: event.content });
        break;

      case 'heartbeat':
        this._emit({ type: 'task.heartbeat', taskId, at: event.at });
        break;

      case 'result':
        record.output = event.output;
        record.status = 'completed';
        this._emit({ type: 'task.completed', taskId, output: event.output });
        this._saveState();
        break;

      case 'error':
        record.error = event.error;
        record.status = 'failed';
        record.retries = (record.retries ?? 0) + 1;
        this._emit({ type: 'task.failed', taskId, error: event.error });
        this._saveState();
        break;
    }
  }

  /**
   * @param {import('./types.js').AgentTaskEvent} event
   */
  _emit(event) {
    if (this.onTaskEvent) {
      try {
        this.onTaskEvent(event);
      } catch {
        // Never let subscriber errors crash the handler
      }
    }
  }

  _saveState() {
    try {
      const serializable = {};
      for (const [id, record] of this.tasks.entries()) {
        serializable[id] = {
          taskId: record.taskId,
          agent: record.agent,
          remoteRunId: record.remoteRunId,
          status: record.status,
          startedAt: record.startedAt,
          updatedAt: record.updatedAt,
          output: record.output,
          error: record.error,
          retries: record.retries,
        };
      }
      fs.writeFileSync(STATE_FILE, JSON.stringify(serializable, null, 2), 'utf8');
    } catch {
      // Non-fatal: disk write failure shouldn't crash the task
    }
  }

  _loadState() {
    try {
      if (!fs.existsSync(STATE_FILE)) return;
      const raw = JSON.parse(fs.readFileSync(STATE_FILE, 'utf8'));
      for (const [id, entry] of Object.entries(raw)) {
        this.tasks.set(id, {
          ...entry,
          streamBuffer: [],
          cancel: undefined,
        });
      }
    } catch {
      // Non-fatal: missing or corrupt state is recoverable
    }
  }
}

// ─── Exports ──────────────────────────────────────────────────────────────────

module.exports = { LongTaskHandler };
