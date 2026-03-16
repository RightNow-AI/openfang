'use client';

/**
 * PlanningPanel — client component that surfaces Structured Planning Mode.
 *
 * Shows: execution path, route reasons, planning status, ordered participant
 * roster, turn summaries, final verdict, conditions/veto reason, next action.
 *
 * Import and render inside any work-detail or inbox-detail view.
 *
 * Props:
 *   scope          {object|null}  — ScopeClassification from planning-api
 *   planningRound  {object|null}  — StructuredPlanningRound from planning-api
 *   loading        {boolean}
 *   error          {string}
 *   onAction       {(action: string) => void}  — called when a next-action is clicked
 *   onRefresh      {() => void}
 */

import { useState } from 'react';
import {
  labelExecutionPath,
  labelPlanningStatus,
  labelRole,
  describeRole,
  labelVerdict,
  verdictBadgeClass,
  pathBadgeClass,
  planningStatusBadgeClass,
  planningNextActions,
  PLANNING_ROLE_ORDER,
} from '../../lib/planning-api';

// ---------------------------------------------------------------------------
// ExecutionPathBadge
// ---------------------------------------------------------------------------
export function ExecutionPathBadge({ path }) {
  if (!path) return null;
  return (
    <span
      data-cy="execution-path-badge"
      className={`badge ${pathBadgeClass(path)}`}
      title={`Execution path: ${path}`}
    >
      {labelExecutionPath(path)}
    </span>
  );
}

