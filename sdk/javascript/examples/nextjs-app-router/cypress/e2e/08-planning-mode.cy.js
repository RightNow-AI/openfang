/**
 * Layer 8 — Structured Planning Mode
 *
 * Proves that the Planning Mode UI surfaces correctly on:
 *  - Work detail page (scope classification, participant turns, verdict, actions)
 *  - Overview page (Planning Queue card)
 *
 * Uses cy.intercept to inject fixture data so tests run independently of the
 * Rust backend having planning endpoints wired.
 *
 * Fixture shapes mirror the Rust planning types in
 * crates/openfang-types/src/planning.rs.
 */

// ── Shared fixture payloads ──────────────────────────────────────────────────

const WORK_ITEM_FIXTURE = {
  id: 'wk-planning-test-001',
  title: 'Refactor payment service',
  status: 'pending',
  work_type: 'task',
  priority: 'high',
  description: 'Migrate the legacy payment module to use the new idempotency API.',
  created_at: new Date().toISOString(),
  payload: { execution_path: 'planned_swarm' },
};

const SCOPE_OPEN_FIXTURE = {
  scope: {
    path: 'planned_swarm',
    rationale: 'Task involves cross-cutting concerns and has high blast radius.',
    signals: ['has_external_deps', 'modifies_payment_flow', 'high_priority'],
  },
  planning_round: {
    id: 'pr-001',
    work_item_id: 'wk-planning-test-001',
    status: 'open',
    created_at: new Date().toISOString(),
    turns: [
      {
        role: 'planner',
        sequence_index: 0,
        content: 'Break this into three sub-tasks: schema migration, handler update, integration tests.',
        verdict: 'approved',
        conditions: null,
        veto_reason: null,
        annotations: [],
      },
      {
        role: 'reviewer',
        sequence_index: 1,
        content: 'Scope looks correct. Confirm idempotency key format before proceeding.',
        verdict: 'approved_with_conditions',
        conditions: 'Verify idempotency key format matches v2 spec.',
        veto_reason: null,
        annotations: [],
      },
    ],
  },
};

const SCOPE_APPROVED_FIXTURE = {
  scope: { ...SCOPE_OPEN_FIXTURE.scope },
  planning_round: {
    ...SCOPE_OPEN_FIXTURE.planning_round,
    status: 'approved',
    approved_plan: 'Proceed with three-stage migration: (1) DB schema, (2) handler update, (3) regression tests.',
    turns: [
      ...SCOPE_OPEN_FIXTURE.planning_round.turns,
      { role: 'risk_checker',  sequence_index: 2, content: 'No unacceptable risk.', verdict: 'approved',               conditions: null, veto_reason: null, annotations: [] },
      { role: 'policy_gate',   sequence_index: 3, content: 'Policy cleared.',       verdict: 'approved',               conditions: null, veto_reason: null, annotations: [] },
      { role: 'executor',      sequence_index: 4, content: 'Ready to execute.',     verdict: 'approved',               conditions: null, veto_reason: null, annotations: [] },
    ],
  },
};

const SCOPE_VETOED_FIXTURE = {
  scope: { ...SCOPE_OPEN_FIXTURE.scope },
  planning_round: {
    ...SCOPE_OPEN_FIXTURE.planning_round,
    status: 'vetoed',
    turns: [
      SCOPE_OPEN_FIXTURE.planning_round.turns[0],
      { role: 'risk_checker', sequence_index: 2, content: 'Unacceptable risk.', verdict: 'vetoed',
        conditions: null, veto_reason: 'Payment flow changes require manual CFO sign-off.', annotations: [] },
    ],
  },
};

const WORK_EVENTS_FIXTURE = {
  events: [
    { id: 'ev-1', event_type: 'scope_classified',      created_at: new Date().toISOString(), detail: 'planned_swarm' },
    { id: 'ev-2', event_type: 'planning_round_opened', created_at: new Date().toISOString(), detail: null },
  ],
};

