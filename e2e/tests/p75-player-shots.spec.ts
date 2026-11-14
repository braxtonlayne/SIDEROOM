import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';
import { ALL_SKINS, serveTone, setSkin, startPlaying } from '../support/playerAudio';

/**
 * SEC-AUD-295 (wave 75; D-789), on the real stack: the floating music player docked at the bottom sits above the
 * Lounge composer (channel, DM, thread), on desktop and phone, in Editorial, Lounge Light and Lounge Dark. Without a
 * composer (or docked at the top) it is where it was. The "base" half of the check sets the clearance the Lounge
 * writes to 0 and measures the same boxes: they intersect, which is the fault the wave fixes.
 * Local only (SR_P75_PLAYER=1); shots go to SR_P75_PLAYER_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P75_PLAYER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p75-player';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };

test.skip(process.env.SR_P75_PLAYER !== '1', 'local visual check only');
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
    bypassCSP: true,
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
async function boxes(page: Page) {
  return page.evaluate(() => {
    const r = (e: Element) => {
      const b = e.getBoundingClientRect();
      return { top: b.top, bottom: b.bottom, left: b.left, right: b.right };
    };
    const dock = document.querySelector('.sr-ply-dock');
    return {
      dock: (dock ? r(dock) : null) as Box,
      composers: Array.from(document.querySelectorAll('.lng-composer-wrap')).map(r) as Box[],
      vh: window.innerHeight,
      clear: document.documentElement.style.getPropertyValue('--sr-composer-clear'),
    };
  });
}
const hit = (a: Box, b: Box) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;

/** The dock is clear of every composer and whole on screen. */
async function expectClear(page: Page, where: string) {
  const m = await boxes(page);
  expect(m.composers.length, where).toBeGreaterThan(0);
  for (const c of m.composers) {
    expect(hit(m.dock, c), `${where}: dock ${JSON.stringify(m.dock)} on ${JSON.stringify(c)}`).toBe(
      false,
    );
  }
  expect(m.dock.top, `${where}: top`).toBeGreaterThanOrEqual(0);
  expect(m.dock.bottom, `${where}: bottom`).toBeLessThanOrEqual(m.vh);
}

async function play(page: Page, skin = 'retro', dock = 'bottom-end') {
  await serveTone(page);
  await startPlaying(page);
  await page.evaluate(
    ([s, d]) => {
      const a = (
        window as unknown as { __srPlayerActions: Partial<Record<string, (v: string) => void>> }
      ).__srPlayerActions;
      a.setSkin!(s!);
      a.setDock!(d!);
    },
    [skin, dock],
  );
  await expect(page.locator(`.sr-ply-dock[data-dock="${dock}"]`)).toBeVisible();
  await page.waitForTimeout(300);
}

