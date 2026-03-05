# OpenFang "Apple Intelligence" UI Overhaul — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the OpenFang dashboard from a traditional sidebar-based developer tool into a premium glassmorphic UI with floating dock navigation, ambient mesh gradient, and Apple Intelligence-inspired aesthetics.

**Architecture:** Pure CSS/HTML/JS overhaul — no backend changes, no build step, no new frameworks. Three CSS files get rewritten (theme, layout, components), HTML restructured (sidebar replaced with dock + command palette), and app.js updated for new navigation.

**Tech Stack:** Alpine.js (existing), vanilla CSS with CSS custom properties, backdrop-filter for glass effects

**Design doc:** `docs/plans/2026-03-05-ui-overhaul-design.md`

---

## Task 1: Create noise.svg asset

**Files:**
- Create: `crates/openfang-api/static/noise.svg`

**Step 1: Create the SVG noise texture**

This is a tiny SVG with feTurbulence that prevents gradient banding on glass panels.

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="200" height="200">
  <filter id="n">
    <feTurbulence type="fractalNoise" baseFrequency="0.75" numOctaves="4" stitchTiles="stitch"/>
    <feColorMatrix type="saturate" values="0"/>
  </filter>
  <rect width="100%" height="100%" filter="url(#n)" opacity="0.04"/>
</svg>
```

**Step 2: Verify it loads**

Open `http://127.0.0.1:50051/noise.svg` in browser — should see a subtle grey noise pattern.

**Step 3: Commit**

```bash
git add crates/openfang-api/static/noise.svg
git commit -m "feat(ui): add noise.svg texture for glass banding prevention"
```

---

## Task 2: Rewrite theme.css — tokens, mesh gradient, glass materials

**Files:**
- Rewrite: `crates/openfang-api/static/css/theme.css` (currently 277 lines)

This is the foundation — everything else depends on these tokens.

**Step 1: Rewrite theme.css**

Replace the entire file. Key changes:
- Add glass material tokens (3 levels: surface, chrome, overlay)
- Add mesh gradient tokens (3 blob colors per theme)
- Add typography tokens (metric, heading, body, label, caption)
- Add new radius scale (20px cards, 12px buttons, 10px badges)
- Keep existing status colors (success, error, warning, info) — they work well
- Keep existing font imports (Inter + Geist Mono) — defined in index_head.html
- Add `font-variant-numeric: tabular-nums` for numeric elements
- Remove all `text-transform: uppercase` defaults
- Add mesh gradient as `body::before` pseudo-element with 90s animation
- Add `@media (prefers-reduced-motion: reduce)` to stop mesh animation
- Mesh gradient uses `position: fixed; z-index: -1` with `transform: translate3d()` animation (GPU composited)

**New token structure:**

```css
:root, [data-theme="light"] {
  /* Existing colors (keep) */
  --accent: #FF5C00;
  --success: #22C55E;
  --error: #EF4444;
  --warning: #F59E0B;
  --info: #3B82F6;

  /* Glass materials — NEW */
  --glass-surface: rgba(255, 255, 255, 0.82);
  --glass-chrome: rgba(255, 255, 255, 0.58);
  --glass-overlay: rgba(255, 255, 255, 0.52);
  --glass-border: rgba(255, 255, 255, 0.18);
  --glass-edge: rgba(255, 255, 255, 0.25);
  --glass-blur-surface: 12px;
  --glass-blur-chrome: 24px;
  --glass-blur-overlay: 40px;
  --glass-scrim: rgba(255, 255, 255, 0.92);  /* for data regions */

  /* Mesh gradient blobs */
  --mesh-1: #FFD6B0;
  --mesh-2: #FFECD2;
  --mesh-3: #E8D5F5;

  /* Text hierarchy (keep similar values, add tokens) */
  --text: #1A1817;
  --text-secondary: #3D3935;
  --text-dim: #6B6560;
  --text-muted: #9A958F;

  /* Typography scale — NEW */
  --type-metric: 28px;
  --type-heading: 18px;
  --type-body: 15px;
  --type-label: 12px;
  --type-caption: 11px;

  /* Radius — larger for premium feel */
  --radius-xs: 6px;
  --radius-sm: 10px;
  --radius-md: 12px;
  --radius-lg: 20px;
  --radius-xl: 24px;
  --radius-pill: 999px;

  /* Layout */
  --dock-height: 64px;
  --header-height: 48px;
  --content-max: 1200px;
  --content-narrow: 720px;
}

[data-theme="dark"] {
  --glass-surface: rgba(30, 28, 26, 0.75);
  --glass-chrome: rgba(30, 28, 26, 0.60);
  --glass-overlay: rgba(30, 28, 26, 0.55);
  --glass-border: rgba(255, 255, 255, 0.08);
  --glass-edge: rgba(255, 255, 255, 0.12);
  --glass-scrim: rgba(20, 18, 16, 0.92);

  --mesh-1: #3D2200;
  --mesh-2: #2A1800;
  --mesh-3: #1A1030;

  /* ... dark text/status colors (keep existing values) ... */
}
```

