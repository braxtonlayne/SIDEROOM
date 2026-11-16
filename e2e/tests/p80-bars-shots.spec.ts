import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import {
  apiPatch,
  apiPost,
  apiPut,
  bootstrapState,
  expect,
  loginApi,
  test,
} from '../support/fixtures';
import { installFakeLibrary, newFake } from '../support/p67-reader-fake';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-320 and 321 (wave 80; D-821, D-822), on the real stack. 320: the Cinema's and the Library's section bars
 * bring the area you are in into view through one hook, without moving the page (the base and the after figures are
 * the same within 1 px). 321: no Arcade or Study page scrolls sideways at 375 or 1280. Rooms with a section theme
 * (Cinema, Library, Arcade) are shot shipped (`sideroom`, set explicitly) and light (`lounge-light`); the Study
 * follows the main theme (Lounge Dark and Lounge Light, set with the account's settings). `data-theme` is read back
 * before each shot. Local only (SR_P80_BARS=1); shots go to SR_P80_BARS_OUT; SR_P80_BASE=1 names the run `base`.
 * This spec measures and logs; the shots are looked at by the builder.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_BARS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-bars';
const MODE = process.env.SR_P80_BASE === '1' ? 'base' : 'after';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;

test.skip(process.env.SR_P80_BARS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };

const log = (line: string) => {
  console.log(`REPORT ${line}`);
  appendFileSync(resolve(OUT, `report-${MODE}.txt`), `${line}\n`);
};

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

const json = (r: Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

/** Sideways scroll of the document, the elements past the right edge, the room links and the area bar. */
async function measure(page: Page) {
  return page.evaluate(() => {
    const de = document.documentElement;
    const iw = de.clientWidth;
    const clipped = (e: Element) => {
      for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
        if (
          getComputedStyle(a).overflowX !== 'visible' &&
          a.getBoundingClientRect().right <= iw + 1
        )
          return true;
      }
      return false;
    };
    const past = Array.from(document.querySelectorAll<HTMLElement>('body *'))
      .filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.right > iw + 1 && !clipped(e);
      })
      .slice(0, 6)
      .map(
        (e) =>
          `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 30)}@${Math.round(e.getBoundingClientRect().right)}`,
      );
    const box = (e: Element | null | undefined) => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return [Math.round(r.left), Math.round(r.right), Math.round(r.top)];
    };
    const rooms = Array.from(document.querySelectorAll('[data-testid="rooms-nav"] a')).map(box);
    const cur = document.querySelector<HTMLElement>(
      '[data-testid="study-areas"] [aria-current="page"]',
    );
    const bar = cur?.closest('header') as HTMLElement | null;
    const themed = document.querySelector('[data-theme]');
    return {
      sw: de.scrollWidth,
      cw: iw,
      docScrollLeft: de.scrollLeft,
      docScrollTop: Math.round(de.scrollTop),
      past,
      rooms,
      area: cur ? { name: cur.textContent, box: box(cur) } : null,
      bar: bar
        ? { box: box(bar), scroll: [bar.scrollLeft, bar.scrollWidth, bar.clientWidth] }
        : null,
      theme: themed?.getAttribute('data-theme') ?? null,
    };
  });
}

