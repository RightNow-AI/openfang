import SetupWizard from '@/components/connect/SetupWizard'

const PLATFORM_NAMES = {
  slack:    'Slack',
  whatsapp: 'WhatsApp',
  email:    'Email',
  calendar: 'Calendar',
  github:   'GitHub',
  notion:   'Notion',
}

export function generateMetadata({ params }) {
  const name = PLATFORM_NAMES[params.platform] ?? params.platform
  return { title: `Set up ${name}` }
}

export default function PlatformSetupPage({ params }) {
  return (
    <div className="flex flex-col items-center pt-4">
      <SetupWizard platform={params.platform} />
    </div>
  )
}
