# PR Quality Gates

This document defines the fixed "review-first" PR workflow for OpenFang.

## Goals

- Prevent fast/partial reviews from entering `main`.
- Keep PRs small enough to review thoroughly.
- Ensure every review finding has a fix + regression validation loop.

## Mandatory Flow

1. Scope freeze
- One concern per PR (or one planned slice in a larger plan).
- State non-goals explicitly in the PR body.

2. Local pre-PR gate
- Run required local checks before changing PR state to ready.
- Capture concrete command output evidence in the PR body.

3. Comprehensive review (before ready)
- Review by severity first: High -> Medium -> Low.
- High issues must be fixed before `Ready for review`.
- Medium issues should be fixed unless there is a documented and accepted tradeoff.

4. Fix + regression loop
- For each finding: patch -> focused regression -> update review notes.
- Re-run gate commands after fixes.

5. Ready for review
- Only switch from Draft when all mandatory checklist items are checked and gate checks are green.

## Required Template Sections

The PR body must include:

- `## Summary`
- `## Scope`
- `## Validation`
- `## Comprehensive Pre-PR Review`
- `## Findings`
- `## Risks`
- `## Rollback`

The CI workflow `pre-pr-review-gate` enforces those sections and required checked items.

## Small PR Slice Rules

- Keep each PR to one theme.
- Prefer additive changes and isolated rollback.
- Include the focused tests/docs in the same slice.
- If a large effort is unavoidable, submit ordered slices (A/B/C...) and keep each independently mergeable.

## Branch Protection Baseline

Apply branch protection to your fork:

```bash
scripts/ci/configure_branch_protection.sh NextDoorLaoHuang-HF/openfang main
```

For a different repository:

```bash
scripts/ci/configure_branch_protection.sh <owner>/<repo> main
```

Default required checks applied by the script:

- `pre-pr-review-gate / pre-pr-review-gate`
- `CI / Check / ubuntu-latest`
- `CI / Test / ubuntu-latest`
- `CI / Clippy`
- `CI / Format`

Optional: customize required checks:

```bash
REQUIRED_CHECKS_JSON='["pre-pr-review-gate / pre-pr-review-gate","CI / Check / ubuntu-latest"]' \
  scripts/ci/configure_branch_protection.sh <owner>/<repo> main
```

## Ready-For-Review Gate

Do not switch a PR to ready unless all are true:

- Pre-PR gate commands passed.
- High findings are resolved.
- Findings/risk/rollback are documented in PR body.
- PR scope remains one concern per PR.