test('the bottom player sits above the composer everywhere, and not otherwise', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Player Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  const starter = await apiPost<{ id: string }>(a, `/channels/${channel}/messages`, {
    content: 'Something to hear while you type',
  });
  const th = await apiPost<{ id: string }>(
    a,
    `/channels/${channel}/messages/${starter.id}/threads`,
    { name: 'Listening thread' },
  );
  await apiPost(a, `/channels/${th.id}/messages`, { content: 'In the thread' });
  const bctx = await browser.newContext({ baseURL: ORIGIN });
  const bUser = await registerUser(bctx.request, 'bob75player');
  await loginApi(bctx.request, bUser);
  const invite = await apiPost<{ code: string }>(a, `/channels/${channel}/invites`, {});
  await apiPost(bctx.request, `/invites/${invite.code}`, {});
  const me = await (await a.get('/api/v1/users/@me')).json();
  const dm = await apiPost<{ id: string }>(bctx.request, '/users/@me/channels', {
    recipient_id: me.id,
  });
  const report: string[] = [];

  for (const theme of THEMES) {
    await apiPatch(a, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);
      await page.goto(`/channels/${guild.id}/${channel}`);
      await expect(page.locator('.lng-composer-wrap textarea')).toBeVisible();
      await play(page);

      // The fault, on the base: with no clearance, the dock lies on the composer.
      const after = await boxes(page);
      await page.evaluate(() =>
        document.documentElement.style.setProperty('--sr-composer-clear', '0px'),
      );
      await page.waitForTimeout(150);
      const base = await boxes(page);
      expect(hit(base.dock, base.composers[0]!), `${tag}: base boxes intersect`).toBe(true);
      await page.evaluate(
        (v) => document.documentElement.style.setProperty('--sr-composer-clear', v),
        after.clear,
      );
      await page.waitForTimeout(150);
      report.push(
        `${tag} base dock ${JSON.stringify(base.dock)} composer ${JSON.stringify(base.composers[0])}; after dock ${JSON.stringify(after.dock)} clear ${after.clear}`,
      );
      await expectClear(page, `${tag} channel`);
      await page.screenshot({ path: resolve(OUT, `channel-${tag}.png`) });

      // Click through: type with the player open, send with the button, open the emoji picker.
      await page.locator('.lng-composer-wrap textarea').fill(`typing under the player ${tag}`);
      await expectClear(page, `${tag} typed`);
      await page.getByRole('button', { name: 'Send message' }).click();
      await expect(
        page.locator('.lng-msg', { hasText: `typing under the player ${tag}` }),
      ).toBeVisible();
      await page.getByRole('button', { name: /Open emoji picker/i }).click();
      await page.waitForTimeout(500);
      await page.screenshot({ path: resolve(OUT, `emoji-${tag}.png`) });
      await page.keyboard.press('Escape');

      // Bottom-start, and a top dock (unchanged).
      await play(page, 'retro', 'bottom-start');
      await expectClear(page, `${tag} bottom-start`);
      await page.screenshot({ path: resolve(OUT, `channel-start-${tag}.png`) });
      await play(page, 'retro', 'top-end');
      const top = await boxes(page);
      expect(Math.round(top.dock.top), `${tag} top dock`).toBe(8);
      await page.screenshot({ path: resolve(OUT, `channel-top-${tag}.png`) });
      await play(page, 'retro', 'bottom-end');

      // A DM, a thread.
      await page.goto(`/channels/@me/${dm.id}`);
      const accept = page.getByRole('button', { name: 'Accept' });
      if (
        await accept.waitFor({ timeout: 6000 }).then(
          () => true,
          () => false,
        )
      )
        await accept.click();
      await expect(page.locator('.lng-composer-wrap textarea')).toBeVisible();
      await play(page);
      await expectClear(page, `${tag} dm`);
      await page.screenshot({ path: resolve(OUT, `dm-${tag}.png`) });
      await page.goto(`/channels/${guild.id}/${channel}`);
      await page.locator('.lng-thread-chip', { hasText: 'Listening thread' }).click();
      await page.waitForTimeout(800);
      await play(page);
      await expectClear(page, `${tag} thread`);
      await page.screenshot({ path: resolve(OUT, `thread-${tag}.png`) });

      // No composer on the page: the dock is where it was.
      await page.goto('/channels/@me');
      await play(page);
      const none = await boxes(page);
      expect(none.composers, `${tag} no composer`).toHaveLength(0);
      expect(none.clear, `${tag} property removed`).toBe('');
      expect(Math.round(none.vh - none.dock.bottom), `${tag} rest offset`).toBe(8);
      await page.screenshot({ path: resolve(OUT, `nocomposer-${tag}.png`) });

      // Every skin clears the composer; the phone shots of each skin (the tallest and the pill among them).
      if (theme === 'lounge-dark') {
        await page.goto(`/channels/${guild.id}/${channel}`);
        await expect(page.locator('.lng-composer-wrap textarea')).toBeVisible();
        await play(page);
        for (const skin of ALL_SKINS) {
          await setSkin(page, skin);
          await page.waitForTimeout(250);
          await expectClear(page, `${tag} skin ${skin}`);
          const m = await boxes(page);
          report.push(
            `${tag} skin ${skin} dock height ${Math.round(m.dock.bottom - m.dock.top)} top ${Math.round(m.dock.top)}`,
          );
          if (vp === 'phone') {
            await page.screenshot({ path: resolve(OUT, `skin-${skin}-${tag}.png`) });
          }
        }
      }
      await c.close();
    }
  }
  console.log(report.join('\n'));
  await bctx.close();
  await octx.close();
});
