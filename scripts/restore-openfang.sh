#!/usr/bin/env bash
set -euo pipefail
umask 0077

usage() {
  cat <<'EOF'
Usage: restore-openfang.sh <backup-dir> [--yes]

Restore an OpenFang runtime backup created by backup-openfang.sh.

Environment:
  OPENFANG_HOME               Restore target (default: $HOME/.openfang)
  OPENFANG_ENV_FILE           Optional external env restore target (for example /etc/openfang/env)
                              If unset, restore auto-detects /etc/openfang/env when present
  OPENFANG_SKIP_SAFETY_BACKUP Set to 1 to skip creating a pre-restore backup
  OPENFANG_ALLOW_LEGACY_RESTORE Set to 1 to restore a directory without BACKUP.txt
  OPENFANG_UID                Target owner uid for restored files (optional)
  OPENFANG_GID                Target owner gid for restored files (optional)
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 1
fi

CONFIRM="false"
BACKUP_DIR=""
for arg in "$@"; do
  case "${arg}" in
    --yes)
      CONFIRM="true"
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [[ -z "${BACKUP_DIR}" ]]; then
        BACKUP_DIR="${arg}"
      else
        echo "Unexpected argument: ${arg}" >&2
        exit 1
      fi
      ;;
  esac
done

if [[ -z "${BACKUP_DIR}" || ! -d "${BACKUP_DIR}" ]]; then
  echo "Backup directory not found: ${BACKUP_DIR}" >&2
  exit 1
fi

OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"
if [[ "${OPENFANG_HOME}" != "/" ]]; then
  OPENFANG_HOME="${OPENFANG_HOME%/}"
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RESTORE_STAMP="$(date +%Y%m%d-%H%M%S)"
HOME_PARENT="$(dirname "${OPENFANG_HOME}")"
HOME_BASENAME="$(basename "${OPENFANG_HOME}")"
STAGING_HOME="${HOME_PARENT}/.${HOME_BASENAME}.restore-${RESTORE_STAMP}-$$"
ROLLBACK_HOME="${HOME_PARENT}/.${HOME_BASENAME}.rollback-${RESTORE_STAMP}-$$"

