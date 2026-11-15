import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { serveTone, startPlaying } from '../support/playerAudio';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-305 (wave 78; D-805), on the real stack: on a phone the end of a Cinema, Library or Arcade page scrolls
 * clear of the floating music player docked at the bottom (`--sr-player-clear`, player/pageEnd.ts + pageEnd.css).
 * Local only (SR_P78_DOCK=1); shots go to SR_P78_DOCK_OUT. SR_P78_BASE=1 measures only, with the variable neutralised
 * (`* { --sr-player-clear: 0px !important }`: the layout is then exactly the base's, which has no such rule) and
 * asserts nothing: used to see the base. Each shot reads `data-theme` back from the room's root.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P78_DOCK_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-dock';
const BASE = process.env.SR_P78_BASE === '1';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const NEUTRAL = '* { --sr-player-clear: 0px !important; }';

test.skip(process.env.SR_P78_DOCK !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

interface Box {
  top: number;
  bottom: number;
}
const log: string[] = [];
const report = (line: string) => {
  log.push(line);
  console.log(`REPORT ${line}`);
};

type Actions = Record<string, (v?: string) => void>;
async function play(page: Page, dock = 'bottom-end') {
  await serveTone(page);
  if (!(await page.locator('.sr-ply-dock').count())) await startPlaying(page);
  await page.evaluate((d) => {
    const a = (window as unknown as { __srPlayerActions: Actions }).__srPlayerActions;
    a.setSkin!('retro');
    a.setDock!(d);
  }, dock);
  await expect(page.locator(`.sr-ply-dock[data-dock="${dock}"]`)).toBeVisible();
  await page.waitForTimeout(500);
}

/** Open a page, queue the player on it (or not), neutralise the variable for the base measurement. */
async function open(page: Page, url: string, ready: string, dock: string | null = 'bottom-end') {
  await page.goto(url);
  await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
  if (dock) await play(page, dock);
  if (BASE) await page.addStyleTag({ content: NEUTRAL });
}

/**
 * A long page needs content under the dock: the dev database has no library or arcade catalogue, so a short page
 * gets a tall filler as its last child (its last row is `#p78-last-row`). Said in the handoff.
 */
async function fill(page: Page, container: string) {
  return page.evaluate((sel) => {
    const el = document.querySelector(sel);
    if (!el) return false;
    if (document.documentElement.scrollHeight > window.innerHeight * 1.5) return false;
    const f = document.createElement('div');
    f.style.cssText = 'block-size: 1400px';
    const r = document.createElement('p');
    r.id = 'p78-last-row';
    r.textContent = 'Last row of a long page';
    r.style.cssText = 'margin: 0; padding: 12px 0; font-size: 16px';
    f.appendChild(r);
    el.appendChild(f);
    return true;
  }, container);
}

/** Scroll to the end, read the last content's bottom edge and the dock's top edge. */
async function atEnd(page: Page, lastSel: string) {
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await page.waitForTimeout(400);
  return page.evaluate((sel) => {
    const last = document.querySelector(sel);
    const dock = document.querySelector('.sr-ply-dock');
    const l = last ? last.getBoundingClientRect() : null;
    const d = dock ? dock.getBoundingClientRect() : null;
    return {
      last: l && { top: l.top, bottom: l.bottom },
      dock: d && { top: d.top, bottom: d.bottom },
      clear: document.documentElement.style.getPropertyValue('--sr-player-clear'),
      scrollHeight: document.documentElement.scrollHeight,
      innerHeight: window.innerHeight,
    };
  }, lastSel) as Promise<{
    last: Box | null;
    dock: Box | null;
    clear: string;
    scrollHeight: number;
    innerHeight: number;
  }>;
}
const rnd = (n: number | undefined) => (n === undefined ? n : Math.round(n));
const theme = (page: Page, root: string) => page.locator(root).first().getAttribute('data-theme');
const readClear = (page: Page) =>
  page.evaluate(() => document.documentElement.style.getPropertyValue('--sr-player-clear'));
const act = (page: Page, name: string, v?: string) =>
  page.evaluate(
    ([n, a]) => (window as unknown as { __srPlayerActions: Actions }).__srPlayerActions[n!]!(a),
    [name, v] as [string, string | undefined],
  );

test('the end of a phone page scrolls clear of the bottom player', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });
  const shipped = (): Prefs => ({
    theme: 'lounge-dark',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom' },
  });
  const light = (room: 'cinema' | 'library'): Prefs => ({
    theme: 'lounge-dark',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', [room]: 'lounge-light' },
  });
  await apiPost(octx.request, '/guilds', { name: 'Page End' });

  async function phone(prefs: Prefs) {
    await setPrefs(prefs);
    const c = await context(browser, octx, PHONE, prefs);
    const p = await c.newPage();
    p.setDefaultTimeout(20_000);
    return { c, p };
  }

  // ---- Cinema, phone.
  const cin: Array<[string, Prefs, string | null, string]> = [
    ['bottom-end', shipped(), 'bottom-end', 'cinema-phone-bottom-end'],
    ['bottom-start', shipped(), 'bottom-start', 'cinema-phone-bottom-start'],
    ['top-end', shipped(), 'top-end', 'cinema-phone-top-end'],
    ['no-track', shipped(), null, 'cinema-phone-no-track'],
    ['light bottom-end', light('cinema'), 'bottom-end', 'cinema-phone-bottom-end-light'],
  ];
  for (const [label, prefs, dock, file] of cin) {
    const { c, p } = await phone(prefs);
    await open(p, '/cinema', '.cin-footer', dock);
    const m = await atEnd(p, '.cin-footer-link:last-of-type');
    const th = await theme(p, '.cin-root');
    report(
      `cinema ${label} theme=${th} clear='${m.clear}' last.bottom=${rnd(m.last?.bottom)} dock.top=${rnd(m.dock?.top)} scrollH=${m.scrollHeight} innerH=${m.innerHeight}`,
    );
    const bottom = !!dock?.startsWith('bottom');
    if (bottom && BASE)
      report(`  base: last link ${m.last!.bottom > m.dock!.top ? 'UNDER' : 'above'} the dock`);
    if (bottom && !BASE)
      expect(m.last!.bottom, `cinema ${label}: last link above the dock`).toBeLessThanOrEqual(
        m.dock!.top + 0.5,
      );
    if (!bottom && !BASE) expect(m.clear, `cinema ${label}: no clearance`).toBe('');
    await p.screenshot({ path: resolve(OUT, `${file}.png`) });

    if (label === 'bottom-end' && !BASE) {
      // Click through: the footer's last link with the player open; dock to the top and back; close the player.
      await p.locator('.cin-footer-link').last().click({ trial: true, timeout: 5000 });
      report('click through: the footer link receives a press with the player open');
      await p.locator('.cin-footer-link').last().click({ timeout: 5000 });
      await expect(p).toHaveURL(/house-notes/);
      await p.goBack();
      await expect(p.locator('.cin-footer')).toBeVisible();
      const first = await readClear(p);
      await act(p, 'setDock', 'top-end');
      await expect.poll(() => readClear(p)).toBe('');
      await act(p, 'setDock', 'bottom-end');
      await expect.poll(() => readClear(p)).toBe(first);
      report(`dock top-end: absent; back to bottom-end: '${first}'`);
      await act(p, 'clear');
      await expect(p.locator('.sr-ply-dock')).toHaveCount(0);
      await expect.poll(() => readClear(p)).toBe('');
      report('player closed: property absent');
    }
    await c.close();
  }

  // ---- Library, phone (home and Browse, shipped; home light).
  const lib: Array<[string, string, Prefs, string]> = [
    ['home shipped', '/library', shipped(), 'library-home-phone-shipped'],
    ['home light', '/library', light('library'), 'library-home-phone-light'],
    ['browse shipped', '/library/browse', shipped(), 'library-browse-phone-shipped'],
  ];
  for (const [label, url, prefs, file] of lib) {
    const { c, p } = await phone(prefs);
    await open(p, url, '.lib-bottomnav');
    const filled = await fill(p, '.lib-content');
    const m = await atEnd(p, filled ? '#p78-last-row' : '.lib-content > *:last-child');
    const nav = await p.evaluate(
      () => document.querySelector('.lib-bottomnav')!.getBoundingClientRect().top,
    );
    const th = await theme(p, '.lib-root');
    report(
      `library ${label} theme=${th} filler=${filled} clear='${m.clear}' last.bottom=${rnd(m.last?.bottom)} dock.top=${rnd(m.dock?.top)} dock.bottom=${rnd(m.dock?.bottom)} nav.top=${rnd(nav)} scrollH=${m.scrollHeight}`,
    );
    if (!BASE) {
      expect(m.last!.bottom, `library ${label}: last row above the dock`).toBeLessThanOrEqual(
        m.dock!.top + 0.5,
      );
      expect(
        m.dock!.bottom,
        `library ${label}: dock above the bottom navigation`,
      ).toBeLessThanOrEqual(nav + 0.5);
    }
    await p.screenshot({ path: resolve(OUT, `${file}.png`) });
    await c.close();
  }

  // ---- Arcade, phone (the member's own theme: the Arcade follows the main theme).
  {
    const { c, p } = await phone(shipped());
    await open(p, '/arcade', '.arc-page');
    const filled = await fill(p, '.arc-page');
    const m = await atEnd(p, filled ? '#p78-last-row' : '.arc-page > *:last-child');
    report(
      `arcade home filler=${filled} clear='${m.clear}' last.bottom=${rnd(m.last?.bottom)} dock.top=${rnd(m.dock?.top)} scrollH=${m.scrollHeight}`,
    );
    if (!BASE)
      expect(m.last!.bottom, 'arcade: last block above the dock').toBeLessThanOrEqual(
        m.dock!.top + 0.5,
      );
    await p.screenshot({ path: resolve(OUT, 'arcade-home-phone.png') });
    await c.close();
  }

  // ---- Desktop: nothing changes (the property is published, no rule applies: no data-mobile, no 40rem query).
  {
    await setPrefs(shipped());
    const c = await context(browser, octx, DESKTOP, shipped());
    const p = await c.newPage();
    p.setDefaultTimeout(20_000);
    await open(p, '/cinema', '.cin-footer');
    const withVar = await atEnd(p, '.cin-footer-link:last-of-type');
    await p.screenshot({ path: resolve(OUT, 'cinema-desktop-bottom-end.png') });
    await p.addStyleTag({ content: NEUTRAL });
    const without = await atEnd(p, '.cin-footer-link:last-of-type');
    report(
      `cinema desktop clear='${withVar.clear}' scrollH with=${withVar.scrollHeight} without=${without.scrollHeight}`,
    );
    if (!BASE) expect(withVar.scrollHeight).toBe(without.scrollHeight);
    await c.close();
  }

  // ---- Lounge, phone, Friends, bottom-end: unchanged against wave 77 (dock 511-747).
  {
    const { c, p } = await phone(shipped());
    await open(p, '/channels/@me', '.lng-tabbar');
    const d = await p.evaluate(() => {
      const b = document.querySelector('.sr-ply-dock')!.getBoundingClientRect();
      return { top: b.top, bottom: b.bottom };
    });
    report(`friends phone dock ${rnd(d.top)}-${rnd(d.bottom)} (wave 77: 511-747)`);
    await p.screenshot({ path: resolve(OUT, 'friends-phone-bottom-end.png') });
    await c.close();
  }

  await setPrefs(shipped());
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
