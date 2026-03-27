#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/openfang-env-common.sh
source "${SCRIPT_DIR}/openfang-env-common.sh"

usage() {
  cat <<'EOF'
Usage: provider-canary-openfang.sh [base-url]

Run a real provider canary against a running OpenFang daemon.

Environment:
  OPENFANG_BASE_URL           Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY            Bearer token for protected endpoints
  OPENFANG_ENV_FILE           Optional external env file (for example /etc/openfang/env)
                              Used to resolve OPENFANG_API_KEY/OPENFANG_LISTEN/OPENFANG_BASE_URL
  OPENFANG_CANARY_AGENT_ID    Existing agent ID to reuse (optional)
  OPENFANG_CANARY_PROVIDER    Provider for a temporary canary agent (required if no agent id)
  OPENFANG_CANARY_MODEL       Model for a temporary canary agent (required if no agent id)
  OPENFANG_CANARY_API_KEY_ENV API key env var name recorded in the temporary manifest (required if no agent id)
  OPENFANG_CANARY_MESSAGE     Message to send (default: Say hello in five words.)
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

EXTERNAL_ENV_FILE="$(openfang_resolve_external_env_file "${OPENFANG_HOME:-$HOME/.openfang}")"
BASE_URL="$(openfang_resolve_base_url "${1:-}" "${EXTERNAL_ENV_FILE}" "http://127.0.0.1:4200")"
API_KEY="$(openfang_resolve_runtime_value "OPENFANG_API_KEY" "${EXTERNAL_ENV_FILE}")"
CANARY_AGENT_ID="${OPENFANG_CANARY_AGENT_ID:-}"
CANARY_PROVIDER="${OPENFANG_CANARY_PROVIDER:-}"
CANARY_MODEL="${OPENFANG_CANARY_MODEL:-}"
CANARY_API_KEY_ENV="${OPENFANG_CANARY_API_KEY_ENV:-}"
CANARY_MESSAGE="${OPENFANG_CANARY_MESSAGE:-Say hello in five words.}"

for cmd in curl python3; do
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "missing required command: ${cmd}" >&2
    exit 1
  fi
done

auth_args=()
if [[ -n "${API_KEY}" ]]; then
  auth_args=(-H "Authorization: Bearer ${API_KEY}")
fi

curl_json() {
  local method="$1"
  local url="$2"
  local body="${3:-}"

  if [[ -n "${body}" ]]; then
    curl -fsS -X "${method}" "${auth_args[@]}" \
      -H "Content-Type: application/json" \
      -d "${body}" \
      "${url}"
  else
    curl -fsS -X "${method}" "${auth_args[@]}" "${url}"
  fi
}

cleanup() {
  if [[ -n "${TEMP_AGENT_ID:-}" ]]; then
    curl -fsS -X DELETE "${auth_args[@]}" "${BASE_URL}/api/agents/${TEMP_AGENT_ID}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

curl_json GET "${BASE_URL}/api/health" >/dev/null
echo "ok  health"

if [[ -z "${CANARY_AGENT_ID}" ]]; then
  for required in CANARY_PROVIDER CANARY_MODEL CANARY_API_KEY_ENV; do
    if [[ -z "${!required}" ]]; then
      echo "missing required environment variable when no canary agent id is provided: OPENFANG_${required}" >&2
      exit 1
    fi
  done

  provider_status="$(curl_json GET "${BASE_URL}/api/providers")"
  python3 - <<'PY' "${provider_status}" "${CANARY_PROVIDER}"
import json
import sys

payload = json.loads(sys.argv[1])
provider_id = sys.argv[2]
if isinstance(payload, dict):
    providers = payload.get("providers")
else:
    providers = payload
if not isinstance(providers, list):
    raise SystemExit(f"/api/providers returned unexpected payload: {payload!r}")
match = next((p for p in providers if p.get("id") == provider_id), None)
if match is None:
    raise SystemExit(f"provider '{provider_id}' not found in /api/providers")

auth_status = str(match.get("auth_status", "")).lower()
key_required = bool(match.get("key_required"))
if key_required and auth_status == "missing":
    raise SystemExit(
        f"provider '{provider_id}' is missing credentials in the running daemon; "
        "configure the provider before running the canary"
    )

print(f"ok  provider {provider_id} auth_status={auth_status or 'unknown'}")
PY

  AGENT_NAME="provider-canary-$(date +%s)"
  manifest_payload="$(AGENT_NAME="${AGENT_NAME}" CANARY_PROVIDER="${CANARY_PROVIDER}" CANARY_MODEL="${CANARY_MODEL}" CANARY_API_KEY_ENV="${CANARY_API_KEY_ENV}" python3 - <<'PY'
import json
import os

manifest = f'''name = "{os.environ["AGENT_NAME"]}"
version = "0.1.0"
description = "Production provider canary"
author = "openfang-ops"
module = "builtin:chat"

[model]
provider = "{os.environ["CANARY_PROVIDER"]}"
model = "{os.environ["CANARY_MODEL"]}"
api_key_env = "{os.environ["CANARY_API_KEY_ENV"]}"
system_prompt = "You are a production canary. Reply concisely."

[capabilities]
tools = []
memory_read = ["*"]
memory_write = ["self.*"]
'''
print(json.dumps({"manifest_toml": manifest}))
PY
)"

  spawn_response="$(AGENT_NAME="${AGENT_NAME}" CANARY_PROVIDER="${CANARY_PROVIDER}" CANARY_MODEL="${CANARY_MODEL}" CANARY_API_KEY_ENV="${CANARY_API_KEY_ENV}" curl_json POST "${BASE_URL}/api/agents" "${manifest_payload}")"
  CANARY_AGENT_ID="$(python3 - <<'PY' "${spawn_response}"
import json
import sys
payload = json.loads(sys.argv[1])
agent_id = payload.get("agent_id")
if not agent_id:
    raise SystemExit(f"spawn agent failed: {payload}")
print(agent_id)
PY
)"
  TEMP_AGENT_ID="${CANARY_AGENT_ID}"
  echo "ok  spawned temp agent ${CANARY_AGENT_ID}"
