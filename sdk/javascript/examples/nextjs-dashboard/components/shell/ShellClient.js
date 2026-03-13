'use client'

import { useState } from 'react'
import Sidebar from './Sidebar'
import Topbar from './Topbar'
import MobileNav from './MobileNav'

export default function ShellClient({ children }) {
  const [drawerOpen, setDrawerOpen] = useState(false)

  return (
    <div className="min-h-screen" style={{ background: 'var(--background)' }}>
      {/* Fixed top bar */}
      <Topbar onMenuClick={() => setDrawerOpen(true)} />

      {/* Mobile drawer backdrop */}
      {drawerOpen && (
        <div
          className="fixed inset-0 z-40 backdrop-blur-sm md:hidden"
          style={{ background: 'rgba(0,0,0,0.4)' }}
          onClick={() => setDrawerOpen(false)}
          aria-hidden="true"
        />
      )}

      {/* Sidebar — fixed on desktop, slide-in drawer on mobile */}
      <aside
        className={[
          'fixed bottom-0 left-0 top-0 z-50 w-60 border-r',
          'flex flex-col',
          'transition-transform duration-200 ease-in-out',
          drawerOpen ? 'translate-x-0' : '-translate-x-full',
          'md:translate-x-0',
        ].join(' ')}
        style={{ background: 'var(--card)', borderColor: 'var(--border)' }}
      >
        <Sidebar onClose={() => setDrawerOpen(false)} />
      </aside>

      {/* Page content — pb accounts for mobile bottom nav + iPhone safe area */}
      <main className="shell-main-pb min-h-screen pt-16 md:ml-60">
        <div className="mx-auto max-w-4xl px-4 py-6 sm:px-6">
          {children}
        </div>
      </main>

      {/* Mobile bottom nav */}
      <MobileNav />
    </div>
  )
}
