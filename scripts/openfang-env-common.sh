#!/usr/bin/env bash

openfang_normalize_path() {
  local path="${1:-}"
  if [[ -z "${path}" ]]; then
    return 0
  fi
  if [[ "${path}" != "/" ]]; then
    path="${path%/}"
  fi
  printf '%s\n' "${path}"
}

openfang_env_file_var() {
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

openfang_ensure_readable_file() {
  local path="$1"
  local description="$2"
  [[ -z "${path}" || ! -e "${path}" ]] && return 0
  if [[ -r "${path}" ]]; then
    return 0
  fi
  echo "${description} is not readable by the current user." >&2
  echo "For systemd deployments, keep /etc/openfang/env readable by the openfang service user (for example mode 0640 with group openfang), or run the script as a user that can read ${path}." >&2
  return 1
}

openfang_auto_detect_external_env_file() {
  local openfang_home="$1"
  local candidate="/etc/openfang/env"
  [[ -f "${candidate}" ]] || return 1

  local normalized_home candidate_home
  normalized_home="$(openfang_normalize_path "${openfang_home}")"
  candidate_home="$(openfang_env_file_var "${candidate}" "OPENFANG_HOME")"
  candidate_home="$(openfang_normalize_path "${candidate_home}")"

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

openfang_resolve_external_env_file() {
  local openfang_home="${1:-${OPENFANG_HOME:-$HOME/.openfang}}"
  local external_env_file="${OPENFANG_ENV_FILE:-}"

  if [[ -z "${external_env_file}" ]]; then
    external_env_file="$(openfang_auto_detect_external_env_file "${openfang_home}" || true)"
  fi

  if [[ -n "${OPENFANG_ENV_FILE:-}" && ! -f "${OPENFANG_ENV_FILE}" ]]; then
    echo "OPENFANG_ENV_FILE was set but file does not exist: ${OPENFANG_ENV_FILE}" >&2
    return 1
  fi

  if [[ -n "${external_env_file}" ]]; then
    openfang_ensure_readable_file "${external_env_file}" "External env file ${external_env_file}" || return 1
  fi

  printf '%s\n' "${external_env_file}"
}

openfang_resolve_runtime_value() {
  local key="$1"
  local external_env_file="${2:-}"
  local default_value="${3:-}"
  local value="${!key:-}"

  if [[ -z "${value}" && -n "${external_env_file}" ]]; then
    value="$(openfang_env_file_var "${external_env_file}" "${key}")"
  fi
  if [[ -z "${value}" ]]; then
    value="${default_value}"
  fi

  printf '%s\n' "${value}"
}

openfang_base_url_from_listen() {
  local listen_addr="${1:-127.0.0.1:4200}"

  python3 - "${listen_addr}" <<'PY'
import ipaddress
import sys

listen = str(sys.argv[1]).strip() or "127.0.0.1:4200"
if listen.startswith("[") and "]:" in listen:
    host, port = listen[1:].split("]:", 1)
elif listen.count(":") == 1:
    host, port = listen.rsplit(":", 1)
elif ":" not in listen:
    host, port = listen, "4200"
else:
    host, port = listen, "4200"

host = host.strip().strip("[]")
if host in {"", "0.0.0.0", "::", "localhost"}:
    host = "127.0.0.1"
else:
    try:
        ip = ipaddress.ip_address(host)
        if ip.version == 6:
            host = f"[{host}]"
    except ValueError:
        pass

print(f"http://{host}:{port}")
PY
}

openfang_resolve_base_url() {
  local cli_arg="${1:-}"
  local external_env_file="${2:-}"
  local fallback="${3:-http://127.0.0.1:4200}"
  local base_url=""
  local listen_addr=""

  if [[ -n "${cli_arg}" ]]; then
    printf '%s\n' "${cli_arg%/}"
    return 0
  fi

  base_url="$(openfang_resolve_runtime_value "OPENFANG_BASE_URL" "${external_env_file}")"
  if [[ -n "${base_url}" ]]; then
    printf '%s\n' "${base_url%/}"
    return 0
  fi

  listen_addr="$(openfang_resolve_runtime_value "OPENFANG_LISTEN" "${external_env_file}")"
  if [[ -n "${listen_addr}" ]]; then
    openfang_base_url_from_listen "${listen_addr}"
    return 0
  fi

  printf '%s\n' "${fallback}"
}
