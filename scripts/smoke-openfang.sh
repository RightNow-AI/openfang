#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: smoke-openfang.sh [base-url]

Run a small smoke test against a running OpenFang daemon.

Environment:
  OPENFANG_BASE_URL  Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY   Bearer token used for protected operational endpoints
  OPENFANG_STRICT_PRODUCTION  Set to 1/true/yes/on to require a machine API key
EOF
}

truthy() {
  local value="${1:-}"
  value="$(printf '%s' "${value}" | tr '[:upper:]' '[:lower:]')"
  case "${value}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

BASE_URL="${1:-${OPENFANG_BASE_URL:-http://127.0.0.1:4200}}"
API_KEY="${OPENFANG_API_KEY:-}"
STRICT_PRODUCTION="${OPENFANG_STRICT_PRODUCTION:-0}"

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for smoke-openfang.sh" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for smoke-openfang.sh" >&2
  exit 1
fi

if truthy "${STRICT_PRODUCTION}" && [[ -z "${API_KEY}" ]]; then
  echo "error OPENFANG_STRICT_PRODUCTION requires OPENFANG_API_KEY so protected smoke checks cannot silently degrade" >&2
  exit 1
fi

auth_args=()
if [[ -n "${API_KEY}" ]]; then
  auth_args=(-H "Authorization: Bearer ${API_KEY}")
fi
smoke_failures=0

curl_with_auth() {
  if (( ${#auth_args[@]} > 0 )); then
    curl -fsS "${auth_args[@]}" "$1"
  else
    curl -fsS "$1"
  fi
}

mark_failure() {
  smoke_failures=1
}

require_json_status() {
  local url="$1"
  local description="$2"
  local expected="$3"
  local body

  body="$(curl_with_auth "${url}")"
  python3 - "${body}" "${description}" "${expected}" <<'PY'
import json
import sys

body, description, expected = sys.argv[1:4]
payload = json.loads(body)
status = payload.get("status")
if status != expected:
    raise SystemExit(
        f"{description} returned unexpected status: {status!r} (expected {expected!r})"
    )
PY
  echo "ok  ${description}"
}

check_http_200() {
  local url="$1"
  curl_with_auth "${url}" >/dev/null
}

check_audit_verify() {
  local url="$1"
  local body

  body="$(curl_with_auth "${url}")"
  python3 - "${body}" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if payload.get("valid") is not True:
    raise SystemExit(f"/api/audit/verify reported invalid audit chain: {payload!r}")

entries = int(payload.get("entries", 0) or 0)
warning = payload.get("warning")
if entries == 0 or warning:
    print(
        warning or "Audit log is empty — smoke passed but this node has no forensic history yet.",
        file=sys.stderr,
    )
PY
}

check_protected() {
  local mode="$1"
  local url="$2"
  local description="$3"
  local expected="${4:-}"

  if [[ "${mode}" == "status" ]]; then
    check_http_200 "${url}" && return 0
  elif [[ "${mode}" == "json-status" ]]; then
    require_json_status "${url}" "${description}" "${expected}" >/dev/null && return 0
  elif [[ "${mode}" == "audit" ]]; then
    check_audit_verify "${url}" && return 0
  fi

  if [[ -n "${API_KEY}" ]]; then
    echo "error ${description} failed even though OPENFANG_API_KEY is set" >&2
    mark_failure
  else
    echo "warn ${description} unavailable with current auth context; set OPENFANG_API_KEY for full smoke coverage" >&2
  fi
  return 1
}

require_json_status "${BASE_URL}/api/health" "health" "ok"

if check_protected "json-status" "${BASE_URL}/api/status" "status" "running"; then
  echo "ok  status"
fi
if check_protected "json-status" "${BASE_URL}/api/health/detail" "health detail" "ok"; then
  echo "ok  health detail"
fi
if check_protected "status" "${BASE_URL}/api/metrics" "metrics"; then
  echo "ok  metrics"
fi
if check_protected "audit" "${BASE_URL}/api/audit/verify" "audit verify"; then
  echo "ok  audit verify"
fi

if curl_with_auth "${BASE_URL}/api/integrations/health" >/dev/null 2>&1; then
  echo "ok  integrations health"
fi

if (( smoke_failures > 0 )); then
  exit 1
fi

echo "Smoke test passed for ${BASE_URL}"
