---
name: planning-system
description: Turn vague goals, projects, daily plans, reviews, or prioritization needs into theory-backed implementation plans without inventing context.
---

# Planning System

## Intent

Turn vague objectives into concrete, small, trackable actions.

For project work, every plan must connect theory to implementation:

- concept
- principle
- operational rule
- concrete task or artifact

Never invent facts, estimates, dates, deadlines, user preferences, available time, energy level, progress, or constraints. If something is unknown, mark it as an assumption to validate or ask the user.

## SOP

Purpose:
Turn a vague objective into a theory-backed implementation plan.

Inputs:

- user objective
- known context or constraints
- available artifacts or notes, if provided
- available time, if provided
- energy level, if provided
- existing tasks or commitments, if provided
- Definition of Done, if provided
- task granularity and estimation details, if provided

Procedure:

1. Gather available context before asking questions.
2. State the objective and desired outcome.
3. Separate known facts, assumptions to validate, and open questions.
4. Decompose vague tasks before estimating duration or scheduling them.
5. Mark each task as estimation-ready or not ready.
6. Ask only for missing information that materially changes the plan.
7. Select the relevant theory and convert it into an operational rule.
8. Produce a decision-complete plan with up to 3 implementation tasks, Definition of Done, and next action.

Stop or escalate:

- If required facts are missing, ask the user or mark assumptions to validate.
- If information is discoverable from provided artifacts, inspect or cite the artifact instead of asking.
- If the plan needs more than 3 tasks, split the scope.
- If tasks are vague, return to objective clarification.
- If the user asks for a timed schedule but task size is unknown, produce a discovery plan or timeboxes instead of invented estimates.
- If the user wants tracker-ready tasks, format them as plain handoff records and name a target tracker only if the user provided one.

## Output Template

```md
Objective:

Desired outcome:

Known facts:
-

Assumptions to validate:
-

Open questions:
-

Decomposition:
- Task:
  Subtasks:
  Estimation readiness:
  Missing detail:

Theory:
- Concept:
  Principle:
  Operational rule:
  Why it matters now:

Implementation:
- Artifact or target:
  Change or action:
  Definition of Done:

Top 3 tasks:
- Task:
  Label:
  Theory link:
  Implementation target:
  Definition of Done:
  Next action:

Review trigger:
```

## Hard Rules

- Do not invent context.
- Use only user-provided, artifact-backed, or explicitly validated information.
- Inspect available context before asking questions.
- Do not ask questions that can be answered from provided context or artifacts.
- Ask only questions that materially change the plan.
- Label uncertain information as an assumption to validate.
- Do not estimate or schedule vague tasks until they are decomposed enough to judge size.
- Distinguish an estimate from a timebox: an estimate predicts likely duration; a timebox limits exploration when duration is unknown.
- If duration depends on unknown scope, tools, materials, energy, or environment, ask for the missing detail or propose a first inspection step.
- Do not format tasks for tracking until the objective, outcome, and next action are clear.
- Limit a session plan to at most 3 main tasks.
- Every task must have a Definition of Done.
- Theory must produce an operational rule before it produces a task.
- Make the next execution step decision-complete: the user should know exactly what to do next.
- Prefer the smallest action that creates visible progress.
- Replace motivation with a smaller executable action.

## Minimal Example

Input:
"I want to start reading more consistently. I drink coffee every morning and want something easy to start with."

Output:

```md
Objective:
Build a consistent reading habit.

Desired outcome:
The user completes a small reading action after morning coffee often enough to review whether the habit is sticking.

Known facts:
- The user wants to read more consistently.
- The user drinks coffee every morning.
- The user wants an easy starting point.

Assumptions to validate:
- Morning coffee is a reliable anchor.
- Reading one page is small enough to start.

Open questions:
- Does the user already have a book selected?
- Should the habit happen on weekdays only or every day?

Theory:
- Concept: Habit stacking
  Principle: A new behavior is easier to start when attached to an existing routine.
  Operational rule: After morning coffee, read one page.
  Why it matters now: The user already has a stable cue.

- Concept: Make it easy
  Principle: Smaller actions reduce friction.
  Operational rule: Start with one page, not a full chapter.
  Why it matters now: The goal is consistency before volume.

Implementation:
- Artifact or target: Morning routine
  Change or action: Place the selected book near the coffee spot before the next morning.
  Definition of Done: The book is visible where coffee is prepared.

Top 3 tasks:
- Task: Select one book
  Label: Design
  Theory link: Make it easy
  Implementation target: Reading setup
  Definition of Done: One book is chosen and placed near the coffee spot.
  Next action: Pick the book before tomorrow morning.

- Task: Perform the first reading action
  Label: Implementation
  Theory link: Habit stacking
  Implementation target: Morning coffee routine
  Definition of Done: After coffee, the user reads at least one page.
  Next action: Put the book next to the coffee setup.

- Task: Review after 3 attempts
  Label: Validation
  Theory link: Make it easy
  Implementation target: Habit review
  Definition of Done: The user records whether the action happened and what made it easier or harder.
  Next action: Set the review trigger.

Review trigger:
After 3 morning coffee attempts.
```

## Anti-Examples

```md
You have 3 hours today, high energy, and should finish the whole planning module by tonight.
```

Problem:
The output invents available time, energy level, deadline, and scope.

```md
12:30-14:00: Install bathroom fixtures.
15:30-17:30: Organize and clean the house.
17:30-19:30: Iterate DoVi.
```

Problem:
The output turns broad tasks into precise time estimates without decomposing the work or validating scope, materials, tools, and Definition of Done.

Better pattern:

```md
First slice:
- Inspect bathroom fixtures for 15 minutes.
  Definition of Done: number of fixtures, mounting surface, tools, and missing materials are known.
- Decide whether installation is estimation-ready.
  If ready: estimate and schedule.
  If not ready: create a shopping or preparation task.
```

```md
Design the whole operating system, choose all runtimes, build memory, integrate every tracker, create evaluations, and automate project tracking all at once.
```

Problem:
The plan jumps beyond known scope and treats unvalidated future choices as if they were already decided.
