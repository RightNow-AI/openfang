/**
 * SSE client example for the backend proxy.
 *
 * Usage:
 *   APP_BACKEND_BASE_URL=http://127.0.0.1:3100 node sse-client.js
 */

const APP_BACKEND_BASE_URL = process.env.APP_BACKEND_BASE_URL || "http://127.0.0.1:3100";

async function main() {
  const response = await fetch(`${APP_BACKEND_BASE_URL}/api/ai/chat/stream`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      userId: "demo-user",
      message: "Say hello in five words.",
    }),
  });

  if (!response.ok || !response.body) {
    throw new Error(`HTTP ${response.status}`);
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }

    buffer += decoder.decode(value, { stream: true });
    const lines = buffer.split("\n");
    buffer = lines.pop() || "";

    for (const line of lines) {
      if (!line.startsWith("data: ")) {
        continue;
      }
      const event = JSON.parse(line.slice(6));
      if (event.type === "text_delta" && event.delta) {
        process.stdout.write(event.delta);
      } else if (event.type === "ready") {
        console.log(`Streaming from agent ${event.agentId}`);
      } else if (event.type === "error") {
        console.error(`\n[stream error] ${event.error}`);
      } else if (event.type === "done") {
        console.log("\n[done]");
      }
    }
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});