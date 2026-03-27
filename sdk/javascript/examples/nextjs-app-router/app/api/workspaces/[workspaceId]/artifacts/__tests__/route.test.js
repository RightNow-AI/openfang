import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('next/server', () => ({
  NextResponse: {
    json: (body, init = {}) => ({ body, status: init?.status ?? 200 }),
  },
}));

const { requireApiPolicyMock, jsonFromAuthErrorMock } = vi.hoisted(() => ({
  requireApiPolicyMock: vi.fn(),
  jsonFromAuthErrorMock: vi.fn((error) => ({ body: { code: error.code }, status: error.status })),
}));

vi.mock('../../../../../../lib/env', () => ({
  env: {
    OPENFANG_API_KEY: 'daemon-key',
  },
}));

vi.mock('../../../../../../lib/api-server', () => ({
  getDaemonUrl: () => 'http://127.0.0.1:50051',
}));

vi.mock('../../../../../../lib/route-authorization', () => ({
  default: {
    jsonFromAuthError: jsonFromAuthErrorMock,
    requireApiPolicy: requireApiPolicyMock,
  },
  jsonFromAuthError: jsonFromAuthErrorMock,
  requireApiPolicy: requireApiPolicyMock,
}));

import { GET } from '../route';
import { jsonFromAuthError, requireApiPolicy } from '../../../../../../lib/route-authorization';

describe('GET /api/workspaces/[workspaceId]/artifacts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    global.fetch = vi.fn().mockResolvedValue({
      status: 200,
      json: async () => ({
        artifacts: [
          {
            artifact_id: 'art-1',
            title: 'summary.md',
          },
        ],
        total: 1,
      }),
    });
  });

  it('returns an auth error when the policy check fails', async () => {
    requireApiPolicy.mockRejectedValue({ status: 403, code: 'forbidden' });

    const response = await GET(
      { headers: new Headers(), method: 'GET' },
      { params: Promise.resolve({ workspaceId: 'ws-1' }) }
    );

    expect(jsonFromAuthError).toHaveBeenCalled();
    expect(response.status).toBe(403);
    expect(global.fetch).not.toHaveBeenCalled();
  });

  it('proxies daemon workspace artifacts and preserves the workspace scope', async () => {
    requireApiPolicy.mockResolvedValue({});

    const response = await GET(
      { headers: new Headers({ cookie: 'secret=1' }), method: 'GET' },
      { params: Promise.resolve({ workspaceId: 'ws-1' }) }
    );

    expect(requireApiPolicy).toHaveBeenCalledWith(
      expect.anything(),
      '/api/workspaces/ws-1/artifacts'
    );
    expect(global.fetch).toHaveBeenCalledWith(
      expect.objectContaining({ href: 'http://127.0.0.1:50051/api/workspaces/ws-1/artifacts' }),
      expect.objectContaining({ method: 'GET', cache: 'no-store' })
    );
    expect(response.status).toBe(200);
    expect(response.body).toEqual({
      workspaceId: 'ws-1',
      artifacts: [{ artifact_id: 'art-1', title: 'summary.md' }],
      total: 1,
    });
  });
});