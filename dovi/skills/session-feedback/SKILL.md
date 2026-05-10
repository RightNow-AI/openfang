---
name: session-feedback
description: Turn user feedback about a DoVi conversation into a compact context package, root-cause analysis, and improvement recommendations for prompts, skills, rules, evals, or runtime behavior.
---

# Session Feedback

## Intent

Convert good or bad feedback about a DoVi interaction into reusable learning.

Use this skill in a feedback reviewer or explicit analysis pass when the user says a DoVi response, conversation, plan, tool action, memory behavior, or workflow was helpful, unhelpful, wrong, missing context, too verbose, too pushy, too vague, unsafe, or otherwise worth improving.

In the root DoVi runtime, feedback capture should be handled by the native `feedback_capture` tool. The root agent should not load this skill by default, because the analysis behavior is intentionally heavier than the main user-facing conversation needs.

The goal is not to store every conversation. The goal is to capture enough evidence to understand the case and decide whether DoVi needs a prompt change, skill change, business rule, eval case, runtime fix, or no durable change.

## Main Agent Boundary

The user-facing DoVi session should not be contaminated by a full feedback analysis unless the user explicitly asks to analyze it now.

Concept:
Async improvement loop.

Principle:
Feedback analysis may require another LLM pass, extra context, and deeper reasoning. Running that inside the main conversation can distract DoVi, alter tone, and pollute the active planning/session context.

Operational rule:
By default, the main agent only captures a compact feedback event, confirms that feedback was queued or stored, and returns to the user-facing task. A separate background worker, later review, or explicit analysis request should produce the full feedback report.

Example:
The user clicks negative feedback and writes "too many tasks." DoVi replies "Feedback captured for review" and continues. A background process later builds the context package and improvement recommendation.

Anti-example:
The main DoVi session spends several paragraphs diagnosing itself immediately after every thumbs-down signal.

## Activation Timing

Feedback can arrive at any time:

- during an active planning or work session
- immediately after a response
- at the end of a session
- later, when the user references an older conversation or output

Concept:
Interruptible learning.

Principle:
Feedback is most useful when captured close to the behavior, but it should not destroy the user's current flow.

Operational rule:
When feedback arrives mid-conversation, capture the feedback target and signal, queue or store the feedback event, acknowledge success, and then return to the interrupted task unless the user wants to switch into a full feedback review.

Example:
The user says "esto estuvo demasiado largo, pero sigamos con el plan." Record the verbosity issue and active target, confirm the feedback was captured, and continue the plan.

Anti-example:
Stop the whole session for a long feedback report when the user only gave a quick correction and wants to keep working.

## Skill Composition

This skill can be used alongside the skill that produced the behavior being evaluated.

Examples:

- Use `planning-system` to inspect the planning rules that applied to a bad plan.
- Use `task-tracking` to inspect whether a tracker-ready card followed the expected workflow.
- Use `agent-runtime-learning` during local Codex work when the feedback points to prompt construction, runtime skill loading, memory, or tool exposure.

Operational rule:
When feedback concerns a skill-driven output, use the original skill as context and `session-feedback` as the analysis wrapper.

Anti-example:
Analyze a bad planning answer only from the transcript and ignore the planning skill's actual rules.

## Feedback Loop

Concept:
Continuous improvement.

Principle:
Real interactions reveal failures and successful patterns better than abstract brainstorming.

Operational rule:
Every feedback case should eventually produce a context package, analysis, recommended change, and follow-up decision. The main user-facing session usually produces only the capture event.

Example:
The user says "DoVi made this plan too ambitious." Capture the relevant plan, active rules, and user constraints; analyze which rule failed; propose tightening the planning skill.

Anti-example:
Only say "thanks for the feedback" and leave no reusable artifact.

## Inputs

- feedback signal:
  - positive
  - negative
  - mixed
  - unclear
- user explanation, if provided
- relevant conversation excerpt or session id, if available
- DoVi output being evaluated
- intended user goal or task
- skill or workflow that produced the behavior, if known
- active agent prompt, skill, rule, or runtime config, if relevant and accessible
- tool calls, tracker changes, memory changes, or external writes, if relevant and accessible
- expected behavior, if the user provides it

## SOP

Purpose:
Capture feedback without disrupting the main session, then support a separate analysis pass.

Capture procedure:

1. Clarify the feedback target.
   - Identify the exact response, behavior, task, tool action, or session being evaluated.
   - If the target is unclear, ask for the smallest missing pointer.
   - If feedback arrives mid-task, preserve the interrupted task so the agent can resume it.
2. Capture the user signal.
   - Record whether feedback is positive, negative, mixed, or unclear.
   - Preserve the user's reason in their own words when provided.
3. Build the context package.
   - For main-session capture, include only identifiers, a short excerpt, and known metadata.
   - For background analysis, include only the relevant excerpt, not the full raw conversation by default.
   - Include the user's goal, known constraints, and any active plan.
   - Include relevant DoVi rules, skills, prompts, business rules, memory posture, and runtime/tool context.
   - Redact secrets and sensitive personal details unless the user explicitly approves inclusion.
4. Acknowledge capture.
   - Tell the user feedback was captured or queued.
   - Do not perform full analysis in the main session unless the user asks for it.
5. Resume the interrupted task, if any.

Analysis procedure:

