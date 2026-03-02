---
name: tester-hand-skill
version: "1.0.0"
description: "Expert knowledge for context-driven testing — exploratory methodology, heuristic oracles, session management, coverage models, risk analysis, and bug advocacy"
runtime: prompt_only
---

# Context-Driven Testing Expert Knowledge

## The Seven Principles

1. **The value of any practice depends on its context** — no practice is universally good or bad
2. **There are good practices in context, but no best practices** — what works here may fail there
3. **People, working together, are the most important part of any project's context** — tools serve people, not the reverse
4. **Projects unfold over time in ways that are often not predictable** — testing must adapt continuously
5. **The product is a solution — if it doesn't solve the problem, nothing else matters** — test against purpose, not just specification
6. **Good software testing is a challenging intellectual process** — it requires skill, judgment, and creativity
7. **Only through judgment and skill can we do the right things at the right times to effectively test our products** — methodology is a guide, not a cage

---

## Testing vs. Checking

Two fundamentally different activities:

| Aspect | Testing | Checking |
|--------|---------|----------|
| Nature | Investigation, exploration, learning | Verification, confirmation, comparison |
| Cognition | Sapient — requires human-like judgment | Algorithmic — can be fully scripted |
| Goal | Find new information, reveal risks | Confirm known expectations still hold |
| Adaptability | Adapts in real-time to observations | Fixed — same steps every time |
| Automation | Cannot be automated (the thinking part) | Can and should be automated |
| Value | Discovers unknown problems | Detects known regressions |

**Critical insight**: When someone says "automate testing," they mean automate **checks**. Testing is the sapient process of designing experiments, interpreting results, and making judgments about quality. Checks are a subset — valuable, but they only confirm what we already expect. They cannot find problems we haven't imagined.

A passing check suite means: "Nothing we already know about has regressed." It does NOT mean: "The product works."

---

## Exploratory Testing

Simultaneous **learning**, **test design**, and **test execution**. Not ad-hoc — it is structured by charters, time-boxes, and heuristics.

### Why It Works
- The tester's brain is the most powerful test design tool
- The next best test is informed by the result of the previous test
- Scripted tests lock in assumptions; exploration challenges them
- Products contain surprises that no script anticipates

### Session-Based Test Management (SBTM)

Organize exploratory testing into accountable, measurable sessions:

**Session structure**:
1. **Charter**: A mission statement — what to explore, what to look for, what technique to use
2. **Time-box**: Fixed duration (30/60/90 min). Prevents rabbit holes and ensures breadth.
3. **Execution**: Explore, take notes, follow threads, apply oracles
4. **Debrief**: Review findings, assess coverage, plan next session

**Charter template**: `Explore [target] using [technique] to discover [information about risk]`

Examples:
- "Explore the login flow using boundary analysis to discover authentication edge cases"
- "Explore the payment module using soap opera scenarios to discover state management failures"
- "Explore the API using claims testing to discover discrepancies between docs and behavior"

**Session metrics** (per session):
- **Bugs found**: Count and severity distribution
- **Questions raised**: Unresolved ambiguities about requirements or behavior
- **Coverage**: Which product areas were touched (SFDPOT dimensions)
- **Session-to-bug ratio**: Efficiency indicator across sessions
- **Percentage of session spent on**: setup / testing / bug investigation / reporting

---

## Heuristic Test Strategy Model (HTSM)

A framework for designing test strategies by analyzing four interconnected elements:

### 1. Project Environment
Factors that shape what testing is possible and valuable:
- **Mission**: What does the customer/stakeholder need from testing?
- **Information**: What docs, specs, code, experts are available?
- **Developer relations**: How responsive is the dev team? How do they handle bug reports?
- **Test environment**: What hardware, tools, data, access do we have?
- **Schedule**: How much time is available? What are the deadlines?
- **Test items**: What deliverables exist to test? What's testable now?

### 2. Product Elements (SFDPOT)
What to cover — the dimensions of the product:

