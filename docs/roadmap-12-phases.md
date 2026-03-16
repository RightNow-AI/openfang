# OpenFang 12-Phase Roadmap

This document defines the next strategic roadmap for OpenFang as an Agent OS.

It is intentionally different from the older launch roadmap:
- the launch roadmap focused on release hardening and competitive parity
- this roadmap focuses on turning OpenFang into a stronger Agent OS with a clear capability-routing model

The core product direction is:
- the user describes a goal
- OpenFang routes that goal to an existing capability
- if no capability exists, OpenFang helps design a new one with the user in the loop
- workflows become reusable compositions of those capabilities

## Principles

- Favor operationally useful work before architectural expansion.
- Prefer additive API improvements before deep runtime rewrites.
- Treat routing, capability creation, and workflow composition as core Agent OS concerns.
- Keep approval and capability boundaries explicit whenever the system can create or activate new agents.
- Defer heavy introspection features until the execution model is stable.

## Current Progress Snapshot

- Phase 1 completed on 2026-03-16
- Phase 2 completed on 2026-03-16
- Phase 3 completed on 2026-03-16
- Phase 4 completed on 2026-03-16
- Phase 5 completed on 2026-03-16
- Phase 6 completed on 2026-03-16
- Phase 7 completed on 2026-03-16
- Phase 8 completed on 2026-03-16
- Phase 9 completed on 2026-03-16
- Phase 10 completed on 2026-03-16
- Phase 11 completed on 2026-03-16
- Phase 12 completed on 2026-03-16
- All 12 phases complete

## Phase 1: API Detail Endpoints

Goal: close small but important API gaps.

Status:
- completed on 2026-03-16

Scope:
- add single-item endpoints for tools
- add single-item endpoints for profiles
- add single-item endpoints for external A2A agents

Why:
- improves API completeness
- reduces UI and client-side guesswork
- low implementation risk

Affected modules:
- `crates/openfang-api/src/server.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-cli/src/main.rs`
- `docs/api-reference.md`

Porting approach:
- directly portable from LibreFang

Done when:
- clients can fetch individual tool, profile, and external A2A agent records directly
- API docs cover the new endpoints

## Phase 2: Cron CRUD Completion

Goal: make scheduled jobs fully editable.

Status:
- completed on 2026-03-16

Scope:
- add `PUT /api/cron/jobs/{id}`

Why:
- completes the cron management surface
- supports real operational editing instead of delete-and-recreate

Affected modules:
- `crates/openfang-api/src/server.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-kernel/src/cron.rs`
- `crates/openfang-cli/src/main.rs`
- `docs/api-reference.md`

Porting approach:
- directly portable from LibreFang

Done when:
- cron jobs can be updated in place
- validation and error behavior match existing cron APIs

Implementation note:
- a phase findings document now exists at `docs/phase-2-cron-crud-findings.md`

## Phase 3: Rich `comms_send`

Goal: make inter-agent and channel-facing communication realistic.

Status:
- completed on 2026-03-16

Scope:
- add `thread_id`
- add attachments

Why:
- supports threaded platforms properly
- supports handoffs and multi-step channel workflows

Affected modules:
- `crates/openfang-types/src/comms.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-api/src/server.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `crates/openfang-runtime/src/kernel_handle.rs`
- channel adapters that support threads or attachments
- SDKs in `sdk/javascript/` and `sdk/python/`

Porting approach:
- directly portable in API shape
- partially portable in runtime and adapter wiring

Done when:
- `comms_send` supports thread-aware replies
- `comms_send` supports file and media attachments
- the API and SDKs expose the new fields

Implementation note:
- a phase findings document now exists at `docs/phase-3-comms-findings.md`

## Phase 4: MCP Server CRUD API

Goal: make MCP server configuration a first-class admin workflow.

Scope:
- add create, update, and delete endpoints for MCP servers

Why:
- OpenFang already understands MCP servers internally
- admin workflows should not require manual config edits for every change

Affected modules:
- `crates/openfang-api/src/server.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-types/src/config.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `docs/mcp-a2a.md`
- `docs/api-reference.md`

Porting approach:
- directly portable from LibreFang

Done when:
- MCP servers can be listed, added, updated, and removed over the API
- config persistence and reload behavior are reliable

## Phase 5: Backup and Restore

Goal: improve operational safety.

Status:
- completed on 2026-03-16

Scope:
- backup creation
- backup listing
- backup deletion
- restore flow

Why:
- high value for upgrades, migrations, testing, and recovery
- strong 1.0-era operational feature

Affected modules:
- `crates/openfang-api/src/server.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-types/`
- `crates/openfang-kernel/`
- `docs/api-reference.md`
- `docs/production-checklist.md`

Porting approach:
- partially portable from LibreFang
- archive format and restore semantics should be implemented OpenFang-native

Done when:
- a backup archive can be created from a running system
- backups are discoverable and removable
- restore behavior is documented and verified

Implementation note:
- a phase findings document now exists at `docs/phase-5-backup-restore-findings.md`

## Phase 6: Deterministic Hand Identity

Goal: make Hands stable across restarts.

Scope:
- deterministic Hand agent UUIDs or equivalent stable identity mapping

Why:
- reduces drift in cron, bindings, and restoration flows
- makes long-lived autonomous behavior easier to reason about

