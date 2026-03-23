# Chief Of Staff Mode

Chief Of Staff Mode is the structured planning and follow-through surface for work that should feel like operating rhythm support, not open-ended chat.

---

## Best Fit

- planning, delegation, and follow-through
- decision support with explicit assumptions and next actions
- recurring executive or operator review loops

## Operator Loop

1. collect context, priorities, and constraints
1. turn them into a plan and clear assignments
1. stop for approvals where external action or risk is involved
1. track progress and summarize status
1. produce a review and next-step brief

## Current Product Surfaces

This family is currently represented more by workflow shape and shell design than by a single standalone route family.

Closest current surfaces:

- [Personal Chief of Staff v1](personal-chief-of-staff-v1.md): product-shaping reference
- `/clients/[clientId]`: structured client shell for pulse, plan, approvals, and results
- `/clients/[clientId]/pulse`
- `/clients/[clientId]/plan`
- `/clients/[clientId]/approvals`
- `/clients/[clientId]/results`

The current implementation leans on the command-center backend and shared client shell rather than a separate backend contract just for Chief Of Staff mode.

## Building Blocks

- [Personal Chief of Staff v1](personal-chief-of-staff-v1.md)
- [Workflows](workflows.md)
- [Agent Templates](agent-templates.md)
- [Integration Contract](integration-contract.md)

## Choose Chief Of Staff Mode When

- the work is mostly planning, coordination, and review
- you want structured outputs with explicit assumptions and next actions
- a shared shell is more important than a narrow vertical workflow

## Next Step

If this is the right surface, start with [Personal Chief of Staff v1](personal-chief-of-staff-v1.md), then use [Command Center](command-center.md) and [API Surfaces](api-surfaces.md) to map the current implementation paths.
