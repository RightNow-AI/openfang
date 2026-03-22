#!/usr/bin/env python3

import ipaddress
import json
import os
import urllib.error
import urllib.request
from pathlib import Path

import tomllib


MAX_INCLUDE_DEPTH = 10
TRUTHY_VALUES = {"1", "true", "yes", "on"}


def truthy(value: str) -> bool:
    return value.strip().lower() in TRUTHY_VALUES


def parse_env_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.exists():
        return values

    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if not key:
            continue
        if len(value) >= 2 and (
            (value.startswith('"') and value.endswith('"'))
            or (value.startswith("'") and value.endswith("'"))
        ):
            value = value[1:-1]
        values[key] = value
    return values


def deep_merge(base: dict, overlay: dict) -> dict:
    for key, value in overlay.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            deep_merge(base[key], value)
        else:
            base[key] = value
    return base


def load_config_with_includes(config_path: Path, visited: set[Path] | None = None, depth: int = 0) -> dict:
    if depth > MAX_INCLUDE_DEPTH:
        raise SystemExit(f"config include depth exceeded {MAX_INCLUDE_DEPTH}")

    if visited is None:
        visited = set()

    canonical_path = config_path.resolve(strict=True)
    if canonical_path in visited:
        raise SystemExit(f"circular config include detected: {config_path}")
    visited.add(canonical_path)

    config_dir = canonical_path.parent
    root = tomllib.loads(canonical_path.read_text(encoding="utf-8"))
    includes = root.get("include") or []
    merged: dict = {}

    if not isinstance(includes, list):
        raise SystemExit("config include must be an array")

    for include in includes:
        if not isinstance(include, str):
            continue
        include_path = Path(include)
        if include_path.is_absolute():
            raise SystemExit(f"config include rejects absolute path: {include}")
        if ".." in include_path.parts:
            raise SystemExit(f"config include rejects path traversal: {include}")
        resolved = (config_dir / include_path).resolve(strict=True)
        try:
            resolved.relative_to(config_dir)
        except ValueError as exc:
            raise SystemExit(f"config include escapes config directory: {include}") from exc
        deep_merge(merged, load_config_with_includes(resolved, visited, depth + 1))

    root.pop("include", None)
    api_section = root.get("api")
    if isinstance(api_section, dict):
        for key in ("api_key", "api_listen", "log_level"):
            if key not in root and key in api_section:
                root[key] = api_section[key]

    deep_merge(merged, root)
    visited.remove(canonical_path)
    return merged


def runtime_override_env() -> dict[str, str]:
    env: dict[str, str] = {}
    external_env_path = os.environ.get("OPENFANG_ENV_FILE", "").strip()
    if external_env_path:
        env.update(parse_env_file(Path(external_env_path)))
    env.update(os.environ)
    return env


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


def load_runtime_config() -> dict:
    openfang_home = Path(os.environ.get("OPENFANG_HOME", "/data")).expanduser()
    config_path = openfang_home / "config.toml"
    if not config_path.exists():
        return {}
    return load_config_with_includes(config_path)


RUNTIME_CONFIG = load_runtime_config()
RUNTIME_ENV = runtime_override_env()
API_KEY = str(RUNTIME_ENV.get("OPENFANG_API_KEY", RUNTIME_CONFIG.get("api_key", ""))).strip()
STRICT_PRODUCTION = truthy(os.environ.get("OPENFANG_STRICT_PRODUCTION", ""))


def resolve_base_url() -> str:
    explicit = os.environ.get("OPENFANG_BASE_URL", "").strip()
    if explicit:
        return explicit.rstrip("/")

    listen_addr = str(
        RUNTIME_ENV.get("OPENFANG_LISTEN", RUNTIME_CONFIG.get("api_listen", "127.0.0.1:4200"))
    ).strip()
    return base_url_from_listen(listen_addr)


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
    if STRICT_PRODUCTION:
        raise SystemExit(
            "OPENFANG_STRICT_PRODUCTION requires an authenticated /api/health/detail probe; "
            "configure OPENFANG_API_KEY or config api_key for the healthcheck"
        ) from exc
    payload = fetch_json("/api/health", with_auth=False)

status = payload.get("status")
if status != "ok":
    raise SystemExit(f"unexpected health status: {status!r}")
