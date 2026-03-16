/**
 * lib/alive-service.js
 *
 * AliveService — the backend orchestrator for all user requests.
 *
 * The frontend talks only to this service.  It never picks agents directly.
 *
 * Flow:
 *   1. Create a parent run for "alive"
 *   2. Emit run.started
 *   3. Route to a specialist (or keep in alive if no match)
 *   4. Emit run.routed
 *   5. Create a child run for the specialist
 *   6. Call OpenFang daemon with the specialist's agentId
 *   7. Emit run.token (full response — daemon is synchronous, no streaming yet)
 *   8. Emit run.completed / run.failed
 *   9. Update run records
 *
 * The `start()` method returns the parent runId immediately.
 * All subsequent work is fire-and-forget: the caller polls via SSE.
 *
 * @typedef {{
 *   type: 'run.started',   runId: string, parentRunId?: string, agent: string
 * } | {
 *   type: 'run.routed',    runId: string, fromAgent: string, toAgent: string, reason: string
 * } | {
 *   type: 'run.token',     runId: string, agent: string, content: string
 * } | {
 *   type: 'run.status',    runId: string, agent: string, status: string
 * } | {
 *   type: 'run.completed', runId: string, agent: string, output: unknown
 * } | {
 *   type: 'run.failed',    runId: string, agent: string, error: string
 * }} RunEvent
 */

'use strict';

const { agentRegistry } = require('./agent-registry');
const { agentRouter } = require('./agent-router');
const { openfangClient } = require('./openfang-client');
const { runStore } = require('./run-store');
const { eventBus } = require('./event-bus');

/**
 * Emit a RunEvent to the event bus AND append it to the run's replay buffer.
 *
 * @param {string} runId
 * @param {RunEvent} event
 */
function emit(runId, event) {
  runStore.appendEvent(runId, event);
  eventBus.emit(runId, event);
}

