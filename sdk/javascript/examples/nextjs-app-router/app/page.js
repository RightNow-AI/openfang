"use client";

import { useEffect, useState, useRef } from "react";

const HEALTH_IDLE = {
  status: "checking",
  label: "Checking OpenFang...",
  openfangReachable: false,
  chatReady: false,
  lastError: null,
  timeoutMs: null,
};
const MAX_VISIBLE_MESSAGES = 20;

function trimMessages(messages) {
  return messages.slice(-MAX_VISIBLE_MESSAGES);
}

function buildHealthState(payload = {}) {
  const status = payload.status || "offline";

  if (status === "connected") {
    return {
      status,
      label: "OpenFang reachable and chat ready",
      openfangReachable: Boolean(payload.openfangReachable),
      chatReady: Boolean(payload.chatReady),
      lastError: payload.lastError || null,
      timeoutMs: payload.timeoutMs || null,
    };
  }

  if (status === "degraded") {
    return {
      status,
      label: payload.lastError
        ? "OpenFang reachable but assistant turns are degraded"
        : "OpenFang reachable but chat is not ready",
      openfangReachable: Boolean(payload.openfangReachable),
      chatReady: Boolean(payload.chatReady),
      lastError: payload.lastError || null,
      timeoutMs: payload.timeoutMs || null,
    };
  }

  return {
    status: "offline",
    label: "OpenFang or route layer unreachable",
    openfangReachable: Boolean(payload.openfangReachable),
    chatReady: Boolean(payload.chatReady),
    lastError: payload.lastError || null,
    timeoutMs: payload.timeoutMs || null,
  };
}

function applyMessagePatch(messages, messageId, patch) {
  return trimMessages(
    messages.map((entry) =>
      entry.id === messageId
        ? {
            ...entry,
            ...patch,
          }
        : entry,
    ),
  );
}

function markDegradedHealth(error, timeoutMs) {
  return buildHealthState({
    status: "degraded",
    openfangReachable: true,
    chatReady: false,
    lastError: error,
    timeoutMs,
  });
}

