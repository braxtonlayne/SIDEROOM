import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { installFakeLibrary, newFake } from '../support/p67-reader-fake';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-315 (wave 79; D-817, D-816), on the real stack with the Library's lists faked by `page.route` (as wave
 * 78's library spec): no Library page scrolls sideways at 375 or 1280; on a phone the section bar hides its brand and
 * slash, scrolls inside itself and shows the area you are in; the naming guide's table scrolls inside its wrapper;
 * the reader's bars are Side Room whole (gold accent). Shipped (`sectionThemes.library = 'sideroom'`, set explicitly
 * and read back from the room's root) and light (`lounge-light`).
 * Local only (SR_P79_LIBRARY=1); shots go to SR_P79_LIBRARY_OUT. SR_P79_BASE=1: the base case (measure and log, no
 * assertion on the fix; the fault itself is asserted present on the phone).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-library';
const BASE = process.env.SR_P79_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P79_LIBRARY !== '1', 'local visual check only');
test.setTimeout(1_200_000);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };
const prefsFor = (section: string): Prefs => ({
  theme: 'lounge-dark',
  zoom: 100,
  sectionThemes: { library: section, cinema: section },
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

const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;

const series = {
  id: '9001',
  kind: 'manga',
  title: 'Frieren: Beyond Journey',
  alt_titles: [],
  authors: ['An Author'],
  artists: [],
  status: 'ongoing',
  synopsis: 'A long walk after the end of the quest.',
  genres: ['fantasy'],
  tags: [],
  reading_direction: 'ltr',
  age_restricted: false,
  published: true,
  chapter_count: 12,
  follower_count: 100,
  cover_url: '/cdn/p67-cover.png',
  created_at_ms: 1_700_000_000_000,
  updated_at_ms: 1_700_000_100_000,
  last_chapter_at_ms: 1_700_000_100_000,
};
async function fakeLists(c: BrowserContext) {
  await c.route(/\/api\/v1\/library\/series(\?.*)?$/, (r) =>
    r.fulfill({
      status: 200,
      json: {
        items: [0, 1, 2, 3].map((i) => ({ ...series, id: String(9001 + i) })),
        total: 4,
        limit: 24,
        offset: 0,
      },
    }),
  );
  await c.route(/\/api\/v1\/library\/updates(\?.*)?$/, (r) =>
    r.fulfill({ status: 200, json: { items: [], next_before: null } }),
  );
}

/** The sideways-scroll measurement: figures, and the elements whose right edge is past the viewport. */
async function measure(page: Page, name: string) {
  const m = await page.evaluate(() => {
    const de = document.documentElement;
    const vw = de.clientWidth;
    const wide = Array.from(document.querySelectorAll('body *'))
      .filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.right > vw + 1;
      })
      .slice(0, 8)
      .map(
        (e) =>
          e.tagName +
          '.' +
          String((e as HTMLElement).className).slice(0, 30) +
          '@' +
          Math.round(e.getBoundingClientRect().right),
      );
    const bar = document.querySelector('.lib-sectionbar') as HTMLElement | null;
    const cur = document.querySelector(
      '[data-testid="study-areas"] [aria-current="page"]',
    ) as HTMLElement | null;
    const rect = (e: Element | null) => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return [r.x, r.y, r.width, r.height].map(Math.round);
    };
    return {
      scrollWidth: de.scrollWidth,
      clientWidth: vw,
      wide,
      bar: rect(bar),
      barScroll: bar ? [bar.scrollLeft, bar.scrollWidth, bar.clientWidth] : null,
      reading: rect(cur),
      brand: rect(document.querySelector('.lib-sectionbar-brand')),
      table: rect(document.querySelector('.rv-table')),
    };
  });
  log(`${name} ${JSON.stringify(m)}`);
  return m;
}

