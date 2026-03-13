import PageHeader from '@/components/common/PageHeader'
import ChatWindow from '@/components/chat/ChatWindow'

export const metadata = { title: 'Talk' }

export default function ChatPage() {
  return (
    <div>
      <PageHeader
        title="Talk"
        description="Ask your assistant anything or give it a task."
      />
      <ChatWindow />
    </div>
  )
}
