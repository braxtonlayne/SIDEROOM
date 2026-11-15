import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { serveTone, startPlaying } from '../support/playerAudio';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-302 (wave 77; D-800), on the real stack: the floating music player docked at the bottom sits above a
 * phone's bottom navigation bar (the Lounge's tab bar on Friends, the Library's bottom navigation), above the
 * composer in a channel (the largest wins: as wave 75 left it), and at 0.5rem on desktop. Also the keyboard (the bar
 * is `display: none`, the clearance must follow) and the D-797 light-Library shot rule.
 * Local only (SR_P77_TABBAR=1); shots go to SR_P77_TABBAR_OUT. SR_P77_TABBAR_MEASURE=1 only measures and logs the
 * boxes (no assertion on the fix): used to see the base.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P77_TABBAR_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p77-tabbar';
const MEASURE = process.env.SR_P77_TABBAR_MEASURE === '1';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P77_TABBAR !== '1', 'local visual check only');
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
  left: number;
  right: number;
}
async function boxes(page: Page, bar: string) {
  return page.evaluate((barSel) => {
    const r = (e: Element | null) => {
      if (!e) return null;
      const b = e.getBoundingClientRect();
      return { top: b.top, bottom: b.bottom, left: b.left, right: b.right };
    };
    return {
      dock: r(document.querySelector('.sr-ply-dock')) as Box,
      bar: r(document.querySelector(barSel)) as Box | null,
      composer: r(document.querySelector('.lng-composer-wrap')) as Box | null,
      vh: window.innerHeight,
      clear: document.documentElement.style.getPropertyValue('--sr-composer-clear'),
    };
  }, bar);
}
const hit = (a: Box, b: Box) =>
  a.left < b.right && b.left < a.right && a.top < b.bottom - 0.5 && b.top < a.bottom - 0.5;
const rounded = (b: Box | null) =>
  b && { top: Math.round(b.top), bottom: Math.round(b.bottom), left: Math.round(b.left) };

type Actions = Record<string, (v: string) => void>;
async function play(page: Page, dock = 'bottom-end') {
  await serveTone(page);
  if (!(await page.locator('.sr-ply-dock').count())) await startPlaying(page);
  await page.evaluate((d) => {
    const a = (window as unknown as { __srPlayerActions: Actions }).__srPlayerActions;
    a.setSkin!('retro');
    a.setDock!(d);
  }, dock);
  await expect(page.locator(`.sr-ply-dock[data-dock="${dock}"]`)).toBeVisible();
  await page.waitForTimeout(400);
}

/** Open a page with the player queued from scratch (a reload clears the queue). */
async function open(page: Page, url: string, ready: string, dock = 'bottom-end') {
  await page.goto(url);
  await expect(page.locator(ready).first()).toBeVisible();
  await play(page, dock);
}

const report: string[] = [];

