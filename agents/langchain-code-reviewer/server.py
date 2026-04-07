"""
LangChain Code Review Agent — A2A-compatible server.

Exposes a code review agent via Google's A2A protocol so that
OpenFang workflows can call it as an external agent.

Start:
    OPENAI_API_KEY=sk-xxx python server.py
    # or with Ollama (no key needed):
    USE_OLLAMA=1 python server.py

Endpoints:
    GET  /.well-known/agent.json   — A2A Agent Card
    POST /a2a                      — JSON-RPC task endpoint
"""

import os
import uuid
import asyncio
from datetime import datetime, timezone

from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse
import uvicorn

from agent import CodeReviewAgent

# ---------------------------------------------------------------------------
# Config
# ---------------------------------------------------------------------------

HOST = os.getenv("HOST", "0.0.0.0")
PORT = int(os.getenv("PORT", "9100"))
BASE_URL = os.getenv("BASE_URL", f"http://127.0.0.1:{PORT}")

app = FastAPI(title="LangChain Code Review Agent")
agent = CodeReviewAgent()

# In-memory task store
tasks: dict[str, dict] = {}

# ---------------------------------------------------------------------------
# A2A Agent Card
# ---------------------------------------------------------------------------

AGENT_CARD = {
    "name": "langchain-code-reviewer",
    "description": (
        "LangChain-powered code review agent. "
        "Analyzes code for bugs, security issues, performance problems, "
        "and style violations. Returns structured review with severity levels."
    ),
    "url": f"{BASE_URL}/a2a",
    "version": "0.1.0",
    "capabilities": {
        "streaming": False,
        "pushNotifications": False,
        "stateTransitionHistory": True,
    },
    "skills": [
        {
            "id": "code-review",
            "name": "Code Review",
            "description": "Review code for correctness, security, performance, and style",
            "tags": ["code", "review", "security", "quality"],
            "examples": [
                "Review this Python function for bugs",
                "Check this Rust code for security issues",
                "Analyze this PR diff for performance problems",
            ],
        },
        {
            "id": "pr-review",
            "name": "Pull Request Review",
            "description": "Review a git diff / pull request",
            "tags": ["pr", "diff", "git"],
            "examples": [
                "Review this PR diff",
                "Analyze these changes",
            ],
        },
    ],
    "defaultInputModes": ["text"],
    "defaultOutputModes": ["text"],
}


@app.get("/.well-known/agent.json")
async def agent_card():
    return JSONResponse(content=AGENT_CARD)


# ---------------------------------------------------------------------------
# A2A JSON-RPC Endpoint
# ---------------------------------------------------------------------------


@app.post("/a2a")
async def a2a_endpoint(request: Request):
    body = await request.json()

    jsonrpc = body.get("jsonrpc", "2.0")
    req_id = body.get("id", 1)
    method = body.get("method", "")
    params = body.get("params", {})

    if method == "tasks/send":
        return await handle_tasks_send(jsonrpc, req_id, params)
    elif method == "tasks/get":
        return handle_tasks_get(jsonrpc, req_id, params)
    elif method == "tasks/cancel":
        return handle_tasks_cancel(jsonrpc, req_id, params)
    else:
        return JSONResponse(content={
            "jsonrpc": jsonrpc,
            "id": req_id,
            "error": {"code": -32601, "message": f"Method not found: {method}"},
        })


async def handle_tasks_send(jsonrpc: str, req_id: int, params: dict):
    message = params.get("message", {})
    session_id = params.get("sessionId")
    task_id = str(uuid.uuid4())

    text_parts = [
        p["text"] for p in message.get("parts", []) if p.get("type") == "text"
    ]
    user_input = "\n".join(text_parts)

    task = {
        "id": task_id,
        "sessionId": session_id,
        "status": {"state": "working", "message": None},
        "messages": [message],
        "artifacts": [],
    }
    tasks[task_id] = task

    try:
        review_result = await asyncio.to_thread(agent.review, user_input)

        agent_message = {
            "role": "agent",
            "parts": [{"type": "text", "text": review_result}],
        }
        task["messages"].append(agent_message)
        task["status"] = {"state": "completed", "message": None}
        task["artifacts"] = [
            {
                "name": "code-review-report",
                "description": "Structured code review report",
                "parts": [{"type": "text", "text": review_result}],
                "index": 0,
                "lastChunk": True,
            }
        ]
    except Exception as e:
        task["status"] = {"state": "failed", "message": str(e)}
        task["messages"].append({
            "role": "agent",
            "parts": [{"type": "text", "text": f"Review failed: {e}"}],
        })

    return JSONResponse(content={
        "jsonrpc": jsonrpc,
        "id": req_id,
        "result": task,
    })


def handle_tasks_get(jsonrpc: str, req_id: int, params: dict):
    task_id = params.get("id", "")
    task = tasks.get(task_id)

    if task is None:
        return JSONResponse(content={
            "jsonrpc": jsonrpc,
            "id": req_id,
            "error": {"code": -32000, "message": f"Task not found: {task_id}"},
        })

    return JSONResponse(content={
        "jsonrpc": jsonrpc,
        "id": req_id,
        "result": task,
    })


def handle_tasks_cancel(jsonrpc: str, req_id: int, params: dict):
    task_id = params.get("id", "")
    task = tasks.get(task_id)

    if task is None:
        return JSONResponse(content={
            "jsonrpc": jsonrpc,
            "id": req_id,
            "error": {"code": -32000, "message": f"Task not found: {task_id}"},
        })

    task["status"] = {"state": "cancelled", "message": None}
    return JSONResponse(content={
        "jsonrpc": jsonrpc,
        "id": req_id,
        "result": task,
    })


# ---------------------------------------------------------------------------
# Health check
# ---------------------------------------------------------------------------

@app.get("/health")
async def health():
    return {"status": "ok", "agent": "langchain-code-reviewer", "tasks": len(tasks)}


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if __name__ == "__main__":
    print(f"Starting LangChain Code Review Agent on {HOST}:{PORT}")
    print(f"Agent Card: {BASE_URL}/.well-known/agent.json")
    print(f"A2A endpoint: {BASE_URL}/a2a")
    uvicorn.run(app, host=HOST, port=PORT)
