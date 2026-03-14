// cypress/support/commands.js
// Custom Cypress commands used across all test suites.

const API = () => Cypress.env('API_BASE'); // 'http://127.0.0.1:50051'

// ─────────────────────────────────────────────────────────────────────────────
// Navigation helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Visit a page and assert the shell (sidebar) is intact.
 * Fails if the page produces a blank body or missing root element.
 *
 * @param {string} path - e.g. '/chat'
 * @param {string} rootCy - data-cy selector for the page root, e.g. 'chat-page'
 */
Cypress.Commands.add('visitPage', (path, rootCy) => {
  cy.visit(path);
  cy.get('[data-cy="sidebar"]', { timeout: 12000 }).should('be.visible');
  cy.get(`[data-cy="${rootCy}"]`, { timeout: 12000 }).should('exist');
  cy.get('h1').should('exist');
  cy.get('body').should('not.be.empty');
});

/**
 * Click a sidebar nav link by its data-cy ID and assert the
 * correct page root renders without crashing.
 *
 * @param {string} navCy  - e.g. 'nav-link-chat'
 * @param {string} href   - e.g. '/chat'
 * @param {string} rootCy - data-cy for the page root
 */
Cypress.Commands.add('navigateSidebar', (navCy, href, rootCy) => {
  cy.get(`[data-cy="${navCy}"]`).click();
  cy.location('pathname').should('eq', href);
  cy.get(`[data-cy="${rootCy}"]`, { timeout: 12000 }).should('exist');
  cy.get('[data-cy="sidebar"]').should('be.visible');
});

// ─────────────────────────────────────────────────────────────────────────────
// API intercept helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Intercept a GET request to the Rust API and optionally stub the response.
 * Returns the alias string.
 *
 * @param {string}  apiPath  - path relative to API_BASE, e.g. '/api/hands'
 * @param {*}       [body]   - optional stub response body
 * @param {number}  [status] - optional HTTP status (default 200)
 * @returns {string} alias (without @)
 */
Cypress.Commands.add('interceptGet', (apiPath, body, status = 200) => {
  const alias = `GET_${apiPath.replace(/\W/g, '_')}`;
  const url = `${API()}${apiPath}`;
  if (body !== undefined) {
    cy.intercept('GET', url, { statusCode: status, body }).as(alias);
  } else {
    cy.intercept('GET', url).as(alias);
  }
  return cy.wrap(alias);
});

/**
 * Intercept a POST request to the Rust API and optionally stub the response.
 */
Cypress.Commands.add('interceptPost', (apiPath, body, status = 200) => {
  const alias = `POST_${apiPath.replace(/\W/g, '_')}`;
  const url = `${API()}${apiPath}`;
  if (body !== undefined) {
    cy.intercept('POST', url, { statusCode: status, body }).as(alias);
  } else {
    cy.intercept('POST', url).as(alias);
  }
  return cy.wrap(alias);
});

/**
 * Intercept a PUT request to the Rust API and optionally stub the response.
 */
Cypress.Commands.add('interceptPut', (apiPath, body, status = 200) => {
  const alias = `PUT_${apiPath.replace(/\W/g, '_')}`;
  const url = `${API()}${apiPath}`;
  if (body !== undefined) {
    cy.intercept('PUT', url, { statusCode: status, body }).as(alias);
  } else {
    cy.intercept('PUT', url).as(alias);
  }
  return cy.wrap(alias);
});

/**
 * Intercept a DELETE request to the Rust API and optionally stub the response.
 */
Cypress.Commands.add('interceptDelete', (apiPath, body, status = 200) => {
  const alias = `DELETE_${apiPath.replace(/\W/g, '_')}`;
  const url = `${API()}${apiPath}`;
  if (body !== undefined) {
    cy.intercept('DELETE', url, { statusCode: status, body }).as(alias);
  } else {
    cy.intercept('DELETE', url).as(alias);
  }
  return cy.wrap(alias);
});

/**
 * Force an API endpoint to return a 500 Internal Server Error.
 * Used in failure-behavior tests to trigger error states.
 *
 * @param {string} method - 'GET' | 'POST' | 'PUT' | 'DELETE'
 * @param {string} apiPath - path relative to API_BASE
 * @param {string} [alias] - optional alias; defaults to auto-generated
 */
Cypress.Commands.add('forceApiError', (method, apiPath, alias) => {
  const a = alias || `ERR_${method}_${apiPath.replace(/\W/g, '_')}`;
  const url = `${API()}${apiPath}`;
  cy.intercept(method.toUpperCase(), url, {
    statusCode: 500,
    body: { error: 'Simulated backend failure' },
  }).as(a);
  return cy.wrap(a);
});

// ─────────────────────────────────────────────────────────────────────────────
// Assertion helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Assert that the page has no blocking console errors.
 * Skips known benign messages (React strict-mode, Next.js dev warnings).
 */
Cypress.Commands.add('verifyNoConsoleErrors', () => {
  cy.window().then((win) => {
    const errors = (win.__consoleErrors || []).filter((msg) => {
      // Filter out known benign dev messages
      return (
        !msg.includes('Warning:') &&
        !msg.includes('act(') &&
        !msg.includes('ReactDOM.render') &&
        !msg.includes('ResizeObserver loop') &&
        !msg.includes('NEXT_NOT_FOUND')
      );
    });
    if (errors.length > 0) {
      throw new Error(`Console errors detected:\n${errors.join('\n')}`);
    }
  });
});

/**
 * Assert that a data-cy element is visible and contains non-empty text.
 */
Cypress.Commands.add('assertVisible', (dataCy) => {
  cy.get(`[data-cy="${dataCy}"]`).should('be.visible').and('not.be.empty');
});

/**
 * Click the page's "Refresh" button (btn-ghost containing "Refresh" text)
 * and wait for loading to settle.
 */
Cypress.Commands.add('clickRefresh', () => {
  cy.contains('button', /refresh/i).click();
});
