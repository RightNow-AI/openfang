/**
 * Tests for lib/skill-preflight.js
 */
import { describe, it, expect } from 'vitest';
import { satisfiesConstraint, runPreflight } from '../skill-preflight';

// ---------------------------------------------------------------------------
// satisfiesConstraint
// ---------------------------------------------------------------------------
describe('satisfiesConstraint', () => {
  it('no constraint always satisfies', () => {
    expect(satisfiesConstraint('1.0.0', '').satisfied).toBe(true);
    expect(satisfiesConstraint('1.0.0', undefined).satisfied).toBe(true);
  });

  it('^major — same major, >=min passes', () => {
    expect(satisfiesConstraint('1.3.0', '^1.2.0').satisfied).toBe(true);
    expect(satisfiesConstraint('1.2.0', '^1.2.0').satisfied).toBe(true);
  });

  it('^major — same major, below min fails', () => {
    expect(satisfiesConstraint('1.1.9', '^1.2.0').satisfied).toBe(false);
  });

  it('^major — different major fails', () => {
    expect(satisfiesConstraint('2.0.0', '^1.2.0').satisfied).toBe(false);
  });

  it('~minor — same major.minor, >=patch passes', () => {
    expect(satisfiesConstraint('0.4.5', '~0.4.0').satisfied).toBe(true);
  });

  it('~minor — different minor fails', () => {
    expect(satisfiesConstraint('0.5.0', '~0.4.0').satisfied).toBe(false);
  });

  it('exact constraint — equal passes', () => {
    expect(satisfiesConstraint('1.2.3', '1.2.3').satisfied).toBe(true);
  });

  it('exact constraint — unequal fails', () => {
    expect(satisfiesConstraint('1.2.4', '1.2.3').satisfied).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// runPreflight — helpers
// ---------------------------------------------------------------------------
const makeSkill = (overrides = {}) => ({
  name:    'calc',
  version: '1.0.0',
  enabled: true,
  tools:   ['add', 'subtract'],
  ...overrides,
});

const makeBinding = (overrides = {}) => ({
  name:       'calc',
  version:    '1.0.0',
  constraint: '',
  required:   true,
  source:     'local',
  ...overrides,
});

const makeManifest = (bindings = [], tools = []) => ({
  name:         'test-agent',
  skills:       bindings,
  capabilities: { tools },
});

// ---------------------------------------------------------------------------
// runPreflight — installed check
// ---------------------------------------------------------------------------
describe('runPreflight — skill_installed', () => {
  it('passes when skill is installed', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding()]),
      localSkills: [makeSkill()],
    });
    expect(result.ok).toBe(true);
    expect(result.checks[0]).toMatchObject({ type: 'skill_installed', status: 'pass' });
  });

  it('fails (required) when skill is not installed', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ name: 'missing' })]),
      localSkills: [],
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('SKILL_NOT_INSTALLED');
  });

  it('warns (optional) when optional skill is not installed', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ name: 'missing', required: false })]),
      localSkills: [],
    });
    expect(result.ok).toBe(true);
    expect(result.warnings[0].code).toBe('SKILL_NOT_INSTALLED');
  });
});

// ---------------------------------------------------------------------------
// runPreflight — enabled check
// ---------------------------------------------------------------------------
describe('runPreflight — skill_enabled', () => {
  it('fails (required) when skill is disabled', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding()]),
      localSkills: [makeSkill({ enabled: false })],
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('SKILL_DISABLED');
  });

  it('warns (optional) when optional skill is disabled', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ required: false })]),
      localSkills: [makeSkill({ enabled: false })],
    });
    expect(result.ok).toBe(true);
    expect(result.warnings[0].code).toBe('SKILL_DISABLED');
  });
});

