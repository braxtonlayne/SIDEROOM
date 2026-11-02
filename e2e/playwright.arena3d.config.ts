import { fileURLToPath } from 'node:url';
import { defineConfig, devices } from '@playwright/test';

/**
 * ACT-A3D-001 (wave 46 gate proof): the arena3d starter needs no sr-server, database or web build. A static server
 * serves the built bundle root (packages/starter-packs/activities/arena3d/dist, `node tools/bundle.mjs`) and the
 * specs in ./arena3d open it directly:
 *   replay.spec.ts  the deterministic replay in Chromium, Firefox and WebKit, against `node replay/run-node.mjs`
 *   smoke.spec.ts   two players (?user=a, ?user=b) over the fake platform see each other in the roster
 *
 *   node packages/starter-packs/activities/arena3d/tools/bundle.mjs
 *   pnpm --filter @sideroom/e2e test:arena3d
 */
const port = Number(process.env.SR_ARENA3D_PORT ?? 8081);
const dist = fileURLToPath(
  new URL('../packages/starter-packs/activities/arena3d/dist', import.meta.url),
);

export default defineConfig({
  testDir: './arena3d',
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  timeout: 120_000,
  expect: { timeout: 60_000 },
  reporter: [['list']],
  use: { baseURL: `http://127.0.0.1:${port}`, trace: 'retain-on-failure' },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
  webServer: {
    command: `node tools/serve-dir.mjs "${dist}" ${port}`,
    cwd: '.',
    url: `http://127.0.0.1:${port}/replay/replay.html`,
    reuseExistingServer: !process.env.CI,
    timeout: 30_000,
  },
});
