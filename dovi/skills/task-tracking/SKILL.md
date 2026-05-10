---
name: task-tracking
description: Convert planned actionable work into Trello-ready tracker cards without using the tracker as the knowledge base.
---

# Task Tracking

## Intent

Convert planned work into external tracker state, initially Trello.

Planning comes first. Tracking should not turn vague goals into vague cards.

## SOP

Purpose:
Represent actionable planned work as tracker-ready cards.

Inputs:

- actionable task
- Definition of Done
- project or objective
- current state, if known
- label, if useful
- due date, only if real
- relevant repo links, if useful

Procedure:

1. Confirm the item is actionable and comes from a plan.
2. Choose the correct list: `Backlog`, `TODO`, `WIP`, or `DONE`.
3. Choose one primary label: `Reading`, `Design`, `Implementation`, `Validation`, or `Decision`.
4. Write a concise card title.
5. Add context, task statement, Definition of Done, and useful links.
6. Keep durable planning rationale in the repository, not in the card.

Stop or escalate:

- If the item is vague, send it back to `planning-system`.
- If Definition of Done is missing, ask for it or draft one for validation.
- If creating or moving a real external card, ask for approval unless explicitly delegated.

## Output Template

```md
Title:

List:

Labels:

Context:

Task:

Definition of Done:
-

Links:
-

Notes:
-
```

## Hard Rules

- One card equals one actionable task.
- Do not use Trello as the knowledge base.
- Do not create cards for vague intentions.
- Do not create cards without Definition of Done.
- Do not add long research excerpts to cards.
- Move cards by execution state, not emotional urgency.
- Keep WIP small.
- Do not perform tracker side effects without explicit delegation or approval.

## Minimal Example

```md
Title: Validate planning-system on one DoVi objective
List: TODO
Labels: Validation
Context: The planning-system skill should be usable before adding more skills.
Task: Run one planning session using `skills/planning-system/SKILL.md`.
Definition of Done:
- One DoVi objective is planned.
- The plan includes theory, implementation, max 3 tasks, and Definition of Done for each task.
Links:
- `skills/planning-system/SKILL.md`
- `project/backlog.md`
```

## Anti-Example

```md
Title: Build DoVi
List: TODO
Labels: Implementation
Definition of Done:
- unclear
```

Problem:
The card is too broad, has no concrete action, and cannot be verified.
