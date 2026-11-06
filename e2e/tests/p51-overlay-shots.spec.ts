import { mkdirSync, writeFileSync } from 'node:fs';
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
import { serveTone, TRACKS } from '../support/playerAudio';

/**
 * D-540 (wave 51, overlay zoom) and the menu layer: the utility drawer with a tool open, a gadget, the pin rail and
 * the player dock at 100 % and 150 % zoom, in Editorial, Lounge Light and Lounge Dark, desktop and phone; a gadget
 * dragged at each zoom must follow the pointer; a Menu opened inside a dialog (the full profile's "More actions")
 * must be the topmost thing under its own items. Local only (SR_P51_OVERLAY=1); shots go to SR_P51_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P51_OUT ?? resolve('test-results/p51-overlay');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const ZOOMS = [100, 150] as const;

test.skip(process.env.SR_P51_OVERLAY !== '1', 'local visual check only');
test.setTimeout(900_000);

const drawer = (page: Page) => page.getByRole('dialog', { name: 'Drawer', exact: true });
const gadget = (page: Page) => page.getByRole('dialog', { name: 'Drawer guide', exact: true });

async function openDrawerByKey(page: Page) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawer(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 30_000 });
}

/** Closes every gadget and unpins every tool, through their own buttons. */
async function clearTools(page: Page) {
  await page.evaluate(() => {
    const press = (sel: string) =>
      document.querySelectorAll<HTMLButtonElement>(sel).forEach((b) => b.click());
    press('.sr-tl-gadget-head button[aria-pressed="true"]');
    press('.sr-tl-gadget-head button:last-of-type');
  });
}

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

