import { mkdirSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-301 (wave 77; D-799), on the real stack: the composer's emoji, GIF and sticker pickers open above the
 * whole composer, their end edge on the composer's end edge (desktop) or inside the screen with 8px to spare
 * (phone). The measurement is "the panel's bottom edge is at or above the composer's top edge"; on the base it is
 * below it (SR_P77_BASE=1 runs only that measurement, and asserts the fault).
 * Local only (SR_P77_PICKER=1); shots go to SR_P77_PICKER_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P77_PICKER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p77-picker';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const FIVE = ['first line', 'second line', 'third line', 'fourth line', 'fifth line'];
const BASE = process.env.SR_P77_BASE === '1';

test.skip(process.env.SR_P77_PICKER !== '1', 'local visual check only');
test.setTimeout(1_800_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

interface Box {
  top: number;
  bottom: number;
  left: number;
  right: number;
}
const box = async (l: Locator): Promise<Box> => {
  const b = (await l.boundingBox())!;
  return { top: b.y, bottom: b.y + b.height, left: b.x, right: b.x + b.width };
};

/** The composer that holds this button (the last one on the page: the thread pane's, when it is open). */
const wrap = (page: Page) => page.locator('.lng-composer-wrap').last();

async function draft(page: Page, lines: string[]) {
  const ta = wrap(page).locator('textarea');
  await ta.fill('');
  await ta.focus();
  for (let i = 0; i < lines.length; i++) {
    if (i > 0) await page.keyboard.press('Shift+Enter');
    await page.keyboard.type(lines[i]!);
  }
  await page.waitForTimeout(150);
}

async function openPicker(page: Page, opener: string, panel: string) {
  await wrap(page).getByRole('button', { name: opener }).click();
  const dialog = page.getByRole('dialog', { name: panel });
  await expect(dialog).toBeVisible();
  await page.waitForTimeout(450);
  return dialog;
}

async function measure(page: Page, dialog: Locator, vp: string, where: string) {
  const p = await box(dialog);
  const c = await box(wrap(page).locator('.lng-composer'));
  const size = page.viewportSize()!;
  const line = `${where}: panel bottom ${p.bottom.toFixed(1)} composer top ${c.top.toFixed(1)} panel right ${p.right.toFixed(1)} composer right ${c.right.toFixed(1)} panel left ${p.left.toFixed(1)}`;
  appendFileSync(resolve(OUT, 'measurements.txt'), `${line}\n`);
  console.log(line);
  return { p, c, size, line, vp };
}

/** The picker ends above the composer; edges as the task says; nothing overflows the page. */
async function expectAbove(page: Page, dialog: Locator, vp: string, where: string) {
  const m = await measure(page, dialog, vp, where);
  expect(m.p.bottom, `${where}: ${m.line}`).toBeLessThanOrEqual(m.c.top + 0.5);
  if (vp === 'desktop') {
    expect(Math.abs(m.p.right - m.c.right), `${where} end edge`).toBeLessThanOrEqual(1);
  } else {
    expect(m.p.left, `${where} phone left`).toBeGreaterThanOrEqual(7.5);
    expect(m.p.right, `${where} phone right`).toBeLessThanOrEqual(m.size.width - 7.5);
  }
  expect(m.p.top, `${where} top on screen`).toBeGreaterThanOrEqual(0);
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(over, `${where} overflow`).toBeLessThanOrEqual(0);
}

async function world(browser: Browser) {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Picker Room' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const starter = await apiPost<{ id: string }>(a, `/channels/${channel}/messages`, {
    content: 'Something to answer',
  });
  const th = await apiPost<{ id: string }>(
    a,
    `/channels/${channel}/messages/${starter.id}/threads`,
    { name: 'Picker thread' },
  );
  await apiPost(a, `/channels/${th.id}/messages`, { content: 'In the thread' });
  for (let i = 0; i < 2; i++) {
    await new Promise((r) => setTimeout(r, 1500));
    await apiPost(a, `/channels/${channel}/messages`, { content: `A message to scroll past ${i}` });
  }
  return { octx, a, guild, channel };
}

