#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: preflight-openfang.sh [--offline] [base-url]

Run production-readiness checks against local runtime files and, when reachable,
the running OpenFang API.

Environment:
  OPENFANG_HOME              Runtime home to inspect (default: $HOME/.openfang)
  OPENFANG_BASE_URL          Base URL override (default: http://127.0.0.1:4200)
  OPENFANG_API_KEY           Bearer token used for protected API checks
  OPENFANG_ENV_FILE          Optional external env file (for example /etc/openfang/env)
                             If unset, preflight only auto-detects /etc/openfang/env when it matches the current OPENFANG_HOME
  OPENFANG_PREFLIGHT_OFFLINE Set to 1/true/yes/on to skip live API checks
  OPENFANG_STRICT_PRODUCTION Set to 1/true/yes/on to fail unless protected checks have a machine API key
EOF
}

truthy() {
  local value="${1:-}"
  value="$(printf '%s' "${value}" | tr '[:upper:]' '[:lower:]')"
  case "${value}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

OFFLINE_MODE="${OPENFANG_PREFLIGHT_OFFLINE:-0}"
BASE_URL_CLI_ARG=""
while [[ $# -gt 0 ]]; do
  case "${1}" in
    -h|--help)
      usage
      exit 0
      ;;
    --offline)
      OFFLINE_MODE="1"
      ;;
    *)
      if [[ -z "${BASE_URL_CLI_ARG}" ]]; then
        BASE_URL_CLI_ARG="${1}"
      else
        echo "Unexpected argument: ${1}" >&2
        usage >&2
        exit 1
      fi
      ;;
  esac
  shift
done

OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"
BASE_URL_OVERRIDE="${BASE_URL_CLI_ARG:-${OPENFANG_BASE_URL:-}}"
CONFIG_PATH="${OPENFANG_HOME}/config.toml"
STRICT_PRODUCTION="${OPENFANG_STRICT_PRODUCTION:-0}"

