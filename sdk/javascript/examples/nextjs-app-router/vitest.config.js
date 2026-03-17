import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { transformWithOxc } from 'vite';

/**
 * Vite 8 + OXC: by default, `lang` is determined from the file extension.
 * `.js` → lang="js" (no JSX parsing). Next.js uses `.js` for React components,
 * so we must pre-transform them with `lang: "jsx"` before OXC sees them.
 */
let resolvedViteConfig;
const jsxInJs = {
  name: 'jsx-in-js',
  enforce: 'pre',
  configResolved(cfg) {
    resolvedViteConfig = cfg;
  },
  async transform(code, id) {
    // Only handle .js files outside node_modules that likely contain JSX
    if (!id.endsWith('.js') || id.includes('node_modules')) return;
    // Quick heuristic: Next.js React files start with 'use client' or import react
    if (!code.includes("'use client'") && !code.includes('"use client"') &&
        !code.includes("from 'react'") && !code.includes('from "react"')) return;
    return transformWithOxc(code, id, {
      lang: 'jsx',
      jsx: { runtime: 'automatic', importSource: 'react' },
      sourcemap: true,
    }, undefined, resolvedViteConfig);
  },
};

export default defineConfig({
  plugins: [jsxInJs, react()],
  // Vite 8 OXC defaults: include=/\.(m?ts|[jt]sx)$/ exclude=/\.js$/
  // Our jsxInJs plugin handles .js files before OXC; still include .jsx/.tsx:
  oxc: {
    include: /\.(m?[jt]sx?)$/,
    exclude: /node_modules/,
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./vitest.setup.js'],
    include: ['**/__tests__/**/*.test.{js,jsx}'],
    exclude: ['node_modules', '.next', 'cypress'],
  },
});
