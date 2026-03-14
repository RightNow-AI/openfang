/**
 * lib/agent-registry.js
 *
 * Loads the agent list from the OpenFang daemon and decorates each entry
 * with visibility metadata.
 *
 * Visibility rules:
 *   public   → exposed to the frontend (only "alive")
 *   internal → hidden specialists, reachable only through alive
 *
 * The INITIAL_ROUTER_AGENTS set defines which agents are wired to the keyword
 * router for this sprint.  All others are registered as internal but will
 * appear dark in the registry until the router is extended.
 *
 * @typedef {{ id: string, title: string, description: string,
 *             visibility: 'public'|'internal', tags: string[],
 *             inputMode: 'chat'|'task'|'structured' }} AgentDescriptor
 */

'use strict';

const { openfangClient } = require('./openfang-client');

// The one and only public entry-point
const PUBLIC_AGENTS = new Set(['alive']);

// The six specialists wired to the router in sprint-1
const INITIAL_ROUTER_AGENTS = new Set([
  'coder',
  'debugger',
  'code-reviewer',
  'researcher',
  'planner',
  'writer',
]);

/** @param {unknown} raw */
function toDescriptor(raw) {
  const id = String(raw?.id ?? '').trim();
  return /** @type {AgentDescriptor} */ ({
    id,
    title: String(raw?.name ?? raw?.id ?? id),
    description: String(raw?.description ?? ''),
    visibility: PUBLIC_AGENTS.has(id) ? 'public' : 'internal',
    tags: Array.isArray(raw?.tags) ? raw.tags : [],
    inputMode: 'chat',
  });
}

const agentRegistry = {
  /** All agents (public + internal) */
  async listAll() {
    const raw = await openfangClient.listAgents();
    const agents = (Array.isArray(raw) ? raw : raw?.agents ?? []).map(toDescriptor);

    // Always include alive even if daemon hasn't loaded it yet
    const hasAlive = agents.some((a) => a.id === 'alive');
    if (!hasAlive) {
      agents.unshift({
        id: 'alive',
        title: 'Alive',
        description: 'Primary agent — entry point for all user requests',
        visibility: 'public',
        tags: ['subagent', 'orchestration', 'alive'],
        inputMode: 'chat',
      });
    }

    return agents;
  },

  /** Only the public entry-points (currently just "alive") */
  async listPublic() {
    const all = await this.listAll();
    return all.filter((a) => a.visibility === 'public');
  },

  /**
   * Internal specialists that are active in the router.
   * Gracefully degrades when a specialist isn't registered in the daemon.
   */
  async listActiveInternal() {
    const all = await this.listAll();
    return all.filter(
      (a) => a.visibility === 'internal' && INITIAL_ROUTER_AGENTS.has(a.id),
    );
  },
};

module.exports = { agentRegistry, PUBLIC_AGENTS, INITIAL_ROUTER_AGENTS };
