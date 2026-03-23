import nextCoreWebVitals from "eslint-config-next/core-web-vitals";
import nextTypescript from "eslint-config-next/typescript";
import reactHooks from "eslint-plugin-react-hooks";

const eslintConfig = [
  ...nextCoreWebVitals,
  ...nextTypescript,
  // CommonJS lib files legitimately use require() — exempt them from the TS rule
  {
    files: ["lib/**/*.js", "*.config.js", "vitest.setup.js"],
    rules: {
      "@typescript-eslint/no-require-imports": "off",
    },
  },
  // Downgrade React Compiler's setState-in-effect to a warning;
  // the pattern (setLoading(true) before async fetch) is intentional.
  {
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      "react-hooks/set-state-in-effect": "warn",
    },
  },
];

export default eslintConfig;
