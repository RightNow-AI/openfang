"""
OMTAE Python Client — REST API client for controlling OMTAE remotely.

Usage:

    from omtae_client import OMTAE

    client = OMTAE("http://localhost:3000")

    # Create an agent
    agent = client.agents.create(template="assistant")
    print(agent["id"])

    # Send a message
    reply = client.agents.message(agent["id"], "Hello!")
    print(reply)

    # Stream a response
    for event in client.agents.stream(agent["id"], "Tell me a joke"):
        if event.get("type") == "text_delta":
            print(event["delta"], end="", flush=True)

Note: This is the REST API *client* library.
      For writing Python agents that run inside OMTAE, see omtae_sdk.py instead.
"""

import json
from typing import Any, Dict, Generator, Optional
from urllib.request import urlopen, Request
from urllib.error import HTTPError
from urllib.parse import urlencode, quote


class OMTAEError(Exception):
    def __init__(self, message: str, status: int = 0, body: str = ""):
        super().__init__(message)
        self.status = status
        self.body = body


class _Resource:
    def __init__(self, client: "OMTAE"):
        self._c = client


class OMTAE:
    """OMTAE REST API client. Zero dependencies — uses only stdlib urllib."""

    def __init__(self, base_url: str, headers: Optional[Dict[str, str]] = None):
        self.base_url = base_url.rstrip("/")
        self._headers = {"Content-Type": "application/json"}
        if headers:
            self._headers.update(headers)

        self.agents = _AgentResource(self)
        self.sessions = _SessionResource(self)
        self.workflows = _WorkflowResource(self)
        self.skills = _SkillResource(self)
        self.channels = _ChannelResource(self)
        self.tools = _ToolResource(self)
        self.models = _ModelResource(self)
        self.providers = _ProviderResource(self)
        self.memory = _MemoryResource(self)
        self.triggers = _TriggerResource(self)
        self.schedules = _ScheduleResource(self)

    def _request(self, method: str, path: str, body: Any = None) -> Any:
        url = self.base_url + path
        data = json.dumps(body).encode() if body is not None else None
        req = Request(url, data=data, headers=self._headers, method=method)
        try:
            with urlopen(req) as resp:
                ct = resp.headers.get("content-type", "")
                text = resp.read().decode()
                if "application/json" in ct:
                    return json.loads(text)
                return text
        except HTTPError as e:
            body_text = e.read().decode() if e.fp else ""
            raise OMTAEError(f"HTTP {e.code}: {body_text}", e.code, body_text) from e

    def _stream(self, method: str, path: str, body: Any = None) -> Generator[Dict, None, None]:
        """SSE streaming. Yields parsed JSON events."""
        url = self.base_url + path
        data = json.dumps(body).encode() if body is not None else None
        headers = dict(self._headers)
        headers["Accept"] = "text/event-stream"
        req = Request(url, data=data, headers=headers, method=method)
        try:
            resp = urlopen(req)
        except HTTPError as e:
            body_text = e.read().decode() if e.fp else ""
            raise OMTAEError(f"HTTP {e.code}: {body_text}", e.code, body_text) from e

        buffer = ""
        while True:
            chunk = resp.read(4096)
            if not chunk:
                break
            buffer += chunk.decode()
            lines = buffer.split("\n")
            buffer = lines.pop()
            for line in lines:
                line = line.strip()
                if line.startswith("data: "):
                    data_str = line[6:]
                    if data_str == "[DONE]":
                        return
                    try:
                        yield json.loads(data_str)
                    except json.JSONDecodeError:
                        yield {"raw": data_str}
        resp.close()

    def health(self) -> Any:
        return self._request("GET", "/api/health")

    def health_detail(self) -> Any:
        return self._request("GET", "/api/health/detail")

    def status(self) -> Any:
        return self._request("GET", "/api/status")

    def version(self) -> Any:
        return self._request("GET", "/api/version")

    def metrics(self) -> str:
        return self._request("GET", "/api/metrics")

    def usage(self) -> Any:
        return self._request("GET", "/api/usage")

    def config(self) -> Any:
        return self._request("GET", "/api/config")


# ── Agent Resource ──────────────────────────────────────────────

