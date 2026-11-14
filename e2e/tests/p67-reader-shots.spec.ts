import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import {
  CHAPTER_ID,
  PAGES,
  dispatch,
  installFakeLibrary,
  newFake,
  progressFrame,
} from '../support/p67-reader-fake';

/**
 * SEC-AUD-207 (wave 67, D-690): the reader's "another device read further" notice and question, in the image
 * reader, Editorial, Lounge Light and Lounge Dark, desktop and phone, with both buttons clicked. The EPUB and PDF
 * readers share the same hook and need a real file (not shot here). Local only (SR_P67_READER=1); the chapter is
 * faked (e2e/support/p67-reader-fake.ts). Shots go to SR_P67_READER_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P67_READER_OUT ?? resolve('test-results/p67-reader');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P67_READER !== '1', 'local visual check only');
test.setTimeout(900_000);

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

test('the notice and the question in the image reader', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);

  for (const theme of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const fake = newFake();
      fake.progress = { page_index: 4, percent: 57, completed: false, updated_at_ms: ++fake.clock };
      const c = await context(browser, octx, size, theme);
      await installFakeLibrary(c, fake);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
      };
      const status = page.getByTestId('reader-status');
      const otherDevice = async (index: number) => {
        fake.progress = {
          page_index: index,
          percent: Math.round((index / (PAGES - 1)) * 100),
          completed: index >= PAGES - 1,
          updated_at_ms: ++fake.clock,
        };
      };

      await page.goto(`/library/r/${CHAPTER_ID}`);
      await expect(status).toContainText(`Page 5 of ${PAGES}`);

      // The notice: the other device went on to page 7. The page stays where it is.
      await otherDevice(6);
      await dispatch(page, 'SR_LIBRARY_UPDATE', progressFrame(fake));
      await expect(page.getByRole('button', { name: 'Jump to page 7' })).toBeVisible();
      await expect(status).toContainText(`Page 5 of ${PAGES}`);
      await shot('notice');
      await page.getByRole('button', { name: 'Jump to page 7' }).click();
      await expect(status).toContainText(`Page 7 of ${PAGES}`);
      await shot('notice-jumped');

      // The question: B turns back from an old position; "Continue from page N".
      await otherDevice(7);
      await page.keyboard.press('ArrowLeft');
      await page.keyboard.press('ArrowLeft');
      await page.keyboard.press('ArrowLeft');
      const dialog = page.getByRole('dialog', { name: 'You read further on another device' });
      await expect(dialog).toBeVisible({ timeout: 10_000 });
      await shot('prompt');
      await dialog.getByRole('button', { name: 'Continue from page 8' }).click();
      await expect(status).toContainText(`Page 8 of ${PAGES}`);
      await shot('prompt-continue');

      // Again; this time "Stay here".
      await otherDevice(7);
      await page.keyboard.press('ArrowLeft');
      await expect(dialog).toBeVisible({ timeout: 10_000 });
      await dialog.getByRole('button', { name: 'Stay here' }).click();
      await expect(dialog).toHaveCount(0);
      await expect.poll(() => fake.progress?.page_index).toBe(6);
      await shot('prompt-stay');

      await c.close();
    }
  }
  await apiPatch(octx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
