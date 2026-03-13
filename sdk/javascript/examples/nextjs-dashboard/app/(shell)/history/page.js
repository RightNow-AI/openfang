import PageHeader from '@/components/common/PageHeader'
import SectionCard from '@/components/cards/SectionCard'
import EmptyState from '@/components/common/EmptyState'
import { listAgents, getAgentSession } from '@/lib/openfang-client'

function MessageRow({ msg }) {
  const role = String(msg.role ?? 'user').toLowerCase()
  const isUser = role === 'user'
  return (
    <div
      className="flex items-start gap-3 py-2.5 border-t first:border-0"
      style={{ borderColor: 'var(--border)' }}
    >
      <span
        className="mt-0.5 flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[10px] font-bold"
        style={
          isUser
            ? { background: 'var(--accent-soft)', color: 'var(--accent)' }
            : { background: 'var(--muted)', color: 'var(--muted-foreground)' }
        }
      >
        {isUser ? 'You' : 'AI'}
      </span>
      <div className="flex-1 min-w-0">
        <p className="text-sm leading-relaxed text-[color:var(--foreground)]">{msg.content}</p>
      </div>
    </div>
  )
}

export const metadata = { title: 'History' }

export default async function HistoryPage() {
  const { data: agents } = await listAgents()
  const firstAgent = agents?.[0]
  const { data: session } = firstAgent
    ? await getAgentSession(firstAgent.id)
    : { data: null }

  const messages = session?.messages ?? []
  const isEmpty = messages.length === 0

  return (
    <div className="space-y-6">
      <PageHeader title="History" description="Your conversations with the assistant." />

      {firstAgent && (
        <p className="text-xs text-[color:var(--muted-foreground)]">
          <span className="mr-1">{firstAgent.identity?.emoji ?? '🤖'}</span>
          Session for <strong>{firstAgent.name}</strong>
        </p>
      )}

      {isEmpty ? (
        <EmptyState
          title="No conversation yet"
          description="Start talking with your assistant to see history here."
          icon={
            <svg className="h-6 w-6" fill="none" viewBox="0 0 24 24" strokeWidth={1.5} stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
            </svg>
          }
        />
      ) : (
        <SectionCard title={`Current session · ${messages.length} message${messages.length === 1 ? '' : 's'}`}>
          <div>
            {messages.map((msg, i) => (
              <MessageRow key={i} msg={msg} />
            ))}
          </div>
        </SectionCard>
      )}
    </div>
  )
}
