import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';

/**
 * XC-VIS-004 (wave 51): the Lounge full-profile dialog must fit the screen at every zoom (it was 428 px wide on a
 * 375 px phone at 150 %). Opens it in Editorial, Lounge Light and Lounge Dark, desktop and phone, at 100, 150 and
 * 200 %, asserts its box stays inside the viewport, and shoots it. Local only (SR_P51_PDLG=1);
 * shots go to SR_P51_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P51_OUT ?? resolve('test-results/p51-pdlg');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const ZOOMS = [100, 150, 200] as const;

test.skip(process.env.SR_P51_PDLG !== '1', 'local visual check only');
test.setTimeout(900_000);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: { theme: string; zoom: number },
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

test('the full-profile dialog fits the screen at every zoom', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Profile Booth' },
  );
  const channelId = guild.channels.find((c) => c.type === 0)!.id;
  const member = await registerUser(
    (await browser.newContext({ baseURL: ORIGIN })).request,
    'zoommate',
  );
  const mctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(mctx.request, member);
  const invite = await apiPost<{ code: string }>(o, `/channels/${channelId}/invites`, {});
  await apiPost(mctx.request, `/invites/${invite.code}`);
  await apiPost(mctx.request, `/channels/${channelId}/messages`, { content: 'profile check line' });

  const report: string[] = [];
  const url = `/channels/${guild.id}/${channelId}`;
  for (const theme of THEMES)
    for (const zoom of ZOOMS) {
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom } } });
      for (const [vp, size] of Object.entries(VIEWPORTS)) {
        const tag = `${vp}-${theme}-z${zoom}`;
        const c = await context(browser, octx, size, { theme, zoom });
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await page.goto(url);
        await page.waitForTimeout(3000);
        await page.locator('.lng-name-btn', { hasText: 'zoommate' }).first().click();
        // At 200 % on a phone the popout button can sit outside the viewport: press it through the DOM.
        await page
          .getByRole('button', { name: 'View Full Profile' })
          .evaluate((b) => (b as HTMLButtonElement).click());
        const dialog = page.getByRole('dialog').filter({ hasText: 'zoommate' }).last();
        await expect(dialog).toBeVisible();
        await page.waitForTimeout(600);
        const m = await dialog.evaluate((d) => {
          const r = d.getBoundingClientRect();
          let worst = 0;
          d.querySelectorAll('*').forEach((e) => {
            const b = e.getBoundingClientRect();
            if (b.width > 0) worst = Math.max(worst, b.right - r.right, r.left - b.left);
          });
          return {
            left: Math.round(r.left),
            right: Math.round(r.right),
            top: Math.round(r.top),
            bottom: Math.round(r.bottom),
            vw: window.innerWidth,
            vh: window.innerHeight,
            childOverflow: Math.round(worst),
            scrollOverflow: Math.round(d.scrollWidth - d.clientWidth),
          };
        });
        report.push(`${tag} ${JSON.stringify(m)}`);
        expect.soft(m.left, `${tag} left`).toBeGreaterThanOrEqual(-1);
        expect.soft(m.right, `${tag} right`).toBeLessThanOrEqual(m.vw + 1);
        expect.soft(m.bottom, `${tag} bottom`).toBeLessThanOrEqual(m.vh + 1);
        await page.screenshot({ path: resolve(OUT, `profile-${tag}.png`) });
        await c.close();
      }
    }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  writeFileSync(resolve(OUT, 'report.txt'), report.join('\n') + '\n');
});
