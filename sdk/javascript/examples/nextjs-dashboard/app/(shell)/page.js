import Link from 'next/link'
import PageHeader from '@/components/common/PageHeader'
import ActionCard from '@/components/cards/ActionCard'
import SectionCard from '@/components/cards/SectionCard'
import StatusBadge from '@/components/health/StatusBadge'
import { fetchHealth, listAgents, getAgentSession, listChannels } from '@/lib/openfang-client'
import { PLATFORMS } from '@/lib/platform-meta'

function RoleIcon({ role }) {
  const isUser = String(role ?? '').toLowerCase() === 'user'
  if (isUser) {
    return (
      <span
        className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[10px] font-bold text-white"
        style={{ background: 'var(--accent)' }}
      >
        Y
      </span>
    )
  }
  return (
    <span
      className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full text-[10px] font-bold text-[color:var(--muted-foreground)]"
      style={{ background: 'var(--muted)' }}
    >
      AI
    </span>
  )
}

export const metadata = { title: 'Home' }

export default async function HomePage() {
  const [
    { data: health, error: healthErr },
    { data: agents },
    { data: channelsData },
  ] = await Promise.all([fetchHealth(), listAgents(), listChannels()])

  const firstAgent = agents?.[0]
  const { data: session } = firstAgent
    ? await getAgentSession(firstAgent.id)
    : { data: null }

  const daemonStatus = healthErr ? 'offline' : (health?.status === 'ok' ? 'connected' : 'degraded')

  const recentMessages = (session?.messages ?? []).slice(-4).map((m, i) => ({
    id: String(i),
    role: String(m.role ?? 'user').toLowerCase(),
    content: m.content,
  }))

  const channelMap = Object.fromEntries(
    (channelsData?.channels ?? []).map((ch) => [ch.name, ch])
  )
  const platforms = PLATFORMS.map((p) => ({
    ...p,
    configured: channelMap[p.id]?.configured ?? false,
  }))

  // eslint-disable-next-line no-unused-vars
  const statusLabel = daemonStatus === 'connected' ? 'Assistant connected' : daemonStatus === 'degraded' ? 'Needs attention' : 'Offline'

  return (
    <div className="space-y-6">
      {/* Status hero */}
      <div className="card flex flex-col gap-4 p-6 sm:flex-row sm:items-center sm:gap-6">
        <div
          className="flex h-14 w-14 shrink-0 items-center justify-center rounded-2xl"
          style={{ background: 'var(--accent)' }}
        >
          <svg className="h-7 w-7 text-white" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="m3.75 13.5 10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75Z" />
          </svg>
        </div>
        <div className="flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h2 className="text-base font-semibold text-[color:var(--foreground)]">
              {daemonStatus === 'connected' ? 'Your assistant is active' : 'Assistant status'}
            </h2>
            <StatusBadge status={daemonStatus} />
          </div>
          <p className="mt-1 text-sm text-[color:var(--muted-foreground)]">
            {healthErr
              ? 'Cannot reach OpenFang daemon. Make sure it is running.'
              : 'Ready to help with tasks, messages, and planning.'}
          </p>
        </div>
        <Link href="/chat" className="btn-primary shrink-0">
          Start talking
        </Link>
      </div>

      {/* Quick actions */}
      <div>
        <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-[color:var(--muted-foreground)]">Quick actions</p>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <ActionCard
            href="/chat"
            accent
            title="Talk to assistant"
            description="Ask a question or give a task."
            icon={
              <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M8.625 9.75a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0H8.25m4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0H12m4.125 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Zm0 0h-.375m-13.5 3.01c0 1.6 1.123 2.994 2.707 3.227 1.087.16 2.185.283 3.293.369V21l4.184-4.183a1.14 1.14 0 0 1 .778-.332 48.294 48.294 0 0 0 5.83-.498c1.585-.233 2.708-1.626 2.708-3.228V6.741c0-1.602-1.123-2.995-2.707-3.228A48.394 48.394 0 0 0 12 3c-2.392 0-4.744.175-7.043.513C3.373 3.746 2.25 5.14 2.25 6.741v6.018Z" />
              </svg>
            }
          />
          <ActionCard
            href="/connect"
            title="Connect a platform"
            description="Add Slack, WhatsApp, email, and more."
            icon={
              <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M13.19 8.688a4.5 4.5 0 0 1 1.242 7.244l-4.5 4.5a4.5 4.5 0 0 1-6.364-6.364l1.757-1.757m13.35-.622 1.757-1.757a4.5 4.5 0 0 0-6.364-6.364l-4.5 4.5a4.5 4.5 0 0 0 1.242 7.244" />
              </svg>
            }
          />
          <ActionCard
            href="/history"
            title="View history"
            description="Review past conversations and tasks."
            icon={
              <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z" />
              </svg>
            }
          />
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {/* Recent activity */}
        <SectionCard
          title="Recent activity"
          action={
            <Link href="/history" className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
              View all
            </Link>
          }
        >
          {recentMessages.length === 0 ? (
            <p className="text-sm text-[color:var(--muted-foreground)]">No recent messages yet.</p>
          ) : (
            <div className="space-y-3">
              {recentMessages.map((msg) => (
                <div key={msg.id} className="flex items-start gap-3">
                  <RoleIcon role={msg.role} />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs text-[color:var(--foreground)]">{msg.content}</p>
                  </div>
                </div>
              ))}
            </div>
          )}
        </SectionCard>

        {/* Connected platforms */}
        <SectionCard
          title="Connected platforms"
          action={
            <Link href="/connect" className="text-xs font-medium" style={{ color: 'var(--accent)' }}>
              Manage
            </Link>
          }
        >
          <div className="space-y-2">
            {platforms.map((p) => (
              <div key={p.id} className="flex items-center justify-between">
                <span className="text-sm text-[color:var(--foreground)]">{p.label}</span>
                {p.configured ? (
                  <StatusBadge status="connected" label="Active" />
                ) : (
                  <Link
                    href={`/connect/${p.id}`}
                    className="text-xs font-medium"
                    style={{ color: 'var(--accent)' }}
                  >
                    Set up
                  </Link>
                )}
              </div>
            ))}
          </div>
        </SectionCard>
      </div>
    </div>
  )
}
