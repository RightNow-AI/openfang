#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: live-api-smoke-openfang.sh [base-url]

Run a stateful smoke loop that spawns an agent, exercises budget reads/writes,
and then kills the agent. Requires a machine API key so all protected endpoints
can be touched without a dashboard session.

Environment:
  OPENFANG_BASE_URL          Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY           Bearer token used for protected endpoints (mandatory)
EOF
}

if [[ "${1:-""}" == "-h" || "${1:-""}" == "--help" ]]; then
  usage
  exit 0
fi

BASE_URL="${1:-${OPENFANG_BASE_URL:-http://127.0.0.1:4200}}"
API_KEY="${OPENFANG_API_KEY:-}"

if [[ -z "${API_KEY}" ]]; then
  echo "error OPENFANG_API_KEY is required for the live API smoke" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "error python3 is required for the live API smoke" >&2
  exit 1
fi

AUTH=(-H "Authorization: Bearer ${API_KEY}")
CONTENT=(-H "Content-Type: application/json")
agent_id=""
agent_name="live-smoke-agent-$(python3 - <<'PY'
import uuid
print(uuid.uuid4().hex[:8])
PY
)"

run_curl() {
  local path="$1"
  curl -fsS "${AUTH[@]}" "${BASE_URL}${path}"
}

cleanup() {
  if [[ -n "${agent_id}" ]]; then
    curl -fsS -X DELETE "${AUTH[@]}" "${BASE_URL}/api/agents/${agent_id}" >/dev/null 2>&1 || true
    return
  fi

  local fallback_ids
  fallback_ids="$(run_curl "/api/agents" | python3 - "${agent_name}" <<'PY'
import json
import sys

payload = json.load(sys.stdin)
target_name = sys.argv[1]
for agent in payload:
    if agent.get("name") == target_name and agent.get("id"):
        print(agent["id"])
PY
)" || return 0

  while IFS= read -r fallback_id; do
    [[ -n "${fallback_id}" ]] || continue
    curl -fsS -X DELETE "${AUTH[@]}" "${BASE_URL}/api/agents/${fallback_id}" >/dev/null 2>&1 || true
  done <<< "${fallback_ids}"
}

trap cleanup EXIT

MANIFEST=$(cat <<'MAN'
name = "__LIVE_SMOKE_AGENT_NAME__"
version = "0.1.0"
description = "Smoke agent"
author = "openfang"
module = "builtin:chat"

[model]
provider = "default"
model = "default"
system_prompt = "You are a helper. Keep replies short."

[capabilities]
tools = ["file_read"]
memory_read = ["*"]
memory_write = ["self.*"]
MAN
)
MANIFEST="${MANIFEST/__LIVE_SMOKE_AGENT_NAME__/${agent_name}}"

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
'
then
  echo "error agent detail missing id" >&2
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

echo "live API smoke completed against ${BASE_URL}"
