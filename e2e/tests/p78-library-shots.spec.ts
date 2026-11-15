import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { installFakeLibrary, newFake } from '../support/p67-reader-fake';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-303 (wave 78; D-802), on the real stack with the Library's lists faked by `page.route` (a series list,
 * updates; the one series and its reader come from wave 67's fake): the Library is whole under any theme chosen
 * for it. Shipped (`sectionThemes.library = 'sideroom'`, set explicitly and read back from the room's root) and
 * light (`lounge-light`), desktop and phone, nine pages each, the phone drawer, Lounge Dark, the reader in light.
 * Local only (SR_P78_LIBRARY=1); shots go to SR_P78_LIBRARY_OUT.
 * SR_P78_BASE=1 runs the base case: the faults of the plan are asserted PRESENT (a near-black sidebar and bottom
 * navigation, cream brand) and the computed colours of the shipped theme are written to computed-base-*.json; the
 * run after the change compares the shipped colours with those files and asserts the contrast figures.
 * SR_P78_TAIL=1 adds the other three Lounge themes (home and Browse, desktop).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P78_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-library';
const BASE = process.env.SR_P78_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const SIZES = { desktop: DESKTOP, phone: PHONE } as const;
type SizeName = keyof typeof SIZES;

test.skip(process.env.SR_P78_LIBRARY !== '1', 'local visual check only');
test.setTimeout(2_400_000);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };
const prefsFor = (section: string): Prefs => ({
  theme: 'lounge-dark',
  zoom: 100,
  sectionThemes: { library: section },
});

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

const KINDS = ['manga', 'manhwa', 'manga', 'manhwa', 'manga', 'manhwa'] as const;
const TITLES = [
  'Frieren: Beyond Journey',
  'The Quiet Lantern',
  'Salt and Paper Moons',
  'A Slow Harbour',
  'Ninth Tide Letters',
  'Garden of Small Storms',
];
const series = (i: number) => ({
  id: String(9001 + i),
  kind: KINDS[i]!,
  title: TITLES[i]!,
  alt_titles: [],
  authors: ['An Author'],
  artists: [],
  status: i === 4 ? 'completed' : 'ongoing',
  synopsis:
    'A long walk after the end of the quest. A second sentence that makes the hero text longer than one line, so that it wraps over the art beside it.',
  genres: ['fantasy', 'slice of life'],
  tags: [],
  reading_direction: 'ltr',
  age_restricted: false,
  published: true,
  chapter_count: 12 + i,
  follower_count: 100 - i,
  cover_url: '/cdn/p67-cover.png',
  created_at_ms: 1_700_000_000_000 + i,
  updated_at_ms: 1_700_000_100_000 + i,
  last_chapter_at_ms: 1_700_000_100_000 + i,
});

async function fakeLists(c: BrowserContext) {
  await c.route(/\/api\/v1\/library\/series(\?.*)?$/, (r) =>
    r.fulfill({
      status: 200,
      json: {
        items: TITLES.map((_, i) => series(i)),
        total: 6,
        limit: 24,
        offset: 0,
      },
    }),
  );
  await c.route(/\/api\/v1\/library\/updates(\?.*)?$/, (r) =>
    r.fulfill({
      status: 200,
      json: {
        items: TITLES.slice(0, 4).map((t, i) => ({
          chapter_id: String(9101 + i),
          chapter_number: 36 + i,
          chapter_title: null,
          published_at_ms: Date.now() - (i + 1) * 3_600_000,
          language: 'en',
          release_group: 'Group X',
          series_id: String(9001 + i),
          series_title: t,
          kind: KINDS[i]!,
          age_restricted: false,
          cover_url: '/cdn/p67-cover.png',
        })),
        next_before: null,
      },
    }),
  );
}

/** The computed background and colour of the first match, or null. */
async function computed(page: Page, sel: string) {
  return page.evaluate((s) => {
    const e = document.querySelector(s);
    if (!e) return null;
    const cs = getComputedStyle(e);
    return { bg: cs.backgroundColor, color: cs.color };
  }, sel);
}

