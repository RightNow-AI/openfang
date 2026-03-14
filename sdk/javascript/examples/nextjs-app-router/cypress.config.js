const { defineConfig } = require('cypress');

module.exports = defineConfig({
  e2e: {
    // Next.js dev server
    baseUrl: 'http://localhost:3002',
    specPattern: 'cypress/e2e/**/*.cy.{js,jsx}',
    supportFile: 'cypress/support/e2e.js',

    viewportWidth: 1400,
    viewportHeight: 900,

    // Give SSR pages and API calls enough time
    defaultCommandTimeout: 12000,
    requestTimeout: 20000,
    responseTimeout: 20000,

    video: false,
    screenshotOnRunFailure: true,

    env: {
      // Rust API base — client components fetch directly from here
      API_BASE: 'http://127.0.0.1:50051',
    },

    setupNodeEvents(on) {
      // Log task for test output readability
      on('task', {
        log(message) {
          console.log(message);
          return null;
        },
      });
    },
  },
});
