#!/usr/bin/env python3

import ipaddress
import json
import os
import urllib.error
import urllib.request


API_KEY = os.environ.get("OPENFANG_API_KEY", "").strip()


def split_host_port(listen_addr: str) -> tuple[str, str]:
    listen = str(listen_addr).strip()
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


def base_url_from_listen(listen_addr: str) -> str:
    host, port = split_host_port(listen_addr)
    normalized = host.strip().strip("[]")
    if normalized in {"", "0.0.0.0", "::", "localhost"}:
        normalized = "127.0.0.1"
    else:
        try:
            ip = ipaddress.ip_address(normalized)
            if ip.version == 6:
                normalized = f"[{normalized}]"
        except ValueError:
            pass
    return f"http://{normalized}:{port}"


def resolve_base_url() -> str:
    explicit = os.environ.get("OPENFANG_BASE_URL", "").strip()
    if explicit:
        return explicit.rstrip("/")

    listen_addr = os.environ.get("OPENFANG_LISTEN", "").strip()
    if listen_addr:
        return base_url_from_listen(listen_addr)

    return "http://127.0.0.1:4200"


BASE_URL = resolve_base_url()


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