export default function HomePage() {
  const [composer, setComposer] = useState("");
  const [loading, setLoading] = useState(false);
  const [health, setHealth] = useState(HEALTH_IDLE);
  const [error, setError] = useState("");
  const [agentId, setAgentId] = useState("");
  const [streamMode, setStreamMode] = useState("idle");
  const [user, setUser] = useState(null);
  const [messages, setMessages] = useState([]);
  const [retryMessage, setRetryMessage] = useState("");

  useEffect(() => {
    let cancelled = false;

    async function bootstrap() {
      try {
        const [sessionResponse, healthResponse, historyResponse] = await Promise.all([
          fetch("/api/session", { cache: "no-store" }),
          fetch("/api/health", { cache: "no-store" }),
          fetch("/api/ai/chat/history?limit=20", { cache: "no-store" }),
        ]);

        if (!sessionResponse.ok) {
          throw new Error(`Session HTTP ${sessionResponse.status}`);
        }

        const sessionPayload = await sessionResponse.json();
        const healthPayload = healthResponse.ok
          ? buildHealthState(await healthResponse.json())
          : buildHealthState({
              status: "offline",
              lastError: `HTTP ${healthResponse.status}`,
            });
        const historyPayload = historyResponse.ok
          ? await historyResponse.json()
          : { messages: [] };
        const restoredMessages = trimMessages(historyPayload.messages || []);
        const lastAgentMessage = [...restoredMessages]
          .reverse()
          .find((entry) => entry.agent_id);

        if (!cancelled) {
          setUser(sessionPayload.user || null);
          setHealth(healthPayload);
          setMessages(restoredMessages);
          setAgentId(lastAgentMessage?.agent_id || "");
        }
      } catch (bootstrapError) {
        if (!cancelled) {
          setHealth(
            buildHealthState({
              status: "offline",
              lastError:
                bootstrapError instanceof Error
                  ? bootstrapError.message
                  : String(bootstrapError),
            }),
          );
        }
      }
    }

    bootstrap();
    return () => {
      cancelled = true;
    };
  }, []);

  async function sendMessage(rawMessage = composer) {
    const message = String(rawMessage || "").trim();
    if (!message || loading) {
      return;
    }

    const createdAt = new Date().toISOString();
    const userMessageId = `local-user-${Date.now()}`;
    const assistantMessageId = `local-assistant-${Date.now()}`;

    setLoading(true);
    setError("");
    setRetryMessage("");
    setStreamMode("connecting");
    setComposer("");
    setMessages((previous) =>
      trimMessages([
        ...previous,
        {
          id: userMessageId,
          role: "user",
          content: message,
          created_at: createdAt,
          updated_at: createdAt,
          request_id: null,
          agent_id: agentId || null,
          status: "complete",
          error: null,
          transport: "stream",
        },
        {
          id: assistantMessageId,
          role: "assistant",
          content: "",
          created_at: createdAt,
          updated_at: createdAt,
          request_id: null,
          agent_id: agentId || null,
          status: "pending",
          error: null,
          transport: "stream",
        },
      ]),
    );

    try {
      const response = await fetch("/api/ai/chat/stream", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ message }),
      });

      const contentType = response.headers.get("content-type") || "";
      if (!response.ok) {
        const payload = contentType.includes("application/json")
          ? await response.json()
          : { error: await response.text() };
        throw new Error(payload.error || `HTTP ${response.status}`);
      }

      if (contentType.includes("application/json")) {
        const payload = await response.json();
        const replyText = payload.reply?.response || "";
        setAgentId(payload.agentId || "");
        setStreamMode(payload.fallback ? "fallback" : "chat_complete");
        setHealth(
          buildHealthState({
            status: "connected",
            openfangReachable: true,
            chatReady: true,
            lastError: null,
            timeoutMs: health.timeoutMs,
          }),
        );
        setMessages((previous) => {
          let nextMessages = applyMessagePatch(previous, userMessageId, {
            request_id: payload.requestId || null,
            agent_id: payload.agentId || null,
            transport: payload.transport || "chat",
            status: "complete",
          });
          nextMessages = applyMessagePatch(nextMessages, assistantMessageId, {
            request_id: payload.requestId || null,
            agent_id: payload.agentId || null,
            transport: payload.transport || "chat",
            content: replyText,
            status: "complete",
            error: null,
          });
          return nextMessages;
        });
        return;
      }

      const reader = response.body?.getReader();
      if (!reader) {
        throw new Error("Streaming response did not include a body");
      }

      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) {
          break;
        }

        buffer += decoder.decode(value, { stream: true });
        const chunks = buffer.split("\n\n");
        buffer = chunks.pop() || "";

        for (const chunk of chunks) {
          const line = chunk
            .split("\n")
            .find((entry) => entry.startsWith("data: "));

          if (!line) {
            continue;
          }

          const event = JSON.parse(line.slice(6));

          if (event.type === "ready") {
            setAgentId(event.agentId || "");
            setStreamMode("streaming");
            setMessages((previous) => {
              let nextMessages = applyMessagePatch(previous, userMessageId, {
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "stream",
                status: "complete",
              });
              nextMessages = applyMessagePatch(nextMessages, assistantMessageId, {
                id: event.assistantMessageId || assistantMessageId,
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "stream",
                status: "pending",
              });
              return nextMessages;
            });
          } else if (event.type === "text_delta") {
            setStreamMode("streaming");
            setMessages((previous) =>
              applyMessagePatch(previous, event.assistantMessageId || assistantMessageId, {
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "stream",
                content: `${
                  previous.find(
                    (entry) =>
                      entry.id === (event.assistantMessageId || assistantMessageId),
                  )?.content || ""
                }${event.delta || ""}`,
                status: "pending",
                error: null,
              }),
            );
          } else if (event.type === "fallback") {
            setAgentId(event.agentId || "");
            setStreamMode("fallback");
            setHealth(
              buildHealthState({
                status: "connected",
                openfangReachable: true,
                chatReady: true,
                lastError: null,
                timeoutMs: health.timeoutMs,
              }),
            );
            setMessages((previous) =>
              applyMessagePatch(previous, event.assistantMessageId || assistantMessageId, {
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "chat",
                content: event.content || "",
                status: event.status || "complete",
                error: null,
              }),
            );
          } else if (event.type === "complete") {
            setStreamMode("complete");
            setHealth(
              buildHealthState({
                status: "connected",
                openfangReachable: true,
                chatReady: true,
                lastError: null,
                timeoutMs: health.timeoutMs,
              }),
            );
            setMessages((previous) =>
              applyMessagePatch(previous, event.assistantMessageId || assistantMessageId, {
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "stream",
                status: event.status || "complete",
                error: null,
              }),
            );
          } else if (event.type === "error") {
            const terminalStatus = event.status || "failed";
            setError(event.error || "Streaming failed");
            setRetryMessage(message);
            setStreamMode(terminalStatus);
            setHealth(markDegradedHealth(event.error || "Streaming failed", health.timeoutMs));
            setMessages((previous) =>
              applyMessagePatch(previous, event.assistantMessageId || assistantMessageId, {
                request_id: event.requestId || null,
                agent_id: event.agentId || null,
                transport: event.transport || "stream",
                status: terminalStatus,
                error: event.error || "Streaming failed",
                content:
                  previous.find(
                    (entry) =>
                      entry.id === (event.assistantMessageId || assistantMessageId),
                  )?.content || "",
              }),
            );
          } else if (event.type === "done") {
            setStreamMode(event.status || "done");
          }
        }
      }
    } catch (sendError) {
      const messageText = sendError instanceof Error ? sendError.message : String(sendError);
      const terminalStatus = /timed out/i.test(messageText) ? "timed_out" : "failed";
      setError(messageText);
      setRetryMessage(message);
      setStreamMode(terminalStatus);
      setHealth(markDegradedHealth(messageText, health.timeoutMs));
      setMessages((previous) =>
        applyMessagePatch(previous, assistantMessageId, {
          status: terminalStatus,
          error: messageText,
          content:
            previous.find((entry) => entry.id === assistantMessageId)?.content || "",
        }),
      );
    } finally {
      setLoading(false);
    }
  }

  const healthColor =
    health.status === "connected"
      ? "#0f9d58"
      : health.status === "degraded"
        ? "#f29900"
        : health.status === "offline"
          ? "#d93025"
          : "#5f6b7a";

  const visibleMessages = trimMessages(messages);

  return (
    <div
      style={{
        maxWidth: 760,
        margin: "40px auto",
        padding: "0 20px 40px",
        fontFamily: "var(--font-sans)",
        color: "var(--text)",
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          gap: 16,
          marginBottom: 20,
          flexWrap: "wrap",
        }}
      >
        <div>
          <h1 style={{ margin: 0, fontSize: 28, fontWeight: 700, color: "var(--text)" }}>OpenFang</h1>
          <p style={{ margin: "6px 0 0", color: "var(--text-dim)" }}>
            Stream-first conversation with server-derived identity and refresh-safe history.
          </p>
        </div>
        <div
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 8,
            border: `1px solid ${healthColor}`,
            borderRadius: 999,
            padding: "8px 12px",
            color: healthColor,
            fontSize: 14,
            fontWeight: 600,
          }}
        >
          <span
            style={{
              width: 10,
              height: 10,
              borderRadius: 999,
              background: healthColor,
              display: "inline-block",
            }}
          />
          {health.label}
        </div>
      </div>

      <section
        style={{
          border: "1px solid var(--border-light)",
          borderRadius: "var(--radius-lg)",
          padding: 16,
          background: "var(--bg-elevated)",
          boxShadow: "var(--shadow-sm)",
        }}
      >
        <label
          htmlFor="message"
          style={{ display: "block", fontWeight: 600, marginBottom: 10, color: "var(--text)" }}
        >
          Message
        </label>
        <textarea
          id="message"
          rows={6}
          value={composer}
          onChange={(event) => setComposer(event.target.value)}
          placeholder="Ask something..."
          style={{
            width: "100%",
            resize: "vertical",
            borderRadius: "var(--radius)",
            border: "1px solid var(--border-light)",
            padding: 12,
            font: "inherit",
            boxSizing: "border-box",
            background: "var(--surface)",
            color: "var(--text)",
          }}
        />
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            marginTop: 12,
            flexWrap: "wrap",
          }}
        >
          <button
            onClick={() => sendMessage()}
            disabled={loading || !composer.trim()}
            style={{
              border: 0,
              borderRadius: "var(--radius)",
              padding: "10px 16px",
              background: loading ? "var(--text-muted)" : "var(--accent)",
              color: "#ffffff",
              cursor: loading ? "not-allowed" : "pointer",
              font: "inherit",
              fontWeight: 600,
            }}
          >
            {loading ? "Streaming..." : "Send"}
          </button>
          <span style={{ color: "var(--text-dim)", fontSize: 14 }}>
            {user
              ? <>Signed-in: <strong>{user.id}</strong></>
              : "Resolving server session..."}
          </span>
        </div>
      </section>

      <section
        style={{
          marginTop: 20,
          border: "1px solid var(--border)",
          borderRadius: "var(--radius-lg)",
          padding: 16,
          background: "var(--surface2)",
        }}
      >
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 12,
            marginBottom: 10,
            flexWrap: "wrap",
          }}
        >
          <strong style={{ color: "var(--text)" }}>Assistant response</strong>
          <span style={{ color: "var(--text-dim)", fontSize: 14 }}>
            {agentId ? `agent ${agentId}` : "no agent yet"}
            {streamMode !== "idle" ? ` · ${streamMode}` : ""}
          </span>
        </div>
        {health.lastError ? (
          <p style={{ margin: "0 0 12px", color: "var(--warning)", fontSize: 14 }}>
            Last runtime error: {health.lastError}
          </p>
        ) : null}
        <div style={{ display: "grid", gap: 12 }}>
          {visibleMessages.length ? (
            visibleMessages.map((entry) => (
              <article
                key={entry.id || `${entry.request_id}-${entry.role}-${entry.created_at}`}
                style={{
                  borderRadius: "var(--radius-lg)",
                  padding: 12,
                  background: entry.role === "user" ? "var(--accent-subtle)" : "var(--bg-elevated)",
                  border: `1px solid ${
                    entry.status === "failed" || entry.status === "timed_out"
                      ? "var(--error-muted)"
                      : entry.status === "pending"
                        ? "var(--warning-muted)"
                        : "var(--border)"
                  }`,
                }}
              >
                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    gap: 12,
                    marginBottom: 8,
                    flexWrap: "wrap",
                    fontSize: 13,
                    color: "var(--text-dim)",
                  }}
                >
                  <strong style={{ color: "var(--text-secondary)" }}>{entry.role === "user" ? "You" : "Assistant"}</strong>
                  <span>
                    {entry.status || "complete"}
                    {entry.transport ? ` · ${entry.transport}` : ""}
                  </span>
                </div>
                <pre
                  style={{
                    whiteSpace: "pre-wrap",
                    margin: 0,
                    fontFamily: "var(--font-mono)",
                    color: "var(--text)",
                  }}
                >
                  {entry.content ||
                    (entry.status === "pending"
                      ? "Waiting for streamed tokens..."
                      : "No content.")}
                </pre>
                {entry.error ? (
                  <p style={{ margin: "8px 0 0", color: "var(--error)", fontSize: 13 }}>
                    {entry.error}
                  </p>
                ) : null}
              </article>
            ))
          ) : (
            <p style={{ margin: 0, color: "var(--text-dim)" }}>No conversation yet.</p>
          )}
        </div>
        {error ? (
          <div style={{ marginTop: 12 }}>
            <p style={{ color: "var(--error)", margin: 0 }}>{error}</p>
            {retryMessage ? (
              <button
                onClick={() => sendMessage(retryMessage)}
                style={{
                  marginTop: 10,
                  border: 0,
                  borderRadius: "var(--radius)",
                  padding: "8px 12px",
                  background: "var(--error)",
                  color: "#fff",
                  cursor: "pointer",
                }}
              >
                Retry assistant turn
              </button>
            ) : null}
          </div>
        ) : null}
      </section>
    </div>
  );
}
