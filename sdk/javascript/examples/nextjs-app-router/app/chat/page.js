import ChatClient from './ChatClient';

export default function ChatPage({ searchParams }) {
  const agentId = searchParams?.agentId ?? null;
  const agentName = searchParams?.agentName ?? null;
  return <ChatClient agentId={agentId} agentName={agentName} />;
}