test('SEC-AUD-320/321: section bars and sideways scroll', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  // ---- the world: one published title; the Library, the Arcade and the Study are faked by page.route
  const t = await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
    kind: 'movie',
    title: `The Long Corridor ${Date.now().toString(36)}`,
    year: 2001,
    runtime_min: 118,
    content_rating: 'PG',
    synopsis: 'A fixture film for the layout check. Two friends cross a city in one night.',
    genres: ['Drama', 'Mystery'],
    languages: ['en'],
    subtitle_languages: ['en'],
    cast: [],
    directors: [],
    writers: [],
    studios: [],
    crew: [],
    alt_titles: [],
    tags: [],
    age_restricted: false,
    external_ids: {},
  });
  await apiPut(o, `/admin/cinema/titles/${t.id}/asset`, { asset_id: '4242' });
  await apiPost(o, `/admin/cinema/titles/${t.id}/publish`);

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
  const tier1 = {
    browser_play: true,
    guest_links: true,
    featured: true,
    desktop_only: false,
    needs_base_game_hint: false,
  };
  const front = {
    featured: [
      {
        application_id: '900',
        name: 'Land.io',
        description: 'A roguelike for a call.',
        blurb: 'Our flagship',
        tier: 'TIER_1_ORIGINAL',
        eligibility: tier1,
        capabilities: ['datachannel', 'guest_play'],
        tip_links: [{ kind: 'kofi', url: 'https://ko-fi.com/landio' }],
        bundle_sha256: 'a'.repeat(64),
        position: 0,
        featured_at_ms: 1,
      },
    ],
    coming_soon: [
      {
        id: '55',
        title: 'OpenMW',
        summary: 'An open engine in the works.',
        tier: 'TIER_2_REIMPLEMENTATION',
        repo_url: 'https://github.com/OpenMW/openmw',
        notify: false,
        created_at_ms: 3,
      },
    ],
    flags: [],
  };
  const files = [
    'Films/heat.mp4',
    'Music/blue.mp3',
    'Books/dune.epub',
    'Comics/akira-volume-one.cbz',
    'Films/a-very-long-file-name-that-should-wrap-or-truncate-cleanly-on-a-phone.mkv',
    'Docs/notes-from-the-meeting-about-the-quarterly-report-final-v3.pdf',
    'Music/the-longest-album-title-in-the-world-with-no-spaces-at-all-in-it.flac',
  ];
  const fakeAll = async (c: BrowserContext) => {
    await installFakeLibrary(c, newFake());
    await c.route(/\/api\/v1\/library\/series(\?.*)?$/, (r) =>
      json(r, {
        items: [0, 1, 2, 3].map((i) => ({ ...series, id: String(9001 + i) })),
        total: 4,
        limit: 24,
        offset: 0,
      }),
    );
    await c.route(/\/api\/v1\/library\/updates(\?.*)?$/, (r) =>
      json(r, { items: [], next_before: null }),
    );
    await c.route('**/api/v1/arcade', (r) => json(r, front));
    await c.route('**/api/v1/users/@me/games/roms', (r) => json(r, { roms: [] }));
    await c.route('**/api/v1/users/@me/storage/mounts', (r) => json(r, []));
    await c.route('**/api/v1/shelves/visible', (r) =>
      json(r, {
        shelves: [
          {
            shelf_id: '9501',
            label: 'Home',
            via: 'own',
            object_count: files.length,
            can_manage: true,
          },
          {
            shelf_id: '9502',
            label: 'Friend',
            via: 'friend',
            owner_id: '777',
            owner_handle: 'pal',
            object_count: 1,
            can_manage: false,
          },
        ],
      }),
    );
    await c.route('**/api/v1/storage/mounts/*/objects*', (r) => {
      const mount = new URL(r.request().url()).pathname.split('/')[5] ?? '';
      return json(r, {
        folders: [],
        objects: (mount === '9501' ? files : ['Shared/akira.cbz']).map((key, i) => ({
          key,
          size: 1000 * (i + 1),
          last_modified: '2026-10-01T00:00:00Z',
        })),
        next_cursor: null,
        can_upload: false,
        can_manage: mount === '9501',
      });
    });
  };

  let shots = 0;
  const all: Record<string, unknown> = {};

  // ---- Part A: the Cinema's and the Library's bars (the bar scrolls on the phone)
  for (const [variant, section] of [
    ['shipped', 'sideroom'],
    ['light', 'lounge-light'],
  ] as const) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: section, library: section, arcade: section },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      if (sizeName !== 'phone') continue;
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs);
      await fakeAll(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      const go = async (url: string, ready: string, name: string, root: string, want: string) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await expect(page.locator(root)).toHaveAttribute('data-theme', want);
        await page.waitForTimeout(900);
        const m = await measure(page);
        all[`${name}-${tag}`] = m;
        log(`${name}-${tag} ${JSON.stringify(m)}`);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
        shots++;
        return m;
      };
      await go('/cinema', '.cin-sectionbar', 'cinema-home', '.cin-root', section);
      await go(`/cinema/t/${t.id}`, '.cin-title-head', 'cinema-title', '.cin-root', section);
      await go('/library', '.lib-home-head', 'library-home', '.lib-root', section);
      await go('/library/s/9001', '.lib-series-head', 'library-series', '.lib-root', section);

      // Click-through (phone, light): every area link in turn, from the Cinema and from the Library.
      if (variant === 'light') {
        for (const [room, url, ready, link] of [
          ['cinema', '/cinema', '.cin-sectionbar', '.cin-sectionbar-nav + .cin-sectionbar-nav a'],
          ['library', '/library', '.lib-home-head', '[data-testid="study-areas"] a'],
        ] as const) {
          await page.goto(url);
          await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
          const names = (await page.locator(link).allTextContents()).map((n) => n.trim());
          log(`click-${room} areas ${JSON.stringify(names)}`);
          for (const n of names) {
            await page.goto(url);
            await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
            await page.locator(link).filter({ hasText: n }).first().click();
            await page.waitForTimeout(1500);
            const m = await measure(page);
            const inBar = !!m.area?.box && !!m.bar && m.area.box[0]! >= -1 && m.area.box[1]! <= 376;
            log(`click-${room}-${n} ${page.url()} inBar=${inBar} ${JSON.stringify(m)}`);
            all[`click-${room}-${n}`] = { ...m, inBar };
            expect
              .soft(m.docScrollLeft, `click ${room} ${n}: the page did not move sideways`)
              .toBe(0);
            expect.soft(m.sw, `click ${room} ${n}: no sideways scroll`).toBeLessThanOrEqual(m.cw);
            if (m.area)
              expect
                .soft(inBar, `click ${room} ${n}: the pressed link is inside the bar`)
                .toBe(true);
          }
        }
      }
      expect(errors, 'no page errors').toEqual([]);
      await c.close();
    }
  }

  // ---- Part B: the Arcade (section theme) and the Study (main theme)
  for (const [variant, main, section] of [
    ['dark', 'lounge-dark', 'sideroom'],
    ['light', 'lounge-light', 'lounge-light'],
  ] as const) {
    const prefs: Prefs = {
      theme: main,
      zoom: 100,
      sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: section },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs);
      await fakeAll(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const go = async (url: string, ready: string, name: string) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await page.waitForTimeout(1200);
        const m = await measure(page);
        all[`${name}-${tag}`] = m;
        log(`${name}-${tag} ${JSON.stringify(m)}`);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
        shots++;
        return m;
      };
      await go('/games', '.arc-root', 'arcade-home');
      await go('/games/emulator', '.arc-roombar', 'emulator');
      await go('/study', '.stu-bar', 'study-home');
      // One area holding six or more files: open the shelf "Home" if the page does not open it itself.
      const home = page.getByRole('button', { name: /^Home/ }).first();
      if (await home.isVisible().catch(() => false)) await home.click().catch(() => undefined);
      await page.waitForTimeout(1500);
      const m = await measure(page);
      const cards = await page.locator('[class*="stu-card"], [class*="stu-row"]').count();
      log(`study-area-${tag} cards=${cards} ${JSON.stringify(m)}`);
      all[`study-area-${tag}`] = { ...m, cards };
      await page.screenshot({ path: resolve(OUT, `study-area-${tag}.png`) });
      shots++;
      await c.close();
    }
  }

  writeFileSync(resolve(OUT, `measure-${MODE}.json`), JSON.stringify(all, null, 2));
  log(`SUMMARY shots=${shots}`);
  await setPrefs({
    theme: 'lounge-dark',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'sideroom' },
  });
  await setStudySurfacesWith(o, false);
  await octx.close();
});
