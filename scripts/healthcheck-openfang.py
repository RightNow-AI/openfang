#!/usr/bin/env python3

import json
import os
import urllib.error
import urllib.request


BASE_URL = os.environ.get("OPENFANG_BASE_URL", "http://127.0.0.1:4200").rstrip("/")
API_KEY = os.environ.get("OPENFANG_API_KEY", "").strip()


def fetch_json(path: str, *, with_auth: bool) -> dict:
    req = urllib.request.Request(f"{BASE_URL}{path}")
    if with_auth and API_KEY:
        req.add_header("Authorization", f"Bearer {API_KEY}")
    with urllib.request.urlopen(req, timeout=3) as resp:
        return json.load(resp)


try:
    payload = fetch_json("/api/health/detail", with_auth=True)
except urllib.error.HTTPError as exc:
    if API_KEY or exc.code not in {401, 403}:
        raise
    payload = fetch_json("/api/health", with_auth=False)

status = payload.get("status")
if status != "ok":
    raise SystemExit(f"unexpected health status: {status!r}")
