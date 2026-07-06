#!/usr/bin/env bash
# Restore OMTAE autonomy wiring after reboot or fresh spawn.
# Usage: bash scripts/restore-autonomy.sh
set -euo pipefail

API="${OMTAE_API:-http://127.0.0.1:4200}"
PIN="${OMTAE_PIN:-839201}"
H="X-OMTAE-Pin: ${PIN}"

curl_api() {
  curl -sf -H "$H" "$@"
}

echo "==> Health"
curl_api "$API/api/health"

echo ""
echo "==> Spawn core agents (idempotent)"
for name in researcher analyst coder ops-fixer; do
  curl -sf -X POST "$API/api/agents" -H "Content-Type: application/json" -H "$H" \
    -d "{\"template\":\"$name\"}" >/dev/null 2>&1 || true
done

agent_id() {
  curl_api "$API/api/agents" | python3 -c "
import sys, json
name = '$1'
for a in json.load(sys.stdin):
    if a.get('name') == name:
        print(a['id'])
        break
"
}

echo ""
echo "==> Activate hands"
activate_hand() {
  local hand="$1" instance="$2"
  curl -sf -X POST "$API/api/hands/${hand}/activate" -H "Content-Type: application/json" -H "$H" \
    -d "{\"instance_name\":\"${instance}\"}" >/dev/null 2>&1 || true
}
activate_hand lead lead-vault
activate_hand collector collector-market
activate_hand browser browser-hand 2>/dev/null || true

OPS="$(agent_id ops-fixer)"
RES="$(agent_id researcher)"
LEAD="$(agent_id lead-vault)"
COLL="$(agent_id collector-market)"

echo "ops-fixer=$OPS researcher=$RES lead=$LEAD collector=$COLL"


echo ""
echo "==> Ensure schedules (skip if already present)"
ensure_schedule() {
  local name="$1" cron="$2" agent="$3" msg="$4"
  local exists
  exists=$(curl_api "$API/api/schedules" | python3 -c "
import sys, json
name = '$name'
for s in json.load(sys.stdin).get('schedules', []):
    if s.get('name') == name:
        print('yes')
        break
")
  if [[ "$exists" == "yes" ]]; then
    echo "  schedule $name: already exists"
    return
  fi
  curl -sf -X POST "$API/api/schedules" -H "Content-Type: application/json" -H "$H" \
    -d "$(python3 -c "import json; print(json.dumps({'name':'$name','cron':'$cron','agent_id':'$agent','message':'''$msg''','enabled':True}))")" \
    | python3 -c "import sys,json; d=json.load(sys.stdin); print('  created', '$name', d.get('id', d))"
}

ensure_schedule "morning-ops-check" "0 13 * * *" "$OPS" \
  "SYSTEM CHECK: Run shell_exec nvidia-smi, curl -s http://127.0.0.1:4200/api/health, curl -s http://127.0.0.1:8000/v1/models. Report GPU, desk, vLLM status with real tool output."

ensure_schedule "daily-lead-report" "0 14 * * *" "$LEAD" \
  "Run lead generation cycle. Write results to /home/jay/vaults/omtae-brain/leads. Use web_search and web_fetch — no fabricated leads."

ensure_schedule "collector-sweep" "0 */6 * * *" "$COLL" \
  "Run intelligence collection sweep on AI agent platforms and local LLM market. Update knowledge graph and write markdown report."

ensure_schedule "nightly-brain-sync" "0 7 * * *" "$RES" \
  "Brain sync: curl -s -H X-OMTAE-Pin:839201 http://127.0.0.1:4200/api/brain/status and file_list /home/jay/vaults/omtae-brain/leads. Summarize lead count and vault health."

BRAIN_WF="${BRAIN_WORKFLOW_ID:-9b3e8715-d5cf-4eb6-8321-63e73b16f2b8}"
ensure_workflow_schedule() {
  local name="$1" cron="$2" workflow="$3"
  local exists
  exists=$(curl_api "$API/api/schedules" | python3 -c "
import sys, json
name = '$name'
for s in json.load(sys.stdin).get('schedules', []):
    if s.get('name') == name:
        print('yes')
        break
")
  if [[ "$exists" == "yes" ]]; then
    echo "  schedule $name: already exists"
    return
  fi
  curl -sf -X POST "$API/api/cron/jobs" -H "Content-Type: application/json" -H "$H" \
    -d "$(python3 -c "import json; print(json.dumps({
      'agent_id': '$RES',
      'name': '$name',
      'schedule': {'kind': 'cron', 'expr': '$cron', 'tz': None},
      'action': {'kind': 'workflow_run', 'workflow_id': '$workflow', 'input': None, 'timeout_secs': None},
      'delivery': {'kind': 'none'},
      'one_shot': False,
    }))")" \
    | python3 -c "import sys,json; d=json.load(sys.stdin); r=json.loads(d.get('result','{}')); print('  created', '$name', r.get('job_id', d))"
}

ensure_workflow_schedule "weekly-brain-pipeline" "0 8 * * 7" "$BRAIN_WF"

echo ""
echo "==> EvoMap heartbeat"
if [[ -x /home/jay/.config/evomap/heartbeat.sh ]]; then
  if ! kill -0 "$(cat /home/jay/.config/evomap/heartbeat.pid 2>/dev/null)" 2>/dev/null; then
    nohup /home/jay/.config/evomap/heartbeat.sh >/dev/null 2>&1 &
    sleep 1
  fi
  echo "  evomap pid=$(cat /home/jay/.config/evomap/heartbeat.pid 2>/dev/null || echo unknown)"
fi

echo ""
echo "==> Summary"
curl_api "$API/api/schedules" | python3 -c "
import sys, json
for s in json.load(sys.stdin).get('schedules', []):
    print(' ', s['name'], s['cron'], s.get('enabled', True))
"
curl_api "$API/api/hands/active" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(' active hands:', d.get('total', 0))
for i in d.get('instances', []):
    print('  ', i['hand_id'], i.get('instance_name') or i['agent_name'])
"
curl_api "$API/api/brain/status" | python3 -c "
import sys, json
d = json.load(sys.stdin)
print(' brain leads:', d.get('counts',{}).get('leads','?'))
"

echo ""
echo "Autonomy restore complete."