**Mesh gradient animation (add to body::before):**

```css
body::before {
  content: '';
  position: fixed;
  inset: -50%;
  width: 200%;
  height: 200%;
  z-index: -1;
  background:
    radial-gradient(ellipse 600px 600px at 20% 30%, var(--mesh-1), transparent),
    radial-gradient(ellipse 500px 500px at 70% 60%, var(--mesh-2), transparent),
    radial-gradient(ellipse 400px 400px at 50% 80%, var(--mesh-3), transparent);
  animation: meshDrift 90s ease-in-out infinite alternate;
  will-change: transform;
}

@keyframes meshDrift {
  0%   { transform: translate3d(0, 0, 0) rotate(0deg); }
  33%  { transform: translate3d(5%, -3%, 0) rotate(2deg); }
  66%  { transform: translate3d(-3%, 5%, 0) rotate(-1deg); }
  100% { transform: translate3d(2%, -2%, 0) rotate(1deg); }
}

@media (prefers-reduced-motion: reduce) {
  body::before { animation: none; }
}
```

**Glass material mixins (as classes):**

```css
.glass-surface {
  background: var(--glass-surface);
  backdrop-filter: blur(var(--glass-blur-surface));
  -webkit-backdrop-filter: blur(var(--glass-blur-surface));
  border: 1px solid var(--glass-border);
  border-top-color: var(--glass-edge);
  box-shadow: inset 0 1px 0 rgba(255,255,255,0.06);
}

.glass-chrome {
  background: var(--glass-chrome);
  backdrop-filter: blur(var(--glass-blur-chrome));
  -webkit-backdrop-filter: blur(var(--glass-blur-chrome));
  border: 1px solid var(--glass-border);
  border-top-color: var(--glass-edge);
}

.glass-overlay {
  background: var(--glass-overlay);
  backdrop-filter: blur(var(--glass-blur-overlay));
  -webkit-backdrop-filter: blur(var(--glass-blur-overlay));
  border: 1px solid var(--glass-border);
  border-top-color: var(--glass-edge);
}

/* Noise overlay for glass panels */
.glass-surface::after,
.glass-chrome::after,
.glass-overlay::after {
  content: '';
  position: absolute;
  inset: 0;
  background: url('/noise.svg');
  opacity: 0.03;
  pointer-events: none;
  border-radius: inherit;
}
```

**Step 2: Verify**

Build to check the static file is served: `cargo build --workspace --lib`
Open dashboard in browser — should see mesh gradient background with no UI (layout not updated yet).

**Step 3: Commit**

```bash
git add crates/openfang-api/static/css/theme.css
git commit -m "feat(ui): rewrite theme.css with glass materials, mesh gradient, and typography tokens"
```

---

## Task 3: Rewrite layout.css — dock nav, floating header, centered content

**Files:**
- Rewrite: `crates/openfang-api/static/css/layout.css` (currently 310 lines)

**Step 1: Rewrite layout.css**

