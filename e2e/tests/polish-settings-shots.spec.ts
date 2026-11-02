import { expect, test, type Page } from '@playwright/test';
import { mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { repoRoot } from '../support/cli';

/**
 * XC-VIS-002 (settings polish): screenshots of the settings, admin and developer pages in a forced theme without
 * touching the shared sweep account. The theme is laid over the `GET /users/@me/settings` response in the browser,
 * so other agents' sweeps (which set the theme on the account) cannot race it. Opt-in with SR_E2E_SWEEP=1.
 *
 * POLISH_PATHS: comma list of `name=/path`, `{g}` is the sweep guild id and `{app}` the sweep app id.
 * POLISH_OUT: output folder. POLISH_THEMES: `light,dark` (default both). POLISH_VIEWPORTS: `desktop,phone`.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OUT = process.env.POLISH_OUT ?? resolve(repoRoot, '..', 'visual-sweep-shots', 'settings');
const VIEWPORTS = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEME_IDS = { light: 'lounge-light', dark: 'lounge-editorial' } as const;

function rows(): { name: string; path: string }[] {
  if (!ON) return [];
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as Record<string, string>;
  return (process.env.POLISH_PATHS ?? '')
    .split(',')
    .filter(Boolean)
    .map((pair) => {
      const [name, path] = pair.split('=') as [string, string];
      return {
        name,
        path: path.replace('{g}', world.guildId ?? '').replace('{app}', world.appId ?? ''),
      };
    });
}

async function forceTheme(page: Page, theme: keyof typeof THEME_IDS) {
  await page.route('**/users/@me/settings', async (route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const res = await route.fetch();
    const body = (await res.json()) as { settings?: { user_prefs?: Record<string, unknown> } };
    body.settings = body.settings ?? {};
    body.settings.user_prefs = { ...(body.settings.user_prefs ?? {}), theme: THEME_IDS[theme] };
    await route.fulfill({ response: res, json: body });
  });
}

test.describe('settings polish shots', () => {
  test.skip(!ON, 'opt-in: SR_E2E_SWEEP=1');
  const themes = (process.env.POLISH_THEMES ?? 'light,dark').split(',') as (keyof typeof THEME_IDS)[];
  const viewports = (process.env.POLISH_VIEWPORTS ?? 'desktop,phone').split(',') as (keyof typeof VIEWPORTS)[];
  for (const row of rows())
    for (const theme of themes)
      for (const viewport of viewports)
        test(`${row.name} ${viewport} ${theme}`, async ({ browser }) => {
          const context = await browser.newContext({
            storageState: resolve(STATE, 'owner.json'),
            viewport: VIEWPORTS[viewport],
            colorScheme: theme,
            baseURL: new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin,
          });
          const page = await context.newPage();
          await forceTheme(page, theme);
          await page.goto(row.path);
          await page.waitForLoadState('networkidle').catch(() => undefined);
          await page.waitForTimeout(600);
          mkdirSync(OUT, { recursive: true });
          await page.screenshot({ path: resolve(OUT, `${row.name}-${viewport}-${theme}.png`), fullPage: true });
          expect(page.url()).not.toContain('/login');
          await context.close();
        });
});