| Dimension | What to Examine | Example Questions |
|-----------|----------------|-------------------|
| **Structure** | Code, components, architecture, dependencies | How are modules coupled? What's the dependency graph? |
| **Function** | Features, operations, user-facing capabilities | What can users do? What are the business rules? |
| **Data** | Inputs, outputs, stored state, transformations | What data types, ranges, formats? Where does data flow? |
| **Platform** | OS, runtime, hardware, network, browsers | What environments must it work on? What varies? |
| **Operations** | Install, config, startup, recovery, maintenance | What happens during upgrades? Backup/restore? |
| **Time** | Concurrency, timeouts, scheduling, state aging | What happens under load? After running for days? Race conditions? |

### 3. Quality Criteria
What "good" means — select the relevant subset for this product:

Capability, Reliability, Usability, Charisma (does it delight?), Security, Scalability, Performance, Installability, Compatibility, Supportability, Testability, Maintainability, Portability, Localizability

### 4. Test Techniques
How to generate tests. Key families:

**Function testing**: Test what it does — features, operations, error handling, business rules.
**Domain testing**: Analyze input/output spaces — equivalence classes, boundaries, special values, combinations.
**Stress testing**: Push beyond expected limits — max load, huge data, resource starvation, rapid input.
**Flow testing**: Test sequences and state transitions — happy paths, interrupts, abort-and-resume, parallel flows.
**Claims testing**: Verify every claim made in specs, docs, help text, marketing, error messages.
**Scenario testing**: Realistic end-to-end stories — "A user tries to do X on a slow connection on a Friday afternoon."
**Soap opera testing**: Exaggerated dramatic scenarios — complex state, interleaved operations, unlikely-but-possible sequences.
**Risk-based testing**: Focus effort on areas with highest probability × impact of failure.
**Touring**: Systematic exploration using tour metaphors — guidebook (follow the docs), money (test what the customer pays for), landmark (navigate between known features), intellectual (ask hard questions), back-alley (visit obscure features), garbage collector (look for dead/deprecated things).

---

## HICCUPPS — Oracle Heuristics

An oracle is a mechanism for recognizing a problem. No oracle is perfect. Use multiple oracles and acknowledge their limitations.

| Oracle | The product should be consistent with... | Example |
|--------|----------------------------------------|---------|
| **History** | Previous versions of itself | "This used to work in v2.3" |
| **Image** | An image the organization wants to project | "Our brand is premium — this error page is sloppy" |
| **Comparable Products** | Similar products or competitors | "Every other file manager supports drag-and-drop" |
| **Claims** | Documentation, specs, help text, marketing | "The docs say max file size is 10MB, but it rejects 8MB" |
| **User expectations** | What a reasonable user would expect | "I expected Ctrl+Z to undo, but it did nothing" |
| **Product** | Other parts of the same product | "Dates are DD/MM/YYYY here but MM/DD/YYYY on the settings page" |
| **Purpose** | The explicit and implicit purpose of the product | "A security tool that exposes its own API without auth" |
| **Standards** | Applicable standards, regulations, conventions | "WCAG requires color contrast ratio ≥ 4.5:1" |

**When oracles conflict**: Note the conflict explicitly. "The spec says X, but the user expects Y." This is itself a finding — the product or spec needs clarification.

---

## Risk-Based Testing

### Risk Identification Heuristics
- **Newness**: New code, new developers, new technology = higher risk
- **Complexity**: Complex logic, many dependencies, deep nesting = higher risk
- **Change frequency**: Code that changes often breaks often
- **Bug history**: Areas with past bugs tend to have more bugs
- **Developer concern**: "I'm not sure about this part" = test it hard
- **Dependency risk**: External services, third-party libraries, platform quirks
- **Ambiguity**: Vague or conflicting requirements = the product was built on assumptions

### Risk Assessment Matrix

| Probability | Impact: Low | Impact: Medium | Impact: High |
|-------------|-------------|----------------|--------------|
| High | Medium priority | High priority | Critical |
| Medium | Low priority | Medium priority | High priority |
| Low | Informational | Low priority | Medium priority |

Reassess risk after each testing session — what you learn changes the risk landscape.

---

## Bug Advocacy

A bug report is an **argument**. Its job is to persuade a busy developer or product manager that this problem deserves attention.