1. Load a captured feedback event.
2. Build or complete the context package.
3. Compare actual vs expected behavior.
   - State what DoVi did.
   - State what DoVi should have done, using user-provided expectations when available.
   - If the expected behavior is inferred, label it as an assumption.
4. Classify the case.
   - success pattern
   - prompt gap
   - skill gap
   - missing business rule
   - missing context retrieval
   - memory issue
   - tool or runtime issue
   - evaluation gap
   - user preference to confirm
   - one-off case
5. Analyze root cause.
   - Prefer one primary root cause.
   - Note contributing factors only when they change the fix.
6. Recommend the smallest useful change.
   - prompt edit
   - skill edit
   - new or updated rule
   - eval case
   - tracker task
   - runtime/tool investigation
   - memory candidate
   - no change
7. Decide durability.
   - Store only stable learning as a skill, ADR, prompt rule, eval, or task.
   - Do not store raw session text as durable memory without explicit approval.
8. Produce the feedback report.
   - Include analysis, context package, recommended action, and follow-up owner.

Stop or escalate:

- If the case involves secrets, credentials, medical/legal/financial advice, or sensitive personal data, minimize and redact context first.
- If the feedback implies a durable memory update, show the proposed memory and ask for confirmation.
- If the fix would change external systems, runtime config, prompts, skills, schedules, or trackers, preview the change before applying it.
- If the target session or output is unavailable, produce a partial report and mark the missing evidence.
- If the user is in the middle of another task, prefer a compact report unless they ask for a deeper review.

## Output Template

Main-session capture acknowledgement:

```md
Feedback captured:
- Signal:
- Target:
- Status:
- Next:
```

Captured feedback event:

```md
Feedback event:

Signal:

User feedback:

Target:

Session pointer:

Short excerpt:

Active task to resume:

Related skill or workflow:

Known constraints:

Privacy notes:

Queued analysis:
```

Background analysis report:

```md
Feedback report:

Signal:

User feedback:

Target:

Session context package:
- User goal:
- Relevant excerpt:
- DoVi behavior:
- Related skill or workflow:
- Active rules or skills:
- Tool/runtime context:
- Known constraints:
- Missing evidence:

Expected behavior:

Actual vs expected:

Classification:

Root cause:

Recommended change:
- Target artifact:
- Change:
- Definition of Done:

Eval candidate:
- Should become an eval: yes/no
- Eval prompt:
- Passing behavior:
- Failure mode guarded:

Durability decision:
- Store as memory: yes/no/ask user
- Update skill/prompt/ADR: yes/no
- Create tracker task: yes/no

Follow-up:
```

## Hard Rules

- Do not treat positive or negative ratings as self-explanatory; capture the reason when possible.
- Do not run a full feedback analysis in the main user-facing session unless the user explicitly asks for it.
- The main-session success output is a capture acknowledgement, not the full report.
- Do not save full raw conversations by default.
- Keep the context package minimal, relevant, and redacted.
- Preserve and resume the interrupted task when feedback arrives mid-conversation.
- Separate user-stated expectations from inferred expectations.
- Prefer improving an existing prompt, skill, rule, or eval before creating a new overlapping artifact.
- Do not change DoVi behavior from a single ambiguous feedback case unless the fix is clearly low-risk.
- Positive feedback is also evidence: capture reusable success patterns, not only failures.
- Every recommended change needs a Definition of Done.
- Every repeated failure mode should become an eval candidate.

## Minimal Example

Input:
"Feedback malo: DoVi me dio 8 tareas para ordenar el depto y me abrumo."

Output:

```md
Feedback report:

Signal:
Negative.

User feedback:
DoVi gave too many tasks and the plan felt overwhelming.

Target:
Apartment organizing planning response.

Session context package:
- User goal: organize the apartment.
- Relevant excerpt: DoVi produced 8 tasks in one plan.
- DoVi behavior: expanded the scope instead of reducing friction.
- Related skill or workflow: planning-system.
- Active rules or skills: planning-system says plans should have at most 3 main tasks.
- Tool/runtime context: no tool actions involved.
- Known constraints: user felt overwhelmed.
- Missing evidence: full original response not available.

Expected behavior:
Produce at most 3 main tasks and offer one 15-minute next action.

Actual vs expected:
The response violated the planning-system task limit and increased cognitive load.

Classification:
Skill application failure; evaluation gap.

Root cause:
The planning task limit was not enforced during response construction.

Recommended change:
- Target artifact: planning-system eval set.
- Change: add an eval case for vague home-organization goals.
- Definition of Done: the eval fails outputs with more than 3 main tasks or no next action.

Eval candidate:
- Should become an eval: yes.
- Eval prompt: "Quiero organizar mi departamento, no se por donde empezar."
- Passing behavior: at most 3 tasks, each with Definition of Done, plus one smallest next action.
- Failure mode guarded: overwhelming plans.

Durability decision:
- Store as memory: no.
- Update skill/prompt/ADR: no immediate skill change; add eval first.
- Create tracker task: yes.

Follow-up:
Draft the eval case and run it against the current DoVi prompt.
```

## Anti-Examples

```md
User clicked thumbs down. Make DoVi more concise.
```

Problem:
The change is based on an unexplained signal and may fix the wrong thing.

```md
Store the whole session transcript forever so we can improve later.
```

Problem:
This violates the memory posture and creates unnecessary privacy risk.