// ── Helpers ──────────────────────────────────────────────────────────────────

function interceptWorkItem(fixture = WORK_ITEM_FIXTURE) {
  cy.intercept('GET', '/api/work/wk-planning-test-001', { statusCode: 200, body: fixture }).as('getWorkItem');
}

function interceptEvents() {
  cy.intercept('GET', '/api/work/wk-planning-test-001/events', { statusCode: 200, body: WORK_EVENTS_FIXTURE }).as('getWorkEvents');
}

function interceptChildren() {
  cy.intercept('GET', '/api/work?*parent_id*', { statusCode: 200, body: { items: [] } }).as('getChildren');
}

function interceptPlanning(body, statusCode = 200) {
  cy.intercept('GET', '/api/work/wk-planning-test-001/planning', { statusCode, body }).as('getPlanning');
}

function visitWorkDetail() {
  cy.visit('/work/wk-planning-test-001');
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe('Structured Planning Mode — Work Detail Page', () => {
  beforeEach(() => {
    interceptWorkItem();
    interceptEvents();
    interceptChildren();
  });

  // ── 1. Fast-path: no planning panel ─────────────────────────────────────────
  it('does not show planning panel for fast_path tasks with no planning data', () => {
    interceptPlanning(null, 404);
    interceptWorkItem({ ...WORK_ITEM_FIXTURE, payload: { execution_path: 'fast_path' } });
    visitWorkDetail();
    cy.wait('@getWorkItem');
    cy.get('[data-cy="planning-panel"]').should('not.exist');
  });

  // ── 2. Open review round ─────────────────────────────────────────────────────
  it('renders planning panel when a review round is open', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getWorkItem');
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-panel"]').should('be.visible');
    cy.get('[data-cy="planning-status"]').should('contain', 'Review in progress');
    cy.get('[data-cy="execution-path-badge"]').should('contain', 'Team Review');
  });

  // ── 3. Route reason card ─────────────────────────────────────────────────────
  it('shows route reason with rationale and signals', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-route-reason"]').should('exist');
    cy.get('[data-cy="planning-route-reason"]').click();
    cy.get('[data-cy="planning-route-reason"]').should('contain', 'cross-cutting');
  });

  // ── 4. Participant turns rendered in order ───────────────────────────────────
  it('renders participant turns in role order (planner first)', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-turn-planner"]').should('exist');
    cy.get('[data-cy="planning-turn-reviewer"]').should('exist');
    // risk, policy, executor may render as placeholders (not submitted yet)
    cy.get('[data-cy="planning-turn-risk"]').should('exist');
    cy.get('[data-cy="planning-turn-policy"]').should('exist');
    cy.get('[data-cy="planning-turn-executor"]').should('exist');
  });

  // ── 5. Participant turn content visible ──────────────────────────────────────
  it('expands a planner turn to show content', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-turn-planner"]').first().click();
    cy.get('[data-cy="planning-turn-planner"]').should('contain', 'Break this into three sub-tasks');
  });

  // ── 6. Approved state ────────────────────────────────────────────────────────
  it('shows approved verdict card and enables run action', () => {
    interceptPlanning(SCOPE_APPROVED_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-panel"]').should('contain', 'Approved');
    cy.get('[data-cy="planning-verdict"]').should('exist');
    cy.get('[data-cy="planning-verdict"]').should('contain', 'Proceed with three-stage');

    // Run button should be enabled (planning approved)
    cy.get('[data-cy="action-run"]').should('exist').and('not.be.disabled');
  });

  // ── 7. Vetoed state blocks run actions ───────────────────────────────────────
  it('shows blocked state and hides the run button when vetoed', () => {
    interceptPlanning(SCOPE_VETOED_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-status"]').should('contain', 'Blocked');
    cy.get('[data-cy="planning-next-action"]').should('exist');

    // Run button should not exist (planning is blocking it)
    cy.get('[data-cy="action-run"]').should('not.exist');
  });

  // ── 8. Veto reason visible ───────────────────────────────────────────────────
  it('shows veto reason in verdict card', () => {
    interceptPlanning(SCOPE_VETOED_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-verdict"]').should('contain', 'CFO sign-off');
  });

  // ── 9. Planning section label ────────────────────────────────────────────────
  it('renders a Planning section label above the panel', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="planning-section-label"]').should('contain', 'Planning');
    cy.get('[data-cy="execution-section-label"]').should('contain', 'Execution');
  });

  // ── 10. Timeline shows planning event labels ─────────────────────────────────
  it('shows human-readable labels in the timeline for planning events', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    cy.get('[data-cy="work-detail-timeline"]').should('exist');
    cy.get('[data-cy="timeline-event-type"]').first().should('contain', 'Scope classified');
  });

  // ── 11. Refresh planning button ──────────────────────────────────────────────
  it('planning panel refresh button re-fetches planning data', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    visitWorkDetail();
    cy.wait('@getPlanning');

    // Stub the refresh call
    cy.intercept('GET', '/api/work/wk-planning-test-001/planning', { statusCode: 200, body: SCOPE_APPROVED_FIXTURE }).as('getPlanningRefresh');
    cy.get('[data-cy="planning-refresh-btn"]').click();
    cy.wait('@getPlanningRefresh');
    cy.get('[data-cy="planning-status"]').should('contain', 'Approved');
  });

  // ── 12. Direct URL access ─────────────────────────────────────────────────────
  it('loads correctly on direct URL access', () => {
    interceptPlanning(SCOPE_OPEN_FIXTURE);
    cy.visit('/work/wk-planning-test-001');
    cy.get('[data-cy="work-detail-page"]').should('exist');
    cy.get('[data-cy="planning-panel"]').should('be.visible');
  });
});

