/**
 * lib/skill-preflight.js
 *
 * Compatibility preflight for agent manifest vs. local skill inventory.
 *
 * Phase 4 version rules:
 *   exact match                     → pass
 *   exact differs, constraint pass  → warn  (VERSION_DRIFT)
 *   exact differs, constraint fail  → fail  (VERSION_CONSTRAINT_FAILED)
 *   exact differs, no constraint    → warn only
 *
 *   required skill failures block spawn / save in strict mode
 *   optional skill failures produce warnings only
 *
 * Single source of truth — spawn route and preflight route both call runPreflight().
 * Version comparison lives only in this module.
 *
 * @typedef {{ ok: boolean, agent: string, checks: object[], errors: object[], warnings: object[] }} PreflightResult
 */

import { parseSkillBindings } from './agent-skills';

// ---------------------------------------------------------------------------
// Version utilities  (internal — exported for unit-testing only via satisfiesConstraint)
// ---------------------------------------------------------------------------

/** "1.2.3" → [1, 2, 3] */
function parseVer(v) {
  return String(v ?? '')
    .split('.')
    .map(p => parseInt(p, 10) || 0);
}

/** Returns negative / 0 / positive like strcmp */
function cmpVersions(a, b) {
  const pa = parseVer(a);
  const pb = parseVer(b);
  for (let i = 0; i < 3; i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * Evaluate whether `installed` satisfies `constraint`.
 *
 * Supported constraint formats:
 *   ^1.2.0  — same major, installed >= 1.2.0
 *   ~0.4.0  — same major.minor, installed >= 0.4.0
 *   1.2.0   — exact string equality required
 *
 * @param {string} installed
 * @param {string} constraint
 * @returns {{ satisfied: boolean }}
 */
export function satisfiesConstraint(installed, constraint) {
  if (!constraint) return { satisfied: true };
  const inst = String(installed ?? '').trim();
  const c    = String(constraint  ?? '').trim();

  if (c.startsWith('^')) {
    const min           = c.slice(1);
    const [iMaj]        = parseVer(inst);
    const [mMaj]        = parseVer(min);
    if (iMaj !== mMaj) return { satisfied: false };
    return { satisfied: cmpVersions(inst, min) >= 0 };
  }

  if (c.startsWith('~')) {
    const min           = c.slice(1);
    const [iMaj, iMin]  = parseVer(inst);
    const [mMaj, mMin]  = parseVer(min);
    if (iMaj !== mMaj || iMin !== mMin) return { satisfied: false };
    return { satisfied: cmpVersions(inst, min) >= 0 };
  }

  // Treat bare version string as an exact constraint
  return { satisfied: inst === c };
}

// ---------------------------------------------------------------------------
// Preflight runner
// ---------------------------------------------------------------------------

/**
 * Run compatibility preflight checks for an agent manifest.
 *
 * @param {object}                    params
 * @param {object}                    params.manifest           Parsed manifest object
 * @param {object[]}                  params.localSkills        Installed skills from daemon
 * @param {Map<string, string[]>}     [params.collisionMap]     tool → [owning skill names]
 * @param {boolean}                   [params.registryAvailable] false when the skills endpoint
 *                                                               could not be reached — avoids
 *                                                               false SKILL_NOT_INSTALLED errors
 * @returns {PreflightResult}
 */
export function runPreflight({ manifest = {}, localSkills = [], collisionMap = new Map(), registryAvailable = true }) {
  const bindings  = parseSkillBindings(manifest);
  const agentName = String(manifest?.name ?? '');

  // ── Short-circuit: registry unavailable ──────────────────────────────────
  // When the skill registry could not be loaded, we cannot distinguish "not
  // installed" from "registry is down".  Return a warning instead of lying
  // about every required skill being missing.
  if (registryAvailable === false) {
    return {
      ok:                true,
      agent:             agentName,
      checks:            [],
      errors:            [],
      warnings:          [{
        code:    'REGISTRY_UNAVAILABLE',
        message: 'Skill registry could not be loaded. Preflight checks were skipped — install-time skill validation did not run.',
      }],
      registryUnavailable: true,
    };
  }
  const agentTools = new Set(
    Array.isArray(manifest?.capabilities?.tools) ? manifest.capabilities.tools : []
  );

  const localMap = new Map(
    (localSkills ?? []).map(s => [String(s?.name ?? ''), s])
  );

  const checks   = [];
  const errors   = [];
  const warnings = [];

  for (const binding of bindings) {
    const { name, version, constraint, required } = binding;
    const local = localMap.get(name);

    // ── Check 1: installed ────────────────────────────────────────────────
    if (!local) {
      const msg = `Skill "${name}" is not installed.`;
      checks.push({ type: 'skill_installed', skill: name, status: 'fail', message: msg });
      const entry = { skill: name, code: 'SKILL_NOT_INSTALLED', message: msg };
      if (required) errors.push(entry); else warnings.push(entry);
      continue;  // remaining checks are meaningless without a local record
    }
    checks.push({ type: 'skill_installed', skill: name, status: 'pass' });

    // ── Check 2: enabled ──────────────────────────────────────────────────
    if (local.enabled === false) {
      const msg = `Skill "${name}" is disabled globally.`;
      checks.push({ type: 'skill_enabled', skill: name, status: 'fail', message: msg });
      const entry = { skill: name, code: 'SKILL_DISABLED', message: msg };
      if (required) errors.push(entry); else warnings.push(entry);
    } else {
      checks.push({ type: 'skill_enabled', skill: name, status: 'pass' });
    }

    // ── Check 3: version ──────────────────────────────────────────────────
    if (version && local.version) {
      if (local.version === version) {
        checks.push({ type: 'skill_version', skill: name, status: 'pass' });
      } else if (constraint) {
        const { satisfied } = satisfiesConstraint(local.version, constraint);
        if (satisfied) {
          const msg = `Installed ${local.version} differs from pinned ${version} but satisfies ${constraint}.`;
          checks.push({ type: 'skill_version', skill: name, status: 'warn', message: msg });
          warnings.push({ skill: name, code: 'VERSION_DRIFT', message: msg });
        } else {
          const msg = `Installed ${local.version} does not satisfy constraint ${constraint} (pinned: ${version}).`;
          checks.push({ type: 'skill_version', skill: name, status: 'fail', message: msg });
          const entry = { skill: name, code: 'VERSION_CONSTRAINT_FAILED', message: msg };
          if (required) errors.push(entry); else warnings.push(entry);
        }
      } else {
        // No constraint — exact mismatch is a warning, never a failure (Phase 4)
        const msg = `Installed ${local.version} differs from pinned ${version}.`;
        checks.push({ type: 'skill_version', skill: name, status: 'warn', message: msg });
        warnings.push({ skill: name, code: 'VERSION_DRIFT', message: msg });
      }
    }

    // ── Check 4: tool name collisions for tools this agent references ─────
    const localTools = Array.isArray(local.tools) ? local.tools : [];
    for (const raw of localTools) {
      const tool = typeof raw === 'string' ? raw : String(raw?.name ?? '');
      if (!tool || !agentTools.has(tool)) continue;

      const owners = collisionMap.get(tool);
      if (owners && owners.length > 1) {
        const msg = `Tool "${tool}" is exposed by multiple installed skills: ${owners.join(', ')}.`;
        checks.push({ type: 'tool_collision', skill: name, tool, status: 'fail', message: msg });
        errors.push({ skill: name, tool, code: 'TOOL_NAME_COLLISION', message: msg });
      }
    }
  }

  return {
    ok:       errors.length === 0,
    agent:    agentName,
    checks,
    errors,
    warnings,
  };
}
