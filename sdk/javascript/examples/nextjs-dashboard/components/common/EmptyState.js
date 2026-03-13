export default function EmptyState({ icon, title, description, action }) {
  return (
    <div className="card flex flex-col items-center justify-center gap-4 px-6 py-16 text-center">
      {icon && (
        <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-gray-100 text-gray-400">
          {icon}
        </div>
      )}
      <div>
        <p className="text-sm font-medium text-gray-900">{title}</p>
        {description && (
          <p className="mt-1 text-sm text-gray-500">{description}</p>
        )}
      </div>
      {action && <div>{action}</div>}
    </div>
  )
}
