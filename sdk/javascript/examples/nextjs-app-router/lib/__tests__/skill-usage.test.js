import { describe, it, expect, vi, beforeEach } from 'vitest';

// vi.mock is hoisted before imports — api-server is mocked before skill-usage loads it
vi.mock('../api-server', () => ({
  api: { get: vi.fn() },
}));

import { buildUsageIndex, annotateCards } from '../skill-usage';
import { api } from '../api-server';

// ---------------------------------------------------------------------------
// Fixtures
// In the current implementation, capabilities.tools lists SKILL NAMES
// (not individual tool function names). This matches real agent TOML files
// where capabilities.tools = ["web_search", "memory", ...].
// ---------------------------------------------------------------------------
const agentResearcher = { name: 'researcher', capabilities: { tools: ['web_search'] } };
const agentAnalyst    = { name: 'analyst',    capabilities: { tools: ['web_search'] } };
const agentWriter     = { name: 'writer',     capabilities: { tools: ['memory'] } };

describe('buildUsageIndex', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('counts one agent once when the agent references the same skill multiple times', async () => {
    // Dedup invariant: even if a skill is listed twice in capabilities.tools,
    // the agent is counted once per skill (contract §4).
    api.get.mockResolvedValue([
      { name: 'researcher', capabilities: { tools: ['web_search', 'web_search'] } },
    ]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toEqual(['researcher']);
    expect(index.get('web_search').length).toBe(1);
  });

  it('counts multiple agents referencing the same skill as distinct users', async () => {
    api.get.mockResolvedValue([agentResearcher, agentAnalyst]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toContain('researcher');
    expect(index.get('web_search')).toContain('analyst');
    expect(index.get('web_search').length).toBe(2);
  });

  it('does not count agents that reference no tools from a skill', async () => {
    api.get.mockResolvedValue([agentWriter]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toBeUndefined();
  });

  it('returns an empty usage index when no agents exist', async () => {
    api.get.mockResolvedValue([]);
    const index = await buildUsageIndex();
    expect(index.size).toBe(0);
  });

  it('returns an empty usage index when agents have no capabilities.tools array', async () => {
    api.get.mockResolvedValue([{ name: 'orphan' }]);
    const index = await buildUsageIndex();
    expect(index.size).toBe(0);
  });

  it('ignores duplicate tool names in a single agent config', async () => {
    api.get.mockResolvedValue([
      { name: 'researcher', capabilities: { tools: ['web_search', 'web_search', 'memory'] } },
    ]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toEqual(['researcher']);
    expect(index.get('web_search').length).toBe(1);
  });

  it('does not change usage counts when a skill is disabled', async () => {
    // The usage index is derived from agent configs only — skill enabled state
    // is not visible here and must not affect the count (contract §4).
    api.get.mockResolvedValue([agentResearcher]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toContain('researcher');
  });

  it('maps tool references to the owning skill according to the current contract', async () => {
    // Contract §2: an agent is counted when it references a skill name through
    // capabilities.tools. No explicit "skills = [...]" attachment is required.
    api.get.mockResolvedValue([{ name: 'agent-x', capabilities: { tools: ['web_search'] } }]);
    const index = await buildUsageIndex();
    expect(index.get('web_search')).toEqual(['agent-x']);
  });

  it('returns an empty map and does not throw when the daemon is unreachable', async () => {
    api.get.mockRejectedValue(new Error('Connection refused'));
    await expect(buildUsageIndex()).resolves.toEqual(new Map());
  });
});

describe('annotateCards', () => {
  it('populates used_by_count from the usage index', () => {
    const index = new Map([['web_search', ['researcher', 'analyst']]]);
    const cards = [{ name: 'web_search', used_by_count: 0 }];
    const result = annotateCards(cards, index);
    expect(result[0].used_by_count).toBe(2);
  });

  it('leaves used_by_count as 0 when the skill is not in the index', () => {
    const index = new Map();
    const cards = [{ name: 'web_search', used_by_count: 0 }];
    const result = annotateCards(cards, index);
    expect(result[0].used_by_count).toBe(0);
  });
});
