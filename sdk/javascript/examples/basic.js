/**
 * Basic example — create an agent and chat with it.
 *
 * Usage:
 *   node basic.js
 */

const { OpenFang } = require("../index");

async function main() {
  const client = new OpenFang(process.env.OPENFANG_BASE_URL || "http://127.0.0.1:50051", {
    headers: process.env.OPENFANG_API_KEY
      ? { Authorization: `Bearer ${process.env.OPENFANG_API_KEY}` }
      : {},
  });

  // Check server health
  const health = await client.health();
  console.log("Server:", health);

  // List existing agents
  const agents = await client.agents.list();
  console.log("Agents:", agents.length);

  // Create a new agent from the "assistant" template
  const agent = await client.agents.create({ template: "assistant" });
  console.log("Created agent:", agent.id);

  // Send a message and get the full response
  const reply = await client.agents.message(agent.id, "What can you help me with?");
  console.log("Reply:", reply);

  // Clean up
  await client.agents.delete(agent.id);
  console.log("Agent deleted.");
}

main().catch(console.error);
