# Agency Mode

Agency Mode is the service-delivery surface for client work that moves from brief to scoped plan to approvals to final delivery.

---

## Best Fit

- client services and retainers
- repeatable delivery playbooks
- external work that needs explicit review before sending or publishing

## Operator Loop

1. capture the client brief
1. generate the delivery plan from the task catalog
1. review tasks that require approval
1. run execution tasks
1. review packaged results and follow-ups

## Current Product Surfaces

Current frontend route family:

- `/agency/new`: start a new agency record and task plan
- `/agency/[clientId]`: overview, task list, approvals, and result links
- `/agency/[clientId]/approvals`: approve client-facing work
- `/agency/[clientId]/results`: inspect completed outputs

The wizard and detail pages are implemented directly in the Next.js app and map to the `agency` mode record and task APIs.

## Backend Contract

Agency Mode uses the mode-family API pattern:

- `POST /modes/agency/records`
- `GET /modes/agency/records`
- `GET /modes/agency/records/{id}`
- `POST /modes/agency/generate-plan`
- `POST /modes/agency/tasks/{id}/run`

The broader mode contract is covered in [Auth Matrix](auth-matrix.md) and the general API behavior is covered in [API Surfaces](api-surfaces.md).

## Typical Work

- intake and scoping
- competitor and business research
- brand voice and delivery planning
- client draft generation
- approval-gated outbound communication and final packaging

## Building Blocks

- [Workflows](workflows.md)
- [Security](security.md)
- [Production Checklist](production-checklist.md)
- [Channels](channels.md)

## Choose Agency Mode When

- the work is client-facing
- approvals are part of the normal operating model
- you need a strong handoff from planning to delivery

## Next Step

If Agency Mode is the right fit, use [Channels](channels.md) to decide where client communication and approval notifications should land, then use [Integrations](integrations.md) to choose how your backend should own the workflow.
