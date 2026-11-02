import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { expect, test } from '@playwright/test';
import { repoRoot } from '../support/cli';

/**
 * Polish round (XC-VIS-002, games): the Activities launcher opened from a text channel's composer, in both themes
 * and both viewports, against the sweep world. Opt-in like the sweep (SR_E2E_SWEEP=1); screenshots go to
 * SR_SWEEP_OUT. Read-only: it opens and closes the launcher and starts nothing.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OUT = process.env.SR_SWEEP_OUT ?? resolve(repoRoot, 'e2e', 'test-results', 'polish-games');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

const ADD = 'Add to message';

test.skip(!ON, 'Opt-in: set SR_E2E_SWEEP=1.');

for (const theme of ['light', 'dark'] as const) {
  for (const [vp, size] of [
    ['desktop', { width: 1280, height: 800 }],
    ['phone', { width: 375, height: 812 }],
  ] as const) {
    test(`activity launcher from a text channel, ${vp} ${theme}`, async ({ browser }) => {
      const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as {
        guildId: string;
        channelId: string;
      };
      const context = await browser.newContext({
        baseURL: ORIGIN,
        viewport: size,
        colorScheme: theme,
        storageState: resolve(STATE, 'owner.json'),
        bypassCSP: true,
      });
      await context.addInitScript(
        (t) => {
          try {
            localStorage.setItem(
              'sr.user.prefs',
              JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
            );
          } catch {
            /* no storage */
          }
        },
        theme === 'light' ? 'lounge-light' : 'lounge-editorial',
      );
      const page = await context.newPage();
      await page.goto(`/channels/${world.guildId}/${world.channelId}`);
      const plus = page.getByRole('button', { name: ADD }).first();
      await expect(plus).toBeVisible({ timeout: 20_000 });
      await plus.click();
      await page.getByRole('menuitem', { name: 'Start an Activity' }).click();
      const dialog = page.getByRole('dialog');
      await expect(dialog).toBeVisible();
      await page.waitForTimeout(800);
      await page.screenshot({ path: resolve(OUT, `activity-launcher-${vp}-${theme}.png`) });
      await context.close();
    });
  }
}

test('start an Activity from a text channel, see the panel, end it', async ({ browser }) => {
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as {
    guildId: string;
    channelId: string;
  };
  const context = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 1280, height: 800 },
    storageState: resolve(STATE, 'owner.json'),
    bypassCSP: true,
  });
  const page = await context.newPage();
  await page.goto(`/channels/${world.guildId}/${world.channelId}`);
  await page.getByRole('button', { name: ADD }).first().click();
  await page.getByRole('menuitem', { name: 'Start an Activity' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /^Start / })
    .first()
    .click();
  const panel = page.locator('.act-panel');
  try {
    await expect(panel).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(2500);
    await page.screenshot({ path: resolve(OUT, 'activity-panel-desktop-dark.png') });
    await page.setViewportSize({ width: 375, height: 812 });
    await page.waitForTimeout(1200);
    await page.screenshot({ path: resolve(OUT, 'activity-panel-phone-dark.png') });
  } finally {
    const end = panel.getByRole('button', { name: 'End' });
    if (await end.isVisible().catch(() => false)) await end.click();
    await expect(panel).toBeHidden({ timeout: 15_000 });
    await context.close();
  }
});

test('the Activity docks while you look at another channel and comes back with Return', async ({
  browser,
}) => {
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as {
    guildId: string;
    channelId: string;
  };
  const context = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 1280, height: 800 },
    storageState: resolve(STATE, 'owner.json'),
    bypassCSP: true,
  });
  const page = await context.newPage();
  await page.goto(`/channels/${world.guildId}/${world.channelId}`);
  const panel = page.locator('.act-panel');
  const end = panel.getByRole('button', { name: 'End for everyone' });
  // A run that stopped half-way may have left one running: end it first.
  await page.waitForTimeout(1500);
  if (await end.isVisible().catch(() => false)) {
    await end.click();
    await expect(panel).toBeHidden({ timeout: 15_000 });
  }
  await page.getByRole('button', { name: ADD }).first().click();
  await page.getByRole('menuitem', { name: 'Start an Activity' }).click();
  await page
    .getByRole('dialog')
    .getByRole('button', { name: /^Start / })
    .first()
    .click();
  try {
    await expect(panel).toBeVisible({ timeout: 15_000 });
    // Somewhere else in the Lounge: the window docks with Return / Leave.
    await page
      .getByRole('link', { name: /announcements/ })
      .first()
      .click();
    const win = page.locator('.act-window[data-mode="docked"]');
    await expect(win).toBeVisible({ timeout: 15_000 });
    await page.waitForTimeout(800);
    await page.screenshot({ path: resolve(OUT, 'activity-docked-desktop.png') });
    await win.getByRole('button', { name: 'Return' }).click();
    await expect(panel).toBeVisible({ timeout: 15_000 });
    await expect(page.locator('.act-window[data-mode="full"]')).toBeVisible();
  } finally {
    await page.keyboard.press('Escape');
    if (await end.isVisible().catch(() => false)) await end.click();
    await expect(panel).toBeHidden({ timeout: 15_000 });
    await context.close();
  }
});
