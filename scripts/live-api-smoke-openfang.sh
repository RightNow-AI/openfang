#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/openfang-env-common.sh
source "${SCRIPT_DIR}/openfang-env-common.sh"

usage() {
  cat <<'EOF'
Usage: live-api-smoke-openfang.sh [base-url]

Run a stateful smoke loop that spawns an agent, exercises budget reads/writes,
and then kills the agent. Requires a machine API key so all protected endpoints
can be touched without a dashboard session.

Environment:
  OPENFANG_BASE_URL          Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY           Bearer token used for protected endpoints (mandatory)
  OPENFANG_ENV_FILE          Optional external env file (for example /etc/openfang/env)
                             Used to resolve OPENFANG_API_KEY/OPENFANG_LISTEN/OPENFANG_BASE_URL
  OPENFANG_CANARY_PROVIDER   Optional provider id for a provider-backed smoke agent
  OPENFANG_CANARY_MODEL      Optional model id for provider-backed smoke
  OPENFANG_CANARY_API_KEY_ENV Optional api_key_env recorded in provider-backed smoke manifest
  OPENFANG_CANARY_MESSAGE    Optional provider smoke message (default: Say hello in five words.)
  OPENFANG_SMOKE_RESTART_CMD Optional shell command to restart daemon/container for persistence check
  OPENFANG_SMOKE_RESTART_TIMEOUT_SECS Restart health wait timeout in seconds (default: 60)
EOF
}

if [[ "${1:-""}" == "-h" || "${1:-""}" == "--help" ]]; then
  usage
  exit 0
fi

EXTERNAL_ENV_FILE="$(openfang_resolve_external_env_file "${OPENFANG_HOME:-$HOME/.openfang}")"
BASE_URL="$(openfang_resolve_base_url "${1:-}" "${EXTERNAL_ENV_FILE}" "http://127.0.0.1:4200")"
API_KEY="$(openfang_resolve_runtime_value "OPENFANG_API_KEY" "${EXTERNAL_ENV_FILE}")"
CANARY_PROVIDER="${OPENFANG_CANARY_PROVIDER:-}"
CANARY_MODEL="${OPENFANG_CANARY_MODEL:-}"
CANARY_API_KEY_ENV="${OPENFANG_CANARY_API_KEY_ENV:-}"
CANARY_MESSAGE="${OPENFANG_CANARY_MESSAGE:-Say hello in five words.}"
RESTART_CMD="${OPENFANG_SMOKE_RESTART_CMD:-}"
RESTART_TIMEOUT_SECS="${OPENFANG_SMOKE_RESTART_TIMEOUT_SECS:-60}"

if [[ -z "${API_KEY}" ]]; then
  echo "error OPENFANG_API_KEY is required for the live API smoke" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "error python3 is required for the live API smoke" >&2
  exit 1
fi
if [[ -n "${RESTART_CMD}" ]] && ! [[ "${RESTART_TIMEOUT_SECS}" =~ ^[0-9]+$ ]]; then
  echo "error OPENFANG_SMOKE_RESTART_TIMEOUT_SECS must be an integer" >&2
  exit 1
fi
provider_fields_set=0
for value in "${CANARY_PROVIDER}" "${CANARY_MODEL}" "${CANARY_API_KEY_ENV}"; do
  if [[ -n "${value}" ]]; then
    provider_fields_set=$((provider_fields_set + 1))
  fi
done
if (( provider_fields_set > 0 && provider_fields_set < 3 )); then
  echo "error provider-backed smoke requires OPENFANG_CANARY_PROVIDER, OPENFANG_CANARY_MODEL, and OPENFANG_CANARY_API_KEY_ENV together" >&2
  exit 1
fi
provider_backed=0
if (( provider_fields_set == 3 )); then
  provider_backed=1
fi

AUTH=(-H "Authorization: Bearer ${API_KEY}")
CONTENT=(-H "Content-Type: application/json")
agent_id=""
cleanup_needed=1
agent_name="live-smoke-agent-$(python3 - <<'PY'
import uuid
print(uuid.uuid4().hex[:8])
PY
)"

run_curl() {
  local path="$1"
  curl -fsS "${AUTH[@]}" "${BASE_URL}${path}"
}