test('the base: the emoji picker covers the composer', async ({ browser }) => {
  test.skip(!BASE, 'base measurement only (SR_P77_BASE=1)');
  const { octx, a, guild, channel } = await world(browser);
  await apiPatch(a, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  for (const [vp, size] of Object.entries(VIEWPORTS)) {
    const c = await context(browser, octx, size, 'lounge-editorial');
    const page = await c.newPage();
    await page.goto(`/channels/${guild.id}/${channel}`);
    await expect(wrap(page).locator('textarea')).toBeVisible();
    if (vp === 'desktop') await draft(page, FIVE);
    const dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
    const m = await measure(page, dialog, vp, `BASE ${vp}`);
    await page.screenshot({ path: resolve(OUT, `base-emoji-${vp}.png`) });
    expect(m.p.bottom, `base ${vp} panel bottom below composer top`).toBeGreaterThan(m.c.top);
    await c.close();
  }
  await octx.close();
});

test('the pickers open above the whole composer', async ({ browser }) => {
  test.skip(BASE, 'not in the base run');
  const { octx, a, guild, channel } = await world(browser);
  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(wrap(page).locator('textarea')).toBeVisible();

      // 1. Emoji picker over an empty composer.
      let dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
      await expectAbove(page, dialog, vp, `${tag} emoji empty`);
      await page.screenshot({ path: resolve(OUT, `emoji-empty-${tag}.png`) });
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);

      // 2. Emoji picker over a five-line draft; click through.
      await draft(page, FIVE);
      dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
      await expectAbove(page, dialog, vp, `${tag} emoji five`);
      await page.screenshot({ path: resolve(OUT, `emoji-five-${tag}.png`) });
      const ta = wrap(page).locator('textarea');
      const before = await ta.inputValue();
      await dialog.locator('.lng-emoji-btn').first().click();
      await expect.poll(async () => (await ta.inputValue()).length).toBeGreaterThan(before.length);
      expect(await ta.inputValue(), `${tag} draft kept`).toContain('first line');
      await expect(ta, `${tag} composer keeps focus`).toBeFocused();
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
      await expect(
        wrap(page).getByRole('button', { name: 'Open emoji picker' }),
        `${tag} emoji button focused after Escape`,
      ).toBeFocused();
      // Press on the message list: the picker closes.
      dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
      const cTop = (await box(wrap(page).locator('.lng-composer'))).top;
      await page.mouse.click(4, cTop - 3);
      await expect(dialog).toHaveCount(0);

      // 3. GIF picker over a five-line draft.
      dialog = await openPicker(page, 'Open GIF picker', 'GIF picker');
      await expectAbove(page, dialog, vp, `${tag} gif five`);
      const gifState = (await dialog.innerText()).replace(/\s+/g, ' ').slice(0, 80);
      appendFileSync(resolve(OUT, 'measurements.txt'), `${tag} gif showed: ${gifState}\n`);
      await page.screenshot({ path: resolve(OUT, `gif-five-${tag}.png`) });
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);

      // 4. Sticker picker over a one-line draft.
      await draft(page, ['just one line']);
      dialog = await openPicker(page, 'Open sticker picker', 'Sticker picker');
      await expectAbove(page, dialog, vp, `${tag} sticker one`);
      await page.screenshot({ path: resolve(OUT, `sticker-one-${tag}.png`) });
      await page.keyboard.press('Escape');
      await expect(dialog).toHaveCount(0);
      await ta.fill('');

      // 5. The thread pane: the same Composer.
      await page.locator('.lng-thread-chip', { hasText: 'Picker thread' }).click();
      await page.waitForTimeout(800);
      await draft(page, FIVE.slice(0, 3));
      dialog = await openPicker(page, 'Open emoji picker', 'Emoji picker');
      await expectAbove(page, dialog, vp, `${tag} thread emoji three`);
      await page.screenshot({ path: resolve(OUT, `thread-emoji-${tag}.png`) });
      await c.close();
    }
  }
  await octx.close();
});
