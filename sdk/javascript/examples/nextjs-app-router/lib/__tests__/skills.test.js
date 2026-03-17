import { describe, it, expect } from 'vitest';
import { normalizeSkillCard, normalizeSkillDetail } from '../skills';

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------
const skillWebRaw = {
  name: 'web_search',
  description: 'Search the web',
  runtime: 'node',
  enabled: true,
  bundled: true,
  version: '0.1.0',
  tools: [
    { name: 'search', description: 'Search provider' },
    { name: 'browse', description: 'Open result' },
  ],
  source: 'bundled',
  entrypoint: 'skills/web_search/index.js',
};

const skillMemoryRaw = {
  name: 'memory',
  description: 'Memory access',
  runtime: 'python',
  enabled: false,
  bundled: false,
  version: '0.2.0',
  tools: [{ name: 'remember', description: 'Store memory' }],
};

const REQUIRED_CARD_FIELDS = ['name', 'description', 'runtime', 'installed', 'enabled', 'bundled', 'version', 'tool_count', 'used_by_count'];
const REQUIRED_DETAIL_FIELDS = [...REQUIRED_CARD_FIELDS, 'source', 'entrypoint', 'prompt_context', 'tools', 'used_by', 'used_by_count'];

// ---------------------------------------------------------------------------
// normalizeSkillCard
// ---------------------------------------------------------------------------
describe('normalizeSkillCard', () => {
  it('returns the required list fields with safe defaults', () => {
    const card = normalizeSkillCard({}, 0);
    for (const field of REQUIRED_CARD_FIELDS) {
      expect(card).toHaveProperty(field);
    }
  });

  it('preserves shared fields consistently for bundled skills', () => {
    const card = normalizeSkillCard(skillWebRaw, 0);
    expect(card.name).toBe('web_search');
    expect(card.bundled).toBe(true);
    expect(card.runtime).toBe('node');
    expect(card.enabled).toBe(true);
    expect(card.version).toBe('0.1.0');
  });

  it('preserves shared fields consistently for custom skills', () => {
    const card = normalizeSkillCard(skillMemoryRaw, 1);
    expect(card.bundled).toBe(false);
    expect(card.enabled).toBe(false);
    expect(card.runtime).toBe('python');
  });

  it('returns tool_count as zero when tools are missing', () => {
    const card = normalizeSkillCard({ name: 'empty' }, 0);
    expect(card.tool_count).toBe(0);
  });

  it('derives tool_count from the tools array length', () => {
    const card = normalizeSkillCard(skillWebRaw, 0);
    expect(card.tool_count).toBe(2);
  });

  it('returns used_by_count from a provided annotation (injected field)', () => {
    const card = normalizeSkillCard({ ...skillWebRaw, used_by_count: 3 }, 0);
    expect(card.used_by_count).toBe(3);
  });

  it('defaults used_by_count to 0 when not provided', () => {
    const card = normalizeSkillCard(skillWebRaw, 0);
    expect(card.used_by_count).toBe(0);
  });

  it('generates a synthetic name when name and id are missing', () => {
    const card = normalizeSkillCard({}, 7);
    expect(card.name).toBe('skill-7');
  });
});

// ---------------------------------------------------------------------------
// normalizeSkillDetail
// ---------------------------------------------------------------------------
describe('normalizeSkillDetail', () => {
  it('returns the required detail fields with safe defaults', () => {
    const detail = normalizeSkillDetail({}, []);
    for (const field of REQUIRED_DETAIL_FIELDS) {
      expect(detail).toHaveProperty(field);
    }
  });

  it('returns tools as an empty array when missing from raw', () => {
    const detail = normalizeSkillDetail({ name: 'empty' }, []);
    expect(Array.isArray(detail.tools)).toBe(true);
    expect(detail.tools.length).toBe(0);
  });

  it('returns used_by as unique agent names only', () => {
    const detail = normalizeSkillDetail(skillWebRaw, ['researcher', 'analyst', 'researcher']);
    const names = detail.used_by;
    const unique = [...new Set(names)];
    expect(names.length).toBe(unique.length);
    expect(names).toContain('researcher');
    expect(names).toContain('analyst');
  });

  it('returns used_by_count equal to used_by length', () => {
    const detail = normalizeSkillDetail(skillWebRaw, ['researcher', 'analyst']);
    expect(detail.used_by_count).toBe(detail.used_by.length);
    expect(detail.used_by_count).toBe(2);
  });

  it('preserves enabled, bundled, runtime, and version across list and detail normalization', () => {
    const card   = normalizeSkillCard(skillWebRaw, 0);
    const detail = normalizeSkillDetail(skillWebRaw, []);
    expect(detail.enabled).toBe(card.enabled);
    expect(detail.bundled).toBe(card.bundled);
    expect(detail.runtime).toBe(card.runtime);
    expect(detail.version).toBe(card.version);
  });

  it('returns used_by as an empty array when no agents are provided', () => {
    const detail = normalizeSkillDetail(skillWebRaw, []);
    expect(Array.isArray(detail.used_by)).toBe(true);
    expect(detail.used_by.length).toBe(0);
    expect(detail.used_by_count).toBe(0);
  });
});
