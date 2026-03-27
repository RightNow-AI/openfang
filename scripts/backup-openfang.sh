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
  OPENFANG_ENV_FILE      Optional external env file to back up (for example /etc/openfang/env)
                         If unset, backup only auto-detects /etc/openfang/env when it matches the current OPENFANG_HOME
  OPENFANG_BINARY_PATH   Optional daemon binary to fingerprint in BACKUP.txt
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
CONFIG_PATH="${OPENFANG_HOME}/config.toml"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

ensure_readable_file() {
  local path="$1"
  local description="$2"
  [[ -z "${path}" || ! -e "${path}" ]] && return 0
  if [[ -r "${path}" ]]; then
    return 0
  fi
  echo "${description} is not readable by the current user." >&2
  echo "Run the backup as a user that can read ${path}, or fix the file mode/group first (for example 0640 with group openfang for /etc/openfang/env)." >&2
  exit 1
}

file_mode() {
  local path="$1"
  local mode
  if mode="$(stat -c '%a' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${mode}"
    return 0
  fi
  if mode="$(stat -f '%Lp' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${mode}"
    return 0
  fi
  return 1
}

normalize_path() {
  local path="${1:-}"
  if [[ -z "${path}" ]]; then
    return 0
  fi
  if [[ "${path}" != "/" ]]; then
    path="${path%/}"
  fi
  printf '%s\n' "${path}"
}

env_file_var() {
  local env_file="$1"
  local wanted="$2"
  [[ -f "${env_file}" ]] || return 0
  awk -F= -v wanted="${wanted}" '
    /^[[:space:]]*#/ { next }
    index($0, "=") == 0 { next }
    {
      key = $1
      sub(/^[[:space:]]+/, "", key)
      sub(/[[:space:]]+$/, "", key)
      if (key != wanted) next
      value = substr($0, index($0, "=") + 1)
      sub(/^[[:space:]]+/, "", value)
      sub(/[[:space:]]+$/, "", value)
      if ((value ~ /^".*"$/) || (value ~ /^'\''.*'\''$/)) {
        value = substr(value, 2, length(value) - 2)
      }
      print value
      exit
    }
  ' "${env_file}"
}

