// cypress/support/e2e.js
// Entry point loaded before every Cypress test.

import './commands';

// ─── Global uncaught-exception handler ───────────────────────────────────────
// By default, any uncaught JS exception in the app will fail the test.
// We intentionally keep this strict but allow React hydration
// mismatches that Next.js emits in dev mode as warnings.
Cypress.on('uncaught:exception', (err) => {
  // Re-throw everything EXCEPT known benign Next.js dev-mode messages.
  const benign =
    err.message.includes('Hydration failed') ||
    err.message.includes('There was an error while hydrating') ||
    // Next.js RSC refresh in dev
    err.message.includes('Router.replace') ||
    err.message.includes('NEXT_NOT_FOUND');

  if (benign) return false; // suppress — do not fail the test
  throw err;              // fail the test on real runtime crashes
});

// ─── Console error monitoring ─────────────────────────────────────────────────
// Tests can call cy.verifyNoConsoleErrors() after page interactions.
// We attach the collector here rather than inside each command so it works
// across navigations within a single test.
before(() => {
  cy.on('window:before:load', (win) => {
    win.__consoleErrors = [];
    const origError = win.console.error.bind(win.console);
    win.console.error = (...args) => {
      win.__consoleErrors.push(args.join(' '));
      origError(...args);
    };
  });
});
