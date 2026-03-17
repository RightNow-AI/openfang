/**
 * lib/skill-usage.js
 *
 * Builds a usage index: skillName → agentNames[]
 *
 * Source of truth: agent manifests returned by GET /api/agents
 * Each agent may have capabilities.tools — an array of skill names.
 *
 * This is server-side only (called from API route handlers and
 * server components). Never import in 'use client' files.
 */
import { api } from './api-server';

/**
 * Fetch all agents and build a map of skill name → agent names.
 *
 * Semantic rule (see docs/skill-state-contract.md §2):
 *   An agent is counted as using a skill when it references one or more
 *   tools exposed by that skill through `capabilities.tools`.
 *   This count is configuration-derived, not runtime-verified.
 *   An agent is counted once per skill, even if it references multiple
 *   tools from that skill.
 *
 * On any error, returns an empty map rather than crashing callers.
 * Usage counts must never be guessed — if agents can't be fetched,
 * the count stays 0 and a warning is logged.
 *
 * @returns {Promise<Map<string, string[]>>}  skillName → agentNames[]
 */
export async function buildUsageIndex() {
  let agents;
  try {
    const data = await api.get('/api/agents');
    agents = Array.isArray(data) ? data : Array.isArray(data?.agents) ? data.agents : [];
  } catch (err) {
    console.warn('[skill-usage] Could not fetch agents for usage index:', err.message);
    return new Map();
  }

  const index = new Map();

  for (const agent of agents) {
    const name = agent?.name ?? agent?.id;
    if (!name) continue;

    const tools = agent?.capabilities?.tools ?? agent?.tools ?? [];
    if (!Array.isArray(tools)) continue;

    for (const tool of tools) {
      const toolName = typeof tool === 'string' ? tool : tool?.name ?? '';
      if (!toolName) continue;
      if (!index.has(toolName)) index.set(toolName, []);
      index.get(toolName).push(String(name));
    }
  }

  return index;
}

/**
 * Annotate a list of skill cards with used_by_count from the usage index.
 *
 * @param {Array<{name: string, used_by_count: number}>} cards
 * @param {Map<string, string[]>} index
 * @returns the same array with used_by_count populated
 */
export function annotateCards(cards, index) {
  return cards.map(card => ({
    ...card,
    used_by_count: index.get(card.name)?.length ?? 0,
  }));
}
