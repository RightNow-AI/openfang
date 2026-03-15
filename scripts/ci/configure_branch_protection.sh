#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: configure_branch_protection.sh <owner/repo> [branch]

Configure GitHub branch protection with:
- required status checks
- at least one approving review
- stale review dismissal
- conversation resolution
- no force-push / no deletion

Environment overrides:
- REQUIRED_CHECKS_JSON='["check-name-1","check-name-2"]'
- REQUIRED_APPROVALS=1

Default REQUIRED_CHECKS_JSON:
- pre-pr-review-gate / pre-pr-review-gate
- CI / Check / ubuntu-latest
- CI / Test / ubuntu-latest
- CI / Clippy
- CI / Format
USAGE
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage >&2
  exit 2
fi

REPO="$1"
BRANCH="${2:-main}"
REQUIRED_APPROVALS="${REQUIRED_APPROVALS:-1}"

command -v gh >/dev/null 2>&1 || { echo "gh is required" >&2; exit 2; }
command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 2; }

DEFAULT_REQUIRED_CHECKS='[
  "pre-pr-review-gate / pre-pr-review-gate",
  "CI / Check / ubuntu-latest",
  "CI / Test / ubuntu-latest",
  "CI / Clippy",
  "CI / Format"
]'
CHECKS_JSON="${REQUIRED_CHECKS_JSON:-$DEFAULT_REQUIRED_CHECKS}"

PAYLOAD="$(CHECKS_JSON="$CHECKS_JSON" REQUIRED_APPROVALS="$REQUIRED_APPROVALS" python3 - <<'PY'
import json
import os
import sys

checks_raw = os.environ.get("CHECKS_JSON", "[]")
approvals_raw = os.environ.get("REQUIRED_APPROVALS", "1")

try:
    checks = json.loads(checks_raw)
except json.JSONDecodeError as exc:
    print(f"invalid REQUIRED_CHECKS_JSON: {exc}", file=sys.stderr)
    sys.exit(2)

if not isinstance(checks, list) or not all(isinstance(x, str) and x.strip() for x in checks):
    print("REQUIRED_CHECKS_JSON must be a JSON string array", file=sys.stderr)
    sys.exit(2)

try:
    approvals = int(approvals_raw)
except ValueError:
    print("REQUIRED_APPROVALS must be an integer", file=sys.stderr)
    sys.exit(2)

if approvals < 1:
    print("REQUIRED_APPROVALS must be >= 1", file=sys.stderr)
    sys.exit(2)

payload = {
    "required_status_checks": {
        "strict": True,
        # Use legacy `contexts` for portability across repos/forks where
        # check-runs may not yet exist at configuration time.
        "contexts": checks,
    },
    "enforce_admins": False,
    "required_pull_request_reviews": {
        "dismiss_stale_reviews": True,
        "require_code_owner_reviews": False,
        "required_approving_review_count": approvals,
        "require_last_push_approval": False,
    },
    "restrictions": None,
    "required_linear_history": True,
    "allow_force_pushes": False,
    "allow_deletions": False,
    "block_creations": False,
    "required_conversation_resolution": True,
    "lock_branch": False,
    "allow_fork_syncing": True,
}

print(json.dumps(payload))
PY
)"

printf 'Applying branch protection: repo=%s branch=%s\n' "$REPO" "$BRANCH"

gh api \
  --method PUT \
  -H "Accept: application/vnd.github+json" \
  "/repos/${REPO}/branches/${BRANCH}/protection" \
  --input - <<<"$PAYLOAD" >/dev/null

printf 'Branch protection applied successfully: %s/%s\n' "$REPO" "$BRANCH"