Replace entirely. Key changes:
- Remove all sidebar styles (.sidebar, .sidebar-*, .nav-*)
- Add floating dock styles (.dock, .dock-item, .dock-more)
- Add floating page header (.page-header as glass-chrome, border-radius: 16px, margin: 12px 16px 0)
- Add centered content layout (.main-content max-width centered, padding for dock clearance)
- Add command palette styles (.cmd-palette, .cmd-palette-input, .cmd-palette-list)
- Keep responsive breakpoints but update for dock (dock stacks icons tighter on mobile)
- Page body gets `padding-bottom: calc(var(--dock-height) + 24px)` for dock clearance

**New layout structure:**

```css
/* Full-viewport app wrapper — no sidebar, just content */
.app-layout {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  position: relative;
}

/* Floating page header */
.page-header {
  /* glass-chrome applied via class in HTML */
  position: sticky;
  top: 0;
  z-index: 50;
  margin: 12px 16px 0;
  padding: 10px 20px;
  border-radius: var(--radius-xl);
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-height: var(--header-height);
}

.page-header h2 {
  font-size: var(--type-heading);
  font-weight: 600;
  letter-spacing: -0.02em;
}

/* Main content — centered with max-width */
.main-content {
  flex: 1;
  width: 100%;
  max-width: var(--content-max);
  margin: 0 auto;
  padding: 0 24px;
}

.main-content > div {
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

.page-body {
  flex: 1;
  padding: 20px 0;
  padding-bottom: calc(var(--dock-height) + 32px);
}

/* ═══ Floating Dock ═══ */
.dock {
  position: fixed;
  bottom: 16px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px;
  border-radius: var(--radius-xl);
  /* glass-chrome applied via class in HTML */
}

.dock-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  width: 48px;
  height: 48px;
  border-radius: var(--radius-md);
  cursor: pointer;
  color: var(--text-dim);
  transition: all 0.2s var(--ease-spring);
  position: relative;
  border: none;
  background: transparent;
  font-family: var(--font-sans);
}

.dock-item:hover {
  color: var(--text);
  background: var(--glass-border);
  transform: translateY(-2px) scale(1.08);
}

.dock-item.active {
  color: var(--accent);
  background: var(--accent-glow);
}

.dock-item.active::after {
  content: '';
  position: absolute;
  bottom: 2px;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: var(--accent);
  box-shadow: 0 0 6px var(--accent);
}

.dock-item svg {
  width: 22px;
  height: 22px;
  stroke: currentColor;
  fill: none;
  stroke-width: 1.8;
  stroke-linecap: round;
  stroke-linejoin: round;
}

/* Dock tooltip on hover */
.dock-tooltip {
  position: absolute;
  bottom: 100%;
  left: 50%;
  transform: translateX(-50%) translateY(4px);
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  font-size: var(--type-caption);
  font-weight: 500;
  white-space: nowrap;
  opacity: 0;
  pointer-events: none;
  transition: all 0.15s var(--ease-smooth);
  background: var(--glass-overlay);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  color: var(--text);
}

.dock-item:hover .dock-tooltip {
  opacity: 1;
  transform: translateX(-50%) translateY(-4px);
}

/* Dock divider between primary items and "more" */
.dock-divider {
  width: 1px;
  height: 28px;
  background: var(--glass-border);
  margin: 0 4px;
  flex-shrink: 0;
}

/* ═══ Command Palette ═══ */
.cmd-backdrop {
  position: fixed;
  inset: 0;
  z-index: 300;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 20vh;
}

.cmd-palette {
  width: 100%;
  max-width: 520px;
  border-radius: var(--radius-xl);
  overflow: hidden;
  animation: cmdIn 0.2s var(--ease-spring);
  /* glass-overlay applied via class in HTML */
}

@keyframes cmdIn {
  from { opacity: 0; transform: scale(0.96) translateY(-8px); }
  to { opacity: 1; transform: scale(1) translateY(0); }
}

.cmd-input {
  width: 100%;
  padding: 16px 20px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--glass-border);
  color: var(--text);
  font-size: var(--type-body);
  font-family: var(--font-sans);
  outline: none;
}

.cmd-input::placeholder {
  color: var(--text-muted);
}

.cmd-list {
  max-height: 320px;
  overflow-y: auto;
  padding: 8px;
}

.cmd-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: var(--radius-md);
  cursor: pointer;
  color: var(--text-dim);
  font-size: var(--type-label);
  font-weight: 500;
  transition: background 0.1s;
}

.cmd-item:hover, .cmd-item.selected {
  background: var(--glass-border);
  color: var(--text);
}

.cmd-item svg {
  width: 16px;
  height: 16px;
  stroke: currentColor;
  fill: none;
  stroke-width: 2;
  stroke-linecap: round;
  stroke-linejoin: round;
  flex-shrink: 0;
}

.cmd-item .cmd-shortcut {
  margin-left: auto;
  font-size: var(--type-caption);
  color: var(--text-muted);
  font-family: var(--font-mono);
}

/* ═══ Status indicator (top-right floating) ═══ */
.status-float {
  position: fixed;
  top: 16px;
  right: 16px;
  z-index: 150;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  border-radius: var(--radius-pill);
  font-size: var(--type-caption);
  font-weight: 500;
  /* glass-chrome via class */
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
  box-shadow: 0 0 6px currentColor;
}

/* ═══ Theme switcher (floating, top-right) ═══ */
.theme-float {
  position: fixed;
  top: 16px;
  left: 16px;
  z-index: 150;
  display: flex;
  gap: 2px;
  padding: 4px;
  border-radius: var(--radius-pill);
  /* glass-chrome via class */
}

/* ═══ Responsive ═══ */
@media (max-width: 768px) {
  .dock { bottom: 8px; padding: 4px; }
  .dock-item { width: 42px; height: 42px; }
  .page-header { margin: 8px 12px 0; }
  .main-content { padding: 0 12px; }
  .page-body { padding: 16px 0; }
  .cmd-palette { max-width: calc(100vw - 24px); }
  .status-float { top: 8px; right: 8px; font-size: 10px; padding: 4px 10px; }
  .theme-float { top: 8px; left: 8px; }
}

@media (max-width: 480px) {
  .dock-tooltip { display: none; }
  .dock-item { width: 38px; height: 38px; }
  .dock-item svg { width: 18px; height: 18px; }
  .page-header { flex-direction: column; gap: 8px; align-items: flex-start; padding: 10px 16px; }
}

@media (min-width: 1400px) {
  .main-content { max-width: 1400px; }
}

/* Touch targets */
@media (pointer: coarse) {
  .dock-item { min-width: 48px; min-height: 48px; }
  .cmd-item { min-height: 44px; }
}

/* Focus mode — hide dock */
.app-layout.focus-mode .dock { display: none; }
.app-layout.focus-mode .status-float { display: none; }
.app-layout.focus-mode .theme-float { display: none; }

/* Page transition */
.page-enter {
  animation: pageIn 0.25s var(--ease-smooth) both;
}

@keyframes pageIn {
  from { opacity: 0; transform: scale(0.98); }
  to { opacity: 1; transform: scale(1); }
}

/* Print */
@media print {
  .dock, .status-float, .theme-float, .cmd-backdrop { display: none !important; }
  .main-content { max-width: 100%; margin: 0; padding: 0; }
  body::before { display: none; }
}
```