else
  echo "ok  reusing agent ${CANARY_AGENT_ID}"
fi

before_global_budget="$(curl_json GET "${BASE_URL}/api/budget")"
before_budget="$(curl_json GET "${BASE_URL}/api/budget/agents/${CANARY_AGENT_ID}")"
before_tokens="$(python3 - <<'PY' "${before_budget}"
import json
import sys
payload = json.loads(sys.argv[1])
print(payload.get("tokens", {}).get("used", 0))
PY
)"

message_payload="$(CANARY_MESSAGE="${CANARY_MESSAGE}" python3 - <<'PY'
import json
import os
print(json.dumps({"message": os.environ["CANARY_MESSAGE"]}))
PY
)"
message_response="$(CANARY_MESSAGE="${CANARY_MESSAGE}" curl_json POST "${BASE_URL}/api/agents/${CANARY_AGENT_ID}/message" "${message_payload}")"
python3 - <<'PY' "${message_response}"
import json
import sys
payload = json.loads(sys.argv[1])
response = str(payload.get("response", "")).strip()
output_tokens = int(payload.get("output_tokens", 0))
if not response:
    raise SystemExit(f"empty canary response: {payload}")
if output_tokens <= 0:
    raise SystemExit(f"expected output_tokens > 0, got {payload}")
PY
echo "ok  llm roundtrip"

after_budget="$(curl_json GET "${BASE_URL}/api/budget/agents/${CANARY_AGENT_ID}")"
python3 - <<'PY' "${before_tokens}" "${after_budget}"
import json
import sys
before = int(sys.argv[1])
payload = json.loads(sys.argv[2])
after = int(payload.get("tokens", {}).get("used", 0))
if after <= before:
    raise SystemExit(
        f"token usage did not increase: before={before} after={after}; "
        "metering/token tracking is not behaving as expected"
    )
print(f"ok  token usage before={before} after={after} delta={after - before}")
PY

after_global_budget="$(curl_json GET "${BASE_URL}/api/budget")"
python3 - <<'PY' "${before_budget}" "${after_budget}" "${before_global_budget}" "${after_global_budget}"
import json
import sys

before_agent = json.loads(sys.argv[1])
after_agent = json.loads(sys.argv[2])
before_global = json.loads(sys.argv[3])
after_global = json.loads(sys.argv[4])
epsilon = 1e-9
spend_increased = False

for scope, before, after in (
    ("agent", before_agent, after_agent),
    ("global", before_global, after_global),
):
    for field in ("hourly_spend", "daily_spend", "monthly_spend"):
        if scope == "agent":
            before_value = float(((before.get(field.split("_")[0]) or {}).get("spend", 0.0)) or 0.0)
            after_value = float(((after.get(field.split("_")[0]) or {}).get("spend", 0.0)) or 0.0)
            label = f"agent {field}"
        else:
            before_value = float(before.get(field, 0.0) or 0.0)
            after_value = float(after.get(field, 0.0) or 0.0)
            label = f"global {field}"

        if after_value + epsilon < before_value:
            raise SystemExit(
                f"{label} moved backwards: before={before_value} after={after_value}"
            )
        if after_value > before_value + epsilon:
            spend_increased = True
        print(f"ok  {label} before={before_value:.6f} after={after_value:.6f}")

if not spend_increased:
    print(
        "warn spend counters did not increase; this can happen on free/local providers. "
        "Token growth already verified the canary request executed.",
        file=sys.stderr,
    )
PY
