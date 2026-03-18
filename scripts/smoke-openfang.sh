#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: smoke-openfang.sh [base-url]

Run a small authenticated smoke test against a running OpenFang daemon.

Environment:
  OPENFANG_BASE_URL  Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY   Bearer token used for protected endpoints
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

BASE_URL="${1:-${OPENFANG_BASE_URL:-http://127.0.0.1:4200}}"
API_KEY="${OPENFANG_API_KEY:-}"

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for smoke-openfang.sh" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for smoke-openfang.sh" >&2
  exit 1
fi

auth_args=()
if [[ -n "${API_KEY}" ]]; then
  auth_args=(-H "Authorization: Bearer ${API_KEY}")
fi

curl_with_auth() {
  if (( ${#auth_args[@]} > 0 )); then
    curl -fsS "${auth_args[@]}" "$1"
  else
    curl -fsS "$1"
  fi
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

require_http_200() {
  local url="$1"
  local description="$2"
  curl_with_auth "${url}" >/dev/null
  echo "ok  ${description}"
}

require_json_status "${BASE_URL}/api/health" "health" "ok"
require_json_status "${BASE_URL}/api/status" "status" "running"
require_json_status "${BASE_URL}/api/health/detail" "health detail" "ok"
require_http_200 "${BASE_URL}/api/metrics" "metrics"
require_http_200 "${BASE_URL}/api/audit/verify" "audit verify"

if curl_with_auth "${BASE_URL}/api/integrations/health" >/dev/null 2>&1; then
  echo "ok  integrations health"
fi

echo "Smoke test passed for ${BASE_URL}"
