#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
SUBMODULE_DIR="$ROOT_DIR/projects/shipinbot"

usage() {
  cat <<'EOF'
Usage:
  scripts/shipinbot-commit.sh --status
  scripts/shipinbot-commit.sh [--no-push] "<submodule-commit-message>" ["<parent-commit-message>"]

Examples:
  scripts/shipinbot-commit.sh --status
  scripts/shipinbot-commit.sh "fix: publish retry guard"
  scripts/shipinbot-commit.sh "fix: publish retry guard" "chore: bump shipinbot"

Behavior:
  1. Commit current changes inside projects/shipinbot
  2. Rebase the submodule branch onto origin/<branch> and push it
  3. Commit the updated submodule pointer in the parent repo
  4. Push the parent repo branch

Safety:
  - The parent repo must be clean except for projects/shipinbot
  - Default parent commit message: "chore: bump shipinbot"
EOF
}

ensure_repo_layout() {
  if [[ ! -d "$SUBMODULE_DIR" ]]; then
    echo "失败：未找到 shipinbot 子模块目录：$SUBMODULE_DIR" >&2
    exit 1
  fi
}

current_branch() {
  git -C "$1" branch --show-current
}

show_status() {
  local sub_branch sub_head root_branch root_pointer sub_status root_status
  sub_branch="$(current_branch "$SUBMODULE_DIR")"
  sub_head="$(git -C "$SUBMODULE_DIR" rev-parse --short HEAD)"
  root_branch="$(current_branch "$ROOT_DIR")"
  root_pointer="$(git -C "$ROOT_DIR" ls-tree HEAD projects/shipinbot | awk '{print $3}')"
  sub_status="$(git -C "$SUBMODULE_DIR" status --short || true)"
  root_status="$(git -C "$ROOT_DIR" status --short || true)"

  echo "Parent repo:      $ROOT_DIR"
  echo "Parent branch:    ${root_branch:-detached}"
  echo "shipinbot path:   $SUBMODULE_DIR"
  echo "shipinbot branch: ${sub_branch:-detached}"
  echo "shipinbot HEAD:   $sub_head"
  echo "Parent pointer:   ${root_pointer:-unknown}"
  echo
  echo "shipinbot status:"
  if [[ -n "$sub_status" ]]; then
    printf '%s\n' "$sub_status"
  else
    echo "(clean)"
  fi
  echo
  echo "parent repo status:"
  if [[ -n "$root_status" ]]; then
    printf '%s\n' "$root_status"
  else
    echo "(clean)"
  fi
}

ensure_parent_clean_except_submodule() {
  local other_changes
  other_changes="$(git -C "$ROOT_DIR" status --short | grep -vE '^[ MADRCU?!]{2} projects/shipinbot$' || true)"
  if [[ -n "$other_changes" ]]; then
    echo "失败：主仓库除了 projects/shipinbot 之外还有未提交改动。" >&2
    echo "请先单独处理这些改动，再执行 shipinbot 提交流程：" >&2
    printf '%s\n' "$other_changes" >&2
    exit 1
  fi
}

submodule_has_changes() {
  [[ -n "$(git -C "$SUBMODULE_DIR" status --short || true)" ]]
}

parent_pointer_matches_submodule() {
  local pointer head
  pointer="$(git -C "$ROOT_DIR" ls-tree HEAD projects/shipinbot | awk '{print $3}')"
  head="$(git -C "$SUBMODULE_DIR" rev-parse HEAD)"
  [[ -n "$pointer" && "$pointer" == "$head" ]]
}

NO_PUSH=0
if [[ "${1:-}" == "--status" ]]; then
  ensure_repo_layout
  show_status
  exit 0
fi
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi
if [[ "${1:-}" == "--no-push" ]]; then
  NO_PUSH=1
  shift
fi

SUBMODULE_MESSAGE="${1:-}"
PARENT_MESSAGE="${2:-chore: bump shipinbot}"

if [[ -z "$SUBMODULE_MESSAGE" ]]; then
  usage >&2
  exit 1
fi

ensure_repo_layout
ensure_parent_clean_except_submodule

SUBMODULE_BRANCH="$(current_branch "$SUBMODULE_DIR")"
PARENT_BRANCH="$(current_branch "$ROOT_DIR")"

if [[ -z "$SUBMODULE_BRANCH" ]]; then
  echo "失败：shipinbot 当前不在分支上（detached HEAD）。" >&2
  exit 1
fi
if [[ -z "$PARENT_BRANCH" ]]; then
  echo "失败：主仓库当前不在分支上（detached HEAD）。" >&2
  exit 1
fi

if submodule_has_changes; then
  git -C "$SUBMODULE_DIR" add -A
  git -C "$SUBMODULE_DIR" commit -m "$SUBMODULE_MESSAGE"
else
  echo "shipinbot 工作区没有未提交改动，跳过子模块代码提交。"
fi

if [[ "$NO_PUSH" -eq 0 ]]; then
  git -C "$SUBMODULE_DIR" fetch origin
  git -C "$SUBMODULE_DIR" rebase "origin/$SUBMODULE_BRANCH"
  git -C "$SUBMODULE_DIR" push origin "$SUBMODULE_BRANCH"
else
  echo "NO_PUSH=1：跳过 shipinbot push。"
fi

if parent_pointer_matches_submodule; then
  echo "主仓库子模块指针已是最新，跳过父仓库提交。"
  exit 0
fi

git -C "$ROOT_DIR" add projects/shipinbot
git -C "$ROOT_DIR" commit -m "$PARENT_MESSAGE"

if [[ "$NO_PUSH" -eq 0 ]]; then
  git -C "$ROOT_DIR" push origin "$PARENT_BRANCH"
else
  echo "NO_PUSH=1：跳过父仓库 push。"
fi
