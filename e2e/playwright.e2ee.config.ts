import { defineConfig, devices } from '@playwright/test';

/**
 * Voice end-to-end encryption in real browsers (VOX-E2E-003, D-249). Same stack as playwright.config.ts. E2EE is on
 * by default in the web build and in sr-server; this config still sets `VITE_SR_VOICE_E2EE=1`, and the server must
 * run with `SR_VOICE__ENABLED=true` (and `voice.e2ee` left on). Set SR_E2E_VOICE=1 when that server is up.
 *
 *   pnpm --filter @sideroom/e2e exec playwright test -c playwright.e2ee.config.ts --project=chromium
 *
 * Fake media: Chromium flags, Firefox prefs; WebKit uses its mock capture devices.
 */
const baseURL = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';

const chromiumArgs = [
  '--use-fake-device-for-media-stream',
  '--use-fake-ui-for-media-stream',
  // Let local peers connect without mDNS candidate obfuscation.
  '--disable-features=WebRtcHideLocalIpsWithMdns',
  // The dev SFU announces 127.0.0.1.
  '--allow-loopback-in-peer-connection',
];

// These prefs are not enough on their own: Firefox never paired with the dev SFU's 127.0.0.1 candidate in CI
// (ICE failed). Run the server with a routable announce address for Firefox:
//   SR_VOICE__ANNOUNCE_IPS="$(node scripts/ice-announce.mjs)"   (see that script and e2e.yml)
const firefoxUserPrefs = {
  'media.navigator.streams.fake': true,
  'media.navigator.permission.disabled': true,
  'media.peerconnection.ice.loopback': true,
  'media.peerconnection.ice.obfuscate_host_addresses': false,
  'media.peerconnection.ice.no_host': false,
};

export default defineConfig({
  testDir: './e2ee',
  globalSetup: './global-setup.ts',
  workers: 1,
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  timeout: 180_000,
  expect: { timeout: 15_000 },
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-report-e2ee' }]],
  outputDir: 'test-results-e2ee',
  use: {
    baseURL,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], launchOptions: { args: chromiumArgs } },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'], launchOptions: { firefoxUserPrefs } },
    },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: {
    command: 'pnpm --filter @sideroom/web dev --host 127.0.0.1 --port 5173 --strictPort',
    cwd: '..',
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    env: { VITE_SR_E2E: '1', VITE_SR_VOICE_E2EE: '1' },
  },
});
