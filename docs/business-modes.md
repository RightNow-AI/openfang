# Business Modes

LegendClaw's business modes are the opinionated product surfaces built on top of the core runtime: workflows, approvals, memory, scheduling, results tracking, and agent routing. This page is the top-level entry point for operators deciding which product shape to use first.

---

## Start Here

- Read [Workflows](workflows.md) for the execution model behind every mode.
- Read [API Reference](api-reference.md) if you are wiring a custom frontend or backend.
- Read [Personal Chief of Staff v1](personal-chief-of-staff-v1.md) for a concrete example of a structured operating surface.
- Read [Launch Roadmap](launch-roadmap.md) for the current sequencing of product-facing work.

## What A Business Mode Is

Each mode packages the same underlying runtime differently:

- a clearer operator workflow
- a tighter agent bundle
- opinionated approval boundaries
- default plans, tasks, and results views
- routing that matches a business job instead of a generic chat session

If you only need raw orchestration, start with [Workflows](workflows.md). If you need a product surface for repeated operational work, start here.

## Current Mode Families

### Command Center

Use Command Center when you need a shared operating layer for onboarding, task assignment, approvals, execution, and results review across client or internal work.

Read more: [Command Center](command-center.md)

Best fit:

- client delivery operations
- account management and approvals
- multi-agent execution with visible status and review loops

Primary references:

- [Workflows](workflows.md)
- [API Reference](api-reference.md)
- [Integration Contract](integration-contract.md)

### Agency Mode

Use Agency mode for service delivery where the system needs to move from brief to plan to approval to execution with clear accountability.

Read more: [Agency Mode](agency-mode.md)

Best fit:

- scoped service work
- repeatable delivery playbooks
- approvals before external actions

Primary references:

- [Workflows](workflows.md)
- [Security](security.md)
- [Production Checklist](production-checklist.md)

### Growth Mode

Use Growth mode for campaign, content, outreach, and optimization loops that need research, asset generation, review, and results tracking.

Read more: [Growth Mode](growth-mode.md)

Best fit:

- lead generation
- content operations
- campaign execution and iteration

Primary references:

- [Workflows](workflows.md)
- [Providers](providers.md)
- [API Reference](api-reference.md)

### School Mode

Use School mode for programs that need structured planning, recurring coordination, and durable records across cohorts or learners.

Read more: [School Mode](school-mode.md)

Best fit:

- curriculum and program operations
- enrollment and intake flows
- recurring student support or follow-up work

Primary references:

- [Workflows](workflows.md)
- [Security](security.md)
- [Configuration](configuration.md)

### Chief Of Staff Mode

Use this mode when the system needs to operate as a structured assistant for planning, delegation, follow-through, and reporting instead of open-ended chat.

Read more: [Chief Of Staff Mode](chief-of-staff-mode.md)

Primary references:

- [Personal Chief of Staff v1](personal-chief-of-staff-v1.md)
- [Workflows](workflows.md)
- [Agent Templates](agent-templates.md)

## How To Choose A Mode

- Start with Command Center if you need a general operating shell with plans, approvals, and results.
- Start with Agency mode if the work is client-facing and requires clear execution gates.
- Start with Growth mode if the main unit of work is campaigns, content, or outreach.
- Start with School mode if the main unit of work is a program with recurring participants.
- Start with Chief Of Staff mode if the main unit of work is decision support, follow-through, and coordination.

## Underlying Building Blocks

Every business mode composes the same lower-level layers:

- [Workflows](workflows.md) for orchestration
- [Agent Templates](agent-templates.md) for role-specific behavior
- [API Reference](api-reference.md) for app and dashboard integration
- [Security](security.md) for approvals, limits, and runtime protections
- [Integration Contract](integration-contract.md) for external app boundaries

## Dedicated Mode Pages

- [Command Center](command-center.md)
- [Agency Mode](agency-mode.md)
- [Growth Mode](growth-mode.md)
- [School Mode](school-mode.md)
- [Chief Of Staff Mode](chief-of-staff-mode.md)

## Next Step

After choosing a mode family, use [Channels](channels.md) to decide where the work should show up and [Integrations](integrations.md) to decide how external tools and apps should connect.