**Step 2: Verify**

`cargo build --workspace --lib` — must compile (checks static file embedding).

**Step 3: Commit**

```bash
git add crates/openfang-api/static/css/layout.css
git commit -m "feat(ui): rewrite layout.css with floating dock, command palette, and centered content"
```

---

## Task 4: Rewrite components.css — glass-styled components

**Files:**
- Rewrite: `crates/openfang-api/static/css/components.css` (currently 3202 lines)

**Step 1: Read the full existing components.css**

Read the entire file in chunks to understand every component that needs restyling:
- Buttons, cards, badges, tables, forms, modals, toggles, toasts
- Chat-specific: messages, input area, agent selector
- Page-specific: stats row, overview grid, agent cards, session table, etc.

**Step 2: Rewrite components.css**

Replace the entire file. Key changes for each component:

**Buttons:**
- `border-radius: var(--radius-md)` (12px)
- `.btn-ghost` gets glass background on hover
- `.btn-primary` keeps accent color, gets `box-shadow: var(--shadow-accent)`
- `:active` scale stays at 0.97

**Cards:**
- `border-radius: var(--radius-lg)` (20px)
- Apply `.glass-surface` properties inline (since cards may not always have the class)
- Hover: lift 4px, edge brightens
- `.card-grid` gap stays 16px, minmax(300px, 1fr)
- Remove card-glow mouse-tracking (replaced by glass material)