test('overlays at 100 % and 150 % across themes, and a menu inside a dialog', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    o,
    '/guilds',
    { name: 'Zoom Booth' },
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
  await apiPost(mctx.request, `/channels/${channelId}/messages`, { content: 'zoom check line' });

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
        await serveTone(page);
        await page.goto(url);
        await page.waitForTimeout(3500);
        // A gadget or pin left from the round before (the layout is synced per account) goes first.
        for (const name of ['Close Drawer guide', 'Unpin from the rail'])
          await page
            .getByRole('button', { name })
            .first()
            .click({ timeout: 2000 })
            .catch(() => {});

        // 1. the drawer with a tool open
        await openDrawerByKey(page);
        await page.keyboard.type('>guide');
        await expect(
          drawer(page).getByRole('tab', { name: 'Tools', selected: true }),
        ).toBeVisible();
        await page.keyboard.press('Enter');
        await page.waitForTimeout(600);
        await page.screenshot({ path: resolve(OUT, `drawer-${tag}.png`) });
        const panel = await drawer(page).boundingBox();
        report.push(`${tag} drawer ${JSON.stringify(panel)}`);
        expect.soft(panel!.y + panel!.height).toBeLessThanOrEqual(size.height + 1);

        // 2. pinned and popped out; the player dock playing; the rail panel open
        const pin = drawer(page).getByRole('button', { name: 'Pin to the rail' });
        if (await pin.isVisible()) await pin.click();
        await drawer(page).getByRole('button', { name: 'Pop out' }).click();
        await expect(gadget(page)).toBeVisible();
        await page.mouse.click(2, 2);
        await page.waitForFunction(
          () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
        );
        await page.evaluate((tracks) => {
          (
            window as unknown as { __srPlayerActions: { play(t: unknown): void } }
          ).__srPlayerActions.play(tracks);
        }, TRACKS);
        await expect(page.locator('.sr-ply-dock')).toBeVisible();
        const zooms = await page.evaluate(() => {
          const z = (s: string) => {
            const e = document.querySelector(s);
            return e ? getComputedStyle(e).zoom : null;
          };
          return {
            gadgets: z('.sr-tl-gadgets'),
            rail: z('.sr-tl-rail-wrap'),
            dock: z('.sr-ply-dock'),
            root: z('.sr-overlay-root'),
          };
        });
        report.push(`${tag} computed zoom ${JSON.stringify(zooms)}`);
        expect.soft(Number(zooms.gadgets)).toBeCloseTo(zoom / 100);
        expect.soft(Number(zooms.dock)).toBeCloseTo(zoom / 100);
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `gadget-dock-${tag}.png`) });

        // the drag follows the pointer (desktop: room to move)
        if (vp === 'desktop') {
          const head = gadget(page).getByRole('toolbar');
          const a = (await head.boundingBox())!;
          const sx = a.x + 40;
          const sy = a.y + a.height / 2;
          await page.mouse.move(sx, sy);
          await page.mouse.down();
          await page.mouse.move(sx + 60, sy + 20, { steps: 4 });
          await page.mouse.move(sx + 120, sy + 40, { steps: 4 });
          const mid = (await head.boundingBox())!;
          await page.mouse.up();
          const b = (await head.boundingBox())!;
          report.push(
            `${tag} drag +120,+40: during ${Math.round(mid.x - a.x)},${Math.round(mid.y - a.y)} after ${Math.round(b.x - a.x)},${Math.round(b.y - a.y)}`,
          );
          expect.soft(Math.abs(b.x - a.x - 120)).toBeLessThanOrEqual(2);
          expect.soft(Math.abs(b.y - a.y - 40)).toBeLessThanOrEqual(2);
          expect.soft(Math.abs(mid.x - a.x - 120)).toBeLessThanOrEqual(2);
        }

        // On a phone the gadget (above the rail, 909 over 908) covers the rail: close it first.
        if (vp === 'phone')
          await page.evaluate(() =>
            document
              .querySelectorAll<HTMLButtonElement>('.sr-tl-gadget-head button:last-of-type')
              .forEach((b) => b.click()),
          );
        const dock = await page.locator('.sr-ply-dock').boundingBox();
        report.push(`${tag} dock ${JSON.stringify(dock)}`);
        expect.soft(dock!.x).toBeGreaterThanOrEqual(0);
        expect.soft(dock!.x + dock!.width).toBeLessThanOrEqual(size.width + 1);
        const rail = page.getByRole('toolbar', { name: 'Pinned tools' });
        // On a phone at 150 % the dock (910) can cover the rail (908): press the button itself.
        if (vp === 'phone')
          await rail
            .getByRole('button')
            .first()
            .evaluate((b) => (b as HTMLButtonElement).click());
        else await rail.getByRole('button').first().click();
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `rail-panel-${tag}.png`) });
        // D-554: the panel just opened is on top of the gadgets and the dock.
        const panelOnTop = await page.evaluate(() => {
          const p = document.querySelector('.sr-tl-rail-panel');
          if (!p) return null;
          const r = p.getBoundingClientRect();
          const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height * 0.75);
          return {
            onTop: !!hit && p.contains(hit),
            z: getComputedStyle(document.querySelector('.sr-tl-rail-wrap')!).zIndex,
          };
        });
        report.push(`${tag} rail-panel ${JSON.stringify(panelOnTop)}`);
        expect.soft(panelOnTop?.onTop).toBe(true);
        // clean up for the next round (the layout is synced per account): unpin and close
        await clearTools(page);
        await page.waitForTimeout(1500);

        // 3. a Menu inside a dialog: the full profile's "More actions"
        await page.goto(url);
        await page.waitForTimeout(3000);
        await clearTools(page);
        await page.locator('.lng-name-btn', { hasText: 'zoommate' }).first().click();
        await page.getByRole('button', { name: 'View Full Profile' }).click();
        const dialog = page.getByRole('dialog').filter({ hasText: 'zoommate' }).last();
        await expect(dialog).toBeVisible();
        // D-555: a Lounge dialog fits the screen at any zoom (the kit divides its max-height by --ui-zoom).
        const box = await dialog.boundingBox();
        report.push(`${tag} lounge-dialog ${JSON.stringify(box)}`);
        expect.soft(box!.y + box!.height).toBeLessThanOrEqual(size.height + 1);
        await dialog.getByRole('button', { name: /More actions for/ }).click();
        const menu = page.getByRole('menu');
        await expect(menu).toBeVisible();
        const top = await menu.evaluate((m) => {
          const items = m.querySelectorAll('[role^="menuitem"]');
          const r = (items[0] ?? m).getBoundingClientRect();
          const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
          const trigger = document.querySelector('[aria-expanded="true"]');
          const t = trigger?.getBoundingClientRect();
          return {
            onTop: !!hit && m.contains(hit),
            z: getComputedStyle(m).zIndex,
            menuTop: Math.round(m.getBoundingClientRect().top),
            triggerBottom: t ? Math.round(t.bottom) : null,
          };
        });
        report.push(`${tag} menu-in-dialog ${JSON.stringify(top)}`);
        expect.soft(top.onTop).toBe(true);
        await page.screenshot({ path: resolve(OUT, `menu-in-dialog-${tag}.png`) });
        await page.keyboard.press('Escape');
        await c.close();
      }
    }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
  writeFileSync(resolve(OUT, 'report.txt'), report.join('\n') + '\n');
});