ensure_readable_file() {
  local path="$1"
  local description="$2"
  [[ -z "${path}" || ! -e "${path}" ]] && return 0
  if [[ -r "${path}" ]]; then
    return 0
  fi
  echo "${description} is not readable by the current user." >&2
  echo "For systemd deployments, keep /etc/openfang/env readable by the openfang service user (for example mode 0640 with group openfang), or run the script as a user that can read ${path}." >&2
  exit 1
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

required_commands=(python3)
if ! truthy "${OFFLINE_MODE}"; then
  required_commands+=(curl)
fi

for cmd in "${required_commands[@]}"; do
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "missing required command: ${cmd}" >&2
    exit 1
  fi
done

if [[ ! -f "${CONFIG_PATH}" ]]; then
  echo "missing config: ${CONFIG_PATH}" >&2
  exit 1
fi

runtime_inspect() {
  local mode="$1"
  python3 - "${CONFIG_PATH}" "${OPENFANG_HOME}" "${mode}" "${EXTERNAL_ENV_FILE}" "${STRICT_PRODUCTION}" <<'PY'
import ipaddress
import os
import stat
import sys
from pathlib import Path
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[import-not-found]

PLACEHOLDER_API_KEYS = {
    "",
    "change-me",
    "changeme",
    "replace-me",
    "replace_me",
    "your-api-key",
    "your_api_key",
    "example",
    "example-key",
    "example_api_key",
    "test",
    "secret",
}
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
        if not key:
            continue
        if len(value) >= 2 and (
            (value.startswith('"') and value.endswith('"'))
            or (value.startswith("'") and value.endswith("'"))
        ):
            value = value[1:-1]
        values[key] = value
    return values


def runtime_helper_env(home_dir):
    env = {}

    # Runtime env files: secrets.env overrides .env
    for env_path in (home_dir / ".env", home_dir / "secrets.env"):
        env.update(parse_env_file(env_path))

    return env


def runtime_override_env(external_env_path):
    env = {}

    # Optional external env file (for example systemd EnvironmentFile path)
    if external_env_path is not None:
        env.update(parse_env_file(external_env_path))

    # Process environment has final override priority.
    env.update(os.environ)
    return env


def deep_merge(base, overlay):
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            deep_merge(base[key], value)
        else:
            base[key] = value
    return base


def load_config_with_includes(
    config_path,
    visited=None,
    depth=0,
):
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


def collect_config_dependency_paths(
    config_path,
    visited=None,
    depth=0,
):
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
    files = [canonical_path]

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
        files.extend(collect_config_dependency_paths(resolved, visited, depth + 1))

    visited.remove(canonical_path)
    return files


def truthy(value):
    return str(value).strip().lower() in {"1", "true", "yes", "on"}


def split_host_port(addr):
    listen = str(addr).strip()
    if not listen:
        return "127.0.0.1", "4200"
    if listen.startswith("[") and "]:" in listen:
        host, port = listen[1:].split("]:", 1)
        return host, port
    if listen.count(":") == 1:
        return listen.rsplit(":", 1)
    if ":" not in listen:
        return listen, "4200"
    return listen, "4200"


def is_loopback_host(host):
    normalized = host.strip().strip("[]")
    if normalized == "localhost":
        return True
    try:
        return ipaddress.ip_address(normalized).is_loopback
    except ValueError:
        return False


def is_public_bind(listen_addr):
    host, _ = split_host_port(listen_addr)
    normalized = host.strip().strip("[]")
    if normalized in {"", "0.0.0.0", "::"}:
        return True
    return not is_loopback_host(normalized)


def is_supported_password_hash(value):
    trimmed = value.strip()
    if trimmed.startswith("$argon2"):
        parts = trimmed.split("$")
        return len(parts) >= 6 and parts[1].startswith("argon2")
    return len(trimmed) == 64 and all(ch in "0123456789abcdefABCDEF" for ch in trimmed)


def base_url_for(listen_addr):
    host, port = split_host_port(listen_addr)
    normalized = host.strip().strip("[]")
    if normalized in {"", "0.0.0.0", "::", "localhost"}:
        normalized = "127.0.0.1"
    try:
        ipaddress.ip_address(normalized)
        if ":" in normalized:
            normalized = f"[{normalized}]"
    except ValueError:
        pass
    return f"http://{normalized}:{port}"


def resolve_config_path(path_value, base_dir):
    if path_value in (None, ""):
        return None
    if not isinstance(path_value, str):
        path_value = str(path_value)
    path = Path(os.path.expanduser(path_value))
    if not path.is_absolute():
        path = base_dir / path
    return path


config_path = Path(sys.argv[1])
openfang_home = Path(sys.argv[2])
mode = sys.argv[3]
external_env_arg = sys.argv[4].strip() if len(sys.argv) > 4 else ""
external_env_path = Path(external_env_arg) if external_env_arg else None
strict_production = truthy(sys.argv[5]) if len(sys.argv) > 5 else False

cfg = load_config_with_includes(config_path)
helper_env = runtime_helper_env(openfang_home)
runtime_env = runtime_override_env(external_env_path)
data_dir = resolve_config_path(cfg.get("data_dir"), openfang_home) or (openfang_home / "data")
memory_cfg = cfg.get("memory") or {}
if not isinstance(memory_cfg, dict):
    memory_cfg = {}
sqlite_path = resolve_config_path(memory_cfg.get("sqlite_path"), openfang_home) or (
    data_dir / "openfang.db"
)

auth_cfg = cfg.get("auth") or {}
if not isinstance(auth_cfg, dict):
    auth_cfg = {}
network_cfg = cfg.get("network") or {}
if not isinstance(network_cfg, dict):
    network_cfg = {}

api_listen = str(
    runtime_env.get("OPENFANG_LISTEN", cfg.get("api_listen", "127.0.0.1:4200"))
).strip()
config_api_key = str(cfg.get("api_key", "")).strip()
runtime_api_key = str(runtime_env.get("OPENFANG_API_KEY", "")).strip()
effective_api_key = runtime_api_key or config_api_key
auth_enabled = bool(auth_cfg.get("enabled", False))
password_hash = str(auth_cfg.get("password_hash", "")).strip()
network_enabled = bool(cfg.get("network_enabled", False))
network_shared_secret = str(network_cfg.get("shared_secret", "")).strip()
max_cron_jobs = int(cfg.get("max_cron_jobs", 500) or 0)
ignored_helper_override_keys = sorted(
    key for key in ("OPENFANG_LISTEN", "OPENFANG_API_KEY") if key in helper_env
)

if mode == "base_url":
    print(base_url_for(api_listen))
    raise SystemExit(0)

if mode == "effective_api_key":
    print(effective_api_key)
    raise SystemExit(0)

if mode == "sqlite_path":
    print(sqlite_path)
    raise SystemExit(0)

if mode != "validate":
    raise SystemExit(f"unknown inspect mode: {mode}")

print("\n== Config Baseline ==")
print(f"ok  api_listen={api_listen}")
print(f"ok  effective_base_url={base_url_for(api_listen)}")
print(f"ok  sqlite_path={sqlite_path}")
if external_env_path is not None and external_env_path.exists():
    print("ok  config_resolution=config.toml + includes; runtime overrides from external env + process env")
else:
    print("ok  config_resolution=config.toml + includes; runtime overrides from process env")

for key in ignored_helper_override_keys:
    print(
        f"warn {key} is set in runtime helper files (.env/secrets.env) but the daemon ignores it there; move it to the real process environment or OPENFANG_ENV_FILE"
    )

if is_public_bind(api_listen) and not effective_api_key and not (auth_enabled and password_hash):
    raise SystemExit(
        f"public bind {api_listen} has no usable authentication; configure api_key or [auth].password_hash"
    )
if auth_enabled and not password_hash:
    raise SystemExit("auth.enabled=true but auth.password_hash is empty")
if auth_enabled and not is_supported_password_hash(password_hash):
    raise SystemExit(
        "auth.enabled=true but auth.password_hash is not a supported Argon2id PHC string or 64-character legacy SHA-256 hex digest"
    )
if network_enabled and not network_shared_secret:
    raise SystemExit("network_enabled=true but network.shared_secret is empty")
if max_cron_jobs > 10_000:
    raise SystemExit("max_cron_jobs exceeds reasonable limit (10000)")
if effective_api_key.strip().lower() in PLACEHOLDER_API_KEYS:
    raise SystemExit("api_key is still a placeholder/example value")

print(f"ok  dashboard_auth={'enabled' if auth_enabled else 'disabled'}")
if runtime_api_key:
    print("ok  api_key=set-via-runtime-env")
elif config_api_key:
    print("ok  api_key=set-in-config")
else:
    print("ok  api_key=not-set")

if os.name == "posix":
    print("\n== Sensitive File Permissions ==")
    candidates = collect_config_dependency_paths(config_path)
    candidates.extend(
        [
            openfang_home / ".env",
            openfang_home / "secrets.env",
            openfang_home / "vault.enc",
        ]
    )
    if external_env_path is not None:
        candidates.append(external_env_path)
    resolved_external_env = None
    if external_env_path is not None and external_env_path.exists():
        resolved_external_env = external_env_path.resolve(strict=True)
    seen = set()
    for candidate in candidates:
        if not candidate.exists():
            continue
        candidate = candidate.resolve(strict=True)
        if candidate in seen:
            continue
        seen.add(candidate)
        mode_bits = stat.S_IMODE(candidate.stat().st_mode)
        if (
            resolved_external_env is not None
            and candidate == resolved_external_env
            and mode_bits & 0o027 == 0
        ):
            print(f"ok  {candidate}")
            continue
        if mode_bits & 0o077:
            message = (
                f"{candidate} permissions are {oct(mode_bits)}; restrict to owner-only access"
            )
            if resolved_external_env is not None and candidate == resolved_external_env:
                message = (
                    f"{candidate} permissions are {oct(mode_bits)}; keep external env files at 0600 or group-readable only (for example 0640 root:openfang)"
                )
            if strict_production:
                raise SystemExit(
                    f"strict production mode rejects loose sensitive file permissions: {message}"
                )
            print(f"warn {message}")
        else:
            print(f"ok  {candidate}")
PY
}

BASE_URL="${BASE_URL_OVERRIDE:-$(runtime_inspect base_url)}"
API_KEY="${OPENFANG_API_KEY:-$(runtime_inspect effective_api_key)}"
SQLITE_PATH="$(runtime_inspect sqlite_path)"
preflight_failures=0
protected_failures=0

if truthy "${STRICT_PRODUCTION}" && [[ -z "${API_KEY}" ]]; then
  echo "error OPENFANG_STRICT_PRODUCTION requires a machine API key so protected readiness, metrics, and audit checks cannot silently degrade" >&2
  exit 1
fi

mark_failure() {
  preflight_failures=1
}

check_writable_path() {
  local path="$1"
  if [[ ! -e "${path}" ]]; then
    return 0
  fi
  if [[ -w "${path}" ]]; then
    echo "ok  writable ${path}"
  else
    echo "error ${path} is not writable by the current user" >&2
    mark_failure
  fi
}

check_json_state_file() {
  local path="$1"
  [[ -f "${path}" ]] || return 0
  if python3 - "${path}" <<'PY'
import json
import sys
from pathlib import Path

json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
PY
  then
    echo "ok  ${path}"
  else
    echo "error ${path} is not valid JSON" >&2
    mark_failure
  fi
}

check_toml_state_file() {
  local path="$1"
  [[ -f "${path}" ]] || return 0
  if python3 - "${path}" <<'PY'
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib  # type: ignore[import-not-found]

tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
PY
  then
    echo "ok  ${path}"
  else
    echo "error ${path} is not valid TOML" >&2
    mark_failure
  fi
}

check_sqlite_quick_check() {
  local path="$1"
  [[ -f "${path}" ]] || return 0
  if python3 - "${path}" <<'PY'
import sqlite3
import sys

conn = sqlite3.connect(f"file:{sys.argv[1]}?mode=ro", uri=True)
try:
    row = conn.execute("PRAGMA quick_check").fetchone()
finally:
    conn.close()

if not row or row[0] != "ok":
    raise SystemExit(1)
PY
  then
    echo "ok  sqlite quick_check ${path}"
  else
    echo "error SQLite quick_check failed for ${path}" >&2
    mark_failure
  fi
}

echo "== Runtime Files =="
for path in \
  "${OPENFANG_HOME}" \
  "${OPENFANG_HOME}/data" \
  "${OPENFANG_HOME}/agents" \
  "${OPENFANG_HOME}/workspaces" \
  "${OPENFANG_HOME}/workflows"; do
  if [[ -e "${path}" ]]; then
    echo "ok  ${path}"
  else
    echo "warn ${path} missing"
  fi
done

if [[ -f "${OPENFANG_HOME}/.env" ]]; then
  echo "ok  ${OPENFANG_HOME}/.env"
else
  echo "warn ${OPENFANG_HOME}/.env missing"
fi

if [[ -n "${EXTERNAL_ENV_FILE}" ]]; then
  if [[ -f "${EXTERNAL_ENV_FILE}" ]]; then
    echo "ok  ${EXTERNAL_ENV_FILE} (external env file)"
  else
    echo "warn ${EXTERNAL_ENV_FILE} missing (external env file)"
  fi
fi

if [[ -f "${OPENFANG_HOME}/vault.enc" ]]; then
  echo "ok  ${OPENFANG_HOME}/vault.enc"
fi

runtime_inspect validate

echo
echo "== Filesystem Readiness =="
for path in \
  "${OPENFANG_HOME}" \
  "${OPENFANG_HOME}/data" \
  "${OPENFANG_HOME}/agents" \
  "${OPENFANG_HOME}/workspaces" \
  "${OPENFANG_HOME}/workflows"; do
  check_writable_path "${path}"
done

echo
echo "== State Integrity =="
check_json_state_file "${OPENFANG_HOME}/hand_state.json"
check_json_state_file "${OPENFANG_HOME}/cron_jobs.json"
check_json_state_file "${OPENFANG_HOME}/custom_models.json"
check_toml_state_file "${OPENFANG_HOME}/integrations.toml"
check_sqlite_quick_check "${SQLITE_PATH}"

echo
echo "== Backup Tooling =="
for script in \
  backup-openfang.sh \
  live-api-smoke-openfang.sh \
  openfang-env-common.sh \
  preflight-openfang.sh \
  provider-canary-openfang.sh \
  restore-openfang.sh \
  smoke-openfang.sh; do
  if [[ -x "$(dirname "$0")/${script}" ]]; then
    echo "ok  scripts/${script}"
  else
    echo "warn scripts/${script} is not executable"
  fi
done

if truthy "${OFFLINE_MODE}"; then
  echo
  echo "warn offline mode is enabled; skipped live API checks at ${BASE_URL}"
elif curl -fsS "${BASE_URL}/api/health" >/dev/null 2>&1; then
  echo
  echo "== Live API =="
  python3 - "$(curl -fsS "${BASE_URL}/api/health")" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
status = payload.get("status")
if status != "ok":
    raise SystemExit(f"/api/health returned status={status!r}, expected 'ok'")
PY
  echo "ok  health"

  auth_args=()
  if [[ -n "${API_KEY}" ]]; then
    auth_args=(-H "Authorization: Bearer ${API_KEY}")
  fi

  curl_with_auth() {
    if (( ${#auth_args[@]} > 0 )); then
      curl -fsS "${auth_args[@]}" "$1"
    else
      curl -fsS "$1"
    fi
  }

  require_ok_status() {
    local url="$1"
    local description="$2"
    local body

    body="$(curl_with_auth "${url}")"
    python3 - "${body}" "${description}" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
description = sys.argv[2]
status = payload.get("status")
if status != "ok":
    raise SystemExit(f"{description} returned status={status!r}, expected 'ok'")
PY
  }

  require_audit_valid() {
    local url="$1"
    local body

    body="$(curl_with_auth "${url}")"
    python3 - "${body}" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if payload.get("valid") is not True:
    raise SystemExit(f"/api/audit/verify reported invalid audit chain: {payload!r}")

entries = int(payload.get("entries", 0) or 0)
warning = payload.get("warning")
if entries == 0 or warning:
    print(
        warning or "Audit log is empty — preflight passed but this node has no forensic history yet.",
        file=sys.stderr,
    )
PY
  }

  for path in /api/status /api/metrics; do
    if curl_with_auth "${BASE_URL}${path}" >/dev/null 2>&1; then
      echo "ok  ${path}"
    else
      if [[ -n "${API_KEY}" ]]; then
        echo "error ${path} unavailable even though an API key is configured for preflight auth" >&2
        protected_failures=1
      else
        echo "warn ${path} unavailable with current auth context"
      fi
    fi
  done

  if require_audit_valid "${BASE_URL}/api/audit/verify" >/dev/null 2>&1; then
    echo "ok  /api/audit/verify"
  else
    if [[ -n "${API_KEY}" ]]; then
      echo "error /api/audit/verify reported an invalid audit chain, or was unavailable under the current auth context" >&2
      protected_failures=1
    else
      echo "warn /api/audit/verify unavailable with current auth context; set OPENFANG_API_KEY for full operational verification" >&2
    fi
  fi

  if require_ok_status "${BASE_URL}/api/health/detail" "/api/health/detail" >/dev/null 2>&1; then
    echo "ok  /api/health/detail"
  else
    if [[ -n "${API_KEY}" ]]; then
      echo "error /api/health/detail is reachable but not ready, or unavailable under the current auth context" >&2
      protected_failures=1
    else
      echo "warn /api/health/detail unavailable or not ready with current auth context"
    fi
  fi

  if [[ -z "${API_KEY}" ]]; then
    echo "warn protected readiness, metrics, and audit checks remain partial without OPENFANG_API_KEY; keep a machine API key for production probes and Prometheus" >&2
  fi

else
  echo
  echo "error live API not reachable at ${BASE_URL}; preflight requires a reachable daemon unless --offline or OPENFANG_PREFLIGHT_OFFLINE=1 is set" >&2
  exit 1
fi

if (( preflight_failures > 0 || protected_failures > 0 )); then
  exit 1
fi