### What Makes a Bug Report Compelling
1. **Reproducible**: Minimal steps that anyone can follow
2. **Specific**: Exact inputs, exact outputs, exact environment
3. **Impactful**: Clearly states who is affected and how badly
4. **Evidenced**: Includes logs, screenshots, data, error messages
5. **Contextualized**: Explains which oracle was violated and why it matters
6. **Neutral tone**: Reports facts, not blame. "The system does X" not "Someone broke Y"

### Severity Classification
| Level | Criteria |
|-------|----------|
| Critical | Data loss, security breach, complete feature failure, crash |
| High | Major feature broken, workaround exists but is painful |
| Medium | Feature partially works, minor data issue, cosmetic in critical path |
| Low | Cosmetic, minor inconvenience, edge case with trivial impact |

### Common Bug Report Failures
- **Missing oracle**: "It looks wrong" — wrong compared to *what*?
- **Not reproducible**: Steps too vague or environment-dependent
- **Buried impact**: The business risk is hidden at the bottom or absent
- **Assumed fix**: "The button should be blue" — report the problem, not the solution
- **Duplicate noise**: Check for existing reports before filing

---

## Coverage Analysis

### Coverage Is Not a Number — It's a Story

"80% code coverage" says nothing about quality. Coverage should answer: **What have we tested, how deeply, and what have we deliberately left untested (and why)?**

### Multi-Dimensional Coverage Model

Track coverage across SFDPOT dimensions per session:

| Depth Level | Meaning |
|-------------|---------|
| **Untouched** | Not tested at all in any session |
| **Shallow** | Briefly exercised, happy path only |
| **Moderate** | Multiple scenarios, some edge cases |
| **Deep** | Thorough investigation, adversarial scenarios, stress |

Aggregate across sessions to build a **product coverage map**. Identify:
- **Hot spots**: Areas tested repeatedly (potential over-testing)
- **Cold spots**: Areas never or barely tested (gaps)
- **Risk-aligned**: Do the deepest-tested areas match the highest-risk areas?

---

## Testability Assessment

If a product is hard to test, that is itself a quality problem. Evaluate:

| Dimension | Question | Poor Testability Signal |
|-----------|----------|----------------------|
| **Observability** | Can I see what the product is doing? | No logs, opaque errors, hidden state |
| **Controllability** | Can I put the product into the state I need? | No test hooks, hard-coded config, long setup |
| **Decomposability** | Can I test parts independently? | Tight coupling, no interfaces, monolith |
| **Simplicity** | Is the product as simple as it could be? | Unnecessary complexity, dead code, over-engineering |
| **Stability** | Does the product stay still long enough to test? | Constantly changing, non-deterministic, flaky behavior |
| **Information** | Do I have specs, docs, and knowledgeable people? | No docs, tribal knowledge, ambiguous requirements |

Report testability problems as findings — they block effective testing and often indicate deeper design issues.

---

## Rapid Testing Principles

Efficient testing under time pressure:

1. **Start with risk**: Don't test everything — test what matters most
2. **Use heuristics, not exhaustive analysis**: Good enough coverage, fast
3. **Test the claims first**: What does the product *promise*? Verify those claims.
4. **Follow the money**: Test what the customer pays for
5. **Look for inconsistency**: Internal contradictions reveal bugs faster than systematic feature walks
6. **Use variation**: Change one thing at a time, observe the effect
7. **Trust your confusion**: If you're confused by the product, users will be too — that's a finding
8. **Stop when the risk is acceptable**: Not when the test plan is "complete"

---

## Test Framing

Before any testing effort, establish:

| Element | Question | Example |
|---------|----------|---------|
| **Mission** | Why are we testing? What decision depends on this? | "Determine if v3.1 is safe to release" |
| **Scope** | What parts of the product are in/out of scope? | "Payment flow only, not admin panel" |
| **Oracles** | How will we recognize a problem? | "HICCUPPS + regression against v3.0 behavior" |
| **Coverage** | How will we know we've tested enough? | "All SFDPOT dimensions at moderate+ depth for payment" |
| **Risk** | What are we most worried about? | "Data loss during concurrent transactions" |
| **Stakeholders** | Who needs the test results? What format? | "Product owner — brief report with risk summary" |

