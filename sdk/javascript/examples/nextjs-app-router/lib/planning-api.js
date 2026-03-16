/**
 * Planning Mode API client.
 *
 * Wraps all Structured Planning endpoints. Every function
 * returns null (not an error) when the planning endpoint returns
 * 404 — meaning the work item hasn't entered planning mode yet.
 *
 * Backend types: crates/openfang-types/src/planning.rs
 */

import { apiClient } from './api-client';

// ---------------------------------------------------------------------------
// Label translations — never expose raw backend enum variants in the UI
// ---------------------------------------------------------------------------

/** @param {string} path — e.g. "fast_path", "planned_swarm", "review_swarm" */
export function labelExecutionPath(path) {
  return {
    fast_path:    'Fast Path',
    planned_swarm:'Team Review',
    review_swarm: 'Full Review',
  }[path] ?? path ?? 'Unknown';
}

/** @param {string} status — PlanningRoundStatus variant */
export function labelPlanningStatus(status) {
  if (!status) return 'Not started';
  return {
    open:     'Review in progress',
    approved: 'Approved',
    vetoed:   'Blocked by policy',
    timed_out:'Timed out',
    aborted:  'Aborted',
  }[status] ?? status;
}

/** @param {string} verdict — PlanningVerdict variant */
export function labelVerdict(verdict) {
  return {
    approved:                'Approved',
    approved_with_conditions:'Approved with conditions',
    deferred:                'Deferred — needs more information',
    vetoed:                  'Blocked',
  }[verdict] ?? verdict ?? '—';
}

/** @param {string} role — PlanningRole variant */
export function labelRole(role) {
  return {
    planner:     'Planner',
    reviewer:    'Reviewer',
    risk_checker:'Risk Checker',
    policy_gate: 'Policy Check',
    executor:    'Executor',
  }[role] ?? role ?? '—';
}

/** Human-readable description of each role's function */
export function describeRole(role) {
  return {
    planner:     'Decomposes the task into a concrete execution plan',
    reviewer:    'Reviews the plan for completeness and correctness',
    risk_checker:'Assesses risk exposure of the proposed execution',
    policy_gate: 'Verifies the plan conforms to project policy',
    executor:    'Confirms readiness and takes execution ownership',
  }[role] ?? '';
}

/** CSS badge class for verdict */
export function verdictBadgeClass(verdict) {
  if (verdict === 'approved') return 'badge-success';
  if (verdict === 'approved_with_conditions') return 'badge-warn';
  if (verdict === 'vetoed') return 'badge-error';
  if (verdict === 'deferred') return 'badge-info';
  return 'badge-dim';
}

/** CSS badge class for execution path */
export function pathBadgeClass(path) {
  if (path === 'fast_path') return 'badge-success';
  if (path === 'planned_swarm') return 'badge-info';
  if (path === 'review_swarm') return 'badge-warn';
  return 'badge-dim';
}

/** CSS badge class for planning round status */
export function planningStatusBadgeClass(status) {
  if (status === 'approved') return 'badge-success';
  if (status === 'vetoed') return 'badge-error';
  if (status === 'open') return 'badge-info';
  if (status === 'timed_out' || status === 'aborted') return 'badge-muted';
  return 'badge-dim';
}

// ---------------------------------------------------------------------------
// The deterministic turn order from PlanningRole::sequence_index()
// ---------------------------------------------------------------------------
export const PLANNING_ROLE_ORDER = [
  'planner',
  'reviewer',
  'risk_checker',
  'policy_gate',
  'executor',
];

// ---------------------------------------------------------------------------
// API functions
// ---------------------------------------------------------------------------

/**
 * GET /api/work/:id/planning
 *
 * Returns:
 * {
 *   scope: ScopeClassification | null,
 *   planning_round: StructuredPlanningRound | null,
 * }
 *
 * Returns null when the endpoint returns 404 (no planning data yet).
 */
export async function getPlanningData(workItemId) {
  try {
    return await apiClient.get(`/api/work/${workItemId}/planning`);
  } catch (e) {
    if (e.status === 404) return null;
    throw e;
  }
}

/**
 * POST /api/work/:id/planning/start
 *
 * Opens a new StructuredPlanningRound for the work item.
 * Returns the created planning round or throws on failure.
 */
export async function startPlanningRound(workItemId) {
  return apiClient.post(`/api/work/${workItemId}/planning/start`, {});
}

/**
 * Derive the "next action" for the planning panel given current state.
 *
 * @param {object|null} scope    - ScopeClassification
 * @param {object|null} round    - StructuredPlanningRound
 * @param {string}      itemStatus - WorkItem.status
 * @returns {{ label: string, action: string, variant: string }[]}
 */
export function planningNextActions(scope, round, itemStatus) {
  const path = scope?.path;

  // Fast path — no planning. Normal execution actions apply.
  if (path === 'fast_path' || (!path && !round)) return [];

  const roundStatus = round?.status;

  if (!round) {
    // Needs planning but no round started
    return [{ label: 'Start team review', action: 'start_planning', variant: 'primary' }];
  }

  if (roundStatus === 'open') {
    return [
      { label: 'View current review', action: 'refresh', variant: 'ghost' },
    ];
  }

  if (roundStatus === 'vetoed') {
    return [
      { label: 'View block reason', action: 'scroll_to_veto', variant: 'ghost' },
      { label: 'Revise work item', action: 'revise', variant: 'primary' },
    ];
  }

  if (roundStatus === 'approved') {
    // Check if any conditions were set
    const conditionTurns = (round.turns || []).filter(
      t => t.verdict === 'approved_with_conditions' && t.conditions?.length > 0
    );
    if (conditionTurns.length > 0) {
      return [
        { label: 'Review conditions', action: 'scroll_to_conditions', variant: 'ghost' },
        { label: 'Continue to execution', action: 'run', variant: 'primary' },
      ];
    }
    return [{ label: 'Run task', action: 'run', variant: 'primary' }];
  }

  if (roundStatus === 'timed_out' || roundStatus === 'aborted') {
    return [{ label: 'Restart team review', action: 'start_planning', variant: 'primary' }];
  }

  return [];
}