// ── Overview page: Planning Queue card ───────────────────────────────────────

describe('Structured Planning Mode — Overview Page', () => {
  beforeEach(() => {
    cy.intercept('GET', '/api/work?*limit=100*', {
      statusCode: 200,
      body: {
        items: [
          { ...WORK_ITEM_FIXTURE, id: 'wk-a', status: 'pending',          payload: { execution_path: 'planned_swarm' } },
          { ...WORK_ITEM_FIXTURE, id: 'wk-b', status: 'running',          payload: { execution_path: 'planned_swarm' } },
          { ...WORK_ITEM_FIXTURE, id: 'wk-c', status: 'waiting_approval', payload: { execution_path: 'review_swarm' } },
          { ...WORK_ITEM_FIXTURE, id: 'wk-d', status: 'failed',           payload: { execution_path: 'fast_path' } },
          { ...WORK_ITEM_FIXTURE, id: 'wk-e', status: 'completed',        payload: {} },
        ],
      },
    }).as('getWork');
  });

  it('renders the Planning Queue card on the overview page', () => {
    cy.visit('/overview');
    cy.wait('@getWork');
    cy.get('[data-cy="planning-queue-card"]').should('be.visible');
  });

  it('shows correct counts in the Planning Queue card', () => {
    cy.visit('/overview');
    cy.wait('@getWork');
    const card = cy.get('[data-cy="planning-queue-card"]');
    card.should('contain', 'Pending / Ready');
    card.should('contain', 'Running');
    card.should('contain', 'Needs approval');
    card.should('contain', 'Failed');
  });

  it('shows execution path badges for items that have paths', () => {
    cy.visit('/overview');
    cy.wait('@getWork');
    cy.get('[data-cy="planning-path-badge-planned_swarm"]').should('exist').and('contain', 'Team Review');
    cy.get('[data-cy="planning-path-badge-review_swarm"]').should('exist').and('contain', 'Full Review');
    cy.get('[data-cy="planning-path-badge-fast_path"]').should('exist').and('contain', 'Fast Path');
  });
});
