# Growth Mode

Growth Mode is the campaign and acquisition surface for work that blends research, creative generation, execution, and iteration.

---

## Best Fit

- campaign planning and launch
- creative testing loops
- outreach and acquisition systems
- marketing execution that needs tight feedback cycles

## Operator Loop

1. define the offer, campaign, or channel goal
1. generate the execution plan from the task catalog
1. produce creative and channel assets
1. approve high-risk assets before publishing
1. review results and plan the next iteration

## Current Product Surfaces

Current frontend route family:

- `/growth/new`: launch a new campaign plan
- `/growth/[campaignId]`: campaign overview and task list
- `/growth/[campaignId]/studio`: video ad studio surface
- `/growth/[campaignId]/approvals`: approval queue
- `/growth/[campaignId]/results`: campaign results and outputs

## Backend Contract

Growth Mode uses the shared mode-family API pattern under `growth`:

- `POST /modes/growth/records`
- `GET /modes/growth/records`
- `GET /modes/growth/records/{id}`
- `POST /modes/growth/generate-plan`
- `POST /modes/growth/tasks/{id}/run`

## Typical Work

- offer and angle development
- competitor ad research
- hook and script generation
- email and creative asset drafting
- optimization planning based on results

## Building Blocks

- [Workflows](workflows.md)
- [Providers And Models](providers-and-models.md)
- [API Surfaces](api-surfaces.md)
- [Channels](channels.md)

## Choose Growth Mode When

- speed of iteration matters more than static planning alone
- the main unit of work is a campaign, asset set, or growth experiment
- provider choice and output speed directly affect the operator experience

## Next Step

If Growth Mode is the right fit, use [Providers And Models](providers-and-models.md) to choose the model stack for research and content work, then use [Channels](channels.md) to decide where notifications, approvals, and results should surface.
