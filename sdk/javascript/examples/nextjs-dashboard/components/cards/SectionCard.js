/**
 * SectionCard — a titled content block.
 * Wrap table rows, lists, or any grouped content inside it.
 */
export default function SectionCard({ title, description, action, children, className = '' }) {
  return (
    <section className={`surface rounded-2xl overflow-hidden ${className}`}>
      {(title || action) && (
        <div className="flex items-center justify-between border-b px-5 py-4" style={{ borderColor: 'var(--border)' }}>
          <div>
            <p className="text-base font-semibold text-[color:var(--foreground)]">{title}</p>
            {description && <p className="mt-0.5 text-xs text-[color:var(--muted-foreground)]">{description}</p>}
          </div>
          {action && <div>{action}</div>}
        </div>
      )}
      <div className="px-5 py-4">{children}</div>
    </section>
  )
}