config_dependency_paths() {
  local config_path="$1"
  [[ -f "${config_path}" ]] || return 0

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

cleanup_restore_workspace() {
  if [[ -n "${STAGING_HOME:-}" && -d "${STAGING_HOME}" ]]; then
    rm -rf "${STAGING_HOME}"
  fi
}

trap cleanup_restore_workspace EXIT

stat_uid() {
  local path="$1"
  local uid
  if uid="$(stat -c '%u' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${uid}"
    return 0
  fi
  if uid="$(stat -f '%u' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${uid}"
    return 0
  fi
  return 1
}

stat_gid() {
  local path="$1"
  local gid
  if gid="$(stat -c '%g' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${gid}"
    return 0
  fi
  if gid="$(stat -f '%g' "${path}" 2>/dev/null)"; then
    printf '%s\n' "${gid}"
    return 0
  fi
  return 1
}

TARGET_UID="${OPENFANG_UID:-}"
TARGET_GID="${OPENFANG_GID:-}"
EXISTING_HOME_UID=""
EXISTING_HOME_GID=""
if [[ -e "${OPENFANG_HOME}" ]]; then
  EXISTING_HOME_UID="$(stat_uid "${OPENFANG_HOME}" || true)"
  EXISTING_HOME_GID="$(stat_gid "${OPENFANG_HOME}" || true)"
fi
if [[ -z "${TARGET_UID}" ]]; then
  TARGET_UID="${EXISTING_HOME_UID}"
fi
if [[ -z "${TARGET_GID}" ]]; then
  TARGET_GID="${EXISTING_HOME_GID}"
fi
if [[ "$(id -u)" == "0" ]]; then
  if [[ -z "${TARGET_UID}" ]] && id -u openfang >/dev/null 2>&1; then
    TARGET_UID="$(id -u openfang)"
  fi
  if [[ -z "${TARGET_GID}" ]] && id -g openfang >/dev/null 2>&1; then
    TARGET_GID="$(id -g openfang)"
  fi
fi

backup_contains_runtime_state() {
  local backup_dir="$1"
  local candidate

  for candidate in \
    "${backup_dir}/config.toml" \
    "${backup_dir}/vault.enc" \
    "${backup_dir}/hand_state.json" \
    "${backup_dir}/cron_jobs.json" \
    "${backup_dir}/custom_models.json" \
    "${backup_dir}/integrations.toml" \
    "${backup_dir}/data/openfang.db" \
    "${backup_dir}/agents" \
    "${backup_dir}/skills" \
    "${backup_dir}/workspaces" \
    "${backup_dir}/workflows"; do
    if [[ -e "${candidate}" ]]; then
      return 0
    fi
  done

  return 1
}

validate_backup_dir() {
  if [[ ! -f "${BACKUP_DIR}/BACKUP.txt" ]]; then
    if [[ "${OPENFANG_ALLOW_LEGACY_RESTORE:-0}" != "1" ]]; then
      echo "Backup directory ${BACKUP_DIR} is missing BACKUP.txt." >&2
      echo "Refusing destructive restore without a manifest. Re-run with OPENFANG_ALLOW_LEGACY_RESTORE=1 only if you have independently verified this backup." >&2
      exit 1
    fi
    echo "warn ${BACKUP_DIR} has no BACKUP.txt manifest; proceeding because OPENFANG_ALLOW_LEGACY_RESTORE=1 is set." >&2
  fi

  if ! backup_contains_runtime_state "${BACKUP_DIR}"; then
    echo "Backup directory ${BACKUP_DIR} does not contain recoverable OpenFang runtime assets." >&2
    echo "Refusing to wipe ${OPENFANG_HOME} for an empty or malformed backup." >&2
    exit 1
  fi
}

validate_backup_dir

backup_manifest_value() {
  local key="$1"
  local value
  value="$(
    grep -E "^${key}=" "${BACKUP_DIR}/BACKUP.txt" 2>/dev/null \
      | tail -n 1 \
      | cut -d '=' -f2-
  )"
  printf '%s' "${value}"
}

BACKUP_EXTERNAL_ENV_SOURCE="$(backup_manifest_value external_env_source)"
EXTERNAL_ENV_FILE="${OPENFANG_ENV_FILE:-}"
if [[ -z "${EXTERNAL_ENV_FILE}" && -f /etc/openfang/env ]]; then
  EXTERNAL_ENV_FILE="/etc/openfang/env"
fi
if [[ -z "${EXTERNAL_ENV_FILE}" && -n "${BACKUP_EXTERNAL_ENV_SOURCE}" ]]; then
  EXTERNAL_ENV_FILE="${BACKUP_EXTERNAL_ENV_SOURCE}"
  echo "warn external env target auto-derived from backup manifest: ${EXTERNAL_ENV_FILE}" >&2
fi

