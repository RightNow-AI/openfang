#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
SUBMODULE_DIR="$ROOT_DIR/projects/shipinbot"
OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"
OPENFANG_ENV_FILE="${OPENFANG_ENV_FILE:-$OPENFANG_HOME/.env}"
STACK_DIR="$OPENFANG_HOME/local-stack"
PID_DIR="$STACK_DIR/pids"
LOG_DIR="$STACK_DIR/logs"

OPENFANG_BASE_URL="${OPENFANG_BASE_URL:-http://127.0.0.1:4200}"
MEDIA_BASE_URL="${MEDIA_BASE_URL:-http://127.0.0.1:8000}"

OPENFANG_PID_FILE="$PID_DIR/openfang.pid"
MEDIA_PID_FILE="$PID_DIR/media-web.pid"
TELEGRAM_PID_FILE="$PID_DIR/telegram-local-api.pid"

OPENFANG_LOG="$LOG_DIR/openfang.log"
MEDIA_LOG="$LOG_DIR/media-web.log"
TELEGRAM_LOG="$LOG_DIR/telegram-local-api.log"

mkdir -p "$PID_DIR" "$LOG_DIR"

usage() {
  cat <<'EOF'
Usage:
  scripts/local-stack.sh start
  scripts/local-stack.sh stop
  scripts/local-stack.sh restart
  scripts/local-stack.sh status
  scripts/local-stack.sh check
  scripts/local-stack.sh logs [openfang|media|telegram]
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "失败：缺少命令 $1" >&2
    exit 1
  fi
}

sync_bootstrap_workflows() {
  local sync_script="$ROOT_DIR/scripts/sync_openfang_bootstrap_workflows.py"
  if [[ ! -f "$sync_script" ]]; then
    return 0
  fi
  python3 "$sync_script"
}

json_status() {
  local url="$1"
  curl -fsS -m 5 "$url" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))'
}

