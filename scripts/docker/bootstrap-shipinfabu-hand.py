#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any


BASE_URL = str(os.environ.get("OPENFANG_BOOTSTRAP_BASE_URL", "http://127.0.0.1:4200")).rstrip("/")
API_KEY = str(os.environ.get("OPENFANG_API_KEY", "")).strip()
HAND_DIR = Path("/app/openfang-hand/shipinfabu")
HAND_ID = "shipinfabu"


def _headers() -> dict[str, str]:
    headers = {"Content-Type": "application/json"}
    if API_KEY:
        headers["Authorization"] = f"Bearer {API_KEY}"
    return headers


def _request(method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
    data = None
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
    req = urllib.request.Request(f"{BASE_URL}{path}", data=data, method=method, headers=_headers())
    with urllib.request.urlopen(req, timeout=10) as resp:
        body = resp.read().decode("utf-8")
    if not body:
        return {}
    return json.loads(body)


def _wait_for_health(timeout_seconds: float = 60.0) -> None:
    deadline = time.time() + timeout_seconds
    last_error = "daemon did not become healthy"
    while time.time() < deadline:
        try:
            body = _request("GET", "/api/health")
            if str(body.get("status") or "").strip().lower() == "ok":
                return
            last_error = f"unexpected health payload: {body}"
        except Exception as exc:  # noqa: BLE001
            last_error = str(exc)
        time.sleep(1)
    raise RuntimeError(last_error)


def _load_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def _active_instances() -> list[dict[str, Any]]:
    body = _request("GET", "/api/hands/active")
    if isinstance(body, dict) and isinstance(body.get("instances"), list):
        return [item for item in body["instances"] if isinstance(item, dict)]
    if isinstance(body, list):
        return [item for item in body if isinstance(item, dict)]
    return []


def _deactivate_existing_instances() -> None:
    for item in _active_instances():
        if str(item.get("hand_id") or "").strip() != HAND_ID:
            continue
        instance_id = str(item.get("instance_id") or "").strip()
        if not instance_id:
            continue
        _request("DELETE", f"/api/hands/instances/{instance_id}")


def _non_empty_env(name: str) -> str | None:
    value = str(os.environ.get(name, "")).strip()
    return value or None


def _build_hand_config() -> dict[str, str]:
    config: dict[str, str] = {
        "media_api_base_url": os.environ.get("SHIPINFABU_MEDIA_API_BASE_URL", "http://media-pipeline-service:8000"),
        "local_source_staging_dir": os.environ.get("SHIPINFABU_LOCAL_SOURCE_STAGING_DIR", "/app/data/ingest"),
        "local_media_intake_dir": os.environ.get("SHIPINFABU_LOCAL_MEDIA_INTAKE_DIR", "/app/data/ingest"),
        "local_media_intake_retention_hours": os.environ.get("SHIPINFABU_LOCAL_MEDIA_INTAKE_RETENTION_HOURS", "12"),
        "bridge_script_path": os.environ.get("SHIPINFABU_BRIDGE_SCRIPT_PATH", "/app/scripts/openfang_clean_publish_bridge.py"),
        "notify_channel": os.environ.get("SHIPINFABU_NOTIFY_CHANNEL", "telegram"),
        "notify_stage_updates": os.environ.get("SHIPINFABU_NOTIFY_STAGE_UPDATES", "true"),
        "poll_interval_seconds": os.environ.get("SHIPINFABU_POLL_INTERVAL_SECONDS", "10"),
        "poll_timeout_seconds": os.environ.get("SHIPINFABU_POLL_TIMEOUT_SECONDS", "1800"),
        "execution_mode": os.environ.get("SHIPINFABU_EXECUTION_MODE", "auto"),
    }
    optional_mappings = {
        "notify_recipient": "SHIPINFABU_NOTIFY_RECIPIENT",
        "media_api_token": "MEDIA_API_TOKEN",
        "publishhub_base_url": "PUBLISHHUB_BASE_URL",
        "publishhub_project_code": "PUBLISHHUB_PROJECT_CODE",
        "publishhub_username": "PUBLISHHUB_USERNAME",
        "publishhub_password": "PUBLISHHUB_PASSWORD",
        "publishhub_author_id": "PUBLISHHUB_AUTHOR_ID",
        "publishhub_author_name": "PUBLISHHUB_AUTHOR_NAME",
        "publishhub_category_id": "PUBLISHHUB_CATEGORY_ID",
        "cloud_profile": "SHIPINFABU_CLOUD_PROFILE",
        "max_cloud_cost": "SHIPINFABU_MAX_CLOUD_COST",
    }
    for key, env_name in optional_mappings.items():
        value = _non_empty_env(env_name)
        if value is not None:
            config[key] = value
    return config


def main() -> int:
    if not HAND_DIR.exists():
        print(f"shipinfabu hand directory not found: {HAND_DIR}", file=sys.stderr)
        return 1

    _wait_for_health()
    _request(
        "POST",
        "/api/hands/upsert",
        {
            "toml_content": _load_text(HAND_DIR / "HAND.toml"),
            "skill_content": _load_text(HAND_DIR / "SKILL.md"),
            "source_path": str(HAND_DIR),
        },
    )
    _deactivate_existing_instances()
    _request("POST", f"/api/hands/{HAND_ID}/activate", {"config": _build_hand_config()})
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        print(f"bootstrap shipinfabu failed: {exc.code} {detail}", file=sys.stderr)
        raise SystemExit(1)
    except Exception as exc:  # noqa: BLE001
        print(f"bootstrap shipinfabu failed: {exc}", file=sys.stderr)
        raise SystemExit(1)
