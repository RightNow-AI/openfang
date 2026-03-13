import Link from 'next/link'
import PageHeader from '@/components/common/PageHeader'
import StatusBadge from '@/components/health/StatusBadge'
import { listChannels } from '@/lib/openfang-client'
import { PLATFORMS } from '@/lib/platform-meta'

export const metadata = { title: 'Connect' }

export default async function ConnectPage() {
  const { data } = await listChannels()
  const channelMap = Object.fromEntries(
    (data?.channels ?? []).map((ch) => [ch.name, ch])
  )

  const platforms = PLATFORMS.map((p) => ({
    ...p,
    configured: channelMap[p.id]?.configured ?? false,
    label: channelMap[p.id]?.display_name ?? p.label,
    description: channelMap[p.id]?.description ?? p.description,
    icon: channelMap[p.id]?.icon ?? p.icon,
  }))

  const connected = platforms.filter((p) => p.configured)
  const available = platforms.filter((p) => !p.configured)

  return (
    <div className="space-y-8">
      <PageHeader
        title="Connect"
        description="Add platforms so your assistant can work across the tools you use."
      />

      {connected.length > 0 && (
        <section>
          <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-[color:var(--muted-foreground)]">
            Active connections
          </p>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {connected.map((p) => (
              <PlatformCard key={p.id} platform={p} />
            ))}
          </div>
        </section>
      )}

      {available.length > 0 && (
        <section>
          <p className="mb-3 text-xs font-semibold uppercase tracking-wider text-[color:var(--muted-foreground)]">
            Available to connect
          </p>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            {available.map((p) => (
              <PlatformCard key={p.id} platform={p} />
            ))}
          </div>
        </section>
      )}
    </div>
  )
}

function PlatformCard({ platform }) {
  return (
    <div className="card flex items-center gap-4 p-4">
      <div
        className="flex h-10 w-10 shrink-0 items-center justify-center rounded-xl text-xl"
        style={{ background: 'var(--muted)' }}
      >
        {platform.icon}
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="text-sm font-semibold text-[color:var(--foreground)]">{platform.label}</p>
          {platform.configured && <StatusBadge status="connected" label="Active" />}
        </div>
        <p className="mt-0.5 text-xs text-[color:var(--muted-foreground)] leading-relaxed">{platform.description}</p>
      </div>
      <div className="shrink-0">
        {platform.configured ? (
          <button className="btn-ghost text-xs px-3 py-1.5">Manage</button>
        ) : (
          <Link href={`/connect/${platform.id}`} className="btn-primary text-xs px-3 py-1.5">
            Set up
          </Link>
        )}
      </div>
    </div>
  )
}
