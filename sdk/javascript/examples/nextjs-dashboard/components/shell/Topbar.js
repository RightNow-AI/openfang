'use client'

import { useState, useEffect } from 'react'
import Link from 'next/link'
import ThemeToggle from '@/components/theme/ThemeToggle'
import { fetchHealth } from '@/lib/openfang-client'

const statusConfig = {
  connected: { label: 'Connected',       dot: 'bg-emerald-500', text: 'text-emerald-400', bg: 'bg-emerald-500/10 border border-emerald-500/20' },
  degraded:  { label: 'Needs attention', dot: 'bg-amber-500',   text: 'text-amber-400',   bg: 'bg-amber-500/10 border border-amber-500/20'     },
  offline:   { label: 'Offline',         dot: 'bg-red-500',     text: 'text-red-400',     bg: 'bg-red-500/10 border border-red-500/20'         },
}

export default function Topbar({ onMenuClick }) {
  const [status, setStatus] = useState('offline')

  useEffect(() => {
    fetchHealth().then(({ data, error }) => {
      if (error) { setStatus('offline'); return }
      setStatus(data?.status === 'ok' ? 'connected' : 'degraded')
    })
  }, [])

  const config = statusConfig[status] ?? statusConfig.offline

  return (
    <header
      className="fixed left-0 right-0 top-0 z-30 flex h-16 items-center border-b px-4 backdrop-blur-sm sm:px-6 md:left-60"
      style={{ background: 'color-mix(in srgb, var(--card) 92%, transparent)', borderColor: 'var(--border)' }}
    >
      {/* Hamburger — mobile only */}
      <button
        onClick={onMenuClick}
        className="mr-3 rounded-lg p-1.5 hover:bg-[color:var(--muted)] text-[color:var(--muted-foreground)] md:hidden"
        aria-label="Open menu"
      >
        <svg className="h-5 w-5" fill="none" viewBox="0 0 24 24" strokeWidth={2} stroke="currentColor">
          <path strokeLinecap="round" strokeLinejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5" />
        </svg>
      </button>

      {/* OpenFang wordmark — mobile only (desktop shows it in the sidebar) */}
      <Link href="/" className="flex items-center gap-2 md:hidden">
        <div className="flex h-6 w-6 items-center justify-center rounded-md bg-[color:var(--accent)]">
          <svg className="h-3.5 w-3.5 text-white" fill="none" viewBox="0 0 24 24" strokeWidth={2.5} stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" d="m3.75 13.5 10.5-11.25L12 10.5h8.25L9.75 21.75 12 13.5H3.75Z" />
          </svg>
        </div>
        <span className="text-sm font-semibold text-[color:var(--foreground)]">OpenFang</span>
      </Link>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Right-side actions */}
      <div className="flex items-center gap-3">
        <ThemeToggle />

        {/* Status badge */}
        <div className={`flex items-center gap-1.5 rounded-full px-3 py-1 text-xs font-medium ${config.bg} ${config.text}`}>
          <span className={`h-1.5 w-1.5 rounded-full ${config.dot}`} />
          {config.label}
        </div>

        {/* User avatar placeholder */}
        <button
          className="flex h-8 w-8 items-center justify-center rounded-full text-xs font-semibold transition-colors"
          style={{ background: 'var(--muted)', color: 'var(--muted-foreground)' }}
        >
          Y
        </button>
      </div>
    </header>
  )
}
