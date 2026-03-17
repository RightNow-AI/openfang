/**
 * Tests for GET /api/skills/[name] (app/api/skills/[name]/route.js)
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('next/server', () => ({
  NextResponse: {
    json: (body, init = {}) => ({ body, status: init?.status ?? 200 }),
  },
}));

vi.mock('../../../../../lib/api-server', () => ({
  api: { get: vi.fn() },
}));

vi.mock('../../../../../lib/skill-usage', () => ({
  buildUsageIndex: vi.fn(),
}));

import { GET } from '../route';
import { api } from '../../../../../lib/api-server';
import { buildUsageIndex } from '../../../../../lib/skill-usage';

const rawSkillWeb = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '0.1.0',
  tools: [{ name: 'search', description: 'Search' }, { name: 'browse', description: 'Open' }],
  source: 'bundled',
  entrypoint: 'skills/web_search/index.js',
};

const makeParams = (name) => ({ params: { name } });

describe('GET /api/skills/[name]', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('returns normalized skill detail with used_by and used_by_count', async () => {
    api.get.mockResolvedValue(rawSkillWeb);
    buildUsageIndex.mockResolvedValue(new Map([['web_search', ['researcher', 'analyst']]]));

    const res = await GET(null, makeParams('web_search'));
    expect(res.status).toBe(200);
    const d = res.body;
    expect(d.name).toBe('web_search');
    expect(d.used_by).toContain('researcher');
    expect(d.used_by).toContain('analyst');
    expect(d.used_by_count).toBe(d.used_by.length);
    expect(d.used_by_count).toBe(2);
    // Detail-only fields
    expect(d).toHaveProperty('source');
    expect(d).toHaveProperty('entrypoint');
    expect(d).toHaveProperty('prompt_context');
    expect(d).toHaveProperty('tools');
  });

  it('returns 404 for an unknown skill', async () => {
    const err = new Error('Not found');
    err.status = 404;
    api.get.mockRejectedValue(err);
    buildUsageIndex.mockResolvedValue(new Map());

    const res = await GET(null, makeParams('does_not_exist'));
    expect(res.status).toBe(404);
    expect(typeof res.body.error).toBe('string');
  });

  it('returns tools as an empty array when the skill exposes none', async () => {
    api.get.mockResolvedValue({ name: 'bare' });
    buildUsageIndex.mockResolvedValue(new Map());

    const res = await GET(null, makeParams('bare'));
    expect(Array.isArray(res.body.tools)).toBe(true);
    expect(res.body.tools.length).toBe(0);
  });

  it('returns unique agent names in used_by', async () => {
    api.get.mockResolvedValue(rawSkillWeb);
    // Intentionally include a duplicate to verify dedup in normalizeSkillDetail
    buildUsageIndex.mockResolvedValue(new Map([['web_search', ['researcher', 'analyst', 'researcher']]]));

    const res = await GET(null, makeParams('web_search'));
    const names = res.body.used_by;
    const unique = [...new Set(names)];
    expect(names.length).toBe(unique.length);
  });

  it('returns detail fields consistent with the list endpoint payload', async () => {
    api.get.mockResolvedValue(rawSkillWeb);
    buildUsageIndex.mockResolvedValue(new Map([['web_search', ['researcher']]]));

    const res = await GET(null, makeParams('web_search'));
    const d = res.body;
    // These fields must agree between list and detail (contract §4)
    expect(d.name).toBe(rawSkillWeb.name);
    expect(d.runtime).toBe(rawSkillWeb.runtime);
    expect(d.enabled).toBe(rawSkillWeb.enabled);
    expect(d.bundled).toBe(rawSkillWeb.bundled);
    expect(d.version).toBe(rawSkillWeb.version);
    expect(d.used_by_count).toBe(1);
  });
});
