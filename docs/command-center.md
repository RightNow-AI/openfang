# Command Center

Command Center is the shared operating shell for client and internal delivery work that needs intake, planning, approvals, execution, and results review in one flow.

---

## Best Fit

- client onboarding and account coordination
- task planning with visible approval gates
- delivery operations that need a persistent record of work and results

## Operator Loop

Command Center works best when the team follows this loop:

1. capture a client brief or internal request
1. generate a task plan
1. review and approve risky work
1. run tasks and monitor progress
1. review outputs and follow-up actions

## Current Product Surfaces

Current frontend route families in the Next.js app:

- `/command-center/new`: start a new client or project through the wizard
- `/command-center/[clientId]`: overview for a single client record
- `/command-center/[clientId]/wizard`: continue plan generation and setup
- `/command-center/[clientId]/approvals`: review work waiting on approval
- `/command-center/[clientId]/results`: inspect completed outputs
- `/clients`: jump into the newer client dashboard shell
- `/clients/[clientId]`: shared shell for ongoing client operations
- `/clients/[clientId]/pulse`: health and status view
- `/clients/[clientId]/plan`: planning and assignment view
- `/clients/[clientId]/approvals`: approvals queue
- `/clients/[clientId]/results`: results and review view

## Backend Contract

Current backend-facing routes and patterns:

- `POST /clients`
- `GET /clients/{id}`
- `PUT /clients/{id}`
- `POST /wizard/generate-plan`
- task, approval, and results reads through the command-center API layer used by the dashboard

Auth expectations and approval coverage are summarized in [Auth Matrix](auth-matrix.md).

## Building Blocks

- [Workflows](workflows.md): orchestration and execution model
- [Integration Contract](integration-contract.md): app-backend boundary and trust model
- [API Surfaces](api-surfaces.md): where the dashboard and backend integration paths fit
- [Personal Chief of Staff v1](personal-chief-of-staff-v1.md): adjacent product pattern for structured execution

## Choose Command Center When

- you need a general-purpose operating layer rather than a narrow specialist mode
- the work spans planning, approval, execution, and review
- the same shell should support both operators and clients

## Next Step

If Command Center is the right surface, use [API Surfaces](api-surfaces.md) to choose the right contract for your frontend or backend, then use [Channels](channels.md) to choose where approvals and updates should appear.
