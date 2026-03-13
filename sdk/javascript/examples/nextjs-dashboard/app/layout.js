import { Inter } from 'next/font/google'
import './globals.css'
import { ThemeProvider } from '@/components/theme/ThemeProvider'

const inter = Inter({
  subsets: ['latin'],
  display: 'swap',
  variable: '--font-inter',
})

// Runs before React hydrates — eliminates flash of wrong theme
const themeBootScript = `
(function() {
  try {
    var saved = localStorage.getItem('openfang-theme') || 'system';
    var resolved = saved === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : saved;
    document.documentElement.classList.toggle('dark', resolved === 'dark');
    document.documentElement.setAttribute('data-theme', resolved);
  } catch (e) {}
})();
`

export const metadata = {
  title: { default: 'OpenFang', template: '%s — OpenFang' },
  description: 'Your personal AI operating layer',
}

export const viewport = {
  width: 'device-width',
  initialScale: 1,
  // maximumScale omitted — users need to be able to zoom (WCAG 1.4.4)
  // iOS input-zoom is prevented via font-size ≥ 16px in globals.css
  viewportFit: 'cover', // enables env(safe-area-inset-*) for iPhone notch/home bar
}

export default function RootLayout({ children }) {
  return (
    <html lang="en" className={inter.variable} suppressHydrationWarning>
      <body className="font-sans antialiased" style={{ background: 'var(--background)', color: 'var(--foreground)' }}>
        <script dangerouslySetInnerHTML={{ __html: themeBootScript }} />
        <ThemeProvider>
          {children}
        </ThemeProvider>
      </body>
    </html>
  )
}
