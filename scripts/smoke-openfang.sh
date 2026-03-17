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

require_json_status() {
  local url="$1"
  local description="$2"
  local body

  body="$(curl -fsS "${auth_args[@]}" "${url}")"
  python3 - "${body}" "${description}" <<'PY'
import json
import sys

body, description = sys.argv[1:3]
payload = json.loads(body)
status = payload.get("status")
if status not in {"ok", "degraded", "running"}:
    raise SystemExit(f"{description} returned unexpected status: {status!r}")
PY
  echo "ok  ${description}"
}

require_http_200() {
  local url="$1"
  local description="$2"
  curl -fsS "${auth_args[@]}" "${url}" >/dev/null
  echo "ok  ${description}"
}

require_json_status "${BASE_URL}/api/health" "health"
require_json_status "${BASE_URL}/api/status" "status"
require_json_status "${BASE_URL}/api/health/detail" "health detail"
require_http_200 "${BASE_URL}/api/metrics" "metrics"
require_http_200 "${BASE_URL}/api/audit/verify" "audit verify"

if curl -fsS "${auth_args[@]}" "${BASE_URL}/api/integrations/health" >/dev/null 2>&1; then
  echo "ok  integrations health"
fi

echo "Smoke test passed for ${BASE_URL}"
