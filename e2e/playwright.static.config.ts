import { defineConfig, devices } from '@playwright/test';

/**
 * Checks that need only the production web build, no API or database (XC-A11Y-004, XC-PWA, XC-SEC):
 * axe on every public route, the service worker, and the enforced CSP. They run against `vite preview`
 * on port 4173, never on the dev ports other suites share.
 *
 *   pnpm --filter @sideroom/web build && pnpm --filter @sideroom/e2e test:static
 */
const port = Number(process.env.SR_STATIC_PORT ?? 4173);
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './static',
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 60_000,
  reporter: [['list']],
  use: { baseURL, trace: 'retain-on-failure' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: {
    command: `pnpm --filter @sideroom/web exec vite preview --host 127.0.0.1 --port ${port} --strictPort`,
    cwd: '..',
    url: baseURL,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
