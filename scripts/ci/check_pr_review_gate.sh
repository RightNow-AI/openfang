#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: check_pr_review_gate.sh [--body-file <path>]

Validate PR body contains required comprehensive pre-PR review sections and checked items.
By default, reads PR body from PR_BODY environment variable.
USAGE
}

BODY_FILE=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --body-file)
      [[ $# -ge 2 ]] || { echo "missing value for --body-file" >&2; exit 2; }
      BODY_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

BODY=""
if [[ -n "$BODY_FILE" ]]; then
  [[ -f "$BODY_FILE" ]] || { echo "body file not found: $BODY_FILE" >&2; exit 2; }
  BODY="$(cat "$BODY_FILE")"
else
  BODY="${PR_BODY:-}"
fi

BODY="$(printf '%s' "$BODY" | tr -d '\r')"
if [[ -z "${BODY//[[:space:]]/}" ]]; then
  echo "PR body is empty; cannot satisfy pre-PR review gate" >&2
  exit 1
fi

missing=0

require_section() {
  local section="$1"
  if ! printf '%s\n' "$BODY" | grep -Eiq "^##[[:space:]]+${section}[[:space:]]*$"; then
    echo "missing required section: ## ${section}" >&2
    missing=1
  fi
}

require_checked_item() {
  local phrase="$1"
  if ! printf '%s\n' "$BODY" | grep -Eiq "^- \[[xX]\][[:space:]].*${phrase}.*$"; then
    echo "missing required checked item containing: ${phrase}" >&2
    missing=1
  fi

  if printf '%s\n' "$BODY" | grep -Eiq "^- \[[[:space:]]\][[:space:]].*${phrase}.*$"; then
    echo "required item is present but unchecked: ${phrase}" >&2
    missing=1
  fi
}

require_section "Summary"
require_section "Scope"
require_section "Validation"
require_section "Comprehensive Pre-PR Review"
require_section "Findings"
require_section "Risks"
require_section "Rollback"

require_checked_item "required pre-PR review gates locally"
require_checked_item "concrete validation evidence"
require_checked_item "comprehensive review and recorded findings severity"
require_checked_item "documented rollback trigger and rollback steps"
require_checked_item "scoped to one concern"

if [[ $missing -ne 0 ]]; then
  echo "pre-PR review gate FAILED" >&2
  exit 1
fi

echo "pre-PR review gate passed"