**Badges:**
- `border-radius: var(--radius-sm)` (10px)
- Remove `text-transform: uppercase`
- Sentence case, slightly larger font (11px)
- Glass-tinted backgrounds for each status

**Tables:**
- `.table-wrap` gets glass-surface + scrim background for legibility
- `border-radius: var(--radius-lg)` (20px)
- Header row uses glass-chrome style
- Hover rows get subtle glow
- Remove uppercase from `th`, use sentence case + font-weight 600

**Forms:**
- Inputs: `border-radius: var(--radius-md)` (12px), glass background
- Focus: accent glow ring (`box-shadow: 0 0 0 3px var(--accent-glow)`)
- Labels: sentence case, no uppercase

**Modals:**
- Glass overlay material
- `border-radius: var(--radius-xl)` (24px)
- Backdrop gets `backdrop-filter: blur(4px)`
- Entry animation: scale(0.96) fade-in

**Toggles:**
- Pill-switch style (iOS-like)
- Accent color when on
- Smooth slide animation

**Toasts:**
- Top-center position
- Glass pill shape (`border-radius: var(--radius-pill)`)
- Slide-down entry animation

**Chat messages:**
- Agent: glass-surface, `border-radius: 20px 20px 20px 8px`, left-aligned
- User: accent-tinted glass, `border-radius: 20px 20px 8px 20px`, right-aligned
- Avatar: 32px circle with status glow ring
- Code blocks: slightly more opaque glass

**Chat input:**
- Glass-chrome material
- `border-radius: 20px`
- Auto-growing textarea
- Send button: circular, accent, slides in

**Agent selector (pill bar):**
- Horizontal scroll, flex, gap 8px
- Each agent: pill shape, glass-surface
- Active: filled with accent glow

**Stats cards:**
- Glass-surface, `border-radius: 20px`
- Big number: `font-size: var(--type-metric)`, `font-variant-numeric: tabular-nums`
- Sparkline/trend indicator support

**Peek panel (agent detail):**
- Right-side slide-in panel
- Glass-overlay material
- `width: 420px`, `border-radius: 24px 0 0 24px`
- Sections with dividers

**Scrollbars:** Keep thin style, glass-tinted thumb.

**Skeleton loading:** Update to glass-based shimmer.

**Empty states:** Centered, softer styling.

**Step 3: Verify**

`cargo build --workspace --lib`

**Step 4: Commit**

```bash
git add crates/openfang-api/static/css/components.css
git commit -m "feat(ui): rewrite components.css with glass materials and premium styling"
```

---

## Task 5: Restructure index_body.html — dock + command palette + new layout

**Files:**
- Rewrite: `crates/openfang-api/static/index_body.html` (currently 4702 lines)

This is the largest and most delicate task. The file contains all page templates.

**Step 1: Read the full index_body.html**

Read in 300-line chunks to map every page template and its Alpine.js bindings. Critical to preserve all `x-data`, `x-init`, `x-if`, `x-for`, `x-show` bindings exactly.

**Step 2: Restructure the HTML**

Key structural changes:

**a) Replace sidebar with dock + status bar + theme switcher:**

Remove the entire `<nav class="sidebar">...</nav>` block and `.sidebar-overlay`.

Add before `</body>`:

