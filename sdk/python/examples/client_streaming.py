#!/usr/bin/env python3
"""
Streaming example — stream agent responses token by token.

Usage:
    python client_streaming.py
"""

import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from omtae_client import OMTAE

client = OMTAE("http://localhost:3000")

# Create an agent
agent = client.agents.create(template="assistant")
print(f"Agent: {agent['id']}")

# Stream the response
print("\n--- Streaming response ---")
for event in client.agents.stream(agent["id"], "Tell me a short story about a robot."):
    event_type = event.get("type", "")
    if event_type == "text_delta" and event.get("delta"):
        print(event["delta"], end="", flush=True)
    elif event_type == "tool_call":
        print(f"\n[Tool call: {event.get('tool')}]")
    elif event_type == "done":
        print("\n--- Done ---")

# Clean up
client.agents.delete(agent["id"])