class _AgentResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/agents")

    def get(self, agent_id: str):
        return self._c._request("GET", f"/api/agents/{agent_id}")

    def create(self, **kwargs):
        return self._c._request("POST", "/api/agents", kwargs)

    def delete(self, agent_id: str):
        return self._c._request("DELETE", f"/api/agents/{agent_id}")

    def stop(self, agent_id: str):
        return self._c._request("POST", f"/api/agents/{agent_id}/stop")

    def clone(self, agent_id: str):
        return self._c._request("POST", f"/api/agents/{agent_id}/clone")

    def update(self, agent_id: str, **data):
        return self._c._request("PUT", f"/api/agents/{agent_id}/update", data)

    def set_mode(self, agent_id: str, mode: str):
        return self._c._request("PUT", f"/api/agents/{agent_id}/mode", {"mode": mode})

    def set_model(self, agent_id: str, model: str):
        return self._c._request("PUT", f"/api/agents/{agent_id}/model", {"model": model})

    def message(self, agent_id: str, text: str, **opts):
        body = {"message": text, **opts}
        return self._c._request("POST", f"/api/agents/{agent_id}/message", body)

    def stream(self, agent_id: str, text: str, **opts) -> Generator[Dict, None, None]:
        """Stream response events. Usage:
            for event in client.agents.stream(id, "Hello"):
                if event.get("type") == "text_delta":
                    print(event["delta"], end="")
        """
        body = {"message": text, **opts}
        return self._c._stream("POST", f"/api/agents/{agent_id}/message/stream", body)

    def session(self, agent_id: str):
        return self._c._request("GET", f"/api/agents/{agent_id}/session")

    def reset_session(self, agent_id: str):
        return self._c._request("POST", f"/api/agents/{agent_id}/session/reset")

    def compact_session(self, agent_id: str):
        return self._c._request("POST", f"/api/agents/{agent_id}/session/compact")

    def list_sessions(self, agent_id: str):
        return self._c._request("GET", f"/api/agents/{agent_id}/sessions")

    def create_session(self, agent_id: str, label: Optional[str] = None):
        return self._c._request("POST", f"/api/agents/{agent_id}/sessions", {"label": label})

    def switch_session(self, agent_id: str, session_id: str):
        return self._c._request("POST", f"/api/agents/{agent_id}/sessions/{session_id}/switch")

    def get_skills(self, agent_id: str):
        return self._c._request("GET", f"/api/agents/{agent_id}/skills")

    def set_skills(self, agent_id: str, skills):
        return self._c._request("PUT", f"/api/agents/{agent_id}/skills", skills)

    def set_identity(self, agent_id: str, **identity):
        return self._c._request("PATCH", f"/api/agents/{agent_id}/identity", identity)

    def patch_config(self, agent_id: str, **config):
        return self._c._request("PATCH", f"/api/agents/{agent_id}/config", config)


# ── Session Resource ────────────────────────────────────────────

class _SessionResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/sessions")

    def delete(self, session_id: str):
        return self._c._request("DELETE", f"/api/sessions/{session_id}")

    def set_label(self, session_id: str, label: str):
        return self._c._request("PUT", f"/api/sessions/{session_id}/label", {"label": label})


# ── Workflow Resource ───────────────────────────────────────────

class _WorkflowResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/workflows")

    def create(self, **workflow):
        return self._c._request("POST", "/api/workflows", workflow)

    def run(self, workflow_id: str, input_data=None):
        return self._c._request("POST", f"/api/workflows/{workflow_id}/run", input_data)

    def runs(self, workflow_id: str):
        return self._c._request("GET", f"/api/workflows/{workflow_id}/runs")


# ── Skill Resource ──────────────────────────────────────────────

class _SkillResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/skills")

    def install(self, **skill):
        return self._c._request("POST", "/api/skills/install", skill)

    def uninstall(self, **skill):
        return self._c._request("POST", "/api/skills/uninstall", skill)

    def search(self, query: str):
        return self._c._request("GET", f"/api/marketplace/search?q={quote(query)}")


# ── Channel Resource ────────────────────────────────────────────

class _ChannelResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/channels")

    def configure(self, name: str, **config):
        return self._c._request("POST", f"/api/channels/{name}/configure", config)

    def remove(self, name: str):
        return self._c._request("DELETE", f"/api/channels/{name}/configure")

    def test(self, name: str):
        return self._c._request("POST", f"/api/channels/{name}/test")


# ── Tool Resource ───────────────────────────────────────────────

class _ToolResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/tools")


# ── Model Resource ──────────────────────────────────────────────

class _ModelResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/models")

    def get(self, model_id: str):
        return self._c._request("GET", f"/api/models/{model_id}")

    def aliases(self):
        return self._c._request("GET", "/api/models/aliases")


# ── Provider Resource ───────────────────────────────────────────

class _ProviderResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/providers")

    def set_key(self, name: str, key: str):
        return self._c._request("POST", f"/api/providers/{name}/key", {"key": key})

    def delete_key(self, name: str):
        return self._c._request("DELETE", f"/api/providers/{name}/key")

    def test(self, name: str):
        return self._c._request("POST", f"/api/providers/{name}/test")


# ── Memory Resource ─────────────────────────────────────────────

class _MemoryResource(_Resource):

    def get_all(self, agent_id: str):
        return self._c._request("GET", f"/api/memory/agents/{agent_id}/kv")

    def get(self, agent_id: str, key: str):
        return self._c._request("GET", f"/api/memory/agents/{agent_id}/kv/{key}")

    def set(self, agent_id: str, key: str, value):
        return self._c._request("PUT", f"/api/memory/agents/{agent_id}/kv/{key}", {"value": value})

    def delete(self, agent_id: str, key: str):
        return self._c._request("DELETE", f"/api/memory/agents/{agent_id}/kv/{key}")


# ── Trigger Resource ────────────────────────────────────────────

class _TriggerResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/triggers")

    def create(self, **trigger):
        return self._c._request("POST", "/api/triggers", trigger)

    def update(self, trigger_id: str, **trigger):
        return self._c._request("PUT", f"/api/triggers/{trigger_id}", trigger)

    def delete(self, trigger_id: str):
        return self._c._request("DELETE", f"/api/triggers/{trigger_id}")


# ── Schedule Resource ───────────────────────────────────────────

class _ScheduleResource(_Resource):

    def list(self):
        return self._c._request("GET", "/api/schedules")

    def create(self, **schedule):
        return self._c._request("POST", "/api/schedules", schedule)

    def update(self, schedule_id: str, **schedule):
        return self._c._request("PUT", f"/api/schedules/{schedule_id}", schedule)

    def delete(self, schedule_id: str):
        return self._c._request("DELETE", f"/api/schedules/{schedule_id}")

    def run(self, schedule_id: str):
        return self._c._request("POST", f"/api/schedules/{schedule_id}/run")
