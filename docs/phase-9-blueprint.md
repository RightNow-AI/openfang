# Phase 9 Blueprint: The `Hand` System & FangHub Marketplace

**Date:** 2026-03-09  
**Status:** In Progress

---

## 1. Goal

Implement the autonomous `Hand` system and the `FangHub` marketplace for discovering, installing, and sharing agent packages. This phase delivers the core user-facing value proposition of OpenFang: agents that work for you.

## 2. Architecture Impact

- **`openfang-hands`:** New crate. Contains the `Hand` manifest parser, lifecycle engine, and state machine.
- **`openfang-skills`:** Extended to include the `FangHub` client for interacting with the marketplace API.
- **`openfang-cli`:** New `openfang hand` and `openfang hub` command suites.
- **`openfang-api`:** New REST endpoints for Hand management and FangHub browsing.
- **`openfang-kernel`:** Extended to include a `HandScheduler` that integrates with the system scheduler (e.g., `cron`).

## 3. Task Breakdown

### Task 9.1: `HAND.toml` Manifest & Parser

**Crate:** `openfang-hands`

- **`manifest.rs`:**
  - Define the `HandManifest` struct with `serde::Deserialize`.
  - **`[hand]` section:** `name`, `version`, `author`, `description`, `repository`.
  - **`[requirements]` section:** `min_openfang_version`, `required_tools`, `required_skills`.
  - **`[schedule]` section:** `cron`, `interval` (mutually exclusive).
  - **`[prompts]` section:** `system_prompt_path`, `skill_prompt_path`.
  - **`[guardrails]` section:** `approval_required_actions` (e.g., `["browser_purchase", "twitter_post"]`).
  - **`[metrics]` section:** `dashboard_metrics` (e.g., `[{name: "Leads Generated", type: "counter"}]`).
- **`parser.rs`:**
  - `load_from_string(content: &str) -> Result<HandManifest>`.
  - `load_from_file(path: &Path) -> Result<HandManifest>`.
- **Tests:**
  - `test_parse_full_manifest`
  - `test_parse_minimal_manifest`
  - `test_parse_error_on_missing_required_fields`
  - `test_parse_error_on_cron_and_interval`

### Task 9.2: Hand Lifecycle & State Machine

**Crate:** `openfang-hands`

- **`state.rs`:**
  - Define `HandState` enum: `Inactive`, `Active`, `Paused`, `Error`.
  - Define `HandInstance` struct: `manifest`, `state`, `last_run`, `error_message`.
- **`lifecycle.rs`:**
  - `HandLifecycleManager` struct holding `HashMap<String, HandInstance>`.
  - `activate(hand_name: &str) -> Result<()>`: moves state to `Active`.
  - `deactivate(hand_name: &str) -> Result<()>`: moves state to `Inactive`.
  - `pause(hand_name: &str) -> Result<()>`: moves state to `Paused`.
  - `resume(hand_name: &str) -> Result<()>`: moves state to `Active`.
  - `get_status(hand_name: &str) -> Option<HandInstance>`.
  - `list_hands() -> Vec<HandInstance>`.
- **Persistence:**
  - `save_state_to_disk(path: &Path)`.
  - `load_state_from_disk(path: &Path)`.
- **Tests:**
  - `test_activate_deactivate_cycle`
  - `test_pause_resume_cycle`
  - `test_state_persistence`

### Task 9.3: Scheduler Integration

**Crate:** `openfang-kernel`

- **`scheduler.rs`:**
  - `HandScheduler` struct.
  - `init()`: loads all active Hands and schedules them based on their `cron` or `interval` settings.
  - `run_hand(hand_name: &str)`: the function that is actually called by the scheduler. It will spawn a new agent loop with the Hand's manifest.
- **Integration:**
  - The `HandScheduler` will be initialized in the `OpenFangKernel`'s `boot_with_config()`.
  - The `activate` and `deactivate` methods in the `HandLifecycleManager` will call the scheduler to add/remove jobs.
- **Tests:**
  - `test_schedule_cron_job`
  - `test_schedule_interval_job`
  - `test_remove_job_on_deactivate`

### Task 9.4: The 7 Core Hands

**Directory:** `openfang/hands/`

- Create a subdirectory for each of the 7 core Hands.
- Each subdirectory will contain:
  - `HAND.toml`
  - `system_prompt.md`
  - `SKILL.md`
- The build script (`xtask`) will be updated to bundle these directories into the final binary.

### Task 9.5: FangHub Client

**Crate:** `openfang-skills`

- **`fanghub.rs`:**
  - `FangHubClient` struct with `base_url`.
  - `search(query: &str) -> Result<Vec<HandSearchResult>>`.
  - `get_manifest(hand_name: &str, version: &str) -> Result<HandManifest>`.
  - `download_hand(hand_name: &str, version: &str, install_path: &Path) -> Result<()>`.
- **`installer.rs`:**
  - `install(hand_name: &str, version: &str)`: downloads and installs a Hand.
  - `update(hand_name: &str)`: updates a Hand to the latest version.
  - `uninstall(hand_name: &str)`.
- **Tests:**
  - `test_search_hands`
  - `test_install_hand`
  - `test_update_hand`

### Task 9.6: CLI Commands

**Crate:** `openfang-cli`

- **`hand.rs`:**
  - `openfang hand list`: lists all available Hands and their status.
  - `openfang hand activate <name>`.
  - `openfang hand deactivate <name>`.
  - `openfang hand pause <name>`.
  - `openfang hand resume <name>`.
  - `openfang hand status <name>`.
- **`hub.rs`:**
  - `openfang hub search <query>`.
  - `openfang hub install <name>@<version>`.
  - `openfang hub update <name>`.
  - `openfang hub uninstall <name>`.

### Task 9.7: REST API Endpoints

**Crate:** `openfang-api`

- **`/api/hands`:**
  - `GET /`: list all Hands.
  - `GET /{name}`: get Hand status.
  - `POST /{name}/activate`.
  - `POST /{name}/deactivate`.
  - `POST /{name}/pause`.
  - `POST /{name}/resume`.
- **`/api/hub`:**
  - `GET /search?q=<query>`.
  - `POST /install`: `{ name: String, version: String }`.
  - `POST /update`: `{ name: String }`.
  - `POST /uninstall`: `{ name: String }`.

## 4. Verification Milestones

1.  **M1:** All `HAND.toml` files for the 7 core Hands parse correctly.
2.  **M2:** `openfang hand activate researcher` successfully runs the researcher agent.
3.  **M3:** `openfang hub install example/test-hand` successfully downloads and installs a test Hand from a mock FangHub server.
4.  **M4:** All Phase 9 tests pass, and there are zero `todo!` or `unimplemented!` macros in the new crates.