// ---------------------------------------------------------------------------
// RouteReasonCard
// ---------------------------------------------------------------------------
function RouteReasonCard({ scope }) {
  const [expanded, setExpanded] = useState(false);
  const signals = scope?.signals ?? [];

  return (
    <div data-cy="planning-route-reason" className="route-reason-card">
      <div
        className="route-reason-card-header"
        onClick={() => setExpanded(e => !e)}
        role="button"
        aria-expanded={expanded}
      >
        <span className="route-reason-card-title">Why this needs team review</span>
        <span style={{ color: 'var(--accent)', fontSize: 11 }}>{expanded ? '▲' : '▼'}</span>
      </div>

      {/* Always show the rationale */}
      {scope?.rationale && (
        <p className="route-reason-rationale">{scope.rationale}</p>
      )}

      {/* Expandable signal list */}
      {expanded && signals.length > 0 && (
        <ul style={{ margin: '8px 0 0', paddingLeft: 16, color: 'var(--text-secondary)', fontSize: 12 }}>
          {signals.map((s, i) => (
            <li key={i} style={{ marginBottom: 3 }}>
              <strong>{s.description || s.code}</strong>
            </li>
          ))}
        </ul>
      )}
      {expanded && signals.length === 0 && (
        <p style={{ margin: '6px 0 0', color: 'var(--text-muted)', fontSize: 12 }}>
          No detailed signals recorded.
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// PlanningTurnRow
// ---------------------------------------------------------------------------
function PlanningTurnRow({ role, turn, isActive }) {
  const [expanded, setExpanded] = useState(false);
  const submitted = !!turn;
  const hasConditions = submitted && turn.conditions?.length > 0;
  const isVeto = submitted && turn.verdict === 'vetoed';

  let statusClass = '';
  if (!submitted && isActive) statusClass = 'role-active';
  else if (submitted && turn.verdict === 'vetoed') statusClass = 'role-vetoed';
  else if (submitted && (turn.verdict === 'approved' || turn.verdict === 'approved_with_conditions')) statusClass = 'role-approved';
  else if (submitted) statusClass = 'role-warning';

  const roleDataCy = {
    planner:     'planning-turn-planner',
    reviewer:    'planning-turn-reviewer',
    risk_checker:'planning-turn-risk',
    policy_gate: 'planning-turn-policy',
    executor:    'planning-turn-executor',
  }[role] ?? `planning-turn-${role}`;

  return (
    <div
      data-cy={roleDataCy}
      className={`participant-row ${statusClass}`}
      style={{ opacity: !submitted && !isActive ? 0.45 : 1 }}
    >
      <div
        style={{ display: 'flex', alignItems: 'center', gap: 8, cursor: submitted ? 'pointer' : 'default' }}
        onClick={() => submitted && setExpanded(e => !e)}
      >
        <span className="participant-row-marker">
          {!submitted && isActive ? '◎' : !submitted ? '○' : isVeto ? '✕' : '✓'}
        </span>

        <div className="participant-row-content">
          <span className="participant-row-name">{labelRole(role)}</span>
          <span className="participant-row-desc">{describeRole(role)}</span>
        </div>

        {submitted && (
          <span className={`badge ${verdictBadgeClass(turn.verdict)}`} style={{ fontSize: 10 }}>
            {labelVerdict(turn.verdict)}
          </span>
        )}
        {!submitted && isActive && (
          <span className="badge badge-info" style={{ fontSize: 10 }}>Waiting</span>
        )}
      </div>

      {submitted && expanded && (
        <div style={{ marginTop: 8, paddingLeft: 26 }}>
          {turn.agent_name && (
            <div style={{ fontSize: 11, color: 'var(--text-muted)', marginBottom: 4 }}>by {turn.agent_name}</div>
          )}
          {turn.content && (
            <p style={{ margin: '0 0 6px', fontSize: 12, color: 'var(--text-secondary)', whiteSpace: 'pre-wrap', lineHeight: 1.55 }}>
              {turn.content}
            </p>
          )}
          {hasConditions && (
            <div className="condition-block">
              <strong style={{ color: 'var(--warning)' }}>Conditions:</strong>
              <ul style={{ margin: '4px 0 0', paddingLeft: 16 }}>
                {turn.conditions.map((c, i) => <li key={i}>{c}</li>)}
              </ul>
            </div>
          )}
          {isVeto && turn.content && (
            <div data-cy="planning-veto-reason" className="veto-block">
              <strong>Blocked by {labelRole(turn.role)}:</strong> {turn.content}
            </div>
          )}
          {turn.annotations?.length > 0 && (
            <div style={{ marginTop: 6, display: 'flex', flexWrap: 'wrap', gap: 4 }}>
              {turn.annotations.map((a, i) => (
                <span
                  key={i}
                  className={`badge ${a.severity === 'blocker' ? 'badge-error' : a.severity === 'warning' ? 'badge-warn' : 'badge-dim'}`}
                  style={{ fontSize: 10 }}
                  title={a.category}
                >
                  {a.text}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// PlanningVerdictCard
// ---------------------------------------------------------------------------
function PlanningVerdictCard({ round }) {
  if (!round || round.status === 'open') return null;

  const isApproved = round.status === 'approved';
  const isVetoed   = round.status === 'vetoed';

  // Find the veto turn if any
  const vetoTurn = isVetoed
    ? (round.turns || []).find(t => t.verdict === 'vetoed')
    : null;

  // Find condition turns
  const conditionTurns = isApproved
    ? (round.turns || []).filter(t => t.verdict === 'approved_with_conditions' && t.conditions?.length > 0)
    : [];

  return (
    <div
      data-cy="planning-verdict"
      className={`verdict-card ${isApproved ? 'verdict-approved' : isVetoed ? 'verdict-vetoed' : ''}`}
    >
      <div className="verdict-card-header">
        <span>{isApproved ? '✓' : isVetoed ? '✕' : '○'}</span>
        <span>{labelPlanningStatus(round.status)}</span>
        {round.locked_at && (
          <span style={{ fontSize: 11, color: 'var(--text-muted)', marginLeft: 'auto', fontWeight: 400 }}>
            {new Date(round.locked_at).toLocaleString()}
          </span>
        )}
      </div>

      {isApproved && round.approved_plan && (
        <p style={{ margin: '4px 0 0', fontSize: 12, color: 'var(--text-secondary)', lineHeight: 1.55 }}>
          {round.approved_plan.slice(0, 200)}{round.approved_plan.length > 200 ? '…' : ''}
        </p>
      )}

      {conditionTurns.length > 0 && (
        <div className="condition-block" style={{ marginTop: 8 }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: 'var(--warning)', marginBottom: 4 }}>
            Conditions to address before execution:
          </div>
          {conditionTurns.flatMap(t => t.conditions).map((c, i) => (
            <div key={i} style={{ fontSize: 12, color: 'var(--text-secondary)', paddingLeft: 10, borderLeft: '2px solid var(--warning-muted)', marginBottom: 3 }}>
              {c}
            </div>
          ))}
        </div>
      )}

      {vetoTurn && (
        <div className="veto-block">
          <strong>Blocked by {labelRole(vetoTurn.role)}:</strong> {vetoTurn.content}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// NextActionPanel
// ---------------------------------------------------------------------------
function NextActionPanel({ scope, round, itemStatus, onAction }) {
  const actions = planningNextActions(scope, round, itemStatus);
  if (actions.length === 0) return null;

  return (
    <div data-cy="planning-next-action" className="next-action-bar">
      {actions.map(({ label, action, variant }) => (
        <button
          key={action}
          className={`btn btn-${variant} btn-sm`}
          onClick={() => onAction(action)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// PlanningPanel — main export
// ---------------------------------------------------------------------------
export default function PlanningPanel({
  scope,
  planningRound,
  loading = false,
  error = '',
  onAction = () => {},
  onRefresh = () => {},
  itemStatus = '',
}) {
  // Determine what's available
  const path = scope?.path;
  const hasRound = !!planningRound;
  const isFastPath = path === 'fast_path';

  // Fast path: still show the badge but no panel
  if (isFastPath && !hasRound) return null;

  // No planning data at all — planning mode not applicable or not loaded yet
  if (!scope && !loading && !error) return null;

  return (
    <section data-cy="planning-panel" className="planning-panel">
      {/* ── Panel header ── */}
      <div className="planning-panel-header">
        <span className="planning-panel-title">Structured Planning Mode</span>

        {path && <ExecutionPathBadge path={path} />}

        {planningRound?.status && (
          <span
            data-cy="planning-status"
            className={`badge ${planningStatusBadgeClass(planningRound.status)}`}
          >
            {labelPlanningStatus(planningRound.status)}
          </span>
        )}

        {!planningRound && !loading && (
          <span data-cy="planning-status" className="badge badge-dim">Not started</span>
        )}

        <button
          data-cy="planning-refresh-btn"
          className="btn btn-ghost btn-xs"
          onClick={onRefresh}
          disabled={loading}
          style={{ marginLeft: 'auto' }}
          aria-label="Refresh planning status"
        >
          {loading ? '…' : '↺'}
        </button>
      </div>

      {/* ── Body ── */}
      <div className="planning-panel-body">
        {error && (
          <div className="error-state" style={{ fontSize: 12 }}>
            ⚠ {error}
            <button className="btn btn-ghost btn-xs" onClick={onRefresh}>Retry</button>
          </div>
        )}

        {loading && !error && (
          <div className="loading-state" style={{ fontSize: 12 }}>
            <div className="spinner" style={{ width: 16, height: 16 }} />
            Loading planning summary…
          </div>
        )}

        {/* Route reason */}
        {scope && !isFastPath && (
          <RouteReasonCard scope={scope} />
        )}

        {/* Verdict card (if terminal) */}
        {planningRound && (
          <PlanningVerdictCard round={planningRound} />
        )}

        {/* Participant roster */}
        {(hasRound || scope) && (
          <div>
          <div className="section-label">Review Participants</div>
            <div className="participants-list">
              {PLANNING_ROLE_ORDER.map((role) => {
                const turn = (planningRound?.turns ?? []).find(t => t.role === role);
                const isActive =
                  planningRound?.status === 'open' &&
                  planningRound?.next_expected_role === role;
                return (
                  <PlanningTurnRow
                    key={role}
                    role={role}
                    turn={turn ?? null}
                    isActive={isActive}
                  />
                );
              })}
            </div>
          </div>
        )}

        {/* Next action */}
        <NextActionPanel
          scope={scope}
          round={planningRound}
          itemStatus={itemStatus}
          onAction={onAction}
        />
      </div>
    </section>
  );
}
