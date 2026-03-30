<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# Python SDK — Official OpenFang Client & Agent Framework

## Purpose

Official Python client library and agent SDK for OpenFang. Provides two interfaces:
1. **Client SDK** (`openfang_client.py`) — async REST client for kernel operations (equivalent to JavaScript SDK)
2. **Agent SDK** (`openfang_sdk.py`) — decorators and utilities for writing Python-based agents

## Key Files

| File | Purpose |
|------|---------|
| `setup.py` | Package metadata — `openfang` package, Python 3.8+ requirement |
| `openfang_sdk.py` | Agent framework — `Agent` class, decorators, input/output helpers |
| `openfang_client.py` | REST client — async HTTP client for kernel operations |
| `examples/` | Example agents and usage patterns |

## For AI Agents

**When to read:** Understand Python agent development, the client API, or extending the SDK.

**Agent SDK usage:**
```python
from openfang_sdk import Agent

agent = Agent()

@agent.on_message
def handle(message: str, context: dict) -> str:
    return f"You said: {message}"

agent.run()
```

**Standalone script:**
```python
from openfang_sdk import read_input, respond

data = read_input()
result = f"Echo: {data['message']}"
respond(result)
```

**Client SDK usage:**
```python
from openfang_client import OpenFangClient

client = OpenFangClient("http://localhost:4200")
agents = await client.agents.list()
reply = await client.agents.message(agent_id, "Hello!")
```

**Key classes:**
- `Agent` — decorator-based agent framework
- `OpenFangClient` — async REST client for kernel operations
- Helper functions: `read_input()`, `respond()`, `update_context()`

**Architecture note:**
- Agent framework uses stdin/stdout for kernel communication (simple, forking-friendly)
- Client SDK uses async/await for REST calls
- Both are lightweight wrappers — state lives in kernel

**Common tasks:**
- Writing simple agents → use `Agent` class with decorators
- Integrating with external libraries → use client SDK for kernel operations
- Debugging agent input/output → use `read_input()`/`respond()` directly