// ---------------------------------------------------------------------------
// runPreflight — version check
// ---------------------------------------------------------------------------
describe('runPreflight — skill_version', () => {
  it('passes on exact version match', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ version: '1.0.0', constraint: '' })]),
      localSkills: [makeSkill({ version: '1.0.0' })],
    });
    const vCheck = result.checks.find(c => c.type === 'skill_version');
    expect(vCheck?.status).toBe('pass');
  });

  it('warns (VERSION_DRIFT) when no constraint and version differs', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ version: '1.0.0', constraint: '' })]),
      localSkills: [makeSkill({ version: '1.1.0' })],
    });
    expect(result.ok).toBe(true);
    expect(result.warnings[0].code).toBe('VERSION_DRIFT');
  });

  it('warns (VERSION_DRIFT) when constraint satisfied but pinned differs', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ version: '1.0.0', constraint: '^1.0.0' })]),
      localSkills: [makeSkill({ version: '1.2.0' })],
    });
    expect(result.ok).toBe(true);
    expect(result.warnings[0].code).toBe('VERSION_DRIFT');
  });

  it('fails (required) when constraint unsatisfied', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ version: '1.0.0', constraint: '^1.0.0' })]),
      localSkills: [makeSkill({ version: '2.0.0' })],
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('VERSION_CONSTRAINT_FAILED');
  });

  it('warns (optional) when optional required and constraint unsatisfied', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ required: false, version: '1.0.0', constraint: '^1.0.0' })]),
      localSkills: [makeSkill({ version: '2.0.0' })],
    });
    expect(result.ok).toBe(true);
    expect(result.warnings[0].code).toBe('VERSION_CONSTRAINT_FAILED');
  });
});

// ---------------------------------------------------------------------------
// runPreflight — tool collision check
// ---------------------------------------------------------------------------
describe('runPreflight — tool_collision', () => {
  it('fails when agent uses colliding tool', () => {
    const collisionMap = new Map([['add', ['calc', 'math']]]);
    const result = runPreflight({
      manifest:    makeManifest([makeBinding()], ['add']),
      localSkills: [makeSkill({ tools: ['add', 'subtract'] })],
      collisionMap,
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('TOOL_NAME_COLLISION');
  });

  it('passes when agent does not use the colliding tool', () => {
    const collisionMap = new Map([['add', ['calc', 'math']]]);
    const result = runPreflight({
      manifest:    makeManifest([makeBinding()], ['subtract']),   // ← different tool
      localSkills: [makeSkill({ tools: ['add', 'subtract'] })],
      collisionMap,
    });
    expect(result.ok).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// runPreflight — no bindings
// ---------------------------------------------------------------------------
describe('runPreflight — no bindings', () => {
  it('passes with ok=true and empty arrays for agents with no skills', () => {
    const result = runPreflight({ manifest: { name: 'legacy', skills: [] }, localSkills: [] });
    expect(result.ok).toBe(true);
    expect(result.checks).toHaveLength(0);
    expect(result.errors).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// runPreflight — registry unavailable
// ---------------------------------------------------------------------------
describe('runPreflight — registry unavailable', () => {
  it('returns ok:true with REGISTRY_UNAVAILABLE warning — does not produce false SKILL_NOT_INSTALLED errors', () => {
    const result = runPreflight({
      manifest:          makeManifest([makeBinding({ name: 'missing-skill' })]),
      localSkills:       [],
      registryAvailable: false,
    });
    expect(result.ok).toBe(true);
    expect(result.registryUnavailable).toBe(true);
    expect(result.warnings).toHaveLength(1);
    expect(result.warnings[0].code).toBe('REGISTRY_UNAVAILABLE');
    expect(result.errors).toHaveLength(0);
    expect(result.checks).toHaveLength(0);
  });

  it('returns ok:true regardless of how many required skills the manifest has', () => {
    const result = runPreflight({
      manifest: makeManifest([
        makeBinding({ name: 'skill-a', required: true }),
        makeBinding({ name: 'skill-b', required: true }),
      ]),
      localSkills:       [],
      registryAvailable: false,
    });
    expect(result.ok).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  it('normal missing-skill check still fails when registry IS available', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ name: 'not-installed' })]),
      localSkills: [],
      // registryAvailable defaults to true
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('SKILL_NOT_INSTALLED');
  });

  it('normal disabled-skill check still fails when registry IS available', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding()]),
      localSkills: [makeSkill({ enabled: false })],
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('SKILL_DISABLED');
  });

  it('version mismatch check still fails when registry IS available', () => {
    const result = runPreflight({
      manifest:    makeManifest([makeBinding({ version: '1.0.0', constraint: '^1.0.0' })]),
      localSkills: [makeSkill({ version: '2.0.0' })],
    });
    expect(result.ok).toBe(false);
    expect(result.errors[0].code).toBe('VERSION_CONSTRAINT_FAILED');
  });
});
