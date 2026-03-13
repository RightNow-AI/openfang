import Link from 'next/link'
import PageHeader from '@/components/common/PageHeader'
import SectionCard from '@/components/cards/SectionCard'
import StatusBadge from '@/components/health/StatusBadge'
import { fetchHealthDetail, listAgents } from '@/lib/openfang-client'

function mapStatus(apiStatus) {
  if (!apiStatus) return 'offline'
  const s = apiStatus.toLowerCase()
  if (s === 'ok' || s === 'healthy' || s === 'running') return 'connected'
  if (s === 'degraded') return 'degraded'
  return 'connected'
}

const checkIcon = {
  connected: (
    <svg className="h-4 w-4 text-emerald-500" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" d="m4.5 12.75 6 6 9-13.5" />
    </svg>
  ),
  degraded: (
    <svg className="h-4 w-4 text-amber-500" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
    </svg>
  ),
  offline: (
    <svg className="h-4 w-4 text-gray-300" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
      <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
    </svg>
  ),
}

export const metadata = { title: 'Fix' }

export default async function HealthPage() {
  const [{ data: health, error: healthErr }, { data: agents }] = await Promise.all([
    fetchHealthDetail(),
    listAgents(),
  ])

  const overallStatus = healthErr ? 'offline' : mapStatus(health?.status)
  const agentCount = agents?.length ?? health?.agent_count ?? 0
  const uptime = health?.uptime_seconds
  const dbStatus = health?.database === 'connected' ? 'connected' : health?.database ? 'degraded' : (healthErr ? 'offline' : 'connected')

  const checks = [
    {
      id: 'daemon',
      label: 'OpenFang daemon',
      status: healthErr ? 'offline' : 'connected',
      detail: healthErr
        ? `Not reachable: ${healthErr}`
        : `v${health?.version ?? '?'} — responding at ${process.env.NEXT_PUBLIC_OPENFANG_BASE_URL ?? '127.0.0.1:50051'}`,
    },
    {
      id: 'database',
      label: 'Database',
      status: dbStatus,
      detail: health?.database ?? (healthErr ? 'unreachable' : 'ok'),
    },
    {
      id: 'agents',
      label: 'Agents loaded',
      status: agentCount > 0 ? 'connected' : 'degraded',
      detail: `${agentCount} agent${agentCount === 1 ? '' : 's'} available`,
    },
  ]
  if (uptime !== undefined) {
    checks.push({
      id: 'uptime',
      label: 'Uptime',
      status: 'connected',
      detail: uptime < 60
        ? `${uptime}s`
        : uptime < 3600
        ? `${Math.floor(uptime / 60)}m ${uptime % 60}s`
        : `${Math.floor(uptime / 3600)}h ${Math.floor((uptime % 3600) / 60)}m`,
    })
  }

  return (
    <div className="space-y-6">
      <PageHeader title="Fix" description="What needs attention right now." />

      {overallStatus === 'offline' && (
        <div className="rounded-2xl border p-5" style={{ borderColor: 'var(--danger)', background: 'rgba(220,38,38,0.06)' }}>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-xl" style={{ background: 'rgba(220,38,38,0.12)' }}>
              <svg className="h-4 w-4" style={{ color: 'var(--danger)' }} fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
              </svg>
            </div>
            <div className="flex-1">
              <p className="text-sm font-semibold" style={{ color: 'var(--danger)' }}>Daemon not reachable</p>
              <p className="mt-1 text-sm text-[color:var(--muted-foreground)]">{healthErr}</p>
              <p className="mt-3 text-xs text-[color:var(--muted-foreground)]">
                Run <code className="rounded px-1 text-xs" style={{ background: 'var(--muted)' }}>openfang start</code> then reload this page.
              </p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Link href="/settings/advanced" className="btn-secondary text-xs px-3 py-1.5">
                  Open settings
                </Link>
              </div>
            </div>
          </div>
        </div>
      )}

      {overallStatus === 'degraded' && (
        <div className="rounded-2xl border p-5" style={{ borderColor: 'var(--warning)', background: 'rgba(202,138,4,0.06)' }}>
          <div className="flex items-start gap-3">
            <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-xl" style={{ background: 'rgba(202,138,4,0.12)' }}>
              <svg className="h-4 w-4" style={{ color: 'var(--warning)' }} fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 9v3.75m-9.303 3.376c-.866 1.5.217 3.374 1.948 3.374h14.71c1.73 0 2.813-1.874 1.948-3.374L13.949 3.378c-.866-1.5-3.032-1.5-3.898 0L2.697 16.126ZM12 15.75h.007v.008H12v-.008Z" />
              </svg>
            </div>
            <div className="flex-1">
              <p className="text-sm font-semibold" style={{ color: 'var(--warning)' }}>System degraded</p>
              <p className="mt-1 text-sm text-[color:var(--muted-foreground)]">
                Daemon is reachable but something needs attention. Check the system checks below.
              </p>
            </div>
          </div>
        </div>
      )}

      <SectionCard title="Overall status">
        <div className="flex items-center justify-between">
          <p className="text-sm text-[color:var(--muted-foreground)]">
            {healthErr ? 'Cannot reach OpenFang daemon.' : 'OpenFang daemon is running.'}
          </p>
          <StatusBadge status={overallStatus} size="lg" />
        </div>
      </SectionCard>

      <SectionCard title="System checks">
        <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
          {checks.map((check) => (
            <div key={check.id} className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
              <div className="shrink-0">{checkIcon[check.status] ?? checkIcon.offline}</div>
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-[color:var(--foreground)]">{check.label}</p>
                {check.detail && (
                  <p className="text-xs text-[color:var(--muted-foreground)]">{check.detail}</p>
                )}
              </div>
              <StatusBadge status={check.status} />
            </div>
          ))}
        </div>
      </SectionCard>
    </div>
  )
}
