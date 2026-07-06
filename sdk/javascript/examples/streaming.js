/**
 * Streaming example — stream agent responses token by token.
 *
 * Usage:
 *   node streaming.js
 */

const { OMTAE } = require("../index");

async function main() {
  const client = new OMTAE("http://localhost:3000");

  // Create an agent
  const agent = await client.agents.create({ template: "assistant" });
  console.log("Agent:", agent.id);

  // Stream the response
  console.log("\n--- Streaming response ---");
  for await (const event of client.agents.stream(agent.id, "Tell me a short story about a robot.")) {
    if (event.type === "text_delta" && event.delta) {
      process.stdout.write(event.delta);
    } else if (event.type === "tool_call") {
      console.log("\n[Tool call:", event.tool, "]");
    } else if (event.type === "done") {
      console.log("\n--- Done ---");
    }
  }

  // Clean up
  await client.agents.delete(agent.id);
}

main().catch(console.error);
