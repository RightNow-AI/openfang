/** @type {import('next').NextConfig} */
const nextConfig = {
  // Turbopack needs explicit instruction to bundle the local file: symlink
  transpilePackages: ['@openfang/sdk'],
  env: {
    NEXT_PUBLIC_OPENFANG_BASE_URL:
      process.env.OPENFANG_BASE_URL || 'http://127.0.0.1:50051',
  },
};

export default nextConfig;