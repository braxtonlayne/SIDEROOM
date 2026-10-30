import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

/**
 * Phase 0 exit-gate e2e (FND-CI-004 / ENT-UI-026).
 *
 * Expects a running stack: Postgres (+ Garage) from deploy/compose/dev.yml, migrated, and `sr-server`
 * listening on 127.0.0.1:8080 with SR_SERVER__PUBLIC_URL=http://localhost:5173 (the API's Origin/CSRF
 * allow-list is the public URL, and the browser talks to the Vite dev server, which proxies /api).
 * `just e2e` does all of that. Playwright itself only starts the Vite dev server.
 *
 * SR_E2E_WEB selects what serves the pages (D-385, SEC-ENT-004):
 *   dev      (default) the Vite dev server on 5173; vite/publicDev.ts serves the public shells by path and asks
 *            sr-server for the gate decision, so a developer sees the production redirects.
 *   server   production topology without Caddy: sr-server serves the built apps/web/dist behind the session gate
 *            (SR_WEB__ROOT, SR_WEB__SERVE_PUBLIC=true) and the browser talks to it directly on port 8080. Build first:
 *            `pnpm --filter @sideroom/web build`. This is what CI's built-bundle leg and the voice soak run.
 *   preview  deprecated, `vite preview` (no gate); kept for local one-off comparisons only.
 */
const webMode = process.env.SR_E2E_WEB ?? 'dev';
const serverPort = process.env.SR_E2E_SERVER_PORT ?? '8080';
const baseURL =
  process.env.SR_E2E_BASE_URL ??
  (webMode === 'server' ? `http://localhost:${serverPort}` : 'http://localhost:5173');
const distRoot = fileURLToPath(new URL('../apps/web/dist', import.meta.url));

// Browser settings for the voice specs, which used to have a config of their own (`playwright.e2ee.config.ts`,
// folded in by OPS-IMM-001: end-to-end encryption is the only voice mode, so every voice spec is an E2EE spec).
// Fake media: Chromium flags, Firefox prefs; WebKit uses its mock capture devices. They change nothing for specs
// that never ask for the microphone or camera.
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
  testDir: './tests',
  globalSetup: './global-setup.ts',
  // One shared database and per-IP rate limits (invite check 10/min): run serially.
  workers: 1,
  fullyParallel: false,
  retries: process.env.CI ? 1 : 0,
  timeout: 120_000,
  expect: { timeout: 10_000 },
  reporter: [['list'], ['html', { open: 'never' }]],
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
  webServer:
    webMode === 'server'
      ? {
          // The server is usually running already (deploy/compose/e2e-run.sh starts it with the same variables, and
          // reuseExistingServer picks it up). Started here it needs the database and storage variables of the
          // environment, like e2e-run.sh sets them.
          command: process.env.SR_SERVER_BIN ?? '../target/debug/sr-server',
          cwd: '.',
          url: `http://127.0.0.1:${serverPort}/readyz`,
          reuseExistingServer: true,
          timeout: 120_000,
          env: {
            SR_WEB__ROOT: process.env.SR_WEB__ROOT ?? distRoot,
            SR_WEB__SERVE_PUBLIC: 'true',
            SR_SERVER__PUBLIC_URL: baseURL,
            SR_SERVER__BIND: `127.0.0.1:${serverPort}`,
          },
        }
      : {
          // SR_E2E_WEB=preview serves the production build (`vite build` first, with VITE_SR_E2E=1) instead of the
          // dev server, without the gate. The voice soak used it (RDY-VOX-002, run 40250340133: ten pages of React's
          // development build starved the runner's CPU); it now runs SR_E2E_WEB=server.
          command:
            webMode === 'preview'
              ? 'pnpm --filter @sideroom/web exec vite preview --host 127.0.0.1 --port 5173 --strictPort'
              : 'pnpm --filter @sideroom/web dev --host 127.0.0.1 --port 5173 --strictPort',
          cwd: '..',
          url: baseURL,
          reuseExistingServer: !process.env.CI,
          timeout: 120_000,
          // Skips the first-run welcome modal (apps/web/src/lounge/shell/Welcome.tsx): every e2e account is new.
          env: { VITE_SR_E2E: '1' },
        },
});