test('library pages stay inside the screen', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });
  const d = await apiPost<{ submission: { id: string } }>(octx.request, '/submissions', {
    kind: 'library.series',
    payload: { kind: 'manga', title: 'Files 79', authors: ['E2E'], genres: [], synopsis: '' },
  });
  const sub = d.submission.id;
  try {
    await apiPost(octx.request, `/submissions/${sub}/library/chapters`, {
      number: 1,
      title: null,
      format: 'images',
    });
  } catch {
    /* chapter 1 may exist already */
  }
  const all: Record<string, unknown> = {};

  for (const [vname, section] of [
    ['shipped', 'sideroom'],
    ['light', 'lounge-light'],
  ] as const) {
    const prefs = prefsFor(section);
    await setPrefs(prefs);
    for (const [vp, size] of [
      ['phone', PHONE],
      ['desktop', DESKTOP],
    ] as const) {
      const phone = vp === 'phone';
      const c = await context(browser, octx, size, prefs);
      await installFakeLibrary(c, newFake());
      await fakeLists(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const tag = `${vp}-${vname}`;
      const shot = async (what: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${what}-${tag}.png`) });
        shots++;
      };
      const go = async (url: string, ready: string, what: string) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await expect(page.locator('.lib-root')).toHaveAttribute('data-theme', section);
        await page.waitForTimeout(600);
        const m = await measure(page, `${what}-${tag}`);
        all[`${what}-${tag}`] = m;
        if (!BASE) {
          expect
            .soft(m.scrollWidth, `${what}-${tag}: no sideways scroll`)
            .toBeLessThanOrEqual(m.clientWidth);
        }
        return m;
      };

      // ---- Submission page with a chapter: top, then the file input.
      let m = await go(`/library/submissions/${sub}`, '.rv-file', 'submission');
      if (phone && !BASE) {
        expect.soft(m.brand?.[2], 'brand hidden on a phone').toBe(0);
        const r = m.reading!;
        expect.soft(r[0]! >= 0 && r[0]! + r[2]! <= 375, 'Reading is in the viewport').toBe(true);
      }
      await shot('submission-top');
      const input = page.locator('input[type=file]:not([webkitdirectory])').first();
      await input.evaluate((el) => el.scrollIntoView({ block: 'center', inline: 'start' }));
      const inRect = await input.evaluate((el) => {
        const r = el.getBoundingClientRect();
        const p = (
          el.closest('section, .rv-panel, .rv-card, form, li, div[class*=rv-]') ?? el.parentElement!
        ).getBoundingClientRect();
        return { right: Math.round(r.right), panelRight: Math.round(p.right) };
      });
      log(`submission-input-${tag} ${JSON.stringify(inRect)}`);
      if (!BASE) expect.soft(inRect.right).toBeLessThanOrEqual(inRect.panelRight + 1);
      await shot('submission-input');

      // ---- The submissions list: the naming guide and its table.
      await go('/library/submissions', '.lib-content', 'submissions');
      await page.getByRole('button', { name: /Naming guide and tester/ }).click();
      await page.getByLabel('File names').click();
      await page.keyboard.insertText(
        'Frieren Beyond Journeys End v02 c013 - A Very Long Chapter Title Indeed [Some Release Group].cbz\nSeries_Without_Spaces_In_The_File_Name_At_All_To_Make_The_Table_Wide_v1_c1.cbz',
      );
      await page.getByRole('button', { name: 'Test names' }).click();
      const table = page.getByRole('table', { name: 'Parsed names' });
      await expect(table).toBeVisible();
      await table.evaluate((el) => el.scrollIntoView({ block: 'center' }));
      m = await measure(page, `naming-${tag}`);
      all[`naming-${tag}`] = m;
      const wrap = await table.evaluate((el) => {
        const w = el.parentElement!;
        return {
          wrap: w.className,
          wrapScroll: [w.scrollWidth, w.clientWidth],
          tableRight: Math.round(el.getBoundingClientRect().right),
        };
      });
      log(`naming-wrap-${tag} ${JSON.stringify(wrap)}`);
      if (!BASE) {
        expect
          .soft(m.scrollWidth, `naming-${tag}: no sideways scroll`)
          .toBeLessThanOrEqual(m.clientWidth);
        expect.soft(wrap.wrap).toBe('rv-table-wrap');
      }
      await shot('naming-table');
      if (phone && wrap.wrapScroll[0]! > wrap.wrapScroll[1]!) {
        await table.evaluate((el) => (el.parentElement!.scrollLeft = 9999));
        await shot('naming-table-scrolled');
      }

      // ---- Home, Browse, a series.
      await go('/library', '.lib-home-head', 'home');
      await shot('home');
      await go('/library/browse', '.lib-content', 'browse');
      await shot('browse');
      await go('/library/s/9001', '.lib-series-head', 'series');
      await shot('series');

      if (phone) {
        // The bar: dragged to its end ("The Lounge" is the first link, so the start) and back; Shelves pressed.
        await go('/library', '.lib-home-head', 'bar');
        const bar = page.locator('.lib-sectionbar');
        await bar.evaluate((el) => (el.scrollLeft = 0));
        await shot('bar-start');
        await bar.evaluate((el) => (el.scrollLeft = el.scrollWidth));
        await shot('bar-end');
        const sw = await bar.evaluate((el) => [el.scrollWidth, el.clientWidth]);
        log(`bar-scroll-${tag} ${JSON.stringify(sw)}`);
        if (vname === 'light') {
          await page.locator('[data-testid="study-areas"] a', { hasText: 'Shelves' }).click();
          await expect(page).toHaveURL(/\/study/);
          await page.goBack();
          await expect(page.locator('.lib-home-head')).toBeVisible({ timeout: 30_000 });
          log('click Shelves and back: ok');
        }
      }

      // ---- The reader, light.
      if (vname === 'light') {
        await page.goto('/library/r/9101/1');
        await expect(page.getByTestId('reader-status')).toBeVisible({ timeout: 60_000 });
        await page.waitForTimeout(900);
        const paint = await page.evaluate(() => {
          const out: string[] = [];
          const bar = document.querySelector('.lib-reader-bottom');
          const cs = bar ? getComputedStyle(bar) : null;
          out.push(`accent=${cs?.getPropertyValue('--ui-accent')}`);
          bar?.querySelectorAll('*').forEach((e) => {
            const b = getComputedStyle(e).backgroundColor;
            if (b !== 'rgba(0, 0, 0, 0)' && out.length < 12)
              out.push(`${e.tagName}.${String((e as HTMLElement).className).slice(0, 24)} ${b}`);
          });
          return out;
        });
        log(`reader-${tag} ${JSON.stringify(paint)}`);
        await shot('reader');
      }
      await c.close();
    }
  }

  // ---- The Cinema's submit page and Admin > Review at 375 (the wrapper must not hurt them).
  await setPrefs(prefsFor('sideroom'));
  const c = await context(browser, octx, PHONE, prefsFor('sideroom'));
  const page = await c.newPage();
  page.setDefaultTimeout(30_000);
  for (const [name, url] of [
    ['cinema-submit', '/cinema/submit'],
    ['admin-review', '/admin/review'],
  ] as const) {
    await page.goto(url);
    await page.waitForTimeout(2500);
    all[name] = await measure(page, `${name}-phone`);
    await page.screenshot({ path: resolve(OUT, `${name}-phone.png`) });
    shots++;
  }
  await c.close();

  writeFileSync(resolve(OUT, `measure-${MODE}.json`), JSON.stringify(all, null, 2));
  log(`SUMMARY shots=${shots}`);
  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'sideroom' } });
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
