# Phase 13 Blueprint: Desktop & UI Polish

**Version:** v0.3.36
**Branch:** `feature/phase-8-stub-implementation`
**Date:** 2026-03-10

---

## Goal

Complete the desktop and UI experience by adding the FangHub marketplace page and Multi-Agent Mesh page to the SPA dashboard, wiring Phase 11/12 features into the Tauri desktop app via new commands, and polishing the overall UX with improved navigation, keyboard shortcuts, and a refreshed tray menu.

---

## Architecture Impact

Phase 13 is primarily a frontend and desktop integration phase. It does not add new backend crates but does extend two existing crates:

- **`openfang-api/static/`** — SPA dashboard HTML/JS/CSS additions
- **`openfang-desktop/src/`** — New Tauri commands and tray menu updates

---

## Task Breakdown

### Task 13.1 — FangHub Page in SPA Dashboard

Add a `Page: FangHub` section to `index_body.html` that allows users to:

- **Browse** packages from the FangHub registry (`GET /api/fanghub/search`)
- **View** package details (description, version, author, download count)
- **Install** a Hand directly from the UI (`POST /api/fanghub/install`)
- **View installed** Hands (links to the Hands page)
- **Search** by keyword or capability tag

The page should use the same card-based layout as the Hands page for consistency.

**Nav entry:** Add `FangHub` to the Extensions section in the sidebar nav.

### Task 13.2 — Multi-Agent Mesh Page in SPA Dashboard

Add a `Page: Mesh` section to `index_body.html` that provides a dedicated view for the Multi-Agent Mesh:

- **Peer list** with connection status, node ID, capabilities, and latency
- **Connect peer** form (enter OFP address to connect to a remote node)
- **A2A agent registry** — list of discovered A2A agents from all connected peers
- **Task routing log** — recent tasks dispatched through the mesh with target, duration, and status
- **Mesh topology** — a simple text-based topology view showing the local node and its connected peers

**Nav entry:** Add `Mesh` to the Automation section in the sidebar nav.

### Task 13.3 — Tauri Desktop Commands for Phase 11/12

Add new Tauri commands to `openfang-desktop/src/commands.rs`:

| Command | Purpose |
|---|---|
| `fanghub_search(query: String)` | Search FangHub registry and return package list |
| `fanghub_install(hand_id: String)` | Install a Hand from FangHub via kernel |
| `mesh_list_peers()` | List all connected OFP mesh peers |
| `mesh_connect_peer(address: String)` | Connect to a new OFP peer by address |
| `mesh_disconnect_peer(peer_id: String)` | Disconnect from an OFP peer |
| `mesh_list_a2a_agents()` | List all A2A agents discovered from mesh peers |

### Task 13.4 — Tray Menu Updates

Update `openfang-desktop/src/tray.rs` to add:

- **Mesh status** indicator in the tray tooltip (e.g., "3 peers connected")
- **Quick actions:** "Open FangHub", "Mesh Status"
- **Separator** between existing items and new Phase 11/12 items

### Task 13.5 — SPA Dashboard Polish

Improvements to the existing SPA dashboard:

- **Version badge** in the sidebar header should show `v0.3.36`
- **Keyboard shortcut** `Cmd/Ctrl+K` opens a command palette (quick-navigate to any page)
- **Breadcrumb** in the page header showing the current section > page
- **Empty states** for all pages that currently show blank content when no data is loaded
- **Loading skeletons** replace the generic spinner on the Overview, Agents, and Hands pages

### Task 13.6 — Integration Tests

Add 6 new integration tests in `maestro-integration-tests/tests/desktop_ui.rs`:

- `test_fanghub_search_api_route` — verifies `GET /api/fanghub/search` returns JSON
- `test_fanghub_install_api_route` — verifies `POST /api/fanghub/install` accepts a hand_id
- `test_mesh_peers_api_route` — verifies `GET /api/mesh/peers` returns JSON
- `test_mesh_connect_api_route` — verifies `POST /api/mesh/connect` accepts an address
- `test_a2a_per_agent_card_route` — verifies `GET /a2a/agents/{id}` returns an agent card
- `test_a2a_send_subscribe_route` — verifies `POST /a2a/tasks/sendSubscribe` returns SSE headers

---

## Verification Milestones

1. `cargo check --workspace --exclude openfang-desktop` passes with 0 errors
2. All 6 new integration tests pass
3. FangHub page renders correctly in the SPA dashboard
4. Mesh page renders correctly in the SPA dashboard
5. New Tauri commands are registered in `lib.rs`

---

## Files Changed

| File | Change |
|---|---|
| `crates/openfang-api/static/index_body.html` | +FangHub page, +Mesh page, +nav entries, +polish |
| `crates/openfang-desktop/src/commands.rs` | +6 new Tauri commands |
| `crates/openfang-desktop/src/tray.rs` | +mesh status, +FangHub quick action |
| `crates/openfang-desktop/src/lib.rs` | Register new commands |
| `crates/maestro-integration-tests/tests/desktop_ui.rs` | +6 new tests |
| `crates/maestro-integration-tests/Cargo.toml` | Register new test binary |
