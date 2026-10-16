import { defineConfig, devices } from '@playwright/test';

/**
 * Phase 0 exit-gate e2e (FND-CI-004 / ENT-UI-026).
 *
 * Expects a running stack: Postgres (+ Garage) from deploy/compose/dev.yml, migrated, and `sr-server`
 * listening on 127.0.0.1:8080 with SR_SERVER__PUBLIC_URL=http://localhost:5173 (the API's Origin/CSRF
 * allow-list is the public URL, and the browser talks to the Vite dev server, which proxies /api).
 * `just e2e` does all of that. Playwright itself only starts the Vite dev server.
 */
const baseURL = process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173';

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
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: {
    command: 'pnpm --filter @sideroom/web dev --host 127.0.0.1 --port 5173 --strictPort',
    cwd: '..',
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
    // Skips the first-run welcome modal (apps/web/src/lounge/shell/Welcome.tsx): every e2e account is new.
    env: { VITE_SR_E2E: '1' },
  },
});
