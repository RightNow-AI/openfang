# School Mode

School Mode is the education and program-operations surface for courses, cohorts, and recurring learner workflows.

---

## Best Fit

- courses and cohort programs
- onboarding and recurring student coordination
- curriculum and assignment production
- educational businesses that need approvals and durable records

## Operator Loop

1. define the program brief and learner promise
1. generate the curriculum and operations plan
1. approve lessons, assignments, and outbound communication
1. run cohort workflows and student support tasks
1. review student health, results, and follow-up work

## Current Product Surfaces

Current frontend route family:

- `/school/new`: create a new program and launch the wizard
- `/school/[programId]`: program overview and task list
- `/school/[programId]/cohort`: cohort and student-health surface
- `/school/[programId]/approvals`: approval queue
- `/school/[programId]/results`: program outputs and results

## Backend Contract

School Mode uses the shared mode-family API pattern under `school`:

- `POST /modes/school/records`
- `GET /modes/school/records`
- `GET /modes/school/records/{id}`
- `POST /modes/school/generate-plan`
- `POST /modes/school/tasks/{id}/run`

## Typical Work

- curriculum outline generation
- lesson and assignment drafting
- onboarding communication
- reminders and student follow-up
- cohort health tracking and testimonial capture

## Building Blocks

- [Workflows](workflows.md)
- [Security](security.md)
- [Configuration](configuration.md)
- [Channels](channels.md)

## Choose School Mode When

- the product has recurring participants or students
- the work combines content, coordination, and support
- program health matters as much as asset generation

## Next Step

If School Mode is the right fit, use [Channels](channels.md) to choose where onboarding and student updates should appear, then use [Integrations](integrations.md) to connect the program backend safely.