```html
<!-- Floating Dock -->
<nav class="dock glass-chrome" role="navigation" aria-label="Main navigation">
  <button class="dock-item" :class="{ active: page === 'agents' }" @click="navigate('agents')" aria-label="Chat">
    <span class="dock-tooltip">Chat</span>
    <svg viewBox="0 0 24 24"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
  </button>
  <button class="dock-item" :class="{ active: page === 'overview' }" @click="navigate('overview')" aria-label="Overview">
    <span class="dock-tooltip">Overview</span>
    <svg viewBox="0 0 24 24"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path d="M9 22V12h6v10"/></svg>
  </button>
  <button class="dock-item" :class="{ active: page === 'sessions' }" @click="navigate('sessions')" aria-label="Agents">
    <span class="dock-tooltip">Agents</span>
    <svg viewBox="0 0 24 24"><path d="m12 2-10 5 10 5 10-5z"/><path d="m2 17 10 5 10-5"/><path d="m2 12 10 5 10-5"/></svg>
  </button>
  <button class="dock-item" :class="{ active: page === 'workflows' }" @click="navigate('workflows')" aria-label="Workflows">
    <span class="dock-tooltip">Workflows</span>
    <svg viewBox="0 0 24 24"><path d="M6 3v12M18 9a9 9 0 0 1-9 9"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/></svg>
  </button>
  <button class="dock-item" :class="{ active: page === 'scheduler' }" @click="navigate('scheduler')" aria-label="Scheduler">
    <span class="dock-tooltip">Scheduler</span>
    <svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/></svg>
  </button>
  <div class="dock-divider"></div>
  <button class="dock-item" @click="cmdOpen = true" aria-label="More pages">
    <span class="dock-tooltip">More (Ctrl+K)</span>
    <svg viewBox="0 0 24 24"><circle cx="4" cy="4" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="4" r="1.5" fill="currentColor" stroke="none"/><circle cx="20" cy="4" r="1.5" fill="currentColor" stroke="none"/><circle cx="4" cy="12" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none"/><circle cx="20" cy="12" r="1.5" fill="currentColor" stroke="none"/><circle cx="4" cy="20" r="1.5" fill="currentColor" stroke="none"/><circle cx="12" cy="20" r="1.5" fill="currentColor" stroke="none"/><circle cx="20" cy="20" r="1.5" fill="currentColor" stroke="none"/></svg>
  </button>
</nav>
```

**b) Add floating status indicator (top-right):**

```html
<div class="status-float glass-chrome" :class="{ 'text-success': connected, 'text-error': !connected && !$store.app.booting }">
  <span class="status-dot"></span>
  <span x-show="connected" x-text="agentCount + ' agents'"></span>
  <span x-show="!connected && $store.app.booting">Connecting...</span>
  <span x-show="!connected && !$store.app.booting">Offline</span>
</div>
```

**c) Add floating theme switcher (top-left):**

```html
<div class="theme-float glass-chrome">
  <button class="theme-opt" :class="{ active: themeMode === 'light' }" @click="setTheme('light')" title="Light">&#9788;</button>
  <button class="theme-opt" :class="{ active: themeMode === 'system' }" @click="setTheme('system')" title="System">&#9675;</button>
  <button class="theme-opt" :class="{ active: themeMode === 'dark' }" @click="setTheme('dark')" title="Dark">&#9790;</button>
</div>
```

**d) Add command palette overlay:**

```html
<template x-if="cmdOpen">
  <div class="cmd-backdrop" @click.self="cmdOpen = false" @keydown.escape.window="cmdOpen = false">
    <div class="cmd-palette glass-overlay" @click.stop>
      <input class="cmd-input" type="text" placeholder="Search pages, actions..." x-model="cmdQuery" x-ref="cmdInput"
        @keydown.arrow-down.prevent="cmdIdx = Math.min(cmdIdx + 1, cmdFiltered.length - 1)"
        @keydown.arrow-up.prevent="cmdIdx = Math.max(cmdIdx - 1, 0)"
        @keydown.enter.prevent="cmdGo(cmdFiltered[cmdIdx])"
        x-init="$nextTick(() => $refs.cmdInput.focus())">
      <div class="cmd-list">
        <template x-for="(item, i) in cmdFiltered" :key="item.page">
          <div class="cmd-item" :class="{ selected: i === cmdIdx }" @click="cmdGo(item)" @mouseenter="cmdIdx = i">
            <span x-html="item.icon"></span>
            <span x-text="item.label"></span>
            <span class="cmd-shortcut" x-show="item.shortcut" x-text="item.shortcut"></span>
          </div>
        </template>
        <div x-show="cmdFiltered.length === 0" style="padding:20px;text-align:center;color:var(--text-muted);font-size:var(--type-label)">No results</div>
      </div>
    </div>
  </div>
</template>
```

