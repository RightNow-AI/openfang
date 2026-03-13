# nextjs-dashboard — Team Task Registry

## Track isolation
This is **Track 2** only.  Track 1 (Next.js + Capacitor mobile shell) has a separate owner, separate PR.
Do not cross-edit files between tracks.

## Coordinator
`team/coordinator`

## Workers and file ownership

| Agent | Domain | Owned paths |
|---|---|---|
| `team/sdk` | SDK | `lib/**` |
| `team/ui` | UI | `app/**`, `components/**`, `app/globals.css` |
| `team/backend-contract` | Contracts | `docs/contracts/**` (read-only on UI/lib) |
| `team/qa` | QA | `.tasks/smoke/**`, `scripts/smoke-check.sh` |

## Frozen files (do not edit unless a bug forces it)
- `components/shell/ShellClient.js`
- `components/shell/Sidebar.js`
- `components/shell/Topbar.js`
- `components/shell/MobileNav.js`
- `components/cards/ActionCard.js`
- `components/cards/SectionCard.js`
- `app/globals.css` — design tokens
- `tailwind.config.js`

## Packet format
Each `.tasks/*.json` file serializes a `TaskPacket` as defined in
`crates/openfang-runtime/src/team/task_packet.rs`.

## Execution order
1. `contracts-lock` (coordinator, must complete first)
2. `task-01-sdk-client` (SDK agent)
3. `task-02-ui-wiring` (UI agent, depends on 01)
4. `task-03-backend-contracts` (backend-contract agent, parallel with 01)
5. `task-04-qa-smoke` (QA agent, depends on 02 + 03)
6. `task-05-coordinator-merge` (coordinator, depends on all prior)
