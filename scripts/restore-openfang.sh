#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: restore-openfang.sh <backup-dir> [--yes]

Restore an OpenFang runtime backup created by backup-openfang.sh.

Environment:
  OPENFANG_HOME               Restore target (default: $HOME/.openfang)
  OPENFANG_SKIP_SAFETY_BACKUP Set to 1 to skip creating a pre-restore backup
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
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -f "${OPENFANG_HOME}/daemon.json" ]] && [[ "${CONFIRM}" != "true" ]]; then
  echo "daemon.json exists under ${OPENFANG_HOME}. Stop the daemon first, then re-run with --yes." >&2
  exit 1
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

mkdir -p "${OPENFANG_HOME}"

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

restore_path "${BACKUP_DIR}/config.toml" "${OPENFANG_HOME}/config.toml"
restore_path "${BACKUP_DIR}/.env" "${OPENFANG_HOME}/.env"
restore_path "${BACKUP_DIR}/vault.enc" "${OPENFANG_HOME}/vault.enc"
restore_path "${BACKUP_DIR}/hand_state.json" "${OPENFANG_HOME}/hand_state.json"
restore_path "${BACKUP_DIR}/cron_jobs.json" "${OPENFANG_HOME}/cron_jobs.json"

if [[ -d "${BACKUP_DIR}/data" ]]; then
  rm -rf "${OPENFANG_HOME}/data"
  cp -a "${BACKUP_DIR}/data" "${OPENFANG_HOME}/data"
fi

for dir_name in agents skills workspaces workflows; do
  restore_path "${BACKUP_DIR}/${dir_name}" "${OPENFANG_HOME}/${dir_name}"
done

rm -f "${OPENFANG_HOME}/daemon.json"

echo "Restore completed."
echo "Next steps:"
echo "  1. Start the daemon"
echo "  2. Run scripts/smoke-openfang.sh"