**e) Update main content wrapper:**

Remove `<main class="main-content">` sidebar-dependent structure.
Replace with:

```html
<main class="main-content">
  <!-- Each page template stays the same internally but gets page-enter class -->
  <template x-if="page === 'overview'">
    <div class="page-enter" x-data="overviewPage" ...>
      <div class="page-header glass-chrome">...</div>
      <div class="page-body">...</div>
    </div>
  </template>
  <!-- ... all other pages ... -->
</main>
```

**f) For each page template:**
- Add `class="page-enter"` to the outer div
- Add `class="glass-chrome"` to `.page-header`
- Add `class="glass-surface"` to cards (`.card`)
- Keep all Alpine.js data bindings exactly as-is
- Keep all x-data function references (overviewPage, agentsPage, chatPage, etc.)

**g) Chat page specific changes:**
- Add agent pill selector bar above messages
- Center the chat column: `max-width: var(--content-narrow); margin: 0 auto`
- Update message container classes for new bubble styles
- Update input area to use glass-chrome + circular send button

**Step 3: Verify**

`cargo build --workspace --lib` — static file embedding must succeed.

**Step 4: Commit**

```bash
git add crates/openfang-api/static/index_body.html
git commit -m "feat(ui): restructure HTML with floating dock, command palette, and glass panels"
```

---

## Task 6: Update app.js — command palette logic and dock navigation

**Files:**
- Modify: `crates/openfang-api/static/js/app.js` (currently 319 lines)

**Step 1: Update the app() function**

Add these new properties to the `app()` return object:

```js
// Command palette state
cmdOpen: false,
cmdQuery: '',
cmdIdx: 0,
cmdItems: [
  { page: 'agents', label: 'Chat', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'overview', label: 'Overview', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'sessions', label: 'Sessions', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'approvals', label: 'Approvals', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'comms', label: 'Comms', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'workflows', label: 'Workflows', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'scheduler', label: 'Scheduler', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'channels', label: 'Channels', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'skills', label: 'Skills', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'hands', label: 'Hands', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'analytics', label: 'Analytics', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'logs', label: 'Logs', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'runtime', label: 'Runtime', icon: '<svg ...>...</svg>', shortcut: '' },
  { page: 'settings', label: 'Settings', icon: '<svg ...>...</svg>', shortcut: '' },
],
```

Add computed property:

```js
get cmdFiltered() {
  if (!this.cmdQuery) return this.cmdItems;
  var q = this.cmdQuery.toLowerCase();
  return this.cmdItems.filter(function(item) {
    return item.label.toLowerCase().indexOf(q) >= 0 || item.page.toLowerCase().indexOf(q) >= 0;
  });
},
```

Add method:

```js
cmdGo(item) {
  if (!item) return;
  this.navigate(item.page);
  this.cmdOpen = false;
  this.cmdQuery = '';
  this.cmdIdx = 0;
},
```

**Step 2: Update keyboard shortcuts**

Change `Ctrl+K` from navigating to agents to opening command palette:

```js
if ((e.ctrlKey || e.metaKey) && e.key === 'k') {
  e.preventDefault();
  self.cmdOpen = !self.cmdOpen;
  self.cmdQuery = '';
  self.cmdIdx = 0;
}
```

Add `Escape` to close command palette:

```js
if (e.key === 'Escape') {
  if (self.cmdOpen) { self.cmdOpen = false; return; }
  // ... existing escape handling
}
```

**Step 3: Remove sidebar-specific code**

- Remove `sidebarCollapsed` property and `toggleSidebar()` method
- Remove `mobileMenuOpen` property
- Remove sidebar localStorage reads/writes
- Keep `focusMode` (still hides dock)

**Step 4: Verify**

`cargo build --workspace --lib`

**Step 5: Commit**

```bash
git add crates/openfang-api/static/js/app.js
git commit -m "feat(ui): add command palette logic and dock navigation to app.js"
```

---

## Task 7: Update page JS files for HTML structure changes

**Files:**
- Modify: `crates/openfang-api/static/js/pages/chat.js` (minor — centered layout classes)
- Modify: `crates/openfang-api/static/js/pages/agents.js` (minor — detail peek panel vs modal)
- Other page JS files: likely no changes needed (they bind to data, not layout)

**Step 1: Review each page JS for layout-dependent code**

Scan each file for references to `.sidebar`, `.modal` positioning, or width calculations that assumed sidebar layout.

**Step 2: Update chat.js**

If the chat page references sidebar width or layout-dependent positioning, update those references. The chat input positioning may need adjustment since it's now centered in a narrow column rather than filling a sidebar-offset content area.

**Step 3: Update agents.js**

If the agent detail modal uses `position: fixed` with sidebar-offset calculations, update to centered/right-panel positioning.

**Step 4: Verify**

`cargo build --workspace --lib`

**Step 5: Commit**

```bash
git add crates/openfang-api/static/js/pages/chat.js crates/openfang-api/static/js/pages/agents.js
git commit -m "feat(ui): update page JS for new centered layout"
```

---

## Task 8: Check static file serving in server.rs

**Files:**
- Read: `crates/openfang-api/src/server.rs`

**Step 1: Verify noise.svg will be served**

Check how static files are served. If using `include_dir!` or `rust-embed`, the new `noise.svg` should be automatically picked up. If static files are listed explicitly, add `noise.svg` to the list.

**Step 2: Fix if needed**

If serving is explicit, add the noise.svg route.

**Step 3: Verify**

`cargo build --workspace --lib`

**Step 4: Commit (only if changes needed)**

```bash
git add crates/openfang-api/src/server.rs
git commit -m "fix(api): serve noise.svg static asset"
```

---

## Task 9: Full build + visual verification

**Step 1: Run all three checks**

```bash
cargo build --workspace --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

All must pass.

**Step 2: Start daemon and verify visually**

```bash
GROQ_API_KEY=<key> target/release/openfang.exe start &
sleep 6
curl -s http://127.0.0.1:50051/api/health
```

**Step 3: Open dashboard in browser and verify:**

- [ ] Mesh gradient visible and animating slowly
- [ ] Floating dock at bottom center with 5 icons + "More"
- [ ] Dock item hover shows tooltip, active shows glow dot
- [ ] Ctrl+K opens command palette with fuzzy search
- [ ] All 14 pages accessible via command palette
- [ ] Page transitions have fade-in animation
- [ ] Cards have glass effect (semi-transparent with blur)
- [ ] Tables/data regions are legible (scrim working)
- [ ] Light/dark theme toggle works (top-left floating)
- [ ] Status indicator shows connected state (top-right floating)
- [ ] Chat page: centered column, glass message bubbles, pill input
- [ ] Agent cards: 20px radius, glass, hover lift
- [ ] Mobile: dock tightens, command palette fits screen
- [ ] Focus mode (Ctrl+Shift+F): hides dock and floating elements
- [ ] `prefers-reduced-motion`: mesh stops, transitions instant
- [ ] No horizontal scrollbar on any page
- [ ] No console errors

**Step 4: Commit final adjustments if any**

```bash
git add -A
git commit -m "fix(ui): visual polish and adjustments from manual testing"
```

---

## Task 10: Final commit — squash or tag

**Step 1: Review all commits**

```bash
git log --oneline -10
```

**Step 2: Create summary commit or tag**

If user prefers a clean history, offer to squash. Otherwise tag the milestone:

```bash
git tag ui-overhaul-v1
```

---

## Dependency Graph

```
Task 1 (noise.svg)     ─┐
Task 2 (theme.css)      ├─→ Task 4 (components.css) ─→ Task 5 (HTML) ─→ Task 6 (app.js) ─→ Task 7 (page JS) ─→ Task 8 (server check) ─→ Task 9 (verify)
Task 3 (layout.css)    ─┘
```

Tasks 1, 2, 3 can run in parallel. Task 4 depends on 2 (tokens). Task 5 depends on 3+4 (layout + component classes). Tasks 6-7 depend on 5 (HTML structure). Task 8 depends on 1. Task 9 depends on all.
