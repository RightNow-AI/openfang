// Named exports only — client-side only (do not import in Server Components).
// Uses the fetch + ReadableStream SSE reader; no external dependencies.

import { getBaseUrl } from './openfang-client'

const MAX_RETRIES = 3
const RETRY_DELAY_MS = 2000

/**
 * streamChat — opens SSE to POST /api/agents/:id/message/stream
 *
 * Returns { abort } — call abort() to cancel immediately.
 *
 * Callbacks:
 *   onChunk(text)    called for each 'chunk' event with the text fragment
 *   onDone(usage)    called when stream completes; usage may be null
 *   onError(message) called on unrecoverable error / max retries exceeded
 *
 * SSE events emitted by the server:
 *   event: chunk  data: { content: string, done: false }
 *   event: done   data: { done: true, usage: { input_tokens, output_tokens } }
 */
export function streamChat(agentId, message, onChunk, onDone, onError) {
  const controller = new AbortController()
  let retries = 0

  async function attempt() {
    if (controller.signal.aborted) return
    try {
      const res = await fetch(
        `${getBaseUrl()}/api/agents/${encodeURIComponent(agentId)}/message/stream`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ message }),
          signal: controller.signal,
        }
      )

      if (!res.ok) {
        const text = await res.text().catch(() => '')
        throw new Error(`HTTP ${res.status}${text ? ': ' + text : ''}`)
      }

      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ''
      let currentEvent = null

      while (true) {
        const { done, value } = await reader.read()
        if (done) {
          onDone(null)
          return
        }
        buffer += decoder.decode(value, { stream: true })
        const lines = buffer.split('\n')
        buffer = lines.pop() ?? ''

        for (const line of lines) {
          if (line.startsWith('event:')) {
            currentEvent = line.slice(6).trim()
          } else if (line.startsWith('data:')) {
            const raw = line.slice(5).trim()
            if (!raw) continue
            try {
              const parsed = JSON.parse(raw)
              if (currentEvent === 'chunk' && typeof parsed.content === 'string') {
                onChunk(parsed.content)
              } else if (currentEvent === 'done') {
                onDone(parsed.usage ?? null)
                reader.cancel()
                return
              }
              // ignore: tool_use, tool_result, phase events
            } catch {
              // non-JSON data line — skip
            }
            currentEvent = null
          } else if (line === '') {
            currentEvent = null
          }
        }
      }
    } catch (err) {
      if (controller.signal.aborted) return
      retries++
      if (retries >= MAX_RETRIES) {
        onError(err?.message ?? 'Stream failed after retries')
        return
      }
      await new Promise((r) => setTimeout(r, RETRY_DELAY_MS))
      attempt()
    }
  }

  attempt()
  return { abort: () => controller.abort() }
}