daemon_health_url() {
  local daemon_info_path="$1"
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
try:
    cfg = load_config_with_includes(path)
except Exception:
    raise SystemExit(1)

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

probe_health_endpoint() {
  local endpoint="$1"
  local source="$2"

  [[ -n "${endpoint}" ]] || return 0

  if curl -fsS --max-time 2 "${endpoint}" >/dev/null 2>&1; then
    echo "OpenFang appears to still be running (${endpoint}, discovered via ${source}). Stop it before restoring." >&2
    exit 1
  fi

  return 0
}

if [[ -f "${OPENFANG_HOME}/daemon.json" ]]; then
  daemon_health_endpoint="$(daemon_health_url "${OPENFANG_HOME}/daemon.json" || true)"
  probe_health_endpoint "${daemon_health_endpoint}" "daemon.json"
  echo "warn stale daemon.json found under ${OPENFANG_HOME}; continuing because the API is not responding." >&2
fi

if [[ -f "${OPENFANG_HOME}/config.toml" ]]; then
  config_health_endpoint="$(config_health_url "${OPENFANG_HOME}/config.toml" || true)"
  probe_health_endpoint "${config_health_endpoint}" "config.toml"
fi

if [[ "${CONFIRM}" != "true" ]]; then
  echo "Restore target: ${OPENFANG_HOME}" >&2
  echo "Backup source:  ${BACKUP_DIR}" >&2
  echo "Re-run with --yes after the daemon is fully stopped." >&2
  exit 1
fi

if [[ "${OPENFANG_SKIP_SAFETY_BACKUP:-0}" != "1" && -d "${OPENFANG_HOME}" ]]; then
  "${SCRIPT_DIR}/backup-openfang.sh" >/dev/null
fi

rm -rf "${STAGING_HOME}" "${ROLLBACK_HOME}"
mkdir -p "${HOME_PARENT}"
mkdir -p "${STAGING_HOME}"

restore_path() {
  local source="$1"
  local target="$2"

  if [[ -d "${source}" ]]; then
    rm -rf "${target}"
    mkdir -p "$(dirname "${target}")"
    cp -a "${source}" "${target}"
  elif [[ -f "${source}" ]]; then
    mkdir -p "$(dirname "${target}")"
    cp -a "${source}" "${target}"
  fi
}

restore_config_tree() {
  local source_config="$1"
  local target_home="$2"

  restore_path "${source_config}" "${target_home}/config.toml"
  [[ -f "${source_config}" ]] || return 0

  local source_root source_abs
  source_root="$(cd "$(dirname "${source_config}")" && pwd -P)"
  source_abs="${source_root}/$(basename "${source_config}")"

  while IFS= read -r -d '' dependency; do
    local rel_path
    if [[ "${dependency}" == "${source_abs}" ]]; then
      continue
    fi
    rel_path="${dependency#${source_root}/}"
    if [[ "${rel_path}" == "${dependency}" ]]; then
      echo "Config dependency escaped backup root: ${dependency}" >&2
      exit 1
    fi
    restore_path "${dependency}" "${target_home}/${rel_path}"
  done < <(config_dependency_paths "${source_config}")
}

restore_external_env_file() {
  local backup_env_file="$1"
  local target_env_file="$2"

  if [[ ! -f "${backup_env_file}" ]]; then
    return 0
  fi

  if [[ -z "${target_env_file}" ]]; then
    echo "warn backup includes external-env.env but no target path was resolved. Set OPENFANG_ENV_FILE to restore it." >&2
    return 0
  fi

  if ! mkdir -p "$(dirname "${target_env_file}")" 2>/dev/null; then
    echo "warn could not create directory for external env file ${target_env_file}; skipping external env restore." >&2
    return 0
  fi
  if ! cp -a "${backup_env_file}" "${target_env_file}" 2>/dev/null; then
    echo "warn could not restore external env file to ${target_env_file}; check permissions or set OPENFANG_ENV_FILE explicitly." >&2
    return 0
  fi
  chmod 600 "${target_env_file}" 2>/dev/null || true
  echo "ok  restored external env file ${target_env_file}"
}

harden_permissions() {
  local target_home="$1"
  chmod go-rwx "${target_home}" 2>/dev/null || true

  if [[ -f "${target_home}/config.toml" ]]; then
    while IFS= read -r -d '' dependency; do
      if [[ -f "${dependency}" ]]; then
        chmod 600 "${dependency}" 2>/dev/null || true
      fi
    done < <(config_dependency_paths "${target_home}/config.toml")
  fi

  local target
  for target in \
    "${target_home}/config.toml" \
    "${target_home}/.env" \
    "${target_home}/secrets.env" \
    "${target_home}/vault.enc" \
    "${target_home}/hand_state.json" \
    "${target_home}/cron_jobs.json" \
    "${target_home}/custom_models.json" \
    "${target_home}/integrations.toml"; do
    if [[ -f "${target}" ]]; then
      chmod 600 "${target}" 2>/dev/null || true
    fi
  done

  for target in \
    "${target_home}/data" \
    "${target_home}/agents" \
    "${target_home}/skills" \
    "${target_home}/workspaces" \
    "${target_home}/workflows"; do
    if [[ -d "${target}" ]]; then
      chmod -R u+rwX,go-rwx "${target}" 2>/dev/null || true
    fi
  done
}

apply_restored_ownership() {
  local target_home="$1"
  local uid="$2"
  local gid="$3"

  if [[ -z "${uid}" && -z "${gid}" ]]; then
    return 0
  fi

  if [[ "$(id -u)" != "0" ]]; then
    if [[ -n "${OPENFANG_UID:-}" || -n "${OPENFANG_GID:-}" ]]; then
      echo "warn OPENFANG_UID/OPENFANG_GID were provided but current user is not root; skipping ownership reassignment." >&2
    fi
    return 0
  fi

  local owner_spec
  if [[ -n "${uid}" && -n "${gid}" ]]; then
    owner_spec="${uid}:${gid}"
  elif [[ -n "${uid}" ]]; then
    owner_spec="${uid}"
  else
    owner_spec=":${gid}"
  fi

  if chown -R "${owner_spec}" "${target_home}" 2>/dev/null; then
    echo "ok  restored ownership ${owner_spec} on ${target_home}"
  else
    echo "warn failed to apply ownership ${owner_spec} on ${target_home}; verify service user write access before restart." >&2
  fi
}

restore_config_tree "${BACKUP_DIR}/config.toml" "${STAGING_HOME}"
restore_path "${BACKUP_DIR}/.env" "${STAGING_HOME}/.env"
restore_path "${BACKUP_DIR}/secrets.env" "${STAGING_HOME}/secrets.env"
restore_path "${BACKUP_DIR}/vault.enc" "${STAGING_HOME}/vault.enc"
restore_path "${BACKUP_DIR}/hand_state.json" "${STAGING_HOME}/hand_state.json"
restore_path "${BACKUP_DIR}/cron_jobs.json" "${STAGING_HOME}/cron_jobs.json"
restore_path "${BACKUP_DIR}/custom_models.json" "${STAGING_HOME}/custom_models.json"
restore_path "${BACKUP_DIR}/integrations.toml" "${STAGING_HOME}/integrations.toml"

if [[ -d "${BACKUP_DIR}/data" ]]; then
  rm -rf "${STAGING_HOME}/data"
  cp -a "${BACKUP_DIR}/data" "${STAGING_HOME}/data"
fi

for dir_name in agents skills workspaces workflows; do
  restore_path "${BACKUP_DIR}/${dir_name}" "${STAGING_HOME}/${dir_name}"
done

rm -f "${STAGING_HOME}/daemon.json"
apply_restored_ownership "${STAGING_HOME}" "${TARGET_UID}" "${TARGET_GID}"
harden_permissions "${STAGING_HOME}"

if [[ -e "${OPENFANG_HOME}" ]]; then
  mv "${OPENFANG_HOME}" "${ROLLBACK_HOME}"
fi

if ! mv "${STAGING_HOME}" "${OPENFANG_HOME}"; then
  echo "Failed to promote restored runtime into place." >&2
  if [[ -e "${ROLLBACK_HOME}" ]]; then
    mv "${ROLLBACK_HOME}" "${OPENFANG_HOME}" || true
  fi
  exit 1
fi

STAGING_HOME=""
rm -rf "${ROLLBACK_HOME}"
restore_external_env_file "${BACKUP_DIR}/external-env.env" "${EXTERNAL_ENV_FILE}"

echo "Restore completed."
echo "Next steps:"
echo "  1. Start the daemon"
echo "  2. Run scripts/smoke-openfang.sh"
echo "  3. Run scripts/preflight-openfang.sh"
