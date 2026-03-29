<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-03-29 | Updated: 2026-03-29 -->

# sdk

## Purpose
Official client SDKs for JavaScript/TypeScript and Python. Both SDKs provide typed client libraries for the OpenFang REST API, including examples and streaming support.

## Key Files
| File | Description |
|------|-------------|
| `javascript/index.js` | JS/TS client implementation |
| `javascript/index.d.ts` | TypeScript type definitions |
| `javascript/package.json` | Node >=18, @openfang/sdk package |
| `javascript/examples/basic.js` | Basic synchronous usage example |
| `javascript/examples/streaming.js` | Streaming message responses |
| `python/openfang_sdk.py` | Python SDK implementation |
| `python/openfang_client.py` | Python HTTP client wrapper |
| `python/setup.py` | Package config, Python >=3.8 |
| `python/examples/client_basic.py` | Basic synchronous usage |
| `python/examples/client_streaming.py` | Streaming message responses |
| `python/examples/echo_agent.py` | Simple agent example |

## For AI Agents

### Working In This Directory
- Both SDKs wrap the OpenFang REST API — keep implementation minimal.
- Add examples that demonstrate realistic usage patterns.
- Ensure all examples are runnable and tested against live daemon.
- Type definitions (TypeScript) must match actual API response structures.
- Document method signatures with request parameters and response types.
- Breaking API changes must be reflected in both SDKs simultaneously.

<!-- MANUAL: -->
