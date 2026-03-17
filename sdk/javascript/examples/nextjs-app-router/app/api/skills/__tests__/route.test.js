/**
 * Tests for GET /api/skills (app/api/skills/route.js)
 *
 * Mocks:
 *   - next/server  →  predictable { body, status } objects
 *   - lib/api-server  →  daemon responses
 *   - lib/skill-usage →  usage index
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('next/server', () => ({
  NextResponse: {
    json: (body, init = {}) => ({ body, status: init?.status ?? 200 }),
  },
}));

vi.mock('../../../../lib/api-server', () => ({
  api: { get: vi.fn() },
}));

vi.mock('../../../../lib/skill-usage', () => ({
  buildUsageIndex: vi.fn(),
  annotateCards: vi.fn(),
}));

import { GET } from '../route';
import { api } from '../../../../lib/api-server';
import { buildUsageIndex, annotateCards } from '../../../../lib/skill-usage';

const rawSkillWeb = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '0.1.0',
  tools: [{ name: 'search' }, { name: 'browse' }],
};

const rawSkillMemory = {
  name: 'memory',
  description: 'Memory access',
  runtime: 'python',
  enabled: false,
  bundled: false,
  version: '0.2.0',
  tools: [{ name: 'remember' }],
};

describe('GET /api/skills', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('returns a normalized list of skills with used_by_count', async () => {
    api.get.mockResolvedValue([rawSkillWeb, rawSkillMemory]);
    const usageIndex = new Map([['web_search', ['researcher', 'analyst']]]);
    buildUsageIndex.mockResolvedValue(usageIndex);
    // Let annotateCards do real work (it's pure — delegate to actual function)
    annotateCards.mockImplementation((cards, idx) =>
      cards.map(c => ({ ...c, used_by_count: idx.get(c.name)?.length ?? 0 }))
    );

    const res = await GET();
    expect(res.status).toBe(200);
    expect(Array.isArray(res.body)).toBe(true);
    const web = res.body.find(s => s.name === 'web_search');
    expect(web).toBeDefined();
    expect(web.used_by_count).toBe(2);
    // Required card fields present
    for (const field of ['name', 'description', 'runtime', 'installed', 'enabled', 'bundled', 'version', 'tool_count', 'used_by_count']) {
      expect(web).toHaveProperty(field);
    }
  });

  it('returns card-safe defaults when optional fields are missing', async () => {
    api.get.mockResolvedValue([{ name: 'bare' }]);
    buildUsageIndex.mockResolvedValue(new Map());
    annotateCards.mockImplementation((cards) => cards.map(c => ({ ...c, used_by_count: 0 })));

    const res = await GET();
    const bare = res.body[0];
    expect(bare.tool_count).toBe(0);
    expect(bare.description).toBe('');
    expect(typeof bare.enabled).toBe('boolean');
  });

  it('returns a 5xx with a readable error message when the daemon fails', async () => {
    const err = new Error('Connection refused');
    err.status = 502;
    api.get.mockRejectedValue(err);

    const res = await GET();
    expect(res.status).toBeGreaterThanOrEqual(500);
    expect(typeof res.body.error).toBe('string');
    expect(res.body.error.length).toBeGreaterThan(0);
  });

  it('keeps used_by_count unchanged when a skill is disabled', async () => {
    api.get.mockResolvedValue([rawSkillMemory]); // enabled: false
    const usageIndex = new Map([['memory', ['writer']]]);
    buildUsageIndex.mockResolvedValue(usageIndex);
    annotateCards.mockImplementation((cards, idx) =>
      cards.map(c => ({ ...c, used_by_count: idx.get(c.name)?.length ?? 0 }))
    );

    const res = await GET();
    const memory = res.body.find(s => s.name === 'memory');
    expect(memory.enabled).toBe(false);
    expect(memory.used_by_count).toBe(1); // unchanged despite disabled
  });
});