capture_openfang_metadata() {
  detect_openfang_binary() {
    local candidate repo_root

    if [[ -n "${OPENFANG_BINARY_PATH:-}" ]]; then
      if [[ -x "${OPENFANG_BINARY_PATH}" ]]; then
        printf '%s\n' "${OPENFANG_BINARY_PATH}"
        return 0
      fi
      echo "warn OPENFANG_BINARY_PATH is not executable: ${OPENFANG_BINARY_PATH}" >&2
    fi

    if candidate="$(command -v openfang 2>/dev/null)" && [[ -n "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi

    repo_root="$(cd "${SCRIPT_DIR}/.." && pwd)"
    for candidate in \
      "${repo_root}/target/release/openfang" \
      "${repo_root}/target/debug/openfang"; do
      if [[ -x "${candidate}" ]]; then
        printf '%s\n' "${candidate}"
        return 0
      fi
    done

    return 1
  }

  find_git_repo_root() {
    local start="${1:-}"
    [[ -n "${start}" ]] || return 1

    if [[ -f "${start}" ]]; then
      start="$(dirname "${start}")"
    fi

    if ! start="$(cd "${start}" 2>/dev/null && pwd -P)"; then
      return 1
    fi

    while [[ -n "${start}" ]]; do
      if git -C "${start}" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        printf '%s\n' "${start}"
        return 0
      fi
      [[ "${start}" == "/" ]] && break
      start="$(dirname "${start}")"
    done

    return 1
  }

  sha256_file() {
    local path="$1"
    local digest=""

    if command -v sha256sum >/dev/null 2>&1; then
      if digest="$(sha256sum "${path}" 2>/dev/null | awk '{print $1}')" && [[ -n "${digest}" ]]; then
        printf '%s\n' "${digest}"
        return 0
      fi
    fi
    if command -v shasum >/dev/null 2>&1; then
      if digest="$(shasum -a 256 "${path}" 2>/dev/null | awk '{print $1}')" && [[ -n "${digest}" ]]; then
        printf '%s\n' "${digest}"
        return 0
      fi
    fi
    if command -v openssl >/dev/null 2>&1; then
      if digest="$(openssl dgst -sha256 "${path}" 2>/dev/null | awk '{print $NF}')" && [[ -n "${digest}" ]]; then
        printf '%s\n' "${digest}"
        return 0
      fi
    fi
    if command -v python3 >/dev/null 2>&1; then
      python3 - "${path}" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
hasher = hashlib.sha256()
with path.open("rb") as fh:
    for chunk in iter(lambda: fh.read(1024 * 1024), b""):
        hasher.update(chunk)
print(hasher.hexdigest())
PY
      return 0
    fi
    return 1
  }

  detect_git_sha() {
    local binary="$1"
    local version="$2"
    local repo_root=""
    local repo_sha=""
    local version_sha=""

    version_sha="$(printf '%s\n' "${version}" | grep -oE '[0-9a-f]{7,40}' | head -n 1 || true)"
    if [[ -n "${version_sha}" ]]; then
      printf '%s\n' "${version_sha}"
      return 0
    fi

    repo_root="$(find_git_repo_root "${binary}" || true)"
    if [[ -n "${repo_root}" ]]; then
      repo_sha="$(git -C "${repo_root}" rev-parse HEAD 2>/dev/null || true)"
      if [[ -n "${repo_sha}" ]]; then
        printf '%s\n' "${repo_sha}"
        return 0
      fi
    fi

    repo_root="$(find_git_repo_root "${SCRIPT_DIR}/.." || true)"
    if [[ -n "${repo_root}" ]]; then
      repo_sha="$(git -C "${repo_root}" rev-parse HEAD 2>/dev/null || true)"
      if [[ -n "${repo_sha}" ]]; then
        printf '%s\n' "${repo_sha}"
        return 0
      fi
    fi

    return 1
  }

  local binary version sha git_sha
  binary="$(detect_openfang_binary || true)"
  if [[ -z "${binary}" ]]; then
    echo ""
    return
  fi
  version="$("${binary}" --version 2>/dev/null | head -n 1 || true)"
  sha="$(sha256_file "${binary}" 2>/dev/null || true)"
  git_sha="$(detect_git_sha "${binary}" "${version}" || true)"
  printf '%s|%s|%s|%s' "${binary}" "${version}" "${sha}" "${git_sha}"
}

auto_detect_external_env_file() {
  local openfang_home="$1"
  local candidate="/etc/openfang/env"
  [[ -f "${candidate}" ]] || return 1

  local normalized_home candidate_home
  normalized_home="$(normalize_path "${openfang_home}")"
  candidate_home="$(env_file_var "${candidate}" "OPENFANG_HOME")"
  candidate_home="$(normalize_path "${candidate_home}")"

  if [[ -n "${candidate_home}" && "${candidate_home}" != "${normalized_home}" ]]; then
    echo "warn ignoring /etc/openfang/env because it belongs to OPENFANG_HOME=${candidate_home}, not ${normalized_home}" >&2
    return 1
  fi
  if [[ -z "${candidate_home}" && "${normalized_home}" != "/var/lib/openfang" ]]; then
    echo "warn ignoring /etc/openfang/env because it does not declare OPENFANG_HOME and current OPENFANG_HOME is ${normalized_home}" >&2
    return 1
  fi

  printf '%s\n' "${candidate}"
}

EXTERNAL_ENV_FILE="${OPENFANG_ENV_FILE:-}"
if [[ -z "${EXTERNAL_ENV_FILE}" ]]; then
  EXTERNAL_ENV_FILE="$(auto_detect_external_env_file "${OPENFANG_HOME}" || true)"
fi

if [[ -n "${OPENFANG_ENV_FILE:-}" && ! -f "${OPENFANG_ENV_FILE}" ]]; then
  echo "OPENFANG_ENV_FILE was set but file does not exist: ${OPENFANG_ENV_FILE}" >&2
  exit 1
fi

if [[ -n "${EXTERNAL_ENV_FILE}" ]]; then
  ensure_readable_file "${EXTERNAL_ENV_FILE}" "External env file ${EXTERNAL_ENV_FILE}"
fi

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

config_runtime_probe() {
  local config_path="$1"
  local external_env_file="${2:-}"

  command -v python3 >/dev/null 2>&1 || return 1

  python3 - "${config_path}" "${external_env_file}" <<'PY'
import ipaddress
import os
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    raise SystemExit(1)

MAX_INCLUDE_DEPTH = 10


def parse_env_file(path):
    values = {}
    if not path.exists():
        return values
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and (
            (value.startswith('"') and value.endswith('"'))
            or (value.startswith("'") and value.endswith("'"))
        ):
            value = value[1:-1]
        if key:
            values[key] = value
    return values


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
external_env_path = Path(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] else None
cfg = load_config_with_includes(path)

effective_env = {}
if external_env_path is not None:
    effective_env.update(parse_env_file(external_env_path))
effective_env.update(os.environ)

listen_addr = str(
    effective_env.get(
        "OPENFANG_LISTEN",
        cfg.get("api_listen", ""),
    )
).strip()
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

api_key = str(
    effective_env.get(
        "OPENFANG_API_KEY",
        cfg.get("api_key", ""),
    )
).strip()
if not api_key and isinstance(cfg.get("api"), dict):
    api_key = str(cfg["api"].get("api_key", "")).strip()

print(f"http://{host}:{port}\t{api_key}")
PY
}

daemon_home_matches() {
  local base_url="$1"
  local expected_home="$2"
  local api_key="${3:-}"
  local status_json
  local curl_args=(-fsS --max-time 2)

  if [[ -n "${api_key}" ]]; then
    curl_args+=(-H "Authorization: Bearer ${api_key}")
  fi

  status_json="$(curl "${curl_args[@]}" "${base_url}/api/status" 2>/dev/null)" || return 1

  OPENFANG_STATUS_JSON="${status_json}" python3 - "${expected_home}" <<'PY'
import json
import os
import sys

expected_home = os.path.realpath(os.path.expanduser(sys.argv[1]))
payload = json.loads(os.environ["OPENFANG_STATUS_JSON"])
actual_home = str(payload.get("home_dir", "")).strip()
if not actual_home:
    raise SystemExit(1)

actual_home = os.path.realpath(os.path.expanduser(actual_home))
raise SystemExit(0 if actual_home == expected_home else 1)
PY
}

daemon_seems_running() {
  local endpoint=""
  local config_base_url=""
  local config_api_key=""

  if [[ -f "${OPENFANG_HOME}/config.toml" ]] && command -v curl >/dev/null 2>&1; then
    IFS=$'\t' read -r config_base_url config_api_key < <(
      config_runtime_probe "${OPENFANG_HOME}/config.toml" "${EXTERNAL_ENV_FILE}" || true
    )
  fi

  if [[ -f "${OPENFANG_HOME}/daemon.json" ]] && command -v curl >/dev/null 2>&1; then
    endpoint="$(daemon_health_url "${OPENFANG_HOME}/daemon.json" || true)"
    if [[ -n "${endpoint}" ]]; then
      local daemon_base_url="${endpoint%/api/health}"
      if daemon_home_matches "${daemon_base_url}" "${OPENFANG_HOME}" "${config_api_key}"; then
        return 0
      fi
      if curl -fsS --max-time 2 "${endpoint}" >/dev/null 2>&1; then
        return 0
      fi
    fi
  fi

  if [[ -n "${config_base_url}" ]] && daemon_home_matches "${config_base_url}" "${OPENFANG_HOME}" "${config_api_key}"; then
      return 0
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

config_requires_python_for_sqlite_metadata() {
  local config_path="$1"
  [[ -f "${config_path}" ]] || return 1

  if grep -Eq '^[[:space:]]*include[[:space:]]*=' "${config_path}"; then
    return 0
  fi
  if grep -Eq '^[[:space:]]*data_dir[[:space:]]*=' "${config_path}"; then
    return 0
  fi
  if grep -Eq '^[[:space:]]*sqlite_path[[:space:]]*=' "${config_path}"; then
    return 0
  fi

  return 1
}

resolve_runtime_sqlite_metadata() {
  local config_path="$1"
  local openfang_home="$2"

  if [[ ! -f "${config_path}" ]]; then
    printf '%s\t%s\n' "${openfang_home}/data/openfang.db" "data/openfang.db"
    return 0
  fi

  if ! command -v python3 >/dev/null 2>&1; then
    if config_requires_python_for_sqlite_metadata "${config_path}"; then
      echo "python3 is required to resolve runtime sqlite_path from ${config_path} when config includes, data_dir, or memory.sqlite_path are used." >&2
      exit 1
    fi
    printf '%s\t%s\n' "${openfang_home}/data/openfang.db" "data/openfang.db"
    return 0
  fi

  python3 - "${config_path}" "${openfang_home}" <<'PY'
import os
import sys
from pathlib import Path
import tomllib

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


def resolve_path(value, home_dir):
    if not value:
        return None
    path = Path(os.path.expanduser(str(value)))
    if not path.is_absolute():
        path = home_dir / path
    return path.resolve(strict=False)


config_path = Path(sys.argv[1])
home_dir = Path(sys.argv[2]).resolve(strict=False)
cfg = load_config_with_includes(config_path)
data_dir = resolve_path(cfg.get("data_dir"), home_dir) or (home_dir / "data")
memory_cfg = cfg.get("memory") or {}
if not isinstance(memory_cfg, dict):
    memory_cfg = {}
sqlite_path = resolve_path(memory_cfg.get("sqlite_path"), home_dir) or (data_dir / "openfang.db")

rel_path = ""
try:
    rel_path = str(sqlite_path.relative_to(home_dir))
except ValueError:
    rel_path = ""

print(f"{sqlite_path}\t{rel_path}")
PY
}

remove_copied_sqlite_artifacts() {
  local rel_path="$1"
  [[ -n "${rel_path}" ]] || return 0
  rm -f \
    "${DEST}/${rel_path}" \
    "${DEST}/${rel_path}-wal" \
    "${DEST}/${rel_path}-shm"
}

copy_if_exists() {
  local source="$1"
  local target_dir="$2"
  if [[ -e "${source}" ]]; then
    mkdir -p "${target_dir}"
    cp -a "${source}" "${target_dir}/"
  fi
}

copy_external_env_file() {
  local source="$1"
  local destination_file="$2"
  if [[ -n "${source}" && -f "${source}" ]]; then
    cp -a "${source}" "${destination_file}"
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
copy_if_exists "${OPENFANG_HOME}/hand_state.json.bak" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/cron_jobs.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/cron_jobs.json.bak" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/custom_models.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/custom_models.json.bak" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/integrations.toml" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/daemon.json" "${DEST}"
copy_external_env_file "${EXTERNAL_ENV_FILE}" "${DEST}/external-env.env"

IFS=$'\t' read -r SQLITE_PATH SQLITE_REL_PATH < <(
  resolve_runtime_sqlite_metadata "${CONFIG_PATH}" "${OPENFANG_HOME}"
)
if [[ -z "${SQLITE_REL_PATH}" ]]; then
  echo "Resolved sqlite_path is outside OPENFANG_HOME and cannot be backed up safely: ${SQLITE_PATH}" >&2
  echo "Move memory.sqlite_path under ${OPENFANG_HOME} or handle the external database path explicitly before treating this backup as production-safe." >&2
  exit 1
fi

copy_tree_without_db "${OPENFANG_HOME}/data" "${DEST}/data"
copy_if_exists "${OPENFANG_HOME}/agents" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/skills" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/workspaces" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/workflows" "${DEST}"
remove_copied_sqlite_artifacts "${SQLITE_REL_PATH}"

if [[ -f "${SQLITE_PATH}" ]]; then
  backup_sqlite "${SQLITE_PATH}" "${DEST}/${SQLITE_REL_PATH}"
fi

EXTERNAL_ENV_MODE=""
if [[ -n "${EXTERNAL_ENV_FILE}" && -f "${EXTERNAL_ENV_FILE}" ]]; then
  EXTERNAL_ENV_MODE="$(file_mode "${EXTERNAL_ENV_FILE}" || true)"
fi

OPENFANG_METADATA="$(capture_openfang_metadata)"
IFS='|' read -r OPENFANG_BINARY OPENFANG_VERSION OPENFANG_BINARY_SHA256 OPENFANG_GIT_SHA <<< "${OPENFANG_METADATA:-|||}"
OPENFANG_BINARY="${OPENFANG_BINARY:-unknown}"
OPENFANG_VERSION="${OPENFANG_VERSION:-unknown}"
OPENFANG_BINARY_SHA256="${OPENFANG_BINARY_SHA256:-unknown}"
OPENFANG_GIT_SHA="${OPENFANG_GIT_SHA:-unknown}"

cat > "${DEST}/BACKUP.txt" <<EOF
created_at=${TIMESTAMP}
source_home=${OPENFANG_HOME}
hostname=$(hostname)
backup_mode=${BACKUP_MODE}
external_env_source=${EXTERNAL_ENV_FILE}
external_env_mode=${EXTERNAL_ENV_MODE}
sqlite_source=${SQLITE_PATH}
sqlite_rel_path=${SQLITE_REL_PATH}
openfang_binary=${OPENFANG_BINARY}
openfang_version=${OPENFANG_VERSION}
openfang_binary_sha256=${OPENFANG_BINARY_SHA256}
openfang_git_sha=${OPENFANG_GIT_SHA}
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
