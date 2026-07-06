#!/usr/bin/env python3
"""
Basic example — create an agent and chat with it via the REST API.

Usage:
    python client_basic.py
"""

import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from omtae_client import OMTAE

client = OMTAE("http://localhost:3000")

# Check server health
health = client.health()
print("Server:", health)

# List existing agents
agents = client.agents.list()
print(f"Agents: {len(agents)}")

# Create a new agent from the "assistant" template
agent = client.agents.create(template="assistant")
print(f"Created agent: {agent['id']}")

# Send a message and get the full response
reply = client.agents.message(agent["id"], "What can you help me with?")
print(f"Reply: {reply}")

# Clean up
client.agents.delete(agent["id"])
print("Agent deleted.")
