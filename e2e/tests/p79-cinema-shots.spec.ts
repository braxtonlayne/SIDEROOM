import { appendFileSync, mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
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
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-313 and 314 (wave 79; D-815, D-816, D-817), on the real stack. 313: filled gold is the theme's accent with
 * its ink, and a surface kept dark is Side Room whole (a kit badge on it is gold). 314: no Cinema page scrolls
 * sideways at 375 or 1280 with 9 titles in a rail, and on a phone the section bar scrolls inside itself with the area
 * you are in in view. Two variants per D-797: the shipped theme (`sectionThemes.cinema = 'sideroom'`, set explicitly)
 * and a light one (`'lounge-light'`), `data-theme` read back before each shot. Local only (SR_P79_CINEMA=1); shots go
 * to SR_P79_CINEMA_OUT. SR_P79_BASE=1 runs on the base (the five CSS files and Shell.tsx held back): it measures and
 * reports instead of asserting the fix, and writes `computed-base-*.json`; the run after writes `computed-after-*.json`
 * and asserts the shipped theme's computed colours equal the base's.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_CINEMA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-cinema';
const BASE = process.env.SR_P79_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const VARIANTS = [
  ['shipped', 'sideroom'],
  ['light', 'lounge-light'],
] as const;

test.skip(process.env.SR_P79_CINEMA !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };

const SVG = (n: number) => {
  const hues = [32, 210, 330, 150, 270];
  const h = hues[n % hues.length]!;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 900" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h} 45% 35%)"/><stop offset="1" stop-color="hsl(${h + 40} 50% 15%)"/></linearGradient></defs><rect width="600" height="900" fill="url(#g)"/><circle cx="420" cy="300" r="170" fill="hsl(${h} 60% 55% / .45)"/><rect x="60" y="560" width="300" height="24" fill="hsl(${h} 30% 80% / .5)"/></svg>`;
};

function pictured(v: unknown, n = { i: 0 }): unknown {
  if (Array.isArray(v)) return v.map((x) => pictured(x, n));
  if (v && typeof v === 'object') {
    const o: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) o[k] = pictured(x, n);
    if ('poster_url' in o && !o.poster_url) o.poster_url = `/__p79/poster-${n.i++}.svg`;
    if ('backdrop_url' in o && !o.backdrop_url) o.backdrop_url = `/__p79/backdrop-${n.i++}.svg`;
    return o;
  }
  return v;
}

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
  discover: unknown,
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
  await c.route('**/__p79/*.svg', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: SVG(Number(/-(\d+)\.svg/.exec(r.request().url())?.[1] ?? 0)),
    }),
  );
  await c.route(/\/api\/v1\/cinema\/discover/, (r: Route) =>
    r.fulfill({ status: 200, contentType: 'application/json', json: pictured(discover) }),
  );
  await c.route(/\/api\/v1\/cinema\/(titles|facets|collections|search|my)/, async (r: Route) => {
    const res = await r.fetch();
    const ct = res.headers()['content-type'] ?? '';
    if (!ct.includes('json')) return r.fulfill({ response: res });
    return r.fulfill({ response: res, json: pictured(await res.json()) });
  });
  return c;
}

type Comp = Record<string, { bg: string; color: string; border: string } | null>;
async function computed(page: Page, sels: string[]): Promise<Comp> {
  return page.evaluate((list) => {
    const out: Record<string, { bg: string; color: string; border: string } | null> = {};
    for (const s of list) {
      const e = document.querySelector(s);
      if (!e) {
        out[s] = null;
        continue;
      }
      const cs = getComputedStyle(e);
      out[s] = { bg: cs.backgroundColor, color: cs.color, border: cs.borderTopColor };
    }
    return out;
  }, sels);
}

const CHROME = [
  '.cin-root',
  '.cin-sectionbar',
  '.cin-sectionbar-brand',
  '.cin-header',
  '.cin-logo-name',
  '.cin-nav-link',
  '.cin-footer',
  '.cin-card h3',
  '.cin-hero',
  '.cin-hero h1',
  '.cin-hero button',
  '.cin-qa',
];

const report = (line: string) => {
  console.log(`REPORT ${line}`);
  appendFileSync(resolve(OUT, `report-${MODE}.txt`), `${line}\n`);
};

/** Sideways scroll of the page and the elements whose box ends past the viewport (outside a scroller of their own). */
async function measure(page: Page) {
  return page.evaluate(() => {
    const iw = window.innerWidth;
    const clipped = (e: Element) => {
      for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
        const o = getComputedStyle(a).overflowX;
        if (o !== 'visible' && a.getBoundingClientRect().right <= iw + 1) return true;
      }
      return false;
    };
    const past = Array.from(document.querySelectorAll<HTMLElement>('body *'))
      .filter((e) => e.getBoundingClientRect().right > iw + 1 && !clipped(e))
      .slice(0, 5)
      .map((e) => `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 28)}`);
    const media = Array.from(document.querySelectorAll<HTMLElement>('.cin-sectionbar a')).find(
      (a) => a.getAttribute('aria-current') === 'page' && a.closest('nav + nav'),
    );
    const mr = media?.getBoundingClientRect();
    return {
      sw: document.documentElement.scrollWidth,
      cw: document.documentElement.clientWidth,
      past,
      mediaBox: mr ? [Math.round(mr.left), Math.round(mr.right)] : null,
      scrollY: Math.round(window.scrollY),
    };
  });
}

test('SEC-AUD-313/314: no sideways scroll; gold fills and dark surfaces', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);

  // ---- the world: three published titles (shown as nine in the rails), a TV channel, a watch party
  const made: { id: string }[] = [];
  for (const [kind, name] of [
    ['movie', 'The Long Corridor'],
    ['series', 'Harbour Lights'],
    ['anime', 'Paper Kites'],
  ] as const) {
    const t = await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
      kind,
      title: `${name} ${Date.now().toString(36)}`,
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
    made.push({ id: t.id });
  }
  const list = await o.get('/api/v1/cinema/titles?limit=20');
  const real = ((await list.json()) as { items: Record<string, unknown>[] }).items;
  expect(real.length, 'published titles are listed').toBeGreaterThan(0);
  const items = Array.from({ length: 9 }, (_, i) => {
    const r = real[i % real.length]!;
    return i < real.length
      ? r
      : { ...r, id: `${String(r.id).slice(0, -3)}${String(i).padStart(3, '0')}` };
  });
  const discover = {
    hero: {
      key: 'hero',
      kind: 'curated',
      display: 'hero',
      title: 'Featured',
      items: items.slice(0, 2),
    },
    rows: [
      {
        key: 'trending',
        kind: 'generated',
        display: 'row',
        title: 'Trending now',
        eyebrow: 'This week',
        items,
      },
      { key: 'animation-banner', kind: 'curated', display: 'banner', title: 'Animation', items },
      {
        key: 'new',
        kind: 'generated',
        display: 'row',
        title: 'New on the room',
        items: [...items].reverse(),
      },
    ],
  };
  let tvId: string | null = null;
  try {
    tvId = (
      await apiPost<{ id: string }>(o, '/admin/tv/channels', {
        number: 7,
        name: 'Night Owl TV',
        kind: 'manual',
        timezone: 'UTC',
        enabled: true,
      })
    ).id;
  } catch {
    const l = await o.get('/api/v1/admin/tv/channels');
    const rows = (await l.json()) as
      { id: string; number: number }[] | { items: { id: string; number: number }[] };
    tvId = (Array.isArray(rows) ? rows : rows.items).find((c) => c.number === 7)?.id ?? null;
  }
  let partyId: string | null = null;
  try {
    partyId = (await apiPost<{ id: string }>(o, '/watch-parties', { title_id: made[0]!.id })).id;
  } catch {
    partyId = null;
  }
  const first = made[0]!.id;
  report(`world titles ${made.map((m) => m.id).join(',')} tv ${tvId} party ${partyId}`);

  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  let shots = 0;
  for (const [variant, sectionTheme] of VARIANTS) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: sectionTheme },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs, discover);
      const page = await c.newPage();
      page.setDefaultTimeout(25_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      const root = page.locator('.cin-root');
      const comp: Comp = {};

      /** Open a page, read the theme back, measure the sideways scroll, then shoot the viewport. */
      const go = async (url: string, ready: string, name: string) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await expect(root).toHaveAttribute('data-theme', sectionTheme);
        await page.waitForTimeout(800);
        const m = await measure(page);
        report(`${tag} ${name} ${JSON.stringify(m)}`);
        if (!BASE) {
          expect(
            m.sw,
            `${tag} ${name}: no sideways scroll (${m.past.join(' ')})`,
          ).toBeLessThanOrEqual(m.cw);
        }
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
        shots++;
        return m;
      };

      // ---- home: rails, section bar
      const home = await go('/cinema', '.cin-hero', 'home');
      Object.assign(comp, await computed(page, CHROME));
      const rail = page.locator('[data-row="trending"] section > div').last();
      const rm = await rail.evaluate((e) => ({ sw: e.scrollWidth, cw: e.clientWidth }));
      const card0 = await page.locator('.cin-card').first().boundingBox();
      report(`${tag} rail ${JSON.stringify(rm)} first card ${JSON.stringify(card0)}`);
      if (!BASE) expect(rm.sw, `${tag}: the rail scrolls inside itself`).toBeGreaterThan(rm.cw);
      if (sizeName === 'phone' && !BASE) {
        expect(home.mediaBox?.[0] ?? 9999, 'Media starts inside the viewport').toBeLessThan(375);
        expect(home.mediaBox?.[1] ?? 9999, 'Media ends inside the viewport').toBeLessThanOrEqual(
          375,
        );
        expect(home.scrollY, 'the page did not move').toBe(0);
      }
      const before = await rail.evaluate((e) => e.scrollLeft);
      await page
        .locator('[data-row="trending"]')
        .getByRole('button', { name: 'Scroll forward' })
        .click();
      await page.waitForTimeout(900);
      const after = await rail.evaluate((e) => e.scrollLeft);
      report(`${tag} rail next: ${before} -> ${after}; document ${(await measure(page)).sw}`);
      if (!BASE) expect(after, `${tag}: next moves the rail`).toBeGreaterThan(before);
      await page.screenshot({ path: resolve(OUT, `home-rail-next-${tag}.png`) });
      shots++;
      await page
        .locator('[data-row="trending"]')
        .getByRole('button', { name: 'Scroll back' })
        .click();
      await page.waitForTimeout(700);
      if (!BASE)
        expect(
          await rail.evaluate((e) => e.scrollLeft),
          `${tag}: back moves the rail`,
        ).toBeLessThan(after);

      // ---- Browse (a genre chip clicked and cleared), a title page
      await go('/cinema/movies', '.cin-chips', 'browse');
      const drama = page.getByRole('button', { name: 'Drama', exact: true }).first();
      if (await drama.count()) {
        await drama.click();
        await expect(page).toHaveURL(/genres=/);
        await page.getByRole('button', { name: 'All', exact: true }).first().click();
        await expect(page).not.toHaveURL(/genres=/);
      }
      await go(`/cinema/t/${first}`, '.cin-title-head', 'title');

      // ---- Live TV: the filled chip and button
      await go('/cinema/tv', '.tv-root', 'tv');
      Object.assign(
        comp,
        await computed(page, ['.tv-chip[aria-pressed="true"]', '.tv-btn-primary', '.tv-crt']),
      );
      const chip = comp['.tv-chip[aria-pressed="true"]'];
      report(`${tag} tv chip ${JSON.stringify(chip)}`);
      // the submit page: measured only (its naming-guide table is the Library's wrapper)
      await page.goto('/cinema/submit');
      await expect(page.locator('.cin-root h1').first()).toBeVisible({ timeout: 60_000 });
      await page.waitForTimeout(600);
      const sub = await measure(page);
      report(`${tag} submit ${JSON.stringify(sub)}`);
      if (!BASE)
        expect(
          sub.sw,
          `${tag} submit: no sideways scroll (${sub.past.join(' ')})`,
        ).toBeLessThanOrEqual(sub.cw);

      // ---- the theatre
      if (partyId) {
        await page.goto(`/cinema/party/${partyId}`);
        await page.waitForTimeout(2000);
        await expect(root).toHaveAttribute('data-theme', sectionTheme);
        const badge = page
          .locator('.wp-root')
          .getByText(/\d+ watching/)
          .first();
        await expect(badge).toBeVisible();
        const b = await badge.evaluate((e) => {
          const cs = getComputedStyle(e);
          return { bg: cs.backgroundColor, color: cs.color, border: cs.borderTopColor };
        });
        comp['party badge'] = b;
        report(`${tag} party badge ${JSON.stringify(b)}`);
        if (variant === 'light' && !BASE) {
          expect(b.bg, 'the theatre badge is gold').toBe('rgb(194, 176, 143)');
          expect(b.color, 'its ink').toBe('rgb(27, 26, 21)');
        }
        const m = await measure(page);
        report(`${tag} party ${JSON.stringify(m)}`);
        await page.screenshot({ path: resolve(OUT, `party-${tag}.png`) });
        shots++;
      }

      // ---- fills in the light variant: the chip is the accent with its ink, like a kit primary button
      if (variant === 'light' && chip && !BASE) {
        const probe = await page.evaluate(() => {
          const d = document.createElement('div');
          d.style.cssText = 'background: var(--ui-accent); color: var(--ui-accent-ink);';
          document.querySelector('.cin-root')!.appendChild(d);
          const cs = getComputedStyle(d);
          const r = { bg: cs.backgroundColor, color: cs.color };
          d.remove();
          return r;
        });
        expect(chip.bg, 'chip fill is the accent').toBe(probe.bg);
        expect(chip.color, 'chip ink is the accent ink').toBe(probe.color);
      }

      // ---- the computed comparison under the shipped theme
      writeFileSync(resolve(OUT, `computed-${MODE}-${tag}.json`), JSON.stringify(comp, null, 2));
      if (variant === 'shipped' && !BASE) {
        const basePath = resolve(OUT, `computed-base-${tag}.json`);
        if (existsSync(basePath)) {
          expect(comp, `${tag}: computed colours equal the base's`).toEqual(
            JSON.parse(readFileSync(basePath, 'utf8')) as Comp,
          );
          report(`${tag} equals the base`);
        }
      }

      // ---- the phone: the section bar at load, dragged to its start, a click through, the menu
      if (sizeName === 'phone') {
        await page.goto('/cinema');
        await expect(page.locator('.cin-sectionbar')).toBeVisible();
        await page.waitForTimeout(700);
        const bar = await page.locator('.cin-sectionbar').evaluate((e) => ({
          sl: e.scrollLeft,
          sw: e.scrollWidth,
          cw: e.clientWidth,
        }));
        report(
          `${tag} section bar at load ${JSON.stringify(bar)} ${JSON.stringify((await measure(page)).mediaBox)}`,
        );
        await page.screenshot({ path: resolve(OUT, `sectionbar-load-${tag}.png`) });
        shots++;
        await page.locator('.cin-sectionbar').evaluate((e) => {
          e.scrollLeft = 0;
        });
        await page.waitForTimeout(300);
        await page.screenshot({ path: resolve(OUT, `sectionbar-start-${tag}.png`) });
        shots++;
        if (variant === 'light') {
          await page.locator('.cin-sectionbar a', { hasText: 'Shelves' }).click();
          await expect(page).toHaveURL(/shelves/);
          await page.waitForTimeout(600);
          report(`${tag} on Shelves ${JSON.stringify(await measure(page))}`);
          await page.goBack();
          await expect(page.locator('.cin-hero').first()).toBeVisible();
        }
        await page.getByRole('button', { name: 'Open cinema menu' }).click();
        await page.waitForTimeout(600);
        await page.screenshot({ path: resolve(OUT, `menu-${tag}.png`) });
        shots++;
        await page.keyboard.press('Escape');
      }
      expect(errors, `${tag}: no page errors`).toEqual([]);
      await c.close();
    }
  }

  // ---- Lounge Dark chosen for the Cinema: home, Live TV, the theatre (desktop)
  {
    const prefs: Prefs = {
      theme: 'lounge-light',
      zoom: 100,
      sectionThemes: { cinema: 'lounge-dark' },
    };
    await setPrefs(prefs);
    const c = await context(browser, octx, SIZES.desktop, prefs, discover);
    const page = await c.newPage();
    for (const [url, ready, name] of [
      ['/cinema', '.cin-hero', 'home'],
      ['/cinema/tv', '.tv-root', 'tv'],
      ...(partyId ? ([[`/cinema/party/${partyId}`, '.wp-root', 'party']] as const) : []),
    ] as const) {
      await page.goto(url);
      await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
      await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', 'lounge-dark');
      await page.waitForTimeout(1500);
      const m = await measure(page);
      report(`lounge-dark ${name} ${JSON.stringify(m)}`);
      if (!BASE) expect(m.sw).toBeLessThanOrEqual(m.cw);
      if (name === 'tv') {
        const ch = await computed(page, ['.tv-chip[aria-pressed="true"]']);
        report(`lounge-dark tv chip ${JSON.stringify(ch)}`);
      }
      if (name === 'party') {
        const b = await page
          .locator('.wp-root')
          .getByText(/\d+ watching/)
          .first()
          .evaluate((e) => getComputedStyle(e).backgroundColor);
        report(`lounge-dark party badge ${b}`);
        if (!BASE) expect(b).toBe('rgb(194, 176, 143)');
      }
      await page.screenshot({ path: resolve(OUT, `${name}-dark-desktop.png`) });
      shots++;
    }
    await c.close();
  }
  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { cinema: 'sideroom' } });
  report(`shots ${shots}`);
  await setStudySurfacesWith(o, false);
  await octx.close();
});
