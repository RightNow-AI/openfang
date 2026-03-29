<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# Alpine.js Dashboard SPA

## Purpose
Web-based dashboard for OpenFang Agent OS. Single-page application (SPA) using Alpine.js that displays agent status, chat interface, monitoring, logs, settings, and workflow builder. Served by the API server.

## Key Files
| File | Purpose |
|------|---------|
| `index_body.html` | Main HTML structure: sidebar navigation, theme switcher, page container, auth prompts |
| `index_head.html` | Head metadata: title, favicon, manifest, viewport |
| `css/theme.css` | Color themes (light/dark/system), CSS variables |
| `css/layout.css` | Page layout: grid, sidebar, responsive mobile |
| `css/components.css` | UI components: buttons, inputs, cards, modals |
| `js/api.js` | HTTP client, WebSocket manager, auth injection, toast notifications |
| `js/katex.js` | Math rendering library |
| `vendor/` | Alpine.js, Chart.js, Monaco Editor, other dependencies |

## Subdirectories
| Directory | Purpose |
|-----------|---------|
| `js/pages/` | Page controllers (agents, chat, overview, logs, settings, etc.) |
| `css/` | Stylesheets (theme, layout, components) |
| `vendor/` | Third-party libraries (Alpine.js, Chart.js, Monaco Editor) |

## For AI Agents
When modifying the dashboard:
- New pages require both HTML section in `index_body.html` and JS controller in `js/pages/`
- CSS changes go in appropriate `css/` file (theme for colors, layout for structure, components for widgets)
- Always maintain responsive design (mobile-first breakpoints)
- Alpine.js directives (`x-data`, `x-if`, `x-for`, `@click`, etc.) drive interactivity
- API calls use `OpenFangToast` for notifications and `api.js` for fetch/WS management
- Test HTML changes by building release binary and accessing `http://127.0.0.1:4200/`
