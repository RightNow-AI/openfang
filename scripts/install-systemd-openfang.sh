#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: install-systemd-openfang.sh [--binary PATH] [--destdir PATH] [--enable] [--skip-backup-timer]

Install the OpenFang systemd deployment baseline for Linux hosts:
  - /usr/local/bin/openfang
  - /usr/local/lib/openfang/{backup,preflight,restore,smoke,...}
  - /etc/systemd/system/openfang.service
  - /etc/systemd/system/openfang-backup.{service,timer}
  - /etc/openfang/env (if missing)
  - /var/lib/openfang/config.toml (if missing)

Options:
  --binary PATH          Use the given OpenFang binary instead of auto-detecting one
  --destdir PATH         Stage files under PATH for packaging/tests instead of writing to /
  --enable               Run strict offline preflight, then systemctl daemon-reload and enable the service
  --skip-backup-timer    Do not enable openfang-backup.timer when --enable is used
  -h, --help             Show this help

Notes:
  - Without --destdir this script must run as root.
  - The systemd unit keeps the canonical runtime paths:
      /usr/local/bin/openfang
      /usr/local/lib/openfang
      /etc/openfang/env
      /var/lib/openfang
  - --enable fails closed if strict offline preflight does not pass.
EOF
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

BINARY_SOURCE=""
DESTDIR="${OPENFANG_DESTDIR:-}"
ENABLE_SERVICE=0
ENABLE_BACKUP_TIMER=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary)
      shift
      [[ $# -gt 0 ]] || {
        echo "--binary requires a path" >&2
        exit 1
      }
      BINARY_SOURCE="$1"
      ;;
    --destdir)
      shift
      [[ $# -gt 0 ]] || {
        echo "--destdir requires a path" >&2
        exit 1
      }
      DESTDIR="$1"
      ;;
    --enable)
      ENABLE_SERVICE=1
      ;;
    --skip-backup-timer)
      ENABLE_BACKUP_TIMER=0
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if [[ -n "${DESTDIR}" && "${DESTDIR}" != "/" ]]; then
  DESTDIR="${DESTDIR%/}"