Affected modules:
- `crates/openfang-hands/src/registry.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `crates/openfang-kernel/src/cron.rs`
- any persistence layer storing Hand instance state

Porting approach:
- partially portable from LibreFang

Done when:
- Hand restoration preserves stable identity semantics
- restart behavior does not silently orphan related state

## Phase 7: Builtin Router

Goal: establish a true system entrypoint for goal routing.

Status:
- completed on 2026-03-16

Scope:
- add a native `builtin:router` execution path
- support deterministic routing to Hands, specialist agents, workflows, or fallback assistant behavior

Why:
- this is the beginning of the Agent OS model, not just another agent template
- users should be able to state goals without pre-selecting the correct executor

Affected modules:
- `crates/openfang-kernel/src/kernel.rs`
- new router module under `crates/openfang-kernel/src/`
- `agents/`
- `crates/openfang-types/src/agent.rs`
- API and CLI surfaces that expose default agents or first-run behavior
- `docs/architecture.md`
- `docs/agent-templates.md`

Porting approach:
- conceptually portable
- implementation should be OpenFang-native

Done when:
- the router can accept a user goal and deterministically choose an execution target
- the router does not depend on a normal LLM chat loop for basic dispatch
- routing behavior is testable and explainable

## Phase 8: Capability Registry for Routing

Goal: route against system capabilities, not only names or templates.

Status:
- completed on 2026-03-16

Scope:
- define a capability-aware registry surface usable by the router
- include Hands, agents, workflows, and other relevant executors

Why:
- a router is only useful if it can discover what the system can actually do
- this creates the semantic layer for future capability creation

Affected modules:
- `crates/openfang-kernel/`
- `crates/openfang-hands/`
- `crates/openfang-skills/`
- workflow and agent registry code
- API surfaces that expose capabilities
- `docs/architecture.md`

Porting approach:
- conceptually portable

Done when:
- router decisions are based on a registry of available capabilities
- capability metadata is explicit enough to support deterministic selection

## Phase 9: User-in-the-Loop Agent Builder

Goal: turn missing capability into a guided creation flow.

Status:
- completed on 2026-03-16

Scope:
- detect when no existing capability fits the request
- propose a draft agent, Hand, or workflow design
- require user review before creation or activation

Why:
- an Agent OS should not stop at routing existing skills
- it should help users expand the system safely

Affected modules:
- `crates/openfang-kernel/`
- `crates/openfang-cli/`
- `crates/openfang-api/`
- `agents/`
- approval and capability enforcement modules
- possible scaffolding helpers in extensions or templates

Porting approach:
- conceptually portable

Done when:
- the system can identify capability gaps
- it can draft a new capability proposal
- user approval is required before creation and activation

## Phase 10: Extensions API

Goal: expose the integration system as an administrable product surface.

Status:
- completed on 2026-03-16

Scope:
- list extensions
- get extension details
- install and uninstall extensions

Why:
- OpenFang already has an internal extensions system
- the API should expose it cleanly for UI and automation

Affected modules:
- `crates/openfang-api/src/server.rs`
- `crates/openfang-api/src/routes.rs`
- `crates/openfang-extensions/`
- `crates/openfang-kernel/src/kernel.rs`
- `docs/api-reference.md`

Porting approach:
- directly portable in API shape
- partially portable in integration details

Done when:
- extensions are visible and manageable through the API
- status and health information are available to clients

## Phase 11: Provider Catalog Modernization

Goal: make provider and model metadata more maintainable.

Status:
- completed on 2026-03-16

Scope:
- move toward a local TOML-based provider and model catalog
- keep runtime loading deterministic

Why:
- hardcoded catalogs become expensive to maintain
- structured catalog files are easier to review and evolve

Affected modules:
- `crates/openfang-runtime/src/model_catalog.rs`
- `crates/openfang-types/src/model_catalog.rs`
- new `catalog/` directory at repo root
- provider docs and API reference

Porting approach:
- partially portable in file structure
- implementation should be OpenFang-native

Done when:
- provider and model metadata can be loaded from local catalog files
- alias handling remains stable
- the change does not regress current provider coverage

## Phase 12: Performance and Scale Layer

Goal: optimize the system after the capability model is stable.

Status:
- completed on 2026-03-16

Scope:
- stable prefix mode
- provider prompt caching improvements
- optional multi-bot routing
- decision trace layer as the last item in this phase

Why:
- these features are useful but should not land before the execution model settles
- decision tracing is most valuable once the routing and builder model exist

Affected modules:
- `crates/openfang-runtime/src/agent_loop.rs`
- `crates/openfang-kernel/src/kernel.rs`
- `crates/openfang-types/src/config.rs`
- model driver modules
- channel routing and configuration code for multi-bot support
- metrics and usage tracking surfaces

Porting approach:
- partially portable for caching concepts
- multi-bot routing is partially portable
- decision trace layer is conceptually portable

Done when:
- performance features are measured, not guessed
- caching behavior is observable and safe
- decision tracing is introduced only after the rest of the execution path is stable

## Priority Summary

Immediate 1.0-oriented work:
- phases 1 through 6

Agent OS core expansion:
- phases 7 through 9

Platform maturity and scale:
- phases 10 through 12

## Notes

- The builtin router is considered a core strategic feature, not a side experiment.
- The agent builder should remain approval-driven and capability-constrained.
- The decision trace layer is intentionally deferred to the end of the roadmap.
