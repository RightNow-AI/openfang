'use client'

import { useState, useRef, useEffect } from 'react'
import { listAgents } from '@/lib/openfang-client'
import { streamChat } from '@/lib/openfang-events'

function Message({ msg }) {
  const isUser = msg.role === 'user'
  return (
    <div className={`flex gap-3 ${isUser ? 'flex-row-reverse' : ''}`}>
      {/* Avatar */}
      <div
        className={[
          'mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full text-xs font-semibold',
          isUser ? 'bg-[color:var(--accent)] text-white' : 'text-[color:var(--muted-foreground)]',
        ].join(' ')}
      style={isUser ? {} : { background: 'var(--muted)' }}
      >
        {isUser ? 'Y' : 'AI'}
      </div>

      {/* Bubble */}
      <div
        className={[
          'max-w-[78%] rounded-2xl px-4 py-2.5 text-sm leading-relaxed',
          isUser
            ? 'rounded-tr-sm bg-[color:var(--accent)] text-white'
            : 'rounded-tl-sm border bg-[color:var(--muted)] text-[color:var(--foreground)]',
        ].join(' ')}
      >
        {msg.content === '…' ? (
          <span className="flex items-center gap-1">
            <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-gray-400 [animation-delay:0ms]" />
            <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-gray-400 [animation-delay:150ms]" />
            <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-gray-400 [animation-delay:300ms]" />
          </span>
        ) : (
          msg.content
        )}
      </div>
    </div>
  )
}

export default function ChatWindow() {
  const [agentId, setAgentId] = useState(null)
  const [agentName, setAgentName] = useState('your assistant')
  const [agentStatus, setAgentStatus] = useState('loading') // 'loading' | 'ready' | 'no-agent' | 'error'
  const [messages, setMessages] = useState([])
  const [input, setInput] = useState('')
  const [sending, setSending] = useState(false)
  const bottomRef = useRef(null)
  const abortRef = useRef(null)

  useEffect(() => {
    listAgents().then(({ data, error }) => {
      if (error || !data || data.length === 0) {
        setAgentStatus(error ? 'error' : 'no-agent')
        setMessages([{
          id: '0',
          role: 'assistant',
          content: error
            ? 'Could not reach OpenFang. Check that the daemon is running.'
            : 'No agents configured yet. Create an agent to get started.',
        }])
        return
      }
      const first = data[0]
      setAgentId(first.id)
      setAgentName(first.name || 'your assistant')
      setAgentStatus('ready')
      setMessages([{
        id: '0',
        role: 'assistant',
        content: `Hello! I\u2019m ${first.name || 'your assistant'}. How can I help you?`,
      }])
    })
    return () => { abortRef.current?.() }
  }, [])

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [messages])

  function handleSend(e) {
    e?.preventDefault()
    const text = input.trim()
    if (!text || sending || !agentId) return

    setInput('')
    setSending(true)

    const userMsg = { id: Date.now().toString(), role: 'user', content: text }
    const placeholderId = `${Date.now()}-ai`
    const placeholder = { id: placeholderId, role: 'assistant', content: '\u2026' }
    setMessages((prev) => [...prev, userMsg, placeholder])

    let accumulated = ''
    const { abort } = streamChat(
      agentId,
      text,
      (chunk) => {
        accumulated += chunk
        setMessages((prev) =>
          prev.map((m) => m.id === placeholderId ? { ...m, content: accumulated } : m)
        )
      },
      () => {
        setSending(false)
        abortRef.current = null
      },
      (errMsg) => {
        setMessages((prev) =>
          prev.map((m) =>
            m.id === placeholderId ? { ...m, content: `Error: ${errMsg}` } : m
          )
        )
        setSending(false)
        abortRef.current = null
      },
    )
    abortRef.current = abort
  }

  return (
    <div className="surface flex h-[calc(100dvh-10rem)] flex-col overflow-hidden rounded-2xl md:h-[calc(100dvh-8rem)]">
      {/* Status bar */}
      <div className="flex items-center gap-2 border-b px-4 py-3" style={{ borderColor: 'var(--border)' }}>
        <span
          className={`h-2 w-2 rounded-full ${
            agentStatus === 'ready' ? 'bg-emerald-500' :
            agentStatus === 'loading' ? 'animate-pulse bg-amber-400' :
            'bg-red-400'
          }`}
        />
        <span className="text-xs text-[color:var(--muted-foreground)]">
          {agentStatus === 'loading' ? 'Connecting\u2026' :
           agentStatus === 'ready' ? agentName :
           agentStatus === 'no-agent' ? 'No agent configured' :
           'Daemon not reachable'}
        </span>
      </div>

      {/* Messages */}
      <div className="scrollbar-none flex-1 space-y-4 overflow-y-auto px-4 py-4">
        {messages.map((msg) => (
          <Message key={msg.id} msg={msg} />
        ))}
        <div ref={bottomRef} />
      </div>

      {/* Input bar */}
      <form
        onSubmit={handleSend}
        className="flex items-end gap-2 border-t px-4 py-3"
        style={{ borderColor: 'var(--border)' }}
      >
        <textarea
          className="input-field min-h-[44px] max-h-32 flex-1 resize-none py-2.5 leading-snug"
          placeholder="Send a message…"
          rows={1}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault()
              handleSend()
            }
          }}
          disabled={sending}
        />
        <button
          type="submit"
          disabled={!input.trim() || sending}
          className="btn-primary h-11 w-11 shrink-0 p-0"
        >
          <svg className="h-4 w-4" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 12 3.269 3.125A59.769 59.769 0 0 1 21.485 12 59.768 59.768 0 0 1 3.27 20.875L5.999 12Zm0 0h7.5" />
          </svg>
        </button>
      </form>
    </div>
  )
}
