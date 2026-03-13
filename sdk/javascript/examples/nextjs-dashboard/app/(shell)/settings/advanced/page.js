import Link from 'next/link'
import PageHeader from '@/components/common/PageHeader'
import SectionCard from '@/components/cards/SectionCard'

export const metadata = { title: 'Advanced settings' }

export default function AdvancedSettingsPage() {
  return (
    <div className="space-y-6">
      <PageHeader
        title="Advanced"
        description="Model, runtime, and debug configuration."
        action={
          <Link href="/settings" className="btn-ghost text-sm">
            Back
          </Link>
        }
      />

      <SectionCard title="Model">
        <div className="space-y-4">
          <div>
            <label className="mb-1.5 block text-xs font-medium text-gray-700">Chat model</label>
            <input
              className="input-field"
              type="text"
              defaultValue="qwen3.5:9b"
              placeholder="e.g. qwen3.5:9b"
            />
          </div>
          <div>
            <label className="mb-1.5 block text-xs font-medium text-gray-700">
              Response timeout (ms)
            </label>
            <input
              className="input-field"
              type="number"
              defaultValue={15000}
              min={1000}
              step={1000}
            />
          </div>
        </div>
      </SectionCard>

      <SectionCard title="Memory &amp; recall">
        <label className="flex cursor-pointer items-center justify-between">
          <span className="text-sm text-gray-700">Enable memory recall</span>
          <input type="checkbox" className="h-4 w-4 rounded accent-accent-600" defaultChecked={false} />
        </label>
      </SectionCard>

      <div className="flex justify-end">
        <button className="btn-primary">Save settings</button>
      </div>
    </div>
  )
}