wait_for_health() {
  local max_attempts="$1"
  local attempt
  for attempt in $(seq 1 "${max_attempts}"); do
    if curl -fsS "${BASE_URL}/api/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

cleanup() {
  [[ "${cleanup_needed}" == "1" ]] || return 0

  if [[ -n "${agent_id}" ]]; then
    curl -fsS -X DELETE "${AUTH[@]}" "${BASE_URL}/api/agents/${agent_id}" >/dev/null 2>&1 || true
    return
  fi

  local fallback_ids
  fallback_ids="$(run_curl "/api/agents" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
target_name = sys.argv[1]
for agent in payload:
    if agent.get("name") == target_name and agent.get("id"):
        print(agent["id"])
' "${agent_name}")" || return 0

  while IFS= read -r fallback_id; do
    [[ -n "${fallback_id}" ]] || continue
    curl -fsS -X DELETE "${AUTH[@]}" "${BASE_URL}/api/agents/${fallback_id}" >/dev/null 2>&1 || true
  done <<< "${fallback_ids}"
}

trap cleanup EXIT

MANIFEST="$(AGENT_NAME="${agent_name}" CANARY_PROVIDER="${CANARY_PROVIDER}" CANARY_MODEL="${CANARY_MODEL}" CANARY_API_KEY_ENV="${CANARY_API_KEY_ENV}" PROVIDER_BACKED="${provider_backed}" python3 - <<'PY'
import os

provider_backed = os.environ["PROVIDER_BACKED"] == "1"
provider = "default"
model = "default"
api_key_line = ""
if provider_backed:
    provider = os.environ["CANARY_PROVIDER"]
    model = os.environ["CANARY_MODEL"]
    api_key_line = f'api_key_env = "{os.environ["CANARY_API_KEY_ENV"]}"\n'

print(
    f'''name = "{os.environ["AGENT_NAME"]}"
version = "0.1.0"
description = "Smoke agent"
author = "openfang"
module = "builtin:chat"

[model]
provider = "{provider}"
model = "{model}"
{api_key_line}system_prompt = "You are a helper. Keep replies short."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
''',
    end="",
)
PY
)"

body=$(python3 - <<PY
import json
import textwrap
man = textwrap.dedent("""
${MANIFEST}
""")
print(json.dumps({"manifest_toml": man}))
PY
)

spawn_resp=$(curl -fsS -X POST "${BASE_URL}/api/agents" "${AUTH[@]}" "${CONTENT[@]}" -d "${body}")
agent_id="$(printf '%s' "${spawn_resp}" | python3 -c 'import json, sys; print(json.load(sys.stdin)["agent_id"])')"
if [[ -z "${agent_id}" ]]; then
  echo "error failed to parse agent_id" >&2
  exit 1
fi

echo "ok spawned agent ${agent_id}"

agents=$(run_curl "/api/agents")
printf '%s' "${agents}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
agent_id = sys.argv[1]
if not any(agent.get("id") == agent_id for agent in payload):
    raise SystemExit(f"spawned agent {agent_id} not present in /api/agents")
' "${agent_id}"
echo "ok listed spawned agent"

agent_detail=$(run_curl "/api/agents/${agent_id}")
echo "ok fetched agent detail"

PATCH_PAYLOAD=$(python3 - <<PY
import json
print(json.dumps({"description": "Smoke agent updated"}))
PY
)
curl -fsS -X PATCH "${BASE_URL}/api/agents/${agent_id}" "${AUTH[@]}" "${CONTENT[@]}" -d "${PATCH_PAYLOAD}" >/dev/null
echo "ok patched agent description"

IDENTITY_PAYLOAD=$(python3 - <<PY
import json
print(json.dumps({"emoji": ":)", "color": "#112233"}))
PY
)
curl -fsS -X PATCH "${BASE_URL}/api/agents/${agent_id}/identity" "${AUTH[@]}" "${CONTENT[@]}" -d "${IDENTITY_PAYLOAD}" >/dev/null
echo "ok patched agent identity"

agent_detail=$(run_curl "/api/agents/${agent_id}")
echo "ok fetched updated agent detail"

UPDATE_PAYLOAD=$(python3 - <<PY
import json
print(json.dumps({"max_cost_per_day_usd": 0.01}))
PY
)
curl -fsS -X PUT "${BASE_URL}/api/budget/agents/${agent_id}" "${AUTH[@]}" "${CONTENT[@]}" -d "${UPDATE_PAYLOAD}" >/dev/null
echo "ok updated agent budget"

budget=$(run_curl "/api/budget/agents/${agent_id}")
echo "ok read back budget"

budget_status=$(run_curl "/api/budget")
echo "ok global budget status"

budget_ranking=$(run_curl "/api/budget/agents")
echo "ok budget ranking"

if ! printf '%s' "${agent_detail}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
if not payload.get("id"):
    raise SystemExit("missing id")
if payload.get("description") != "Smoke agent updated":
    raise SystemExit("description patch mismatch")
identity = payload.get("identity") or {}
if identity.get("emoji") != ":)":
    raise SystemExit("identity emoji mismatch")
if identity.get("color") != "#112233":
    raise SystemExit("identity color mismatch")
'
then
  echo "error agent detail did not reflect config/identity updates" >&2
  exit 1
fi
if ! printf '%s' "${budget}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
if abs(payload["daily"]["limit"] - 0.01) >= 1e-12:
    raise SystemExit("budget readback mismatch")
'
then
  echo "error agent budget readback did not reflect the live update" >&2
  exit 1
fi
if ! printf '%s' "${budget_status}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
required = ["hourly_spend", "daily_spend", "monthly_spend", "alert_threshold"]
missing = [key for key in required if key not in payload]
if missing:
    raise SystemExit(f"missing keys: {missing}")
