#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
OPENFANG_BIN="${OPENFANG_BIN:-}"
OPENFANG_BASE_URL="${OPENFANG_BASE_URL:-http://127.0.0.1:4200}"
MEDIA_BASE_URL="${MEDIA_BASE_URL:-http://127.0.0.1:8000}"
TELEGRAM_LOCAL_API_PORT="${TELEGRAM_LOCAL_API_PORT:-8081}"
OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"

failures=0
missing_requirements=0

check_ok() {
  printf 'OK   %s\n' "$1"
}

check_warn() {
  printf 'WARN %s\n' "$1"
}

check_fail() {
  printf 'FAIL %s\n' "$1"
  failures=$((failures + 1))
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    check_fail "缺少命令: $1"
    return 1
  fi
  return 0
}

json_status() {
  local url="$1"
  curl -fsS -m 5 "$url" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("status",""))'
}

url_port() {
  python3 - "$1" "$2" <<'PY'
import sys
from urllib.parse import urlparse

parsed = urlparse(sys.argv[1])
default_port = int(sys.argv[2])
print(parsed.port or default_port)
PY
}

resolve_openfang_bin() {
  if [[ -n "$OPENFANG_BIN" ]]; then
    printf '%s\n' "$OPENFANG_BIN"
    return 0
  fi

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

  printf '%s\n' "$ROOT_DIR/target/debug/openfang"
}

count_matching_processes() {
  local pattern="$1"
  pgrep -fal "$pattern" 2>/dev/null | grep -v "pgrep -fal" | wc -l | tr -d ' '
}

openfang_daemon_process_count() {
  ps -Ao command= 2>/dev/null | python3 -c '
import re
import sys

count = 0
patterns = (
    re.compile(r"(^|[ /])openfang(\.exe)?\s+start($|\s)"),
    re.compile(r"\bcargo\s+run\b.*(?:\s-p\s+openfang-cli\b|\s--package\s+openfang-cli\b).*\s--\s+start($|\s)"),
)

for raw in sys.stdin:
    command = " ".join(raw.split())
    if any(pattern.search(command) for pattern in patterns):
        count += 1

print(count)
'
}

single_listener_pid() {
  local port="$1"
  lsof -nP -iTCP:"$port" -sTCP:LISTEN -Fp 2>/dev/null | sed -n 's/^p//p'
}

telegram_local_api_mode() {
  python3 - "$OPENFANG_HOME" "$TELEGRAM_LOCAL_API_PORT" <<'PY'
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

openfang_home = Path(sys.argv[1]).expanduser()
default_port = int(sys.argv[2])
config_path = openfang_home / "config.toml"
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

api_url = str(telegram.get("api_url") or "").strip()
if not api_url:
    print("local")
    raise SystemExit(0)

parsed = urlparse(api_url)
host = (parsed.hostname or "").strip().lower()
port = parsed.port or default_port
if host in {"127.0.0.1", "localhost", "::1"} and port == default_port:
    print("local")
elif host in {"127.0.0.1", "localhost", "::1"}:
    print("local-other-port")
else:
    print("remote")
PY
}

echo "Host-host stack check"
echo "Root: $ROOT_DIR"
echo

if ! require_cmd curl; then
  missing_requirements=1
fi
if ! require_cmd python3; then
  missing_requirements=1
fi
if ! require_cmd lsof; then
  missing_requirements=1
fi
if [[ "$missing_requirements" -ne 0 ]]; then
  echo
  echo "Stack check failed: missing required commands."
  exit 1
fi

OPENFANG_BIN="$(resolve_openfang_bin)"
OPENFANG_PORT="$(url_port "$OPENFANG_BASE_URL" 4200)"
MEDIA_PORT="$(url_port "$MEDIA_BASE_URL" 8000)"

if [[ ! -x "$OPENFANG_BIN" ]]; then
  check_fail "OpenFang binary 不存在或不可执行: $OPENFANG_BIN"
else
  check_ok "OpenFang binary: $OPENFANG_BIN"
fi

if [[ "$(json_status "$OPENFANG_BASE_URL/api/health" 2>/dev/null || true)" == "ok" ]]; then
  check_ok "OpenFang API 健康: $OPENFANG_BASE_URL/api/health"
else
  check_fail "OpenFang API 不健康: $OPENFANG_BASE_URL/api/health"
fi

if [[ "$(json_status "$MEDIA_BASE_URL/healthz" 2>/dev/null || true)" == "ok" ]]; then
  check_ok "shipinbot 媒体服务健康: $MEDIA_BASE_URL/healthz"
else
  check_fail "shipinbot 媒体服务不健康: $MEDIA_BASE_URL/healthz"
fi

if [[ -x "$OPENFANG_BIN" ]]; then
  hand_active_output="$("$OPENFANG_BIN" hand active 2>/dev/null || true)"
  if printf '%s\n' "$hand_active_output" | grep -q 'shipinfabu'; then
    check_ok "shipinfabu Hand 处于 Active"
  else
    check_fail "shipinfabu Hand 未激活"
  fi