wait_for_http_ok() {
  local url="$1"
  local timeout="${2:-40}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if [[ "$(json_status "$url" 2>/dev/null || true)" == "ok" ]]; then
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_http_ok_or_exit() {
  local url="$1"
  local pid="$2"
  local timeout="${3:-40}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if [[ "$(json_status "$url" 2>/dev/null || true)" == "ok" ]]; then
      return 0
    fi
    if ! pid_running "$pid"; then
      return 1
    fi
    sleep 1
  done
  return 1
}

wait_for_port_listener() {
  local port="$1"
  local timeout="${2:-20}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if lsof -iTCP:"$port" -sTCP:LISTEN -Pn >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

wait_for_port_gone() {
  local port="$1"
  local timeout="${2:-20}"
  local deadline=$((SECONDS + timeout))
  while (( SECONDS < deadline )); do
    if ! lsof -iTCP:"$port" -sTCP:LISTEN -Pn >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

load_runtime_env() {
  if [[ -f "$OPENFANG_ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$OPENFANG_ENV_FILE"
    set +a
  fi
}

resolve_openfang_bin() {
  local candidate
  for candidate in \
    "$ROOT_DIR/target/debug/openfang" \
    "$ROOT_DIR/target/release/openfang"
  do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  if command -v openfang >/dev/null 2>&1; then
    command -v openfang
    return 0
  fi
  echo "$ROOT_DIR/target/debug/openfang"
}

resolve_media_cli_bin() {
  printf '%s\n' "${VIDEO_WATERMARK_BIN:-$SUBMODULE_DIR/.venv/bin/video-watermark}"
}

resolve_media_python_bin() {
  printf '%s\n' "${VIDEO_WATERMARK_PYTHON:-$SUBMODULE_DIR/.venv/bin/python}"
}

resolve_media_config() {
  printf '%s\n' "${VIDEO_WATERMARK_CONFIG:-$SUBMODULE_DIR/config/project.yaml}"
}

resolve_pid() {
  local pid_file="$1"
  if [[ -f "$pid_file" ]]; then
    cat "$pid_file"
  fi
}

pid_running() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

cleanup_pid_file() {
  local pid_file="$1"
  if [[ -f "$pid_file" ]]; then
    rm -f "$pid_file"
  fi
}

pid_listener_on_port() {
  local port="$1"
  lsof -nP -iTCP:"$port" -sTCP:LISTEN -Fp 2>/dev/null | sed -n 's/^p//p' | head -n 1
}

pid_cwd() {
  local pid="$1"
  lsof -a -p "$pid" -d cwd -Fn 2>/dev/null | sed -n 's/^n//p' | head -n 1
}

pid_command() {
  local pid="$1"
  ps -p "$pid" -o command= 2>/dev/null | sed 's/^ *//'
}

list_openfang_daemon_pids() {
  ps -Ao pid=,command= 2>/dev/null | python3 -c '
import re
import sys

patterns = (
    re.compile(r"(^|[ /])openfang(\.exe)?\s+start($|\s)"),
    re.compile(r"\bcargo\s+run\b.*(?:\s-p\s+openfang-cli\b|\s--package\s+openfang-cli\b).*\s--\s+start($|\s)"),
)

for raw in sys.stdin:
    line = raw.strip()
    if not line:
        continue
    parts = line.split(None, 1)
    if len(parts) != 2:
        continue
    pid, command = parts
    normalized = " ".join(command.split())
    if any(pattern.search(normalized) for pattern in patterns):
        print(pid)
'
}

telegram_local_api_mode() {
  python3 - "$OPENFANG_HOME" <<'PY'
import sys
from pathlib import Path
from urllib.parse import urlparse

try:
    import tomllib
except ModuleNotFoundError:
    try:
        import tomli as tomllib  # type: ignore[import-not-found]
    except ModuleNotFoundError:
        print("unknown")
        raise SystemExit(0)

home = Path(sys.argv[1]).expanduser()
config_path = home / "config.toml"
if not config_path.exists():
    print("unknown")
    raise SystemExit(0)

try:
    payload = tomllib.loads(config_path.read_text(encoding="utf-8"))
except Exception:
    print("unknown")
    raise SystemExit(0)

channels = payload.get("channels")
telegram = channels.get("telegram") if isinstance(channels, dict) else None
if not isinstance(telegram, dict) or not bool(telegram.get("use_local_api")):
    print("disabled")
    raise SystemExit(0)

if bool(telegram.get("auto_start_local_api")):
    print("managed-by-openfang")
    raise SystemExit(0)

api_url = str(telegram.get("api_url") or "").strip()
port = int(telegram.get("local_api_port") or 8081)
if not api_url:
    print(f"local:{port}")
    raise SystemExit(0)

parsed = urlparse(api_url)
host = (parsed.hostname or "").strip().lower()
resolved_port = parsed.port or port
if host in {"127.0.0.1", "localhost", "::1"}:
    print(f"local:{resolved_port}")
else:
    print("remote")
PY
}

telegram_local_api_port() {
  local mode
  mode="$(telegram_local_api_mode)"
  if [[ "$mode" == local:* ]]; then
    printf '%s\n' "${mode#local:}"
  else
    printf '8081\n'
  fi
}

telegram_api_id() {
  python3 - "$OPENFANG_HOME" <<'PY'
import sys
from pathlib import Path
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[import-not-found]

config_path = Path(sys.argv[1]).expanduser() / "config.toml"
payload = tomllib.loads(config_path.read_text(encoding="utf-8"))
telegram = (payload.get("channels") or {}).get("telegram") or {}
print(str(telegram.get("telegram_api_id") or "").strip())
PY
}

telegram_api_hash_env() {
  python3 - "$OPENFANG_HOME" <<'PY'
import sys
from pathlib import Path
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[import-not-found]

config_path = Path(sys.argv[1]).expanduser() / "config.toml"
payload = tomllib.loads(config_path.read_text(encoding="utf-8"))
telegram = (payload.get("channels") or {}).get("telegram") or {}
print(str(telegram.get("telegram_api_hash_env") or "TELEGRAM_API_HASH").strip())
PY
}

resolve_telegram_binary() {
  if [[ -x "$OPENFANG_HOME/bin/telegram-bot-api" ]]; then
    printf '%s\n' "$OPENFANG_HOME/bin/telegram-bot-api"
    return 0
  fi
  if command -v telegram-bot-api >/dev/null 2>&1; then
    command -v telegram-bot-api
    return 0
  fi
  return 1
}

start_telegram() {
  local mode port binary api_id api_hash_env api_hash pid
  mode="$(telegram_local_api_mode)"
  if [[ "$mode" == "disabled" || "$mode" == "remote" || "$mode" == "unknown" || "$mode" == "managed-by-openfang" ]]; then
    echo "telegram-local-api: skip ($mode)"
    return 0
  fi
  port="$(telegram_local_api_port)"
  if wait_for_port_listener "$port" 1; then
    echo "telegram-local-api: already listening on $port"
    return 0
  fi
  if ! binary="$(resolve_telegram_binary)"; then
    echo "失败：未找到 telegram-bot-api 二进制" >&2
    return 1
  fi
  api_id="$(telegram_api_id)"
  api_hash_env="$(telegram_api_hash_env)"
  load_runtime_env
  api_hash="${!api_hash_env:-}"
  if [[ -z "$api_id" || -z "$api_hash" ]]; then
    echo "失败：telegram local api 缺少 api_id 或 $api_hash_env" >&2
    return 1
  fi
  nohup "$binary" \
    --api-id "$api_id" \
    --api-hash "$api_hash" \
    --local \
    --http-port "$port" \
    --dir "$OPENFANG_HOME/telegram-local-api-data" \
    >"$TELEGRAM_LOG" 2>&1 < /dev/null &
  pid=$!
  echo "$pid" > "$TELEGRAM_PID_FILE"
  if ! wait_for_port_listener "$port" 20; then
    echo "失败：telegram-local-api 未在端口 $port 就绪" >&2
    tail -n 40 "$TELEGRAM_LOG" >&2 || true
    return 1
  fi
  echo "telegram-local-api: started pid=$pid port=$port"
}

start_openfang() {
  local binary pid
  sync_bootstrap_workflows
  if [[ "$(json_status "$OPENFANG_BASE_URL/api/health" 2>/dev/null || true)" == "ok" ]]; then
    echo "openfang: already healthy"
    return 0
  fi
  binary="$(resolve_openfang_bin)"
  if [[ ! -x "$binary" ]]; then
    echo "openfang binary missing, building debug binary..."
    cargo build -p openfang-cli
  fi
  binary="$(resolve_openfang_bin)"
  load_runtime_env
  nohup "$binary" start >"$OPENFANG_LOG" 2>&1 < /dev/null &
  pid=$!
  echo "$pid" > "$OPENFANG_PID_FILE"
  if ! wait_for_http_ok_or_exit "$OPENFANG_BASE_URL/api/health" "$pid" 40; then
    echo "失败：OpenFang API 未就绪" >&2
    tail -n 60 "$OPENFANG_LOG" >&2 || true
    return 1
  fi
  echo "openfang: started pid=$pid"
}

start_media() {
  local cli_bin python_bin config_path pid
  if [[ "$(json_status "$MEDIA_BASE_URL/healthz" 2>/dev/null || true)" == "ok" ]]; then
    echo "media-web: already healthy"
    return 0
  fi
  cli_bin="$(resolve_media_cli_bin)"
  python_bin="$(resolve_media_python_bin)"
  config_path="$(resolve_media_config)"
  if [[ ! -f "$config_path" ]]; then
    echo "失败：未找到媒体服务配置文件 $config_path" >&2
    return 1
  fi
  if [[ -x "$cli_bin" ]]; then
    nohup "$cli_bin" web --host 127.0.0.1 --port 8000 --config "$config_path" >"$MEDIA_LOG" 2>&1 < /dev/null &
  elif [[ -x "$python_bin" ]]; then
    nohup "$python_bin" "$SUBMODULE_DIR/scripts/run_media_web.py" --host 127.0.0.1 --port 8000 --config "$config_path" >"$MEDIA_LOG" 2>&1 < /dev/null &
  elif command -v uv >/dev/null 2>&1; then
    nohup uv run video-watermark web --host 127.0.0.1 --port 8000 --config "$config_path" >"$MEDIA_LOG" 2>&1 < /dev/null &
  else
    echo "失败：既没找到 media CLI，也没找到 Python launcher，也没找到 uv。" >&2
    return 1
  fi
  pid=$!
  echo "$pid" > "$MEDIA_PID_FILE"
  if ! wait_for_http_ok_or_exit "$MEDIA_BASE_URL/healthz" "$pid" 40; then
    echo "失败：shipinbot 媒体服务未就绪" >&2
    tail -n 60 "$MEDIA_LOG" >&2 || true
    return 1
  fi
  echo "media-web: started pid=$pid"
}

stop_by_pid_file() {
  local pid_file="$1"
  local name="$2"
  local pid
  pid="$(resolve_pid "$pid_file" || true)"
  if pid_running "$pid"; then
    kill "$pid" 2>/dev/null || true
    echo "$name: sent TERM to pid=$pid"
  fi
  cleanup_pid_file "$pid_file"
}

stop_openfang() {
  local binary pid stale_pid
  binary="$(resolve_openfang_bin)"
  if [[ "$(json_status "$OPENFANG_BASE_URL/api/health" 2>/dev/null || true)" == "ok" ]]; then
    "$binary" stop >/dev/null 2>&1 || true
  fi
  stop_by_pid_file "$OPENFANG_PID_FILE" "openfang"
  if ! wait_for_port_gone 4200 20; then
    pid="$(pid_listener_on_port 4200 || true)"
    if [[ -n "$pid" ]]; then
      kill "$pid" 2>/dev/null || true
    fi
  fi
  while IFS= read -r stale_pid; do
    [[ -n "$stale_pid" ]] || continue
    kill "$stale_pid" 2>/dev/null || true
  done < <(list_openfang_daemon_pids || true)
  wait_for_port_gone 4200 20 || true
}

stop_media() {
  local pid cwd command
  stop_by_pid_file "$MEDIA_PID_FILE" "media-web"
  pid="$(pid_listener_on_port 8000 || true)"
  if [[ -n "$pid" ]]; then
    cwd="$(pid_cwd "$pid")"
    command="$(pid_command "$pid")"
    if [[ "$cwd" == "$SUBMODULE_DIR" ]] || [[ "$command" == *"video-watermark web"* ]] || [[ "$command" == *"run_media_web.py"* ]]; then
      kill "$pid" 2>/dev/null || true
    fi
  fi
  wait_for_port_gone 8000 20 || true
}

stop_telegram() {
  local port pid command
  port="$(telegram_local_api_port)"
  stop_by_pid_file "$TELEGRAM_PID_FILE" "telegram-local-api"
  pid="$(pid_listener_on_port "$port" || true)"
  if [[ -n "$pid" ]]; then
    command="$(pid_command "$pid")"
    if [[ "$command" == *"telegram-bot-api"* ]]; then
      kill "$pid" 2>/dev/null || true
    fi
  fi
  wait_for_port_gone "$port" 20 || true
}

show_logs() {
  case "${1:-}" in
    openfang) tail -n 80 "$OPENFANG_LOG" 2>/dev/null || true ;;
    media) tail -n 80 "$MEDIA_LOG" 2>/dev/null || true ;;
    telegram) tail -n 80 "$TELEGRAM_LOG" 2>/dev/null || true ;;
    "")
      echo "--- openfang ---"
      tail -n 40 "$OPENFANG_LOG" 2>/dev/null || true
      echo
      echo "--- media-web ---"
      tail -n 40 "$MEDIA_LOG" 2>/dev/null || true
      echo
      echo "--- telegram-local-api ---"
      tail -n 40 "$TELEGRAM_LOG" 2>/dev/null || true
      ;;
    *)
      usage >&2
      exit 1
      ;;
  esac
}

status_stack() {
  echo "Managed runtime dir: $STACK_DIR"
  echo "OpenFang log:        $OPENFANG_LOG"
  echo "Media log:           $MEDIA_LOG"
  echo "Telegram log:        $TELEGRAM_LOG"
  echo
  "$ROOT_DIR/scripts/check-host-stack.sh" || true
}

check_stack() {
  "$ROOT_DIR/scripts/check-host-stack.sh"
}

main() {
  require_cmd curl
  require_cmd python3
  require_cmd lsof

  case "${1:-}" in
    start)
      start_telegram
      start_openfang
      start_media
      check_stack
      ;;
    stop)
      stop_openfang
      stop_media
      stop_telegram
      ;;
    restart)
      stop_openfang
      stop_media
      stop_telegram
      start_telegram
      start_openfang
      start_media
      check_stack
      ;;
    status)
      status_stack
      ;;
    check)
      check_stack
      ;;
    logs)
      show_logs "${2:-}"
      ;;
    *)
      usage >&2
      exit 1
      ;;
  esac
}

main "$@"
