import { redirect } from 'next/navigation';

/**
 * Root page — redirects to Overview (the app's home dashboard).
 * The chat interface lives at /chat.
 */
export default function RootPage() {
  redirect('/overview');
}