test('the bottom player clears the phone bottom navigation', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    octx.request,
    '/guilds',
    { name: 'Tab Booth' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  await apiPost(octx.request, `/channels/${channel}/messages`, { content: 'Something to hear' });
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });

  for (const theme of THEMES) {
    const prefs: Prefs = { theme, zoom: 100 };
    await setPrefs(prefs);
    const tag = (s: string) => `${s}-${theme}`;

    // ---- Lounge, phone
    const pc = await context(browser, octx, PHONE, prefs);
    const page = await pc.newPage();
    page.setDefaultTimeout(20_000);

    await open(page, '/channels/@me', '.lng-tabbar');
    let m = await boxes(page, '.lng-tabbar');
    report.push(
      `${tag('friends')} dock ${JSON.stringify(rounded(m.dock))} bar ${JSON.stringify(rounded(m.bar))} clear '${m.clear}' vh ${m.vh} intersects ${hit(m.dock, m.bar!)}`,
    );
    if (!MEASURE) {
      expect(hit(m.dock, m.bar!), `${tag('friends')}: dock on the tab bar`).toBe(false);
      expect(m.dock.bottom, 'friends: dock bottom above bar top').toBeLessThanOrEqual(
        m.bar!.top + 0.5,
      );
    }
    await page.screenshot({ path: resolve(OUT, `friends-phone-${theme}.png`) });

    // Click through: every tab of the bar with the player open.
    const bar = page.locator('.lng-tabbar');
    for (const b of await bar.getByRole('button').all()) {
      await b.click({ timeout: 5000 });
      await page.waitForTimeout(250);
    }
    await bar.getByRole('button').first().click();

    if (theme === 'lounge-dark' && !MEASURE) {
      // The keyboard hides the bar by CSS (--sr-vvh on the root): the clearance must follow, and come back.
      const clear = () => boxes(page, '.lng-tabbar').then((x) => x.clear);
      const before = await clear();
      await page.evaluate(() => document.documentElement.style.setProperty('--sr-vvh', '400px'));
      await expect.poll(clear, { timeout: 3000 }).toBe('0px');
      report.push(`keyboard on Friends: clear ${before} -> 0px (observer fired)`);
      await page.evaluate(() => document.documentElement.style.removeProperty('--sr-vvh'));
      await expect.poll(clear, { timeout: 3000 }).toBe(before);
      report.push(`keyboard off: clear back to ${before}`);
    }

    // Bottom-start.
    await play(page, 'bottom-start');
    m = await boxes(page, '.lng-tabbar');
    if (!MEASURE) expect(hit(m.dock, m.bar!), 'bottom-start on the tab bar').toBe(false);
    await page.screenshot({ path: resolve(OUT, `friends-start-phone-${theme}.png`) });

    // Dock to the top and back.
    await play(page, 'top-end');
    m = await boxes(page, '.lng-tabbar');
    expect(Math.round(m.dock.top), 'top dock').toBe(8);
    await play(page, 'bottom-end');
    m = await boxes(page, '.lng-tabbar');
    if (!MEASURE) expect(hit(m.dock, m.bar!)).toBe(false);

    // A channel: the composer above the tab bar; the figure of wave 75.
    await open(page, `/channels/${guild.id}/${channel}`, '.lng-composer-wrap textarea');
    m = await boxes(page, '.lng-tabbar');
    report.push(
      `${tag('channel')} dock ${JSON.stringify(rounded(m.dock))} composer ${JSON.stringify(rounded(m.composer))} bar ${JSON.stringify(rounded(m.bar))} clear '${m.clear}'`,
    );
    expect(hit(m.dock, m.composer!), 'channel: dock on the composer').toBe(false);
    expect(m.dock.bottom, 'channel: dock bottom above the composer').toBeLessThanOrEqual(
      m.composer!.top + 0.5,
    );
    await page.screenshot({ path: resolve(OUT, `channel-phone-${theme}.png`) });

    if (theme === 'lounge-dark' && !MEASURE) {
      const clear = () => boxes(page, '.lng-tabbar').then((x) => x.clear);
      const before = await clear();
      await page.evaluate(() => document.documentElement.style.setProperty('--sr-vvh', '400px'));
      await page.waitForTimeout(500);
      report.push(`keyboard on channel: clear ${before} -> '${await clear()}'`);
      await page.evaluate(() => document.documentElement.style.removeProperty('--sr-vvh'));
      await page.waitForTimeout(500);
      report.push(`keyboard off channel: '${await clear()}'`);
    }
    await pc.close();

    // ---- Lounge, desktop: nothing changes.
    const dc = await context(browser, octx, DESKTOP, prefs);
    const dp = await dc.newPage();
    dp.setDefaultTimeout(20_000);
    await open(dp, '/channels/@me', '.lng-root');
    m = await boxes(dp, '.lng-tabbar');
    report.push(
      `${tag('friends-desktop')} dock ${JSON.stringify(rounded(m.dock))} clear '${m.clear}'`,
    );
    expect(m.bar, 'no bar on desktop').toBeNull();
    expect(m.clear, 'no clearance on desktop Friends').toBe('');
    expect(Math.round(m.vh - m.dock.bottom), 'rest offset').toBe(8);
    await dp.screenshot({ path: resolve(OUT, `friends-desktop-${theme}.png`) });
    await dc.close();
  }

  // ---- Library, phone: D-797: the shipped theme, and a light theme chosen for the section.
  const variants: Array<[string, Prefs]> = [
    ['shipped', { theme: 'lounge-dark', zoom: 100 }],
    ['light', { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'lounge-light' } }],
  ];
  for (const [name, prefs] of variants) {
    await setPrefs(prefs);
    const lc = await context(browser, octx, PHONE, prefs);
    const lp = await lc.newPage();
    lp.setDefaultTimeout(20_000);
    for (const [label, url] of [
      ['home', '/library'],
      ['browse', '/library/browse'],
    ] as const) {
      await open(lp, url, '.lib-bottomnav');
      const m = await boxes(lp, '.lib-bottomnav');
      report.push(
        `library-${label}-${name} dock ${JSON.stringify(rounded(m.dock))} bar ${JSON.stringify(rounded(m.bar))} clear '${m.clear}' intersects ${hit(m.dock, m.bar!)}`,
      );
      if (!MEASURE) {
        expect(hit(m.dock, m.bar!), `library ${label} ${name}: dock on the bottom nav`).toBe(false);
        expect(m.dock.bottom).toBeLessThanOrEqual(m.bar!.top + 0.5);
      }
      await lp.screenshot({ path: resolve(OUT, `library-${label}-phone-${name}.png`) });
    }
    // Click through: each bottom link with the player open.
    await open(lp, '/library', '.lib-bottomnav');
    const links = lp.locator('.lib-bottomnav-link');
    expect(await links.count()).toBe(4);
    for (let i = 0; i < 4; i++) {
      await links.nth(i).click({ timeout: 5000 });
      await expect(links.nth(i)).toHaveAttribute('aria-current', 'page');
      await lp.waitForTimeout(250);
    }
    await lc.close();
  }
  await setPrefs({ theme: 'lounge-dark', zoom: 100 });

  // ---- The Cinema on a phone: what does the player lie over? (nothing changed there)
  const cc = await context(browser, octx, PHONE, { theme: 'lounge-dark', zoom: 100 });
  const cp = await cc.newPage();
  cp.setDefaultTimeout(20_000);
  await cp.goto('/cinema');
  await cp.waitForTimeout(1500);
  await play(cp);
  const cm = await boxes(cp, '.nothing');
  report.push(
    `cinema-phone dock ${JSON.stringify(rounded(cm.dock))} clear '${cm.clear}' url ${cp.url()}`,
  );
  await cp.screenshot({ path: resolve(OUT, 'cinema-phone.png') });
  await cc.close();

  await setStudySurfacesWith(octx.request, false);
  console.log(report.join('\n'));
  await octx.close();
});
