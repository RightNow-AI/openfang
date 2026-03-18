/**
 * LEGACY COMPATIBILITY ROUTE — POST /api/ai/chat/stream
 *
 * This route is NOT part of the current dashboard architecture.
 * It is retained for backward compatibility with the original SDK example
 * contract (SSE streaming pattern) and any external integrations that
 * depend on it.
 *
 * Current dashboard route for streaming: /api/agents/[id]/chat
 * Current dashboard docs:               README.md § API Routes
 *
 * Do NOT add new UI features here. If the compatibility layer is no longer
 * needed, remove this file and lib/openfang-proxy.js together.
 */
import { NextResponse } from "next/server";

import { applyIdentityCookie, resolveUserIdentity } from "../../../../../lib/auth";
import { env } from "../../../../../lib/env";
import {
  failureStatus,
  sendMessage,
  streamMessage,
  recordTurnFailure,
  recordTurnSuccess,
} from "../../../../../lib/openfang-proxy";
import { updateConversationMessage } from "../../../../../lib/session-store";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

function sseEvent(payload) {
  return `data: ${JSON.stringify(payload)}\n\n`;
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error);
}

function applyIdentityCookieHeader(response, identity) {
  if (!identity?.isNew) {
    return response;
  }

  response.headers.append(
    "Set-Cookie",
    `openfang_example_session=${identity.sessionToken}; Path=/; HttpOnly; SameSite=Lax; Max-Age=${60 * 60 * 24 * 30}`,
  );

  return response;
}

async function persistStreamProgress(upstream, assistantReply) {
  return updateConversationMessage(upstream.assistantMessageId, {
    agent_id: upstream.agentId,
    request_id: upstream.requestId,
    transport: "stream",
    content: assistantReply,
    status: "pending",
    error: null,
  });
}

async function persistStreamFailure(upstream, assistantReply, error) {
  const failure = await recordTurnFailure(error, "stream");
  await updateConversationMessage(upstream.assistantMessageId, {
    agent_id: upstream.agentId,
    request_id: upstream.requestId,
    transport: "stream",
    content: assistantReply,
    status: failure.status,
    error: failure.error,
  });

  return failure;
}