/** Text against the colour it sits on (the nearest opaque backgrounds composed); images and gradients are ignored. */
async function contrast(loc: Locator): Promise<number> {
  return loc.evaluate((el) => {
    const parse = (v: string) => {
      const m = /rgba?\(([^)]+)\)/.exec(v);
      if (!m) return [0, 0, 0, 0];
      const p = m[1]!
        .split(/[ ,/]+/)
        .filter(Boolean)
        .map(Number);
      return [p[0]!, p[1]!, p[2]!, p.length > 3 ? p[3]! : 1];
    };
    const layers: number[][] = [];
    for (let n: Element | null = el; n; n = n.parentElement) {
      const b = parse(getComputedStyle(n).backgroundColor);
      if (b[3]! > 0) layers.push(b);
      if (b[3]! >= 0.999) break;
    }
    let bg = [255, 255, 255];
    for (const l of layers.reverse())
      bg = [0, 1, 2].map((i) => l[i]! * l[3]! + bg[i]! * (1 - l[3]!));
    const fg = parse(getComputedStyle(el).color);
    const fgc = [0, 1, 2].map((i) => fg[i]! * fg[3]! + bg[i]! * (1 - fg[3]!));
    const lum = (c: number[]) => {
      const f = (v: number) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(c[0]!) + 0.7152 * f(c[1]!) + 0.0722 * f(c[2]!);
    };
    const a = lum(fgc);
    const b = lum(bg);
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  });
}

const report = (line: string) => console.log(`REPORT ${line}`);

