import Link from 'next/link'
import PageHeader from '@/components/common/PageHeader'
import SectionCard from '@/components/cards/SectionCard'

export const metadata = { title: 'Settings' }

export default function SettingsPage() {
  return (
    <div className="space-y-6">
      <PageHeader
        title="Settings"
        description="Configure your assistant and preferences."
      />

      <SectionCard title="Advanced">
        <p className="text-sm text-gray-500 mb-3">
          Model configuration, timeout settings, and debug tools.
        </p>
        <Link href="/settings/advanced" className="btn-secondary text-sm">
          Open advanced settings
        </Link>
      </SectionCard>
    </div>
  )
}