export async function POST(request) {
  let identity;
  let message = "";
  let metadata;

  try {
    identity = await resolveUserIdentity(request);
    const body = await request.json();
    message = String(body.message || "").trim();
    metadata =
      body.metadata && typeof body.metadata === "object" ? body.metadata : undefined;

    if (!message) {
      return applyIdentityCookie(
        NextResponse.json({ error: "message is required" }, { status: 400 }),
        identity,
      );
    }

    const upstream = await streamMessage(identity.userId, message, metadata);
    const encoder = new TextEncoder();

    const stream = new ReadableStream({
      async start(controller) {
        let reader;
        let assistantReply = "";
        let streamCompleted = false;
        let fallbackCompleted = false;
        let terminalEventSent = false;

        async function emit(payload) {
          controller.enqueue(encoder.encode(sseEvent(payload)));
        }

        async function readWithTimeout() {
          return Promise.race([
            reader.read(),
            new Promise((_, reject) => {
              setTimeout(() => {
                reject(new Error(`Stream timed out after ${env.OPENFANG_TIMEOUT_MS}ms`));
              }, env.OPENFANG_TIMEOUT_MS);
            }),
          ]);
        }

        async function fallbackToChat(streamError) {
          const persistedFailure = await persistStreamFailure(
            upstream,
            assistantReply,
            streamError,
          );

          try {
            const fallback = await sendMessage(identity.userId, message, metadata, {
              agentId: upstream.agentId,
              requestId: upstream.requestId,
              assistantMessageId: upstream.assistantMessageId,
              skipUserMessageWrite: true,
              transport: "chat",
            });

            fallbackCompleted = true;
            await emit({
              type: "fallback",
              requestId: fallback.requestId,
              agentId: fallback.agentId,
              assistantMessageId: fallback.assistantMessageId,
              transport: "chat",
              status: "complete",
              content: fallback.reply?.response || "",
            });
            await emit({
              type: "done",
              requestId: fallback.requestId,
              agentId: fallback.agentId,
              assistantMessageId: fallback.assistantMessageId,
              transport: "chat",
              status: "complete",
            });
            terminalEventSent = true;
            return;
          } catch (fallbackError) {
            await emit({
              type: "error",
              requestId: upstream.requestId,
              agentId: upstream.agentId,
              assistantMessageId: upstream.assistantMessageId,
              transport: "chat",
              status: failureStatus(fallbackError),
              error: errorMessage(fallbackError),
              previousError: persistedFailure.error,
            });
            await emit({
              type: "done",
              requestId: upstream.requestId,
              agentId: upstream.agentId,
              assistantMessageId: upstream.assistantMessageId,
              transport: "chat",
              status: failureStatus(fallbackError),
            });
            terminalEventSent = true;
          }
        }

        try {
          await emit({
            type: "ready",
            agentId: upstream.agentId,
            requestId: upstream.requestId,
            assistantMessageId: upstream.assistantMessageId,
            transport: "stream",
            status: "pending",
          });

          reader = upstream.body.getReader();
          const decoder = new TextDecoder();
          let buffer = "";

          while (true) {
            const { done, value } = await readWithTimeout();
            if (done) {
              break;
            }

            buffer += decoder.decode(value, { stream: true });
            const events = buffer.split("\n\n");
            buffer = events.pop() || "";

            for (const chunk of events) {
              const line = chunk
                .split("\n")
                .find((entry) => entry.startsWith("data: "));

              if (!line) {
                continue;
              }

              const payload = JSON.parse(line.slice(6));

              if (payload.type === "error") {
                throw new Error(payload.error || "Stream failed");
              }

              if (typeof payload.content === "string" && payload.content) {
                assistantReply += payload.content;
                await persistStreamProgress(upstream, assistantReply);
                await emit({
                  type: "text_delta",
                  requestId: upstream.requestId,
                  agentId: upstream.agentId,
                  assistantMessageId: upstream.assistantMessageId,
                  delta: payload.content,
                  status: "pending",
                });
              }

              if (payload.done === true) {
                streamCompleted = true;
              }
            }
          }

          if (!streamCompleted && !assistantReply) {
            throw new Error("Stream ended before the assistant produced a reply");
          }

          await updateConversationMessage(upstream.assistantMessageId, {
            agent_id: upstream.agentId,
            request_id: upstream.requestId,
            transport: "stream",
            content: assistantReply,
            status: "complete",
            error: null,
          });
          await recordTurnSuccess("stream");

          await emit({
            type: "complete",
            requestId: upstream.requestId,
            agentId: upstream.agentId,
            assistantMessageId: upstream.assistantMessageId,
            transport: "stream",
            status: "complete",
          });
          await emit({
            type: "done",
            requestId: upstream.requestId,
            agentId: upstream.agentId,
            assistantMessageId: upstream.assistantMessageId,
            transport: "stream",
            status: "complete",
          });
          terminalEventSent = true;
        } catch (streamError) {
          await fallbackToChat(streamError);
        } finally {
          if (reader) {
            try {
              await reader.cancel();
            } catch {
            }
            reader.releaseLock();
          }

          if (!terminalEventSent && !fallbackCompleted && assistantReply) {
            await emit({
              type: "done",
              requestId: upstream.requestId,
              agentId: upstream.agentId,
              assistantMessageId: upstream.assistantMessageId,
              transport: "stream",
              status: "failed",
            });
          }

          controller.close();
        }
      },
    });

    return applyIdentityCookieHeader(
      new Response(stream, {
        headers: {
          "Content-Type": "text/event-stream",
          "Cache-Control": "no-cache, no-transform",
          Connection: "keep-alive",
          "X-Accel-Buffering": "no",
        },
      }),
      identity,
    );
  } catch (error) {
    if (!identity || !message) {
      return applyIdentityCookie(
        NextResponse.json({ error: errorMessage(error) }, { status: 500 }),
        identity,
      );
    }

    try {
      const fallback = await sendMessage(identity.userId, message, metadata, {
        transport: "chat",
      });

      return applyIdentityCookie(
        NextResponse.json({ ...fallback, fallback: "chat" }),
        identity,
      );
    } catch (fallbackError) {
      return applyIdentityCookie(
        NextResponse.json(
          { error: errorMessage(fallbackError) },
          { status: 500 },
        ),
        identity,
      );
    }
  }
}
