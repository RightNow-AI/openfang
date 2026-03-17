/**
 * Tests for PUT /api/skills/[name]/enabled
 * (app/api/skills/[name]/enabled/route.js)
 */
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('next/server', () => ({
  NextResponse: {
    json: (body, init = {}) => ({ body, status: init?.status ?? 200 }),
  },
}));

vi.mock('../../../../../lib/api-server', () => ({
  api: { put: vi.fn() },
}));

import { PUT } from '../enabled/route';
import { api } from '../../../../../lib/api-server';

const makeRequest = (body) => ({
  json: () => Promise.resolve(body),
});

const makeParams = (name) => ({ params: { name } });

describe('PUT /api/skills/[name]/enabled', () => {
  beforeEach(() => { vi.clearAllMocks(); });

  it('enables a skill and returns the updated state', async () => {
    api.put.mockResolvedValue({ name: 'web_search', enabled: true });

    const res = await PUT(makeRequest({ enabled: true }), makeParams('web_search'));
    expect(res.status).toBe(200);
    expect(res.body.name).toBe('web_search');
    expect(res.body.enabled).toBe(true);
  });

  it('disables a skill and returns the updated state', async () => {
    api.put.mockResolvedValue({ name: 'web_search', enabled: false });

    const res = await PUT(makeRequest({ enabled: false }), makeParams('web_search'));
    expect(res.status).toBe(200);
    expect(res.body.enabled).toBe(false);
  });

  it('returns 400 for a missing enabled boolean (empty body)', async () => {
    const res = await PUT(makeRequest({}), makeParams('web_search'));
    expect(res.status).toBe(400);
    expect(typeof res.body.error).toBe('string');
  });

  it('returns 400 for a non-boolean enabled value', async () => {
    const res = await PUT(makeRequest({ enabled: 'yes' }), makeParams('web_search'));
    expect(res.status).toBe(400);
    expect(typeof res.body.error).toBe('string');
  });

  it('returns 400 for malformed JSON', async () => {
    const badReq = { json: () => Promise.reject(new SyntaxError('Unexpected token')) };
    const res = await PUT(badReq, makeParams('web_search'));
    expect(res.status).toBe(400);
  });

  it('returns 404 for an unknown skill', async () => {
    const err = new Error('Not found');
    err.status = 404;
    api.put.mockRejectedValue(err);

    const res = await PUT(makeRequest({ enabled: true }), makeParams('ghost'));
    expect(res.status).toBe(404);
    expect(typeof res.body.error).toBe('string');
  });

  it('returns 502 when the daemon proxy fails', async () => {
    const err = new Error('upstream timeout');
    err.status = 502;
    api.put.mockRejectedValue(err);

    const res = await PUT(makeRequest({ enabled: false }), makeParams('web_search'));
    expect(res.status).toBe(502);
    expect(typeof res.body.error).toBe('string');
  });

  it('does not mutate used_by or agent configuration state', async () => {
    // The response shape must only contain name + enabled — no used_by field
    api.put.mockResolvedValue({ name: 'web_search', enabled: false });

    const res = await PUT(makeRequest({ enabled: false }), makeParams('web_search'));
    expect(res.body).not.toHaveProperty('used_by');
    expect(res.body).not.toHaveProperty('used_by_count');
    expect(Object.keys(res.body).sort()).toEqual(['enabled', 'name']);
  });
});
