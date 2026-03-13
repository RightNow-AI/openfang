// Static platform metadata for the 5 contracts-defined channels.
// IDs match the channel names in GET /api/channels.
// `configured` comes from the live API — this file is content only.

export const PLATFORMS = [
  {
    id: 'slack',
    label: 'Slack',
    icon: '💬',
    description: 'Receive messages and respond in any channel or DM.',
    steps: [
      { id: 1, title: 'Create a Slack app', detail: 'Go to api.slack.com/apps and create a new app for your workspace.' },
      { id: 2, title: 'Copy your Bot Token', detail: 'Enable the bot scope and copy the xoxb- token from the OAuth section.' },
      { id: 3, title: 'Add the token', detail: 'Paste your Bot Token below to connect OpenFang to Slack.' },
    ],
  },
  {
    id: 'whatsapp',
    label: 'WhatsApp',
    icon: '📱',
    description: 'Turn WhatsApp messages into assistant tasks via the Meta Cloud API.',
    steps: [
      { id: 1, title: 'Create a Meta app', detail: 'Go to developers.facebook.com and create a WhatsApp Business app.' },
      { id: 2, title: 'Get a phone number', detail: 'Add a phone number in the WhatsApp dashboard and copy your number ID.' },
      { id: 3, title: 'Add credentials', detail: 'Enter your Phone Number ID and Access Token below.' },
    ],
  },
  {
    id: 'email',
    label: 'Email',
    icon: '✉️',
    description: 'Let your assistant draft and send replies from your inbox.',
    steps: [
      { id: 1, title: 'Choose your provider', detail: 'Select Gmail, Outlook, or a custom IMAP server.' },
      { id: 2, title: 'Authorise access', detail: 'Grant OpenFang read and send permissions for your inbox.' },
      { id: 3, title: 'Set preferences', detail: 'Choose which folders and labels to monitor.' },
    ],
  },
  {
    id: 'sms',
    label: 'SMS',
    icon: '📨',
    description: 'Send and receive SMS messages via Twilio or a compatible carrier.',
    steps: [
      { id: 1, title: 'Get a Twilio account', detail: 'Sign up at twilio.com and purchase a phone number.' },
      { id: 2, title: 'Copy credentials', detail: 'Copy your Account SID and Auth Token from the Twilio console.' },
      { id: 3, title: 'Enter your number', detail: 'Paste your Twilio phone number and credentials below.' },
    ],
  },
  {
    id: 'discord',
    label: 'Discord',
    icon: '🎮',
    description: 'Connect your assistant to a Discord server or DM channel.',
    steps: [
      { id: 1, title: 'Create a Discord bot', detail: 'Go to discord.com/developers/applications and create a new application.' },
      { id: 2, title: 'Copy the Bot Token', detail: 'Enable the bot and copy the token from the Bot tab.' },
      { id: 3, title: 'Add the bot token', detail: 'Paste your Discord bot token below to finish setup.' },
    ],
  },
]

/** Lookup by channel id */
export function getPlatform(id) {
  return PLATFORMS.find((p) => p.id === id) ?? null
}
