/**
 * StatusBadge — inline pill for connected / needs attention / offline states.
 * Uses alpha-based color classes so it reads cleanly on both light and dark surfaces.
 *
 * status: 'connected' | 'degraded' | 'offline'
 * size: 'sm' (default) | 'lg'
 */

const config = {
  connected: {
    dot: 'bg-emerald-500',
    cls: 'bg-emerald-500/15 text-emerald-400 border border-emerald-500/20',
    label: 'Connected',
  },
  degraded: {
    dot: 'bg-amber-500',
    cls: 'bg-amber-500/15 text-amber-400 border border-amber-500/20',
    label: 'Needs attention',
  },
  offline: {
    dot: 'bg-red-500',
    cls: 'bg-red-500/15 text-red-400 border border-red-500/20',
    label: 'Offline',
  },
}

export default function StatusBadge({ status = 'offline', label, size = 'sm' }) {
  const c = config[status] ?? config.offline
  const displayLabel = label ?? c.label

  if (size === 'lg') {
    return (
      <div className={`inline-flex items-center gap-2 rounded-2xl px-4 py-2 ${c.cls}`}>
        <span className={`h-2 w-2 rounded-full ${c.dot}`} />
        <span className="text-sm font-semibold">{displayLabel}</span>
      </div>
    )
  }

  return (
    <span className={`inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium ${c.cls}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${c.dot}`} />
      {displayLabel}
    </span>
  )
}
