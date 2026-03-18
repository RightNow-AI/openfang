#!/usr/bin/env bash
set -euo pipefail
umask 0077

usage() {
  cat <<'EOF'
Usage: backup-openfang.sh [backup-root]

Create a timestamped OpenFang runtime backup under:
  ${1:-$HOME/openfang-backups}/openfang-YYYYmmdd-HHMMSS

Environment:
  OPENFANG_HOME          Runtime home to back up (default: $HOME/.openfang)
  OPENFANG_KEEP_BACKUPS  Number of backups to keep in backup-root (default: 5)
  OPENFANG_ALLOW_LIVE_BACKUP
                         Set to 1 only when you intentionally accept a live,
                         potentially point-in-time-inconsistent filesystem copy
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"
BACKUP_ROOT="${1:-$HOME/openfang-backups}"
KEEP_BACKUPS="${OPENFANG_KEEP_BACKUPS:-5}"
ALLOW_LIVE_BACKUP="${OPENFANG_ALLOW_LIVE_BACKUP:-0}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
mkdir -p "${BACKUP_ROOT}"
chmod 700 "${BACKUP_ROOT}"
DEST="${BACKUP_ROOT}/openfang-${TIMESTAMP}"

if [[ ! -d "${OPENFANG_HOME}" ]]; then
  echo "OpenFang home does not exist: ${OPENFANG_HOME}" >&2
  exit 1
fi

daemon_health_url() {
  local daemon_info_path="$1"

  command -v python3 >/dev/null 2>&1 || return 1

  python3 - "${daemon_info_path}" <<'PY'
import ipaddress
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    info = json.loads(path.read_text(encoding="utf-8"))
except Exception:
    raise SystemExit(1)

listen_addr = str(info.get("listen_addr", "")).strip()
if not listen_addr:
    raise SystemExit(1)

if listen_addr.startswith("[") and "]:" in listen_addr:
    host, port = listen_addr[1:].split("]:", 1)
elif listen_addr.count(":") == 1:
    host, port = listen_addr.rsplit(":", 1)
else:
    host, port = listen_addr, "4200"

host = host.strip().strip("[]")
if host in {"", "0.0.0.0", "::", "localhost"}:
    host = "127.0.0.1"
else:
    try:
        ipaddress.ip_address(host)
        if ":" in host:
            host = f"[{host}]"
    except ValueError:
        pass

print(f"http://{host}:{port}/api/health")
PY
}

config_health_url() {
  local config_path="$1"

  command -v python3 >/dev/null 2>&1 || return 1

  python3 - "${config_path}" <<'PY'
import ipaddress
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    raise SystemExit(1)

MAX_INCLUDE_DEPTH = 10


def deep_merge(base, overlay):
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            deep_merge(base[key], value)
        else:
            base[key] = value
    return base


def load_config_with_includes(config_path, visited=None, depth=0):
    if depth > MAX_INCLUDE_DEPTH:
        raise SystemExit(f"config include depth exceeded {MAX_INCLUDE_DEPTH}")

    if visited is None:
        visited = set()

    canonical_path = config_path.resolve(strict=True)
    if canonical_path in visited:
        raise SystemExit(f"circular config include detected: {config_path}")
    visited.add(canonical_path)

    config_dir = canonical_path.parent
    root = tomllib.loads(canonical_path.read_text(encoding="utf-8"))
    includes = root.get("include") or []
    merged = {}

    if not isinstance(includes, list):
        raise SystemExit("config include must be an array")

    for include in includes:
        if not isinstance(include, str):
            continue
        include_path = Path(include)
        if include_path.is_absolute():
            raise SystemExit(f"config include rejects absolute path: {include}")
        if ".." in include_path.parts:
            raise SystemExit(f"config include rejects path traversal: {include}")
        resolved = (config_dir / include_path).resolve(strict=True)
        try:
            resolved.relative_to(config_dir)
        except ValueError as exc:
            raise SystemExit(f"config include escapes config directory: {include}") from exc
        deep_merge(merged, load_config_with_includes(resolved, visited, depth + 1))

    root.pop("include", None)
    api_section = root.get("api")
    if isinstance(api_section, dict):
        for key in ("api_key", "api_listen", "log_level"):
            if key not in root and key in api_section:
                root[key] = api_section[key]

    deep_merge(merged, root)
    visited.remove(canonical_path)
    return merged

path = Path(sys.argv[1])
cfg = load_config_with_includes(path)

listen_addr = str(cfg.get("api_listen", "")).strip()
if not listen_addr and isinstance(cfg.get("api"), dict):
    listen_addr = str(cfg["api"].get("api_listen", "")).strip()
if not listen_addr:
    raise SystemExit(1)

if listen_addr.startswith("[") and "]:" in listen_addr:
    host, port = listen_addr[1:].split("]:", 1)
elif listen_addr.count(":") == 1:
    host, port = listen_addr.rsplit(":", 1)
else:
    host, port = listen_addr, "4200"

host = host.strip().strip("[]")
if host in {"", "0.0.0.0", "::", "localhost"}:
    host = "127.0.0.1"
else:
    try:
        ipaddress.ip_address(host)
        if ":" in host:
            host = f"[{host}]"
    except ValueError:
        pass

print(f"http://{host}:{port}/api/health")
PY
}

daemon_seems_running() {
  local endpoint=""

  if [[ -f "${OPENFANG_HOME}/daemon.json" ]] && command -v curl >/dev/null 2>&1; then
    endpoint="$(daemon_health_url "${OPENFANG_HOME}/daemon.json" || true)"
    if [[ -n "${endpoint}" ]] && curl -fsS --max-time 2 "${endpoint}" >/dev/null 2>&1; then
      return 0
    fi
  fi

  if [[ -f "${OPENFANG_HOME}/config.toml" ]] && command -v curl >/dev/null 2>&1; then
    endpoint="$(config_health_url "${OPENFANG_HOME}/config.toml" || true)"
    if [[ -n "${endpoint}" ]] && curl -fsS --max-time 2 "${endpoint}" >/dev/null 2>&1; then
      return 0
    fi
  fi

  return 1
}

BACKUP_MODE="offline"
if daemon_seems_running; then
  BACKUP_MODE="live"
  if [[ "${ALLOW_LIVE_BACKUP}" != "1" ]]; then
    echo "OpenFang appears to still be running." >&2
    echo "Stop the daemon before backing up, or re-run with OPENFANG_ALLOW_LIVE_BACKUP=1 if you intentionally accept a live filesystem copy." >&2
    exit 1
  fi
  echo "warn OpenFang appears to still be running; proceeding because OPENFANG_ALLOW_LIVE_BACKUP=1 is set." >&2
fi

mkdir -p "${DEST}"
chmod 700 "${DEST}"

config_dependency_paths() {
  local config_path="$1"

  [[ -f "${config_path}" ]] || return 0

  if ! command -v python3 >/dev/null 2>&1; then
    if grep -Eq '^[[:space:]]*include[[:space:]]*=' "${config_path}"; then
      echo "python3 is required to back up config include files from ${config_path}" >&2
      exit 1
    fi
    return 0
  fi

  python3 - "${config_path}" <<'PY'
import sys
from pathlib import Path
import tomllib

MAX_INCLUDE_DEPTH = 10


def collect_paths(path: Path, visited=None, depth=0):
    if depth > MAX_INCLUDE_DEPTH:
        raise SystemExit(f"config include depth exceeded {MAX_INCLUDE_DEPTH}")

    if visited is None:
        visited = set()

    canonical = path.resolve(strict=True)
    if canonical in visited:
        raise SystemExit(f"circular config include detected: {path}")
    visited.add(canonical)

    root = tomllib.loads(canonical.read_text(encoding="utf-8"))
    files = [canonical]
    includes = root.get("include") or []
    if not isinstance(includes, list):
        raise SystemExit("config include must be an array")

    config_dir = canonical.parent
    for include in includes:
        if not isinstance(include, str):
            continue
        include_path = Path(include)
        if include_path.is_absolute():
            raise SystemExit(f"config include rejects absolute path: {include}")
        if ".." in include_path.parts:
            raise SystemExit(f"config include rejects path traversal: {include}")
        resolved = (config_dir / include_path).resolve(strict=True)
        try:
            resolved.relative_to(config_dir)
        except ValueError as exc:
            raise SystemExit(f"config include escapes config directory: {include}") from exc
        files.extend(collect_paths(resolved, visited, depth + 1))

    visited.remove(canonical)
    return files


for dependency in collect_paths(Path(sys.argv[1])):
    sys.stdout.buffer.write(str(dependency).encode("utf-8"))
    sys.stdout.buffer.write(b"\0")
PY
}

copy_if_exists() {
  local source="$1"
  local target_dir="$2"
  if [[ -e "${source}" ]]; then
    mkdir -p "${target_dir}"
    cp -a "${source}" "${target_dir}/"
  fi
}

copy_tree_without_db() {
  local source_dir="$1"
  local target_dir="$2"
  [[ -d "${source_dir}" ]] || return 0

  mkdir -p "${target_dir}"
  shopt -s nullglob dotglob
  for entry in "${source_dir}"/*; do
    local name
    name="$(basename "${entry}")"
    case "${name}" in
      openfang.db|openfang.db-wal|openfang.db-shm)
        continue
        ;;
    esac
    cp -a "${entry}" "${target_dir}/"
  done
  shopt -u nullglob dotglob
}

backup_sqlite() {
  local source_db="$1"
  local target_db="$2"

  mkdir -p "$(dirname "${target_db}")"

  if command -v sqlite3 >/dev/null 2>&1; then
    sqlite3 "${source_db}" ".backup '${target_db}'"
    return 0
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "${source_db}" "${target_db}" <<'PY'
import pathlib
import sqlite3
import sys

source_db, target_db = sys.argv[1:3]
pathlib.Path(target_db).parent.mkdir(parents=True, exist_ok=True)

source = sqlite3.connect(f"file:{source_db}?mode=ro", uri=True)
target = sqlite3.connect(target_db)
with target:
    source.backup(target)
target.close()
source.close()
PY
    return 0
  fi

  echo "Need either sqlite3 or python3 to create a consistent SQLite backup." >&2
  exit 1
}

copy_config_dependencies() {
  local config_path="$1"
  [[ -f "${config_path}" ]] || return 0

  local home_root config_abs
  home_root="$(cd "${OPENFANG_HOME}" && pwd -P)"
  config_abs="$(cd "$(dirname "${config_path}")" && pwd -P)/$(basename "${config_path}")"

  while IFS= read -r -d '' dependency; do
    local rel_path
    if [[ "${dependency}" == "${config_abs}" ]]; then
      continue
    fi
    rel_path="${dependency#${home_root}/}"
    if [[ "${rel_path}" == "${dependency}" ]]; then
      echo "Config dependency escaped ${OPENFANG_HOME}: ${dependency}" >&2
      exit 1
    fi
    mkdir -p "${DEST}/$(dirname "${rel_path}")"
    cp -a "${dependency}" "${DEST}/${rel_path}"
  done < <(config_dependency_paths "${config_path}")
}

copy_if_exists "${OPENFANG_HOME}/config.toml" "${DEST}"
copy_config_dependencies "${OPENFANG_HOME}/config.toml"
copy_if_exists "${OPENFANG_HOME}/.env" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/secrets.env" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/vault.enc" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/hand_state.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/cron_jobs.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/custom_models.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/integrations.toml" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/daemon.json" "${DEST}"

copy_tree_without_db "${OPENFANG_HOME}/data" "${DEST}/data"
copy_if_exists "${OPENFANG_HOME}/agents" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/skills" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/workspaces" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/workflows" "${DEST}"

DB_PATH="${OPENFANG_HOME}/data/openfang.db"
if [[ -f "${DB_PATH}" ]]; then
  backup_sqlite "${DB_PATH}" "${DEST}/data/openfang.db"
fi

cat > "${DEST}/BACKUP.txt" <<EOF
created_at=${TIMESTAMP}
source_home=${OPENFANG_HOME}
hostname=$(hostname)
backup_mode=${BACKUP_MODE}
EOF

if [[ "${KEEP_BACKUPS}" =~ ^[0-9]+$ ]] && (( KEEP_BACKUPS > 0 )); then
  backups=()
  while IFS= read -r backup_dir; do
    backups+=("${backup_dir}")
  done < <(find "${BACKUP_ROOT}" -maxdepth 1 -type d -name 'openfang-*' | sort -r)
  if (( ${#backups[@]} > KEEP_BACKUPS )); then
    for old_backup in "${backups[@]:KEEP_BACKUPS}"; do
      rm -rf "${old_backup}"
    done
  fi
fi

if [[ -d "${DEST}" ]]; then
  chmod -R u+rwX,go-rwx "${DEST}"
fi

echo "Backup written to ${DEST}"