test('the Library under the shipped theme and a light theme chosen for the section', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const sub = (
    await apiPost<{ submission: { id: string } }>(octx.request, '/submissions', {
      kind: 'library.series',
      payload: {
        kind: 'manga',
        title: 'Light Library 78',
        authors: ['E2E'],
        genres: [],
        synopsis: '',
      },
    })
  ).submission.id;
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });

  const variants: Array<[string, string]> = [
    ['shipped', 'sideroom'],
    ['light', 'lounge-light'],
  ];
  const shipped: Record<string, unknown> = {};
  const light: Record<string, unknown> = {};

  for (const [name, section] of variants) {
    const prefs = prefsFor(section);
    await setPrefs(prefs);
    for (const sizeName of Object.keys(SIZES) as SizeName[]) {
      const phone = sizeName === 'phone';
      const c = await context(browser, octx, SIZES[sizeName], prefs);
      await installFakeLibrary(c, newFake());
      await fakeLists(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const tag = `${sizeName}-${name}`;
      const shot = async (what: string) => {
        await page.waitForTimeout(500);
        await page.screenshot({ path: resolve(OUT, `${what}-${tag}.png`) });
      };
      const readBack = async () => {
        await expect(page.locator('.lib-root')).toHaveAttribute('data-theme', section);
      };
      const go = async (url: string, ready: string) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await readBack();
      };

      // ---- Home, top.
      await go('/library', '.lib-home-head');
      await expect(page.locator('.lib-genre-chip').first()).toBeVisible();
      await page.waitForTimeout(800);
      const shell = phone ? '.lib-bottomnav' : '.lib-side';
      const probe = {
        root: await computed(page, '.lib-root'),
        shell: await computed(page, shell),
        brand: await computed(page, '.lib-sectionbar-brand'),
        nav: await computed(page, phone ? '.lib-bottomnav-link' : '.lib-nav-link'),
        chip: await computed(page, '.lib-genre-chip'),
        sidebarInner: await computed(page, phone ? '.lib-topbar' : '.lib-sidebar'),
      };
      (name === 'shipped' ? shipped : light)[sizeName] = probe;
      writeFileSync(
        resolve(OUT, `computed-${MODE}-${name}-${sizeName}.json`),
        JSON.stringify(probe, null, 2),
      );
      report(`${tag} ${JSON.stringify(probe)}`);
      await shot('home');

      if (name === 'light') {
        if (BASE) {
          // The faults of the plan, on the base.
          expect(probe.shell?.bg, `${tag}: the shell is near-black on the base`).toBe(
            'rgb(22, 22, 19)',
          );
          expect(probe.brand?.color, `${tag}: the brand is cream on the base`).toBe(
            'rgb(236, 230, 214)',
          );
        } else {
          expect(probe.shell?.bg, `${tag}: the shell takes the theme`).not.toBe('rgb(22, 22, 19)');
          expect(probe.brand?.color, `${tag}: the brand takes the theme`).not.toBe(
            'rgb(236, 230, 214)',
          );
        }
        const figs: Array<[string, Locator, number]> = [
          ['brand', page.locator('.lib-sectionbar-brand'), 4.5],
          ['hero heading', page.locator('.lib-home-head').getByRole('heading').first(), 3],
          [
            'curiosity heading',
            page.getByRole('heading', { name: 'Follow your curiosity' }).first(),
            3,
          ],
          ['genre chip', page.locator('.lib-genre-chip').first(), 4.5],
          [
            phone ? 'bottom link' : 'sidebar link',
            page.locator(phone ? '.lib-bottomnav-link' : '.lib-nav-link').last(),
            4.5,
          ],
        ];
        const gold = page.locator('.lib-btn-gold').first();
        if (await gold.count()) figs.push(['gold button', gold, 4.5]);
        for (const [what, loc, min] of figs) {
          const r = await contrast(loc);
          report(`${tag} contrast ${what} ${r.toFixed(2)} (min ${min})`);
          if (!BASE) expect(r, `${tag}: ${what} contrast`).toBeGreaterThanOrEqual(min);
        }
      }

      // ---- Home, scrolled to the genre tiles.
      await page
        .getByRole('heading', { name: 'Follow your curiosity' })
        .first()
        .scrollIntoViewIfNeeded();
      await page.mouse.wheel(0, 120);
      await shot('home-curiosity');

      // ---- Browse (kind chip, sort select), the Filters panel.
      await go('/library/browse', '.lib-browse-bar');
      await expect(page.locator('.lib-grid, .lib-list-view').first()).toBeVisible();
      if (name === 'light') {
        const chip = page.locator('.lib-browse-bar button[aria-pressed]').first();
        const sel = page.locator('.lib-select select');
        const r1 = await contrast(chip);
        const r2 = await contrast(sel);
        report(`${tag} contrast kind chip ${r1.toFixed(2)} sort select ${r2.toFixed(2)}`);
        const bg = await chip.evaluate((e) => getComputedStyle(e).backgroundColor);
        report(`${tag} kind chip bg ${bg}`);
        if (!BASE) {
          expect(r1, 'kind chip contrast').toBeGreaterThanOrEqual(4.5);
          expect(r2, 'sort select contrast').toBeGreaterThanOrEqual(4.5);
        }
      }
      await shot('browse');
      await page.locator('.lib-browse-bar button[aria-pressed]').first().click();
      await expect(page.locator('.lib-browse-bar button[aria-pressed="true"]')).toHaveCount(1);
      await shot('browse-kind');
      await page.locator('.lib-browse-bar button[aria-pressed="true"]').click(); // back to all kinds
      await page.getByRole('button', { name: /^Filters/ }).click();
      await expect(page.getByRole('dialog')).toBeVisible();
      if (name === 'light') {
        const dlg = await computed(page, '[role="dialog"]');
        report(`${tag} filters dialog ${JSON.stringify(dlg)}`);
      }
      await shot('browse-filters');
      await page.keyboard.press('Escape');
      await expect(page.getByRole('dialog')).toBeHidden();

      // ---- A series page.
      await go('/library/s/9001', '.lib-series-head');
      await shot('series');

      // ---- Rankings, My library, Updates, a submission.
      await go('/library/rankings', '.lib-h1, h1');
      await shot('rankings');
      await go('/library/me', '.lib-h1, h1');
      await shot('me');
      await go('/library/updates', '.lib-h1, h1');
      await shot('updates');
      await go(`/library/submissions/${sub}`, '.lib-h1, h1');
      await page.waitForTimeout(500);
      await shot('submission');

      // ---- Click through (light): every link of the shell with aria-current following.
      if (name === 'light') {
        await go('/library', '.lib-home-head');
        if (phone) {
          const links = page.locator('.lib-bottomnav-link');
          expect(await links.count()).toBe(4);
          for (let i = 0; i < 4; i++) {
            await links.nth(i).click();
            await expect(links.nth(i)).toHaveAttribute('aria-current', 'page');
          }
          await go('/library', '.lib-home-head');
          await page.getByRole('button', { name: 'Open library menu' }).click();
          await expect(page.getByRole('dialog')).toBeVisible();
          const dlg = await computed(page, '[role="dialog"]');
          report(`${tag} drawer ${JSON.stringify(dlg)}`);
          await shot('drawer');
          await page.keyboard.press('Escape');
        } else {
          const links = page.locator('.lib-side a.lib-nav-link');
          const n = await links.count();
          expect(n).toBeGreaterThan(3);
          for (let i = 0; i < n; i++) {
            const l = page.locator('.lib-side a.lib-nav-link').nth(i);
            await l.click();
            await expect(page.locator('.lib-side a.lib-nav-link').nth(i)).toHaveAttribute(
              'aria-current',
              'page',
            );
          }
        }
        // The reader keeps its own page; its bars in the light theme.
        await page.goto('/library/r/9101/1');
        await expect(page.getByTestId('reader-status')).toBeVisible({ timeout: 60_000 });
        await readBack();
        await page.waitForTimeout(800);
        await shot('reader');
      } else {
        await go('/library', '.lib-home-head');
        if (phone) {
          await page.getByRole('button', { name: 'Open library menu' }).click();
          await expect(page.getByRole('dialog')).toBeVisible();
          await shot('drawer');
          await page.keyboard.press('Escape');
        }
      }
      await c.close();
    }
  }

  // ---- The shipped colours are what they were on the base (written by the base run).
  if (!BASE) {
    for (const sizeName of Object.keys(SIZES)) {
      const f = resolve(OUT, `computed-base-shipped-${sizeName}.json`);
      if (existsSync(f)) {
        const was = JSON.parse(readFileSync(f, 'utf8'));
        expect(shipped[sizeName], `shipped colours unchanged (${sizeName})`).toEqual(was);
        report(`shipped ${sizeName}: computed colours equal to the base`);
      } else report(`shipped ${sizeName}: no base file to compare with`);
    }
  }

  // ---- Lounge Dark as the section theme: one palette.
  {
    const prefs = prefsFor('lounge-dark');
    await setPrefs(prefs);
    const c = await context(browser, octx, DESKTOP, prefs);
    await installFakeLibrary(c, newFake());
    await fakeLists(c);
    const page = await c.newPage();
    page.setDefaultTimeout(30_000);
    await page.goto('/library');
    await expect(page.locator('.lib-genre-chip').first()).toBeVisible({ timeout: 60_000 });
    await expect(page.locator('.lib-root')).toHaveAttribute('data-theme', 'lounge-dark');
    await page.waitForTimeout(800);
    report(
      `lounge-dark shell ${JSON.stringify(await computed(page, '.lib-side'))} brand ${JSON.stringify(await computed(page, '.lib-sectionbar-brand'))}`,
    );
    await page.screenshot({ path: resolve(OUT, 'home-desktop-lounge-dark.png') });
    await c.close();
  }

  await setPrefs(prefsFor('sideroom'));
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});

test('the other Lounge themes as the section theme (tail)', async ({ browser }) => {
  test.skip(process.env.SR_P78_TAIL !== '1', 'tail only');
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  for (const section of ['lounge-ash', 'lounge-onyx', 'lounge-editorial'] as const) {
    const prefs = prefsFor(section);
    await apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: prefs } });
    const c = await context(browser, octx, DESKTOP, prefs);
    await installFakeLibrary(c, newFake());
    await fakeLists(c);
    const page = await c.newPage();
    page.setDefaultTimeout(30_000);
    for (const [what, url, ready] of [
      ['home', '/library', '.lib-genre-chip'],
      ['browse', '/library/browse', '.lib-browse-bar'],
    ] as const) {
      await page.goto(url);
      await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
      await expect(page.locator('.lib-root')).toHaveAttribute('data-theme', section);
      await page.waitForTimeout(800);
      await page.screenshot({ path: resolve(OUT, `tail-${what}-desktop-${section}.png`) });
    }
    await c.close();
  }
  await apiPatch(octx.request, '/users/@me/settings', {
    settings: { user_prefs: prefsFor('sideroom') },
  });
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