fi

openfang_listener="$(single_listener_pid "$OPENFANG_PORT")"
if [[ -n "$openfang_listener" ]] && [[ "$(printf '%s\n' "$openfang_listener" | wc -l | tr -d ' ')" == "1" ]]; then
  check_ok "${OPENFANG_PORT} 只有一个监听进程: PID $openfang_listener"
else
  check_fail "${OPENFANG_PORT} 监听异常"
fi

media_listener="$(single_listener_pid "$MEDIA_PORT")"
if [[ -n "$media_listener" ]] && [[ "$(printf '%s\n' "$media_listener" | wc -l | tr -d ' ')" == "1" ]]; then
  check_ok "${MEDIA_PORT} 只有一个监听进程: PID $media_listener"
else
  check_fail "${MEDIA_PORT} 监听异常"
fi

openfang_count="$(openfang_daemon_process_count)"
if [[ "$openfang_count" == "1" ]]; then
  check_ok "OpenFang daemon 进程单实例"
elif [[ "$openfang_count" == "0" ]]; then
  if [[ -n "$openfang_listener" ]]; then
    check_fail "${OPENFANG_PORT} 正在监听，但未识别到 OpenFang daemon 命令行"
  else
    check_fail "未发现 OpenFang daemon 进程"
  fi
else
  check_fail "发现多个 OpenFang daemon 进程: $openfang_count"
fi

media_count="$(count_matching_processes 'video-watermark web|run_media_web.py')"
if [[ "$media_count" == "1" ]]; then
  check_ok "shipinbot 媒体服务进程单实例"
elif [[ "$media_count" == "0" ]]; then
  check_fail "未发现 shipinbot 媒体服务进程"
else
  check_fail "发现多个 shipinbot 媒体服务进程: $media_count"
fi

telegram_mode="$(telegram_local_api_mode)"
if [[ "$telegram_mode" == "local" ]]; then
  telegram_listener="$(single_listener_pid "$TELEGRAM_LOCAL_API_PORT")"
  if [[ -n "$telegram_listener" ]] && [[ "$(printf '%s\n' "$telegram_listener" | wc -l | tr -d ' ')" == "1" ]]; then
    check_ok "${TELEGRAM_LOCAL_API_PORT} 只有一个监听进程: PID $telegram_listener"
  else
    check_fail "${TELEGRAM_LOCAL_API_PORT} 监听异常"
  fi

  telegram_count="$(count_matching_processes 'telegram-bot-api')"
  if [[ "$telegram_count" == "1" ]]; then
    check_ok "telegram-bot-api 进程单实例"
  elif [[ "$telegram_count" == "0" ]]; then
    check_fail "未发现 telegram-bot-api 进程"
  else
    check_fail "发现多个 telegram-bot-api 进程: $telegram_count"
  fi
elif [[ "$telegram_mode" == "local-other-port" ]]; then
  check_warn "Telegram Local Bot API 配置指向本机非默认端口，已跳过 ${TELEGRAM_LOCAL_API_PORT} 固定端口检查"
elif [[ "$telegram_mode" == "remote" ]]; then
  check_warn "Telegram Local Bot API 配置指向非本机地址，已跳过本机 8081/进程检查"
elif [[ "$telegram_mode" == "disabled" ]]; then
  check_ok "当前配置未要求本机 telegram-bot-api"
else
  check_warn "无法从配置判断 telegram-bot-api 是否应在本机运行，已跳过固定检查"
fi

daemon_json_path="$OPENFANG_HOME/daemon.json"
if [[ -f "$daemon_json_path" ]]; then
  daemon_pid="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("pid",""))' "$daemon_json_path" 2>/dev/null || true)"
  if [[ -n "$daemon_pid" ]] && [[ "$daemon_pid" == "$openfang_listener" ]]; then
    check_ok "daemon.json 与 ${OPENFANG_PORT} 监听 PID 一致"
  else
    check_warn "daemon.json 与实际监听 PID 不一致（可能是陈旧状态文件）"
  fi
else
  check_warn "缺少 $daemon_json_path（当前不再把它当唯一真相源）"
fi

if command -v launchctl >/dev/null 2>&1; then
  if launchctl list 2>/dev/null | grep -Eq 'openfang|telegram'; then
    check_warn "launchctl 中仍有 openfang/telegram 相关条目，当前宿主机口径不建议混用"
  else
    check_ok "launchctl 中没有残留 openfang/telegram 自启动条目"
  fi
else
  check_ok "当前主机无 launchctl，跳过自启动残留检查"
fi

echo
if [[ "$failures" -eq 0 ]]; then
  echo "Stack healthy and clean."
else
  echo "Stack check failed: $failures problem(s)."
  exit 1
fi
