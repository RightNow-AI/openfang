#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: backup-openfang.sh [backup-root]

Create a timestamped OpenFang runtime backup under:
  ${1:-$HOME/openfang-backups}/openfang-YYYYmmdd-HHMMSS

Environment:
  OPENFANG_HOME          Runtime home to back up (default: $HOME/.openfang)
  OPENFANG_KEEP_BACKUPS  Number of backups to keep in backup-root (default: 5)
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

OPENFANG_HOME="${OPENFANG_HOME:-$HOME/.openfang}"
BACKUP_ROOT="${1:-$HOME/openfang-backups}"
KEEP_BACKUPS="${OPENFANG_KEEP_BACKUPS:-5}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
DEST="${BACKUP_ROOT}/openfang-${TIMESTAMP}"

if [[ ! -d "${OPENFANG_HOME}" ]]; then
  echo "OpenFang home does not exist: ${OPENFANG_HOME}" >&2
  exit 1
fi

mkdir -p "${DEST}"

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

copy_if_exists "${OPENFANG_HOME}/config.toml" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/.env" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/vault.enc" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/hand_state.json" "${DEST}"
copy_if_exists "${OPENFANG_HOME}/cron_jobs.json" "${DEST}"
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

echo "Backup written to ${DEST}"
