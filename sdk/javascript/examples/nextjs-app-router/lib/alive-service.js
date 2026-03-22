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
 *   6. Stream OpenFang daemon output from the specialist's agentId
 *   7. Emit incremental run.token events as content arrives
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
 *   type: 'run.phase',     runId: string, agent: string, phase: string, detail?: string|null
 * } | {
 *   type: 'run.tool',      runId: string, agent: string, tool: string, input?: unknown
 * } | {
 *   type: 'run.status',    runId: string, agent: string, status: string
 * } | {
 *   type: 'run.completed', runId: string, agent: string, output: unknown
 * } | {
 *   type: 'run.failed',    runId: string, agent: string, error: string
 * }} RunEvent
 */

'use strict';

const fs = require('node:fs/promises');
const path = require('node:path');

const { agentRouter } = require('./agent-router');
const { openfangClient } = require('./openfang-client');
const { runStore } = require('./run-store');
const { eventBus } = require('./event-bus');

const FIRST_REPORT_TOKEN_TIMEOUT_MS = 60_000;
const RESEARCHER_NAME = 'researcher';

async function loadResearcherManifest() {
  const candidatePaths = [
    path.resolve(process.cwd(), 'agents', 'researcher', 'agent.toml'),
    path.resolve(process.cwd(), '..', '..', '..', '..', 'agents', 'researcher', 'agent.toml'),
    path.resolve(__dirname, '..', '..', '..', '..', '..', 'agents', 'researcher', 'agent.toml'),
  ];

  for (const candidatePath of candidatePaths) {
    try {
      return await fs.readFile(candidatePath, 'utf8');
    } catch (error) {
      if (error?.code !== 'ENOENT') {
        throw error;
      }
    }
  }

  throw new Error('Could not find agents/researcher/agent.toml');
}

function buildResearchFallbackMessage(message) {
  return [
    '[SYSTEM OVERRIDE: You are the lead research analyst.]',
    'You MUST output a long-form, highly detailed, and structured research report.',
    'Use markdown headings, numbered findings, sources with URLs, confidence, and open questions.',
    'Do not give a short, conversational, or empty answer.',
    'If live data is unavailable, synthesize the best known current state and state your assumptions.',
    '',
    `User Request: ${message}`,
  ].join('\n');
}

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

