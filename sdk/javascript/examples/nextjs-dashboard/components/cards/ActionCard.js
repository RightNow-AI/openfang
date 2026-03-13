import Link from 'next/link'

/**
 * ActionCard — a prominent card with an icon, title, description, and a link.
 * Used on the Home page for quick-access actions.
 */
export default function ActionCard({ href, icon, title, description, accent = false }) {
  return (
    <Link
      href={href}
      className="surface block rounded-2xl p-5 transition-transform hover:-translate-y-0.5"
    >
      <div
        className="mb-3 inline-flex h-10 w-10 items-center justify-center rounded-xl text-[color:var(--accent-foreground)]"
        style={{ background: accent ? 'var(--accent)' : 'var(--accent-soft)', color: accent ? 'var(--accent-foreground)' : 'var(--accent)' }}
      >
        {icon}
      </div>
      <div>
        <p className="text-sm font-semibold text-[color:var(--foreground)] transition-colors">
          {title}
        </p>
        {description && (
          <p className="mt-0.5 text-xs leading-relaxed text-[color:var(--muted-foreground)]">{description}</p>
        )}
      </div>
    </Link>
  )
}
