/**
 * Tests for lib/dev-token-guard.js
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { guardDevToken } from '../dev-token-guard';

function makeRequest(headers = {}) {
  return {
    headers: {
      get: (name) => headers[name.toLowerCase()] ?? null,
    },
  };
}

describe('guardDevToken \u2014 guard disabled', () => {
  beforeEach(() => {
    delete process.env.OPENFANG_REQUIRE_DEV_TOKEN;
  });

  it('returns null (allow) when OPENFANG_REQUIRE_DEV_TOKEN is not set', () => {
    expect(guardDevToken(makeRequest())).toBeNull();
  });

  it('returns null (allow) when OPENFANG_REQUIRE_DEV_TOKEN is empty string', () => {
    process.env.OPENFANG_REQUIRE_DEV_TOKEN = '';
    expect(guardDevToken(makeRequest())).toBeNull();
  });
});

describe('guardDevToken \u2014 guard enabled', () => {
  beforeEach(() => {
    process.env.OPENFANG_REQUIRE_DEV_TOKEN = 'test-secret-123';
  });

  afterEach(() => {
    delete process.env.OPENFANG_REQUIRE_DEV_TOKEN;
  });

  it('returns null (allow) when the correct token is provided', () => {
    const result = guardDevToken(makeRequest({ 'x-dev-token': 'test-secret-123' }));
    expect(result).toBeNull();
  });

  it('returns 401 response when X-Dev-Token header is absent', async () => {
    const result = guardDevToken(makeRequest());
    expect(result).not.toBeNull();
    expect(result.status).toBe(401);
    const body = await result.json();
    expect(body.code).toBe('DEV_TOKEN_REQUIRED');
  });

  it('returns 401 response when X-Dev-Token has the wrong value', async () => {
    const result = guardDevToken(makeRequest({ 'x-dev-token': 'wrong-token' }));
    expect(result).not.toBeNull();
    expect(result.status).toBe(401);
    const body = await result.json();
    expect(body.code).toBe('DEV_TOKEN_REQUIRED');
  });

  it('returns 401 for empty string token when guard is enabled', async () => {
    const result = guardDevToken(makeRequest({ 'x-dev-token': '' }));
    expect(result).not.toBeNull();
    expect(result.status).toBe(401);
  });
});