fi
if [[ -n "${DESTDIR}" && "${DESTDIR}" != /* ]]; then
  echo "--destdir must be an absolute path" >&2
  exit 1
fi
if (( ENABLE_SERVICE )) && [[ -n "${DESTDIR}" ]]; then
  echo "--enable cannot be used together with --destdir" >&2
  exit 1
fi
if [[ -z "${DESTDIR}" && "$(id -u)" != "0" ]]; then
  echo "install-systemd-openfang.sh must run as root unless --destdir is used" >&2
  exit 1
fi

BIN_TARGET="/usr/local/bin/openfang"
HELPER_DIR="/usr/local/lib/openfang"
SYSTEMD_DIR="/etc/systemd/system"
ENV_FILE="/etc/openfang/env"
CONFIG_TARGET="/var/lib/openfang/config.toml"
HOME_DIR="/var/lib/openfang"
DATA_DIR="/var/lib/openfang/data"

HELPER_SCRIPTS=(
  backup-openfang.sh
  openfang-env-common.sh
  preflight-openfang.sh
  restore-openfang.sh
  smoke-openfang.sh
  live-api-smoke-openfang.sh
  provider-canary-openfang.sh
)

stage_path() {
  local path="$1"
  if [[ -n "${DESTDIR}" ]]; then
    printf '%s%s\n' "${DESTDIR}" "${path}"
  else
    printf '%s\n' "${path}"
  fi
}

group_exists() {
  if command -v getent >/dev/null 2>&1; then
    getent group openfang >/dev/null 2>&1
    return
  fi
  grep -q '^openfang:' /etc/group 2>/dev/null
}

ensure_service_account() {
  [[ -n "${DESTDIR}" ]] && return 0

  if ! group_exists; then
    groupadd --system openfang
  fi

  if ! id -u openfang >/dev/null 2>&1; then
    useradd --system --home "${HOME_DIR}" --shell /usr/sbin/nologin -g openfang openfang
  fi
}

detect_binary_source() {
  local candidate

  if [[ -n "${BINARY_SOURCE}" ]]; then
    if [[ ! -x "${BINARY_SOURCE}" ]]; then
      echo "OpenFang binary is not executable: ${BINARY_SOURCE}" >&2
      exit 1
    fi
    printf '%s\n' "${BINARY_SOURCE}"
    return 0
  fi

  for candidate in \
    "${REPO_ROOT}/target/release/openfang" \
    "${REPO_ROOT}/target/debug/openfang"; do
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  if candidate="$(command -v openfang 2>/dev/null)" && [[ -n "${candidate}" && -x "${candidate}" ]]; then
    printf '%s\n' "${candidate}"
    return 0
  fi

  echo "Could not find an executable OpenFang binary. Build one first or pass --binary PATH." >&2
  exit 1
}

ensure_dir() {
  local mode="$1"
  local path="$2"
  install -d -m "${mode}" "$(stage_path "${path}")"
}

ensure_owned_dir() {
  local mode="$1"
  local owner="$2"
  local group="$3"
  local path="$4"
  if [[ -n "${DESTDIR}" ]]; then
    install -d -m "${mode}" "$(stage_path "${path}")"
  else
    install -d -m "${mode}" -o "${owner}" -g "${group}" "$(stage_path "${path}")"
  fi
}

install_regular_file() {
  local mode="$1"
  local source="$2"
  local dest="$3"
  install -m "${mode}" "${source}" "$(stage_path "${dest}")"
}

install_if_missing() {
  local mode="$1"
  local source="$2"
  local dest="$3"
  local owner="${4:-}"
  local group="${5:-}"
  local staged_dest

  staged_dest="$(stage_path "${dest}")"
  if [[ -e "${staged_dest}" ]]; then
    echo "keep ${dest}"
    return 0
  fi

  ensure_dir 0755 "$(dirname "${dest}")"
  if [[ -n "${DESTDIR}" || -z "${owner}" || -z "${group}" ]]; then
    install -m "${mode}" "${source}" "${staged_dest}"
  else
    install -m "${mode}" -o "${owner}" -g "${group}" "${source}" "${staged_dest}"
  fi
}

same_file() {
  local left="$1"
  local right="$2"
  [[ -e "${left}" && -e "${right}" ]] || return 1
  [[ "$(cd "$(dirname "${left}")" && pwd -P)/$(basename "${left}")" == "$(cd "$(dirname "${right}")" && pwd -P)/$(basename "${right}")" ]]
}

run_installed_preflight() {
  OPENFANG_HOME="${HOME_DIR}" \
    OPENFANG_ENV_FILE="${ENV_FILE}" \
    OPENFANG_STRICT_PRODUCTION=1 \
    "${HELPER_DIR}/preflight-openfang.sh" --offline
}

ensure_service_account

OPENFANG_BINARY="$(detect_binary_source)"

echo "Installing OpenFang systemd assets"
echo "  binary source: ${OPENFANG_BINARY}"
echo "  binary target: ${BIN_TARGET}"
echo "  helper dir:    ${HELPER_DIR}"
echo "  systemd dir:   ${SYSTEMD_DIR}"
echo "  env file:      ${ENV_FILE}"
echo "  runtime home:  ${HOME_DIR}"
if [[ -n "${DESTDIR}" ]]; then
  echo "  staging root:  ${DESTDIR}"
fi

ensure_dir 0755 /usr/local/bin
ensure_dir 0755 "${HELPER_DIR}"
ensure_dir 0755 /etc
ensure_dir 0755 /etc/openfang
ensure_dir 0755 "${SYSTEMD_DIR}"
ensure_owned_dir 0700 openfang openfang "${HOME_DIR}"
ensure_owned_dir 0700 openfang openfang "${DATA_DIR}"

if [[ -z "${DESTDIR}" ]] && same_file "${OPENFANG_BINARY}" "${BIN_TARGET}"; then
  echo "keep ${BIN_TARGET}"
else
  install_regular_file 0755 "${OPENFANG_BINARY}" "${BIN_TARGET}"
fi

for script_name in "${HELPER_SCRIPTS[@]}"; do
  install_regular_file 0755 "${REPO_ROOT}/scripts/${script_name}" "${HELPER_DIR}/${script_name}"
done
install_regular_file 0755 "${REPO_ROOT}/scripts/healthcheck-openfang.py" "${HELPER_DIR}/healthcheck-openfang.py"

install_regular_file 0644 "${REPO_ROOT}/deploy/openfang.service" "${SYSTEMD_DIR}/openfang.service"
install_regular_file 0644 "${REPO_ROOT}/deploy/openfang-backup.service" "${SYSTEMD_DIR}/openfang-backup.service"
install_regular_file 0644 "${REPO_ROOT}/deploy/openfang-backup.timer" "${SYSTEMD_DIR}/openfang-backup.timer"

install_if_missing 0640 "${REPO_ROOT}/deploy/openfang.env.example" "${ENV_FILE}" root openfang
install_if_missing 0600 "${REPO_ROOT}/openfang.toml.example" "${CONFIG_TARGET}" openfang openfang

if (( ENABLE_SERVICE )); then
  if ! command -v systemctl >/dev/null 2>&1; then
    echo "systemctl is required for --enable" >&2
    exit 1
  fi

  echo "Running strict offline preflight before enabling the service"
  if ! run_installed_preflight; then
    echo "Refusing to enable openfang.service because strict offline preflight failed." >&2
    echo "Edit ${ENV_FILE} and ${CONFIG_TARGET}, then re-run with --enable." >&2
    exit 1
  fi

  systemctl daemon-reload
  if (( ENABLE_BACKUP_TIMER )); then
    systemctl enable --now openfang-backup.timer
  fi
  systemctl enable --now openfang.service
  echo "Enabled openfang.service"
  if (( ENABLE_BACKUP_TIMER )); then
    echo "Enabled openfang-backup.timer"
  fi
else
  echo
  echo "Installed systemd assets without starting the service."
  echo "Next steps:"
  echo "  1. Edit ${ENV_FILE}"
  echo "  2. Review ${CONFIG_TARGET}"
  echo "  3. Run ${HELPER_DIR}/preflight-openfang.sh --offline"
  echo "  4. Re-run this script with --enable, or manually run:"
  echo "     sudo systemctl daemon-reload"
  if (( ENABLE_BACKUP_TIMER )); then
    echo "     sudo systemctl enable --now openfang-backup.timer"
  fi
  echo "     sudo systemctl enable --now openfang.service"
fi
