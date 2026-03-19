/** @type {import('tailwindcss').Config} */
module.exports = {
  darkMode: ["class", '[data-theme="dark"]'],
  content: [
    "./app/**/*.{js,ts,jsx,tsx,mdx}",
    "./components/**/*.{js,ts,jsx,tsx,mdx}",
    "./lib/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  corePlugins: {
    // Keep Tailwind from resetting existing hand-rolled CSS
    preflight: false,
  },
  theme: {
    extend: {
      borderRadius: {
        sm: "var(--radius-sm)",
        md: "var(--radius-md)",
        lg: "var(--radius-lg)",
      },
      boxShadow: {
        sm: "var(--shadow-sm)",
        md: "var(--shadow-md)",
        lg: "var(--shadow-md)",
      },
      colors: {
        bg:            "rgb(var(--tw-bg) / <alpha-value>)",
        "bg-elevated": "rgb(var(--tw-bg-elevated) / <alpha-value>)",
        "bg-sidebar":  "rgb(var(--tw-bg-sidebar) / <alpha-value>)",
        "bg-card":     "rgb(var(--tw-bg-card) / <alpha-value>)",
        "bg-muted":    "rgb(var(--tw-bg-muted) / <alpha-value>)",
        "bg-subtle":   "rgb(var(--tw-bg-subtle) / <alpha-value>)",

        text:           "rgb(var(--tw-text) / <alpha-value>)",
        "text-soft":    "rgb(var(--tw-text-soft) / <alpha-value>)",
        "text-muted":   "rgb(var(--tw-text-muted) / <alpha-value>)",
        "text-inverse": "rgb(var(--tw-text-inverse) / <alpha-value>)",

        border:          "rgb(var(--tw-border) / <alpha-value>)",
        "border-strong": "rgb(var(--tw-border-strong) / <alpha-value>)",

        brand:          "rgb(var(--tw-brand) / <alpha-value>)",
        "brand-hover":  "rgb(var(--tw-brand-hover) / <alpha-value>)",
        "brand-active": "rgb(var(--tw-brand-active) / <alpha-value>)",
        "brand-soft":   "rgb(var(--tw-brand-soft) / <alpha-value>)",
        "brand-soft-2": "rgb(var(--tw-brand-soft-2) / <alpha-value>)",

        black:       "rgb(var(--tw-black) / <alpha-value>)",
        "black-soft":"rgb(var(--tw-black-soft) / <alpha-value>)",

        success:       "rgb(var(--tw-success) / <alpha-value>)",
        "success-soft":"rgb(var(--tw-success-soft) / <alpha-value>)",
        warning:       "rgb(var(--tw-warning) / <alpha-value>)",
        "warning-soft":"rgb(var(--tw-warning-soft) / <alpha-value>)",
        danger:        "rgb(var(--tw-danger) / <alpha-value>)",
        "danger-soft": "rgb(var(--tw-danger-soft) / <alpha-value>)",
        info:          "rgb(var(--tw-info) / <alpha-value>)",
        "info-soft":   "rgb(var(--tw-info-soft) / <alpha-value>)",
      },
      ringColor: {
        brand: "rgb(var(--tw-brand) / 0.45)",
      },
    },
  },
  plugins: [],
};