const aliveService = {
  /**
   * Start a new top-level run through alive.
   *
   * Returns the parentRunId immediately. The run progresses asynchronously.
   * Subscribe to `GET /api/runs/:runId/events` to follow progress.
   *
   * @param {{ sessionId: string, message: string }} opts
   * @returns {Promise<{ runId: string, status: 'queued' }>}
   */
  async start({ sessionId, message }) {
    // Create parent run synchronously so the caller gets a runId to poll
    const parentRun = await runStore.create({
      sessionId,
      agent: 'alive',
      input: message,
    });

    // Fire-and-forget — the route handler returns before this finishes
    setImmediate(() => {
      aliveService._execute(parentRun, sessionId, message).catch((err) => {
        const errMsg = err instanceof Error ? err.message : String(err);
        runStore.setStatus(parentRun.runId, 'failed', { error: errMsg });
        emit(parentRun.runId, {
          type: 'run.failed',
          runId: parentRun.runId,
          agent: 'alive',
          error: errMsg,
        });
      });
    });

    return { runId: parentRun.runId, status: 'queued' };
  },

  /**
   * Internal async executor. Called from start() via setImmediate.
   *
   * @param {import('./run-store').RunRecord} parentRun
   * @param {string} sessionId
   * @param {string} message
   */
  async _execute(parentRun, sessionId, message) {
    const { runId: parentRunId } = parentRun;

    // Mark alive running
    runStore.setStatus(parentRunId, 'running');
    emit(parentRunId, {
      type: 'run.started',
      runId: parentRunId,
      agent: 'alive',
    });

    // Fetch the daemon agent list once — reused for both routing context and ID resolution
    const rawDaemonAgents = await openfangClient.listAgents().catch(() => []);
    const daemonList = Array.isArray(rawDaemonAgents)
      ? rawDaemonAgents
      : (rawDaemonAgents?.agents ?? []);

    // Route: find a specialist or handle directly with alive
    // Pass the already-fetched daemon list so agentRegistry doesn't re-fetch
    const { agent: specialistId, reason } = agentRouter.select({
      message,
      availableAgents: daemonList,
    });

    const logicalTarget = specialistId ?? 'alive';

    function resolveAgentId(logicalName) {
      // 1. Exact ID match
      const byId = daemonList.find((a) => a.id === logicalName);
      if (byId) return byId.id;
      // 2. Title/name match (case-insensitive)
      const lower = logicalName.toLowerCase();
      const byTitle = daemonList.find(
        (a) => (a.name ?? a.title ?? '').toLowerCase() === lower,
      );
      if (byTitle) return byTitle.id;
      // 3. Fallback: prefer 'assistant', then first available
      const assistant = daemonList.find((a) =>
        (a.name ?? a.title ?? '').toLowerCase().includes('assistant'),
      );
      if (assistant) return assistant.id;
      return daemonList[0]?.id ?? null;
    }

    const targetAgent = resolveAgentId(logicalTarget);
    if (!targetAgent) {
      throw new Error('No agents available in daemon');
    }
    const targetLabel = logicalTarget; // human-readable name for events

    emit(parentRunId, {
      type: 'run.routed',
      runId: parentRunId,
      fromAgent: 'alive',
      toAgent: targetLabel,
      reason,
    });

    // Create child run (even when alive handles it directly, record the leaf)
    const childRun = await runStore.create({
      sessionId,
      agent: targetLabel,
      input: message,
      parentRunId,
    });

    runStore.setStatus(childRun.runId, 'running');
    emit(parentRunId, {
      type: 'run.started',
      runId: childRun.runId,
      parentRunId,
      agent: targetLabel,
    });

    // Call the daemon
    let result;
    try {
      result = await openfangClient.sendMessage(targetAgent, message, sessionId);
    } catch (err) {
      const errMsg = err instanceof Error ? err.message : String(err);
      runStore.setStatus(childRun.runId, 'failed', { error: errMsg });
      emit(parentRunId, {
        type: 'run.failed',
        runId: childRun.runId,
        agent: targetLabel,
        error: errMsg,
      });
      runStore.setStatus(parentRunId, 'failed', { error: errMsg });
      emit(parentRunId, {
        type: 'run.failed',
        runId: parentRunId,
        agent: 'alive',
        error: errMsg,
      });
      return;
    }

    const responseText = String(result?.response ?? '');

    // Child completed
    runStore.setStatus(childRun.runId, 'completed', { output: responseText });
    emit(parentRunId, {
      type: 'run.token',
      runId: childRun.runId,
      agent: targetLabel,
      content: responseText,
    });
    emit(parentRunId, {
      type: 'run.completed',
      runId: childRun.runId,
      agent: targetLabel,
      output: responseText,
    });

    // Parent (alive) merges and completes — for now output == child output
    // Later, call alive agent to summarize/reformat
    runStore.setStatus(parentRunId, 'completed', { output: responseText });
    emit(parentRunId, {
      type: 'run.completed',
      runId: parentRunId,
      agent: 'alive',
      output: responseText,
    });
  },

  /**
   * Cancel a run by ID. Marks both the run and any queued children as cancelled.
   *
   * @param {string} runId
   */
  async cancel(runId) {
    const run = await runStore.get(runId);
    if (!run) throw new Error(`Run not found: ${runId}`);
    if (run.status === 'completed' || run.status === 'failed') return; // already terminal

    runStore.setStatus(runId, 'cancelled');
    emit(runId, { type: 'run.status', runId, agent: run.agent, status: 'cancelled' });

    const children = await runStore.getChildren(runId);
    for (const child of children) {
      if (child.status === 'queued' || child.status === 'running') {
        runStore.setStatus(child.runId, 'cancelled');
        emit(runId, { type: 'run.status', runId: child.runId, agent: child.agent, status: 'cancelled' });
      }
    }
  },
};

module.exports = { aliveService };
