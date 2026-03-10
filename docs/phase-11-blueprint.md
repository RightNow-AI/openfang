# Phase 11 Blueprint: FangHub Marketplace

**Date:** 2026-03-09  
**Status:** Not Started

---

## 1. Goal

To build **FangHub**, a centralized, public marketplace for discovering, installing, and sharing autonomous `Hand` packages. This phase will deliver a public-facing web UI for browsing Hands, a backend API for managing the registry, and a CLI for developers to publish their own Hands. The core objective is to create a vibrant ecosystem around OpenFang, enabling community contributions and extending the platform's capabilities.

## 2. Architecture Impact

This phase introduces three major new components and significantly enhances one existing crate:

1.  **`fanghub-ui` (New Crate):** A static, public-facing web application built with **Vite + React + TypeScript + TailwindCSS**. This will be the primary user interface for browsing and discovering Hands.
2.  **`fanghub-registry` (New Crate):** A backend service built with **Axum + SurrealDB** that provides the API for publishing, searching, and retrieving Hand packages. It will manage user accounts, package metadata, and versioning.
3.  **`fang-cli` (New Crate):** A command-line interface for developers to authenticate, package, and publish their Hands to the FangHub registry.
4.  **`openfang-kernel` (Enhancement):** The kernel will be updated to query the FangHub registry for available Hands, in addition to its local registry.

### System Diagram

```mermaid
graph TD
    subgraph User
        A[Developer] -->|publishes| B(fang-cli)
        C[End User] -->|browses| D{fanghub-ui (React App)}
    end

    subgraph FangHub Infrastructure
        B -->|uploads| E(fanghub-registry API)
        D -->|queries| E
        E -->|stores/retrieves| F(SurrealDB)
    end

    subgraph OpenFang Kernel
        G(openfang-kernel) -->|installs from| E
    end

    style A fill:#cce5ff,stroke:#333,stroke-width:2px
    style C fill:#cce5ff,stroke:#333,stroke-width:2px
    style G fill:#d5e8d4,stroke:#333,stroke-width:2px
```

## 3. Task Breakdown

This phase is broken down into 8 distinct, sequential tasks.

| Task | Title | Key Deliverables |
|---|---|---|
| **11.1** | `fanghub-registry`: Schema & API | SurrealDB schema for users, packages, versions. Axum routes for `POST /publish`, `GET /search`, `GET /packages/{id}`. |
| **11.2** | `fang-cli`: Authentication & Publish | `fang login`, `fang package`, `fang publish` commands. GPG signing of manifests. |
| **11.3** | `fanghub-ui`: Hand Discovery UI | React components for search bar, package list, and detailed package view. |
| **11.4** | `fanghub-ui`: User Authentication | Login/logout flow using GitHub OAuth. Display user's published packages. |
| **11.5** | Kernel Integration | `openfang-kernel` queries FangHub API. `install_from_fanghub(hand_id)` method. |
| **11.6** | End-to-End Testing | Integration test: `fang publish` → `fanghub-ui` shows package → `openfang-kernel` installs it. |
| **11.7** | Documentation | Update `README.md`, `ROADMAP.md`. Create `docs/fanghub-publishing-guide.md`. |
| **11.8** | Deployment & Launch | Deploy `fanghub-registry` and `fanghub-ui` to production. Announce launch. |

## 4. Verification Milestones

1.  **Milestone 1 (Tasks 11.1-11.2):** A developer can successfully publish a signed Hand package to the registry via the `fang-cli`.
2.  **Milestone 2 (Tasks 11.3-11.4):** An end user can browse the FangHub UI, search for the published Hand, and view its details. A developer can log in and see their published packages.
3.  **Milestone 3 (Task 11.5-11.6):** An OpenFang instance can successfully install and activate the Hand published in Milestone 1 by referencing its FangHub ID.
4.  **Milestone 4 (Tasks 11.7-11.8):** All documentation is updated, the services are deployed, and FangHub is officially launched.
