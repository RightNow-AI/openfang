export default function PageHeader({ title, description, action }) {
  return (
    <div className="mb-6 flex items-start justify-between gap-4">
      <div>
        <h1 className="text-xl font-semibold tracking-tight" style={{ color: 'var(--foreground)' }}>{title}</h1>
        {description && (
          <p className="mt-1 text-sm" style={{ color: 'var(--muted-foreground)' }}>{description}</p>
        )}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  )
}