async function ensureResearcherAgent(parentRunId, daemonList) {
  const existingResearcher = daemonList.find(
    (agent) => (agent.name ?? agent.title ?? '').toLowerCase() === RESEARCHER_NAME,
  );
  if (existingResearcher) {
    return {
      daemonList,
      researcher: existingResearcher,
    };
  }

  emit(parentRunId, {
    type: 'run.phase',
    runId: parentRunId,
    agent: 'alive',
    phase: 'spawning_agent',
    detail: 'researcher',
  });

  const manifestToml = await loadResearcherManifest();
  await openfangClient.spawnAgentFromManifest(manifestToml);
  const refreshedList = await openfangClient.listAgents();
  const researcher = refreshedList.find(
    (agent) => (agent.name ?? agent.title ?? '').toLowerCase() === RESEARCHER_NAME,
  );

  if (!researcher) {
    throw new Error('Researcher agent spawn completed but the daemon still does not list researcher');
  }

  emit(parentRunId, {
    type: 'run.phase',
    runId: parentRunId,
    agent: 'alive',
    phase: 'agent_ready',
    detail: 'researcher',
  });

  return {
    daemonList: refreshedList,
    researcher,
  };
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
    let daemonList = Array.isArray(rawDaemonAgents)
      ? rawDaemonAgents
      : (rawDaemonAgents?.agents ?? []);

    // Route: find a specialist or handle directly with alive
    // Pass the already-fetched daemon list so agentRegistry doesn't re-fetch
    const { agent: specialistId, reason } = agentRouter.select({
      message,
      availableAgents: daemonList,
    });

    const logicalTarget = specialistId ?? 'alive';

    if (logicalTarget.toLowerCase().includes(RESEARCHER_NAME)) {
      const ensured = await ensureResearcherAgent(parentRunId, daemonList);
      daemonList = ensured.daemonList;
    }

    function resolveTarget(logicalName) {
      // 1. Exact ID match
      const byId = daemonList.find((a) => a.id === logicalName);
      if (byId) {
        return {
          id: byId.id,
          label: byId.name ?? byId.title ?? logicalName,
        };
      }
      // 2. Title/name match (case-insensitive)
      const lower = logicalName.toLowerCase();
      const byTitle = daemonList.find(
        (a) => (a.name ?? a.title ?? '').toLowerCase() === lower,
      );
      if (byTitle) {
        return {
          id: byTitle.id,
          label: byTitle.name ?? byTitle.title ?? logicalName,
        };
      }
      // 3. Fallback: prefer exact 'assistant', then assistant-like agents, then first available
      const exactAssistant = daemonList.find(
        (a) => (a.name ?? a.title ?? '').toLowerCase() === 'assistant',
      );
      if (exactAssistant) {
        return {
          id: exactAssistant.id,
          label: exactAssistant.name ?? exactAssistant.title ?? 'assistant',
        };
      }
      const assistant = daemonList.find((a) =>
        (a.name ?? a.title ?? '').toLowerCase().includes('assistant'),
      );
      if (assistant) {
        return {
          id: assistant.id,
          label: assistant.name ?? assistant.title ?? 'assistant',
        };
      }
      const first = daemonList[0];
      if (!first) return null;
      return {
        id: first.id,
        label: first.name ?? first.title ?? logicalName,
      };
    }

    const resolvedTarget = resolveTarget(logicalTarget);
    if (!resolvedTarget) {
      throw new Error('No agents available in daemon');
    }
    const targetAgent = resolvedTarget.id;
    const targetLabel = logicalTarget === 'alive'
      ? 'alive'
      : resolvedTarget.label;
    const needsResearchFallbackPrompt =
      logicalTarget.toLowerCase().includes('researcher') &&
      !targetLabel.toLowerCase().includes('researcher');
    const finalMessage = needsResearchFallbackPrompt
      ? buildResearchFallbackMessage(message)
      : message;

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

    // Stream the daemon output and persist progress incrementally.
    let responseText = '';
    let streamCompleted = false;
    const streamController = new AbortController();
    const firstTokenTimeout = setTimeout(() => {
      if (!responseText.trim()) {
        streamController.abort(new Error('Research agent did not produce any report text within 60s'));
      }
    }, FIRST_REPORT_TOKEN_TIMEOUT_MS);
    try {
      await openfangClient.streamMessage(targetAgent, finalMessage, async ({ event, data }) => {
        if (event === 'chunk' && typeof data?.content === 'string' && data.content) {
          clearTimeout(firstTokenTimeout);
          responseText += data.content;
          runStore.setOutput(childRun.runId, responseText);
          runStore.setOutput(parentRunId, responseText);
          emit(parentRunId, {
            type: 'run.token',
            runId: childRun.runId,
            agent: targetLabel,
            content: data.content,
          });
          return;
        }

        if (event === 'phase') {
          emit(parentRunId, {
            type: 'run.phase',
            runId: childRun.runId,
            agent: targetLabel,
            phase: String(data?.phase ?? 'running'),
            detail: data?.detail ?? null,
          });
          return;
        }

        if (event === 'tool_use' || event === 'tool_result') {
          emit(parentRunId, {
            type: 'run.tool',
            runId: childRun.runId,
            agent: targetLabel,
            tool: String(data?.tool ?? 'unknown'),
            input: data?.input,
          });
          return;
        }

        if (event === 'done') {
          streamCompleted = true;
        }
      }, { signal: streamController.signal });
    } catch (err) {
      clearTimeout(firstTokenTimeout);
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
    clearTimeout(firstTokenTimeout);

    if (!responseText.trim()) {
      const errMsg = streamCompleted
        ? 'Research agent completed without producing a report'
        : 'Daemon stream ended before the assistant produced a reply';
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

    // Child completed
    runStore.setStatus(childRun.runId, 'completed', { output: responseText });
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