'
then
  echo "error global budget status is missing required fields" >&2
  exit 1
fi
if ! printf '%s' "${budget_ranking}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
if not isinstance(payload.get("agents"), list):
    raise SystemExit("budget ranking missing agents list")
if "total" not in payload:
    raise SystemExit("budget ranking missing total")
'
then
  echo "error budget ranking payload is malformed" >&2
  exit 1
fi

if (( provider_backed == 1 )); then
  before_budget="$(run_curl "/api/budget/agents/${agent_id}")"
  before_tokens="$(printf '%s' "${before_budget}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
print(payload.get("tokens", {}).get("used", 0))
')"
  MESSAGE_PAYLOAD="$(CANARY_MESSAGE="${CANARY_MESSAGE}" python3 - <<'PY'
import json
import os
print(json.dumps({"message": os.environ["CANARY_MESSAGE"]}))
PY
)"
  message_response="$(curl -fsS -X POST "${BASE_URL}/api/agents/${agent_id}/message" "${AUTH[@]}" "${CONTENT[@]}" -d "${MESSAGE_PAYLOAD}")"
  if ! printf '%s' "${message_response}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
response = str(payload.get("response", "")).strip()
output_tokens = int(payload.get("output_tokens", 0))
if not response:
    raise SystemExit("provider smoke response is empty")
if output_tokens <= 0:
    raise SystemExit(f"expected output_tokens > 0, got {payload}")
'
  then
    echo "error provider-backed message roundtrip failed" >&2
    exit 1
  fi
  after_budget="$(run_curl "/api/budget/agents/${agent_id}")"
  if ! printf '%s' "${after_budget}" | python3 -c 'import json, sys
before = int(sys.argv[1])
payload = json.load(sys.stdin)
after = int(payload.get("tokens", {}).get("used", 0))
if after <= before:
    raise SystemExit(f"token usage did not increase: before={before} after={after}")
' "${before_tokens}"
  then
    echo "error provider-backed smoke did not increase token usage" >&2
    exit 1
  fi
  echo "ok provider-backed message roundtrip"
fi

if [[ -n "${RESTART_CMD}" ]]; then
  restart_budget_before="$(run_curl "/api/budget/agents/${agent_id}")"
  curl -fsS -X POST "${AUTH[@]}" "${BASE_URL}/api/shutdown" >/dev/null 2>&1 || true
  if ! bash -lc "${RESTART_CMD}"; then
    echo "error restart command failed: ${RESTART_CMD}" >&2
    exit 1
  fi
  max_attempts=$((RESTART_TIMEOUT_SECS > 0 ? RESTART_TIMEOUT_SECS : 60))
  if ! wait_for_health "${max_attempts}"; then
    echo "error service did not return healthy after restart within ${max_attempts}s" >&2
    exit 1
  fi
  restarted_agents="$(run_curl "/api/agents")"
  if ! printf '%s' "${restarted_agents}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
target = sys.argv[1]
if not any(agent.get("id") == target for agent in payload):
    raise SystemExit(f"agent {target} missing after restart")
' "${agent_id}"
  then
    echo "error smoke agent missing after restart" >&2
    exit 1
  fi
  restarted_detail="$(run_curl "/api/agents/${agent_id}")"
  if ! printf '%s' "${restarted_detail}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
if payload.get("description") != "Smoke agent updated":
    raise SystemExit("description did not persist across restart")
identity = payload.get("identity") or {}
if identity.get("emoji") != ":)" or identity.get("color") != "#112233":
    raise SystemExit("identity did not persist across restart")
'
  then
    echo "error smoke agent state did not persist across restart" >&2
    exit 1
  fi
  restarted_budget="$(run_curl "/api/budget/agents/${agent_id}")"
  if ! python3 - <<'PY' "${restart_budget_before}" "${restarted_budget}"
import json
import sys

before = json.loads(sys.argv[1])
after = json.loads(sys.argv[2])
if abs(float((after.get("daily") or {}).get("limit", 0.0)) - float((before.get("daily") or {}).get("limit", 0.0))) >= 1e-12:
    raise SystemExit("daily budget limit changed after restart")
PY
  then
    echo "error budget state did not persist across restart" >&2
    exit 1
  fi
  echo "ok restart persistence verification"
fi

killed_agent_id="${agent_id}"
curl -fsS -X DELETE "${AUTH[@]}" "${BASE_URL}/api/agents/${agent_id}" >/dev/null
agent_id=""
echo "ok killed agent ${killed_agent_id}"

agents_after_delete=$(run_curl "/api/agents")
if ! printf '%s' "${agents_after_delete}" | python3 -c 'import json, sys
payload = json.load(sys.stdin)
agent_id = sys.argv[1]
if any(agent.get("id") == agent_id for agent in payload):
    raise SystemExit(f"agent {agent_id} still present after delete")
' "${killed_agent_id}"
then
  echo "error killed agent still appears in /api/agents" >&2
  exit 1
fi
cleanup_needed=0

echo "live API smoke completed against ${BASE_URL}"
