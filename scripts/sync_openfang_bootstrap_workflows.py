#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import sys
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[1]
SOURCE_DIR = ROOT_DIR / "openfang-workflows"
MANAGED_PREFIX = "bootstrap-shipinfabu-"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Sync repo-managed OpenFang bootstrap workflows into the runtime workflows directory."
    )
    parser.add_argument(
        "--source-dir",
        default=str(SOURCE_DIR),
        help="Directory containing repo-managed OpenFang workflow JSON files.",
    )
    parser.add_argument(
        "--target-dir",
        default="",
        help="Destination workflows directory. Defaults to $OPENFANG_HOME/workflows.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print planned actions without writing files.",
    )
    return parser.parse_args()


def default_openfang_home() -> Path:
    raw = os.environ.get("OPENFANG_HOME", "~/.openfang")
    return Path(raw).expanduser().resolve()


def resolve_target_dir(raw: str) -> Path:
    if raw.strip():
        return Path(raw).expanduser().resolve()
    return default_openfang_home() / "workflows"


def validate_workflow(path: Path) -> dict[str, object]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise SystemExit(f"无效 workflow 文件 {path}: {exc}") from exc
    if not isinstance(payload, dict):
        raise SystemExit(f"无效 workflow 文件 {path}: 顶层必须是 JSON object")

    required = ("id", "name", "description", "steps", "created_at")
    missing = [key for key in required if key not in payload]
    if missing:
        raise SystemExit(f"无效 workflow 文件 {path}: 缺少字段 {', '.join(missing)}")

    steps = payload.get("steps")
    if not isinstance(steps, list) or not steps:
        raise SystemExit(f"无效 workflow 文件 {path}: steps 必须是非空数组")

    return payload


def copy_if_changed(source: Path, target: Path, *, dry_run: bool) -> bool:
    try:
        source_bytes = source.read_bytes()
    except OSError as exc:
        raise SystemExit(f"读取源 workflow 失败 {source}: {exc}") from exc

    if target.exists():
        try:
            if target.read_bytes() == source_bytes:
                return False
        except OSError as exc:
            raise SystemExit(f"读取目标 workflow 失败 {target}: {exc}") from exc

    if dry_run:
        return True

    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)
    return True


def remove_stale_managed_files(target_dir: Path, desired_names: set[str], *, dry_run: bool) -> list[Path]:
    removed: list[Path] = []
    if not target_dir.exists():
        return removed
    for path in sorted(target_dir.glob(f"{MANAGED_PREFIX}*.json")):
        if path.name in desired_names:
            continue
        removed.append(path)
        if not dry_run:
            path.unlink(missing_ok=True)
    return removed


def main() -> int:
    args = parse_args()
    source_dir = Path(args.source_dir).expanduser().resolve()
    target_dir = resolve_target_dir(args.target_dir)

    if not source_dir.exists():
        raise SystemExit(f"workflow 源目录不存在: {source_dir}")

    source_files = sorted(source_dir.glob("*.json"))
    if not source_files:
        print(f"没有找到 repo-managed workflow: {source_dir}")
        return 0

    desired_names: set[str] = set()
    copied = 0
    unchanged = 0

    for source in source_files:
        validate_workflow(source)
        desired_names.add(source.name)
        changed = copy_if_changed(source, target_dir / source.name, dry_run=args.dry_run)
        if changed:
            copied += 1
            print(f"{'would sync' if args.dry_run else 'synced'} {source.name}")
        else:
            unchanged += 1

    removed = remove_stale_managed_files(target_dir, desired_names, dry_run=args.dry_run)
    for path in removed:
        print(f"{'would remove' if args.dry_run else 'removed'} stale {path.name}")

    print(
        f"workflow bootstrap sync complete: source={source_dir} target={target_dir} "
        f"updated={copied} unchanged={unchanged} removed={len(removed)}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
