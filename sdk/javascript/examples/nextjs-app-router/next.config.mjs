/** @type {import('next').NextConfig} */

// NEXT_EXPORT=1 activates a fully-static output for Capacitor (mobile) and
// Electron (desktop) packaging. The server build remains the default so that
// the 80+ API routes continue to work when the app is served by Next.js normally.
const isStaticExport = process.env.NEXT_EXPORT === '1';

// Security headers applied to every HTML page served by Next.js.
// The Rust API daemon (port 50051) sets its own headers via middleware.rs —
// these cover the Next.js frontend (port 3002).
const SECURITY_HEADERS = [
  // Prevent MIME-type sniffing
  { key: 'X-Content-Type-Options', value: 'nosniff' },
  // Deny framing from any origin (clickjacking)
  { key: 'X-Frame-Options', value: 'DENY' },
  // Enable XSS filter in older browsers
  { key: 'X-XSS-Protection', value: '1; mode=block' },
  // Limit referrer to origin only when crossing origins
  { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
  // Deny access to sensitive device APIs
  {
    key: 'Permissions-Policy',
    value: 'camera=(), microphone=(), geolocation=(), payment=()',
  },
  // Prevent base-tag injection and form hijacking; restrict object embeds.
  // script-src includes 'unsafe-inline'/'unsafe-eval' required by Next.js
  // hydration and dev-mode HMR. Tighten to nonce-based in production once
  // a CSP nonce middleware is wired in.
  {
    key: 'Content-Security-Policy',
    value: [
      "default-src 'self'",
      "script-src 'self' 'unsafe-inline' 'unsafe-eval'",
      "style-src 'self' 'unsafe-inline'",
      "img-src 'self' data: blob: https:",
      // Allow WebSocket to the daemon for streaming + hot-reload
      `connect-src 'self' http://127.0.0.1:50051 ws://127.0.0.1:50051 ${process.env.OPENFANG_BASE_URL || ''}`.trim(),
      "font-src 'self' data:",
      "object-src 'none'",
      "base-uri 'self'",
      "form-action 'self'",
      "frame-ancestors 'none'",
    ].join('; '),
  },
];

const nextConfig = {
  // Turbopack needs explicit instruction to bundle the local file: symlink
  transpilePackages: ['@openfang/sdk'],
  env: {
    NEXT_PUBLIC_OPENFANG_BASE_URL:
      process.env.OPENFANG_BASE_URL || 'http://127.0.0.1:50051',
  },

  // Security headers on all pages (static export cannot use headers(), skipped there)
  ...(!isStaticExport && {
    async headers() {
      return [{ source: '/(.*)', headers: SECURITY_HEADERS }];
    },
  }),

  // Static export mode — used by `npm run build:static` for Capacitor/Electron.
  // Outputs to `out/` which is what capacitor.config.ts points `webDir` at.
  ...(isStaticExport && {
    output: 'export',
    trailingSlash: true,
    images: { unoptimized: true },
  }),
};

export default nextConfig;