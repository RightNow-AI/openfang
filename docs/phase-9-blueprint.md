# Phase 9 Blueprint: The `Hand` System & FangHub Marketplace

**Date:** 2026-03-09  
**Status:** Blueprinting

---

## 1. Goal

Implement the autonomous `Hand` system and the `FangHub` marketplace for discovering, installing, and sharing agent packages. This phase brings the core value proposition of OpenFang to life: agents that work for you, not just when you prompt them.

## 2. Core Concepts

- **Hand:** A pre-packaged, autonomous agent capability that runs on a schedule or trigger. It is a self-contained unit of work with its own manifest, system prompt, skills, and guardrails.
- **FangHub:** A public marketplace for discovering, sharing, and installing `Hand` packages. It functions like a package manager for agents.

## 3. Architecture & Crate Impact

| Crate | Role in Phase 9 |
|---|---|
| `openfang-hands` | **Primary.** Implements the `Hand` manifest (`HAND.toml`) parser, lifecycle management (activate, pause, status), and the scheduler that runs the Hands. |
| `openfang-skills` | **Primary.** Implements the `FangHub` client for searching, installing, and updating Hands from the marketplace. |
| `openfang-cli` | **Secondary.** Adds the `openfang hand` and `openfang hub` command suites for managing Hands and interacting with FangHub. |
| `openfang-api` | **Secondary.** Exposes REST endpoints for managing Hands and browsing the marketplace from the web UI. |
| `openfang-desktop` | **Secondary.** Integrates Hand status and notifications into the system tray and desktop app UI. |

## 4. Task Breakdown

| Task | Description | Crate(s) | Estimated Effort |
|---|---|---|---|
| **9.1** | **`HAND.toml` Manifest & Parser:** Design and implement the `HAND.toml` manifest format, including sections for `[hand]`, `[prompt]`, `[schedule]`, `[tools]`, `[skills]`, and `[guardrails]`. Build the parser in `openfang-hands`. | `openfang-hands` | Medium |
| **9.2** | **Hand Lifecycle Management:** Implement the core logic for `activate`, `pause`, `resume`, and `deactivate` a Hand. This includes state management (persisted in SurrealDB) and scheduler integration. | `openfang-hands` | High |
| **9.3** | **Scheduler Integration:** Integrate the Hand lifecycle with the existing `maestro-scheduler` (or build a dedicated scheduler) to run Hands based on their `[schedule]` cron/interval definitions. | `openfang-hands` | Medium |
| **9.4** | **Build the 7 Core Hands:** Implement the system prompts, skill selections, and guardrails for the 7 core Hands: `Clip`, `Lead`, `Collector`, `Predictor`, `Researcher`, `Twitter`, and `Browser`. | `openfang-hands` | High |
| **9.5** | **`FangHub` Client:** Implement the client in `openfang-skills` to interact with the (mock) FangHub API: `search`, `install`, `update`, `publish`. | `openfang-skills` | Medium |
| **9.6** | **CLI Commands:** Implement the `openfang hand` and `openfang hub` command suites in `openfang-cli`. | `openfang-cli` | Medium |
| **9.7** | **API Endpoints:** Expose REST endpoints in `openfang-api` for managing Hands and browsing the marketplace. | `openfang-api` | Low |
| **9.8** | **UI Integration:** Integrate Hand management and FangHub browsing into the web UI and desktop app. | `openfang-desktop` | Medium |

## 5. Milestones & Verification

1. **Milestone 1 (Hand Execution):** A user can manually place a `HAND.toml` file in the `hands/` directory and activate it via the CLI, seeing it execute on schedule. Verification: `openfang hand activate my-hand` runs successfully.
2. **Milestone 2 (Core Hands):** All 7 core Hands are implemented and can be activated, demonstrating a range of autonomous capabilities. Verification: Each of the 7 Hands can be activated and produces its expected output.
3. **Milestone 3 (FangHub Client):** A user can install a Hand from the (mock) FangHub marketplace via the CLI. Verification: `openfang hub install researcher` successfully downloads and installs the Researcher Hand.
4. **Milestone 4 (Full Integration):** All CLI commands, API endpoints, and UI components are functional. Verification: A user can manage Hands and browse FangHub from the web UI, desktop app, and CLI.
