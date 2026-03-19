import type { CapacitorConfig } from '@capacitor/cli';

const config: CapacitorConfig = {
  appId: 'com.openfang.app',
  appName: 'OpenFang',
  // Points at the static export produced by `npm run build:static` (next build
  // with output: 'export').  Run `npm run cap:sync` after every static build.
  webDir: 'out',
  android: {
    backgroundColor: '#050A14',
  },
  ios: {
    backgroundColor: '#050A14',
  },
};

export default config;
