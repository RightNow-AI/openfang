# OpenFang Dashboard UI Overhaul — "Apple Intelligence" Design

**Date:** 2026-03-05
**Style:** Apple Intelligence / Liquid Glass inspired
**Scope:** Full UI overhaul — CSS rewrite + HTML restructure + new navigation paradigm
**Stack:** Alpine.js + vanilla CSS (no framework change, no build step)

---

## 1. Visual Foundation

### Material Hierarchy (3 levels)

| Level | Use | Blur | Opacity (light / dark) |
|-------|-----|------|----------------------|
| Surface | Cards, tables, data regions | 12px | 0.82 / 0.75 |
| Chrome | Nav dock, page headers | 24px | 0.70 / 0.60 |
| Overlay | Modals, command palette, peek panels | 40px | 0.65 / 0.55 |

Data regions get an additional scrim layer for legibility — slightly more opaque base behind tables/charts.

### Edge Highlights (not flat borders)

```css
border: 1px solid rgba(255,255,255,0.08);
border-top: 1px solid rgba(255,255,255,0.18);
box-shadow: inset 0 1px 0 rgba(255,255,255,0.06);
```

CSS noise texture overlay (noise.svg at 3-4% opacity) on glass panels to prevent gradient banding.

### Ambient Mesh Gradient

- 3 blobs: muted orange, warm peach, soft lavender
- Isolated `::before` pseudo-element on body, `position: fixed`
- Animated via `transform: translate3d()` only (GPU-composited, no repaints)
- 90s drift cycle
- `@media (prefers-reduced-motion: reduce)` -> static gradient
- Dark mode: same blobs at 30% brightness

### Border Radius Scale

| Element | Radius |
|---------|--------|
| Cards, panels | 20px |
| Buttons, inputs | 12px |
| Badges | 10px |
| Small pills | 999px (full pill) |

### Typography Tokens

Font stack: Inter (body) + Geist Mono (code/data)

| Token | Size | Weight | Use |
|-------|------|--------|-----|
| --type-metric | 28px | 700 | Big stat numbers |
| --type-heading | 18px | 600 | Page/section titles |
| --type-body | 15px | 400 | Default text |
| --type-label | 12px | 500 | Form labels, nav items (sentence case) |
| --type-caption | 11px | 400 | Muted metadata |

All numeric elements: `font-variant-numeric: tabular-nums` to prevent metric jitter.
No `text-transform: uppercase` — sentence case throughout.

### Motion

- Spring easing: `cubic-bezier(0.34, 1.56, 0.64, 1)` for interactions
- Smooth easing: `cubic-bezier(0.4, 0, 0.2, 1)` for transitions
- Page transitions: `opacity + scale(0.98)` fade-in, 250ms
- Card stagger: 30ms delay per card
- `@media (prefers-reduced-motion: reduce)` honored everywhere

---

## 2. Theme Tokens

### Light Mode
```
--glass-bg:      rgba(255, 255, 255, 0.72)
--glass-chrome:  rgba(255, 255, 255, 0.58)
--glass-overlay: rgba(255, 255, 255, 0.52)
--glass-border:  rgba(255, 255, 255, 0.18)
--glass-edge:    rgba(255, 255, 255, 0.25)
--mesh-1:        #FFD6B0 (peach)
--mesh-2:        #FFECD2 (cream)
--mesh-3:        #E8D5F5 (lavender)
```

### Dark Mode
```
--glass-bg:      rgba(30, 28, 26, 0.75)
--glass-chrome:  rgba(30, 28, 26, 0.60)
--glass-overlay: rgba(30, 28, 26, 0.55)
--glass-border:  rgba(255, 255, 255, 0.08)
--glass-edge:    rgba(255, 255, 255, 0.12)
--mesh-1:        #3D2200 (dim orange)
--mesh-2:        #2A1800 (deep amber)
--mesh-3:        #1A1030 (deep violet)
```

---

## 3. Layout — Floating Dock Navigation

Replace sidebar with a floating bottom dock (macOS Dock style):

- Horizontally centered at viewport bottom, `position: fixed`
- Glass Chrome material, `border-radius: 24px`, `padding: 6px`
- 5 primary icons: Chat, Overview, Agents, Workflows, Scheduler
- Icons 24px with tooltip-style label slide-up on hover
- Active item: glow ring + filled icon variant
- "More" icon (grid dots) at end -> opens command palette

### Command Palette

- `Ctrl+K` or click "More" in dock
- Glass Overlay material, centered, `max-width: 520px`
- Fuzzy search across all pages + actions
- Recent pages shown by default
- Keyboard navigable (arrow keys + Enter)

### Page Header

- Slim floating bar at top, Glass Chrome material
- `border-radius: 16px`, `margin: 12px 16px 0`
- Page title left, contextual actions right
- No full-width border — floats above content

### Content Area

- Full viewport behind mesh gradient
- Pages as centered glass panels, `max-width: 1200px`
- Cards use Surface material
- `padding: 24px 32px` desktop, responsive down

### Navigation Hierarchy

**Primary (dock):** Chat, Overview, Agents, Workflows, Scheduler

**Secondary (command palette):** Sessions, Approvals, Comms, Logs, Channels, Skills, Hands, Runtime, Settings, Analytics

---

## 4. Chat Page — Centered Conversation

- Single centered column, `max-width: 720px`
- Agent messages: Glass Surface, left-aligned, `border-radius: 20px 20px 20px 8px`
- User messages: Accent-tinted glass (orange 10%), right-aligned, `border-radius: 20px 20px 8px 20px`
- Avatar: 32px circle with agent icon, status glow ring
- Typing indicator: 3 dots with staggered bounce
- Agent selector: horizontal pill bar at top, active = filled pill with glow
- Input: fixed bottom, Glass Chrome, `border-radius: 20px`, auto-growing textarea
- Send button: circular accent, slides in when text present
- Empty state: centered greeting + suggested action chips

---

## 5. Agents Page

- Grid of agent cards, `max-width: 1200px`, centered
- Cards (Glass Surface, `border-radius: 20px`):
  - Avatar + name + status badge (pill)
  - Model label (caption, muted)
  - Last active + quick actions (Chat, Pause, Config as ghost pills)
  - Hover: lift 4px, border brightens, status glow
- Detail: right-side peek panel (Glass Overlay, 420px wide)
  - Config, recent sessions, cost sparkline, hands, skills

---

## 6. Overview Page

- 4 metric cards (Glass Surface, `border-radius: 20px`)
  - Big number (tabular-nums) + label + sparkline/trend arrow
  - Active agents, Total spend, Messages today, Uptime
- 2-column grid below:
  - Agent status list (compact rows, status dots)
  - Recent activity feed (timeline)

---

## 7. Secondary Pages

All follow: centered glass panel, `max-width: 1100px`, Surface material with scrim for tables.

| Page | Treatment |
|------|-----------|
| Sessions | Glass table rows, expandable message history |
| Approvals | Card list, Approve/Deny pill buttons, pending glow |
| Comms | Channel list + message feed |
| Logs | Monospace viewer, opaque scrim, terminal feel |
| Channels | Card grid with status indicators |
| Skills | Card grid, name + description + attached agents |
| Hands | Card grid, bundled vs custom labels |
| Runtime | Metric cards + config key-value table |
| Settings | Form sections in glass cards, pill toggles |
| Analytics | Chart panels with heavy scrim |
| Scheduler | Cron cards with countdown, enable/disable toggle |
| Workflows | Flow cards with status badges |

---

## 8. Component Refresh

| Component | Current | New |
|-----------|---------|-----|
| Buttons | 6px radius, flat | 12px, glass ghost default, accent solid primary |
| Badges | 20px radius, uppercase | 10px, sentence case, glass-tinted |
| Tables | Bordered wrapper, flat rows | Glass Surface + scrim, hover row glow |
| Forms | Standard inputs | 12px radius, glass bg, accent focus glow |
| Modals | Flat card overlay | Glass Overlay, 24px radius, backdrop blur |
| Toggles | Checkbox | Pill-switch (iOS), accent when on |
| Cards | 12px radius, border | 20px, glass material, edge highlight |
| Toasts | Bottom-right stack | Top-center, glass pill, slide-down |
| Scrollbars | Thin custom | Same thin, glass-tinted thumb |

---

## 9. File Changes

| File | Action |
|------|--------|
| `theme.css` | Complete rewrite — tokens, mesh gradient, glass materials |
| `layout.css` | Complete rewrite — dock nav, floating header, centered content |
| `components.css` | Complete rewrite — all components to glass style |
| `index_body.html` | Restructure — remove sidebar, add dock + command palette |
| `js/app.js` | Update — dock navigation, command palette logic, page transitions |
| Page JS files | Minor updates where HTML structure changes |
| New: `noise.svg` | Tiny noise texture for glass banding prevention |

**No backend changes required.**

---

## 10. Accessibility

- All contrast ratios meet WCAG AA on both themes (glass opacity tuned for this)
- `prefers-reduced-motion` disables all animation
- Command palette fully keyboard navigable
- Focus rings preserved (accent glow)
- Touch targets >= 44px on coarse pointers
- Semantic HTML maintained (nav, main, role attributes)

---

## 11. Performance

- Mesh gradient on isolated composited layer (no repaints)
- `backdrop-filter` hardware-accelerated on modern browsers
- Fallback: solid semi-transparent backgrounds for browsers without backdrop-filter support
- No additional JS dependencies
- No build step added
