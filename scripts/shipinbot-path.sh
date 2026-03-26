#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
ACTIVE_DIR="$ROOT_DIR/projects/shipinbot"

find_archive_dir() {
  local candidate=""
  shopt -s nullglob
  local matches=("$HOME"/Desktop/_local_dev_archive_*/shipinbot)
  shopt -u nullglob
  if [[ "${#matches[@]}" -eq 0 ]]; then
    return 1
  fi
  for candidate in "${matches[@]}"; do
    :
  done
  printf '%s\n' "$candidate"
}

print_status() {
  local archive_dir="" active_commit="" archive_commit=""
  if [[ -d "$ACTIVE_DIR" ]]; then
    active_commit="$(git -C "$ACTIVE_DIR" rev-parse --short HEAD 2>/dev/null || true)"
  fi
  if archive_dir="$(find_archive_dir 2>/dev/null)"; then
    archive_commit="$(git -C "$archive_dir" rev-parse --short HEAD 2>/dev/null || true)"
  else
    archive_dir=""
  fi

  printf 'Active shipinbot path: %s\n' "$ACTIVE_DIR"
  if [[ -n "$active_commit" ]]; then
    printf 'Active shipinbot commit: %s\n' "$active_commit"
  fi
  if [[ -n "$archive_dir" ]]; then
    printf 'Archived standalone copy: %s\n' "$archive_dir"
    if [[ -n "$archive_commit" ]]; then
      printf 'Archived copy commit: %s\n' "$archive_commit"
    fi
    printf 'Archive note: historical copy only; do not use it as the default runtime entry.\n'
  else
    printf 'Archived standalone copy: not found\n'
  fi
}

case "${1:-}" in
  --path)
    printf '%s\n' "$ACTIVE_DIR"
    ;;
  --archive-path)
    find_archive_dir 2>/dev/null || true
    ;;
  --status|"")
    print_status
    ;;
  *)
    echo "Usage: $0 [--path|--archive-path|--status]" >&2
    exit 1
    ;;
esac
