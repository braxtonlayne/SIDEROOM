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
 * SEC-AUD-304 (wave 78; D-803), on the real stack: the Cinema is whole under any theme chosen for it, and what shows
 * pictures stays dark. Two variants per D-797: the shipped theme (`sectionThemes.cinema = 'sideroom'`, set explicitly)
 * and a light one (`'lounge-light'`), at 1280x800 and 375x812. `data-theme` is read back from the room's root before
 * every shot. The titles are real (created through the admin API and published); the pictures are faked: the JSON
 * answers of `/cinema/*` get an SVG poster and backdrop, and the Discover rows are filled from the real titles.
 * Local only (SR_P78_CINEMA=1); shots go to SR_P78_CINEMA_OUT. SR_P78_BASE=1 runs on the base (the CSS held back):
 * it asserts the fault (cream text on the white page) instead of the fix and writes `computed-base-*.json`; the run
 * after the change writes `computed-after-*.json` and asserts the shipped theme's computed colours equal the base's.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P78_CINEMA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-cinema';
const BASE = process.env.SR_P78_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const VARIANTS = [
  ['shipped', 'sideroom'],
  ['light', 'lounge-light'],
] as const;

test.skip(process.env.SR_P78_CINEMA !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };
type Computed = Record<string, { bg: string; color: string } | null>;

const SVG = (n: number) => {
  const hues = [32, 210, 330, 150, 270];
  const h = hues[n % hues.length]!;
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 900" preserveAspectRatio="xMidYMid slice"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h} 45% 35%)"/><stop offset="1" stop-color="hsl(${h + 40} 50% 15%)"/></linearGradient></defs><rect width="600" height="900" fill="url(#g)"/><circle cx="420" cy="300" r="170" fill="hsl(${h} 60% 55% / .45)"/><rect x="60" y="560" width="300" height="24" fill="hsl(${h} 30% 80% / .5)"/></svg>`;
};

/** Give every poster and backdrop of a JSON answer a picture (a fresh stack has no stored image). */
function pictured(v: unknown, n = { i: 0 }): unknown {
  if (Array.isArray(v)) return v.map((x) => pictured(x, n));
  if (v && typeof v === 'object') {
    const o: Record<string, unknown> = {};
    for (const [k, x] of Object.entries(v)) o[k] = pictured(x, n);
    if ('poster_url' in o && !o.poster_url) o.poster_url = `/__p78/poster-${n.i++}.svg`;
    if ('backdrop_url' in o && !o.backdrop_url) o.backdrop_url = `/__p78/backdrop-${n.i++}.svg`;
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
  await c.route('**/__p78/*.svg', (r: Route) =>
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

/** The computed background and text colour of the first match of each selector, null when it is not on the page. */
async function computed(page: Page, sels: string[]): Promise<Computed> {
  return page.evaluate((list) => {
    const out: Record<string, { bg: string; color: string } | null> = {};
    for (const s of list) {
      const e = document.querySelector(s);
      if (!e) {
        out[s] = null;
        continue;
      }
      const cs = getComputedStyle(e);
      out[s] = { bg: cs.backgroundColor, color: cs.color };
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
  '.cin-footer-brand',
  '.cin-card h3',
  '.cin-hero',
  '.cin-hero h1',
  '.cin-title-head h1',
  '.cin-chip-all',
  '.cin-qa',
];

/** WCAG contrast of every visible text element outside the picture surfaces, against its nearest opaque background. */
async function lowContrast(page: Page) {
  return page.evaluate(() => {
    const parse = (s: string) => {
      const m = /rgba?\(([^)]+)\)/.exec(s);
      if (!m) return null;
      const p = m[1]!
        .split(/[ ,/]+/)
        .filter(Boolean)
        .map(Number);
      return { r: p[0]!, g: p[1]!, b: p[2]!, a: p[3] ?? 1 };
    };
    const lum = (c: { r: number; g: number; b: number }) => {
      const f = (v: number) => {
        const x = v / 255;
        return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    };
    const skip =
      '.cin-hero,.cin-title-head,.cpl-player,.wp-root,.tv-crt,.cin-qa,[data-poster-link]';
    const bad: string[] = [];
    const root = document.querySelector('.cin-root');
    if (!root) return bad;
    for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
      if (e.closest(skip)) continue;
      const own = Array.from(e.childNodes).some(
        (n) => n.nodeType === 3 && (n.textContent ?? '').trim().length > 0,
      );
      if (!own) continue;
      const r = e.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      const cs = getComputedStyle(e);
      if (cs.visibility === 'hidden' || cs.opacity === '0') continue;
      const fg = parse(cs.color);
      if (!fg) continue;
      let bg: { r: number; g: number; b: number } | null = null;
      let image = false;
      for (let a: HTMLElement | null = e; a; a = a.parentElement) {
        const s = getComputedStyle(a);
        if (s.backgroundImage !== 'none') image = true;
        const c = parse(s.backgroundColor);
        if (c && c.a > 0.95) {
          bg = c;
          break;
        }
      }
      if (!bg || image) continue;
      const l1 = lum(fg);
      const l2 = lum(bg);
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      const size = parseFloat(cs.fontSize);
      const large = size >= 24 || (size >= 18.66 && Number(cs.fontWeight) >= 700);
      if (ratio < (large ? 3 : 4.5)) {
        const t = (e.textContent ?? '').trim().slice(0, 40);
        bad.push(
          `${e.tagName.toLowerCase()}.${e.className.toString().slice(0, 30)} "${t}" ${ratio.toFixed(2)}`,
        );
      }
    }
    return bad;
  });
}

const report = (line: string) => {
  console.log(`REPORT ${line}`);
  appendFileSync(resolve(OUT, `report-${MODE}.txt`), `${line}\n`);
};

test('the Cinema is whole in the shipped theme and in a light one; pictures stay dark', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);

  // ---- the world: three published titles, a TV channel, a watch party
  const made: { id: string; wire?: unknown }[] = [];
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
      synopsis: 'A fixture film for the theme check. Two friends cross a city in one night.',
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
  const items = ((await list.json()) as { items: Record<string, unknown>[] }).items;
  expect(items.length, 'published titles are listed').toBeGreaterThan(0);
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
    const ch = await apiPost<{ id: string }>(o, '/admin/tv/channels', {
      number: 7,
      name: 'Night Owl TV',
      kind: 'manual',
      timezone: 'UTC',
      enabled: true,
    });
    tvId = ch.id;
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
      const go = async (url: string, ready: string, name: string, full = false) => {
        await page.goto(url);
        await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
        await expect(root).toHaveAttribute('data-theme', sectionTheme);
        await page.waitForTimeout(700);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`), fullPage: full });
        shots++;
      };

      // ---- home (hero, rows, footer)
      await go('/cinema', '.cin-hero', 'home', true);
      const chrome = await computed(page, CHROME);
      report(`${tag} home computed ${JSON.stringify(chrome)}`);
      writeFileSync(resolve(OUT, `computed-${MODE}-${tag}.json`), JSON.stringify(chrome, null, 2));
      if (variant === 'light') {
        if (BASE) {
          // The fault: the dark palette's text on the white page.
          const fault = [chrome['.cin-logo-name']?.color, chrome['.cin-footer']?.color];
          expect(fault, 'base: cream text on the light page').toEqual([
            'rgb(236, 230, 214)',
            'rgb(169, 164, 147)',
          ]);
        } else {
          expect(chrome['.cin-logo-name']?.color, 'brand text follows the theme').toBe(
            'rgb(49, 51, 56)',
          );
          expect(chrome['.cin-footer']?.color, 'footer text follows the theme').not.toBe(
            'rgb(169, 164, 147)',
          );
          // The hero is a picture surface: it keeps the cream text.
          expect(chrome['.cin-hero h1']?.color, 'hero title stays light').toBe(
            'rgb(236, 230, 214)',
          );
          expect(chrome['.cin-card h3']?.color, 'card title follows the theme').toBe(
            'rgb(49, 51, 56)',
          );
        }
      } else {
        const basePath = resolve(OUT, `computed-base-${tag}.json`);
        if (!BASE && existsSync(basePath)) {
          expect(chrome, `${tag}: computed colours equal the base's`).toEqual(
            JSON.parse(readFileSync(basePath, 'utf8')) as Computed,
          );
          report(`${tag} equals the base`);
        }
      }
      const low = await lowContrast(page);
      report(`${tag} home contrast below 4.5 (3 large): ${low.length} ${JSON.stringify(low)}`);
      if (variant === 'light' && !BASE) expect(low, 'home: readable text').toEqual([]);

      // ---- Browse and its filters
      await go('/cinema/movies', '.cin-chips', 'browse');
      const lowB = await lowContrast(page);
      report(`${tag} browse contrast: ${lowB.length} ${JSON.stringify(lowB)}`);
      if (variant === 'light' && !BASE) expect(lowB, 'browse: readable text').toEqual([]);
      const filters = page.locator('.cin-filters-btn');
      if (await filters.count()) {
        await filters.first().click();
        await page.waitForTimeout(600);
        await page.screenshot({ path: resolve(OUT, `browse-filters-${tag}.png`) });
        shots++;
        await page.keyboard.press('Escape');
        await page.waitForTimeout(300);
      }
      const chip = page.getByRole('button', { name: 'Drama', exact: true }).first();
      if (await chip.count()) {
        await chip.click();
        await expect(page).toHaveURL(/genres=/);
        await page.getByRole('button', { name: 'All', exact: true }).first().click();
        await expect(page).not.toHaveURL(/genres=/);
      }

      // ---- Search: the overlay open, then closed with Escape
      await go('/cinema', '.cin-header', 'home-again');
      await page.getByRole('button', { name: 'Search the cinema' }).click();
      const input = page.locator('.cin-search-input');
      await expect(input).toBeVisible();
      await input.fill('Long');
      await page.waitForTimeout(900);
      await page.screenshot({ path: resolve(OUT, `search-${tag}.png`) });
      shots++;
      const lowS = await lowContrast(page);
      report(`${tag} search contrast: ${lowS.length} ${JSON.stringify(lowS)}`);
      await page.keyboard.press('Escape');
      await expect(input).toBeHidden();

      // ---- the title page
      await go(`/cinema/t/${first}`, '.cin-title-head', 'title', true);
      const t = await computed(page, ['.cin-title-head', '.cin-title-head h1']);
      report(`${tag} title head ${JSON.stringify(t)}`);
      if (variant === 'light' && !BASE)
        expect(t['.cin-title-head h1']?.color, 'title heading stays light').toBe(
          'rgb(236, 230, 214)',
        );
      const lowT = await lowContrast(page);
      report(`${tag} title contrast: ${lowT.length} ${JSON.stringify(lowT)}`);
      if (variant === 'light' && !BASE) expect(lowT, 'title: readable text').toEqual([]);

      // ---- the player (no asset behind the id: the player's own message, on black)
      await go(`/cinema/watch/${first}`, '.cpl-player', 'player');
      const p = await computed(page, ['.cpl-player', '.cpl-player .cpl-title']);
      report(`${tag} player ${JSON.stringify(p)}`);
      expect(p['.cpl-player']?.bg, 'player stays black').toBe('rgb(0, 0, 0)');

      // ---- the submit page, My list
      await go('/cinema/submit', '.cin-submit, .cin-root h1', 'submit', true);
      const lowU = await lowContrast(page);
      report(`${tag} submit contrast: ${lowU.length} ${JSON.stringify(lowU)}`);
      await go('/cinema/my-list', '.cpl-my', 'my-list', true);
      const lowM = await lowContrast(page);
      report(`${tag} my-list contrast: ${lowM.length} ${JSON.stringify(lowM)}`);

      // ---- Live TV: the guide, then the screen
      await go('/cinema/tv', '.tv-root', 'tv', true);
      const tv = await computed(page, [
        '.tv-root',
        '.tv-h1',
        '.tv-crt',
        '.tv-crt-screen',
        '.tv-chan',
      ]);
      report(`${tag} tv ${JSON.stringify(tv)}`);
      const lowV = await lowContrast(page);
      report(`${tag} tv contrast: ${lowV.length} ${JSON.stringify(lowV)}`);
      if (variant === 'light' && !BASE) expect(lowV, 'tv: readable text').toEqual([]);
      if (tvId) await go(`/cinema/tv/watch/${tvId}`, '.cpl-player', 'tv-watch');
      if (partyId) {
        await page.goto(`/cinema/party/${partyId}`);
        await page.waitForTimeout(1800);
        await expect(root).toHaveAttribute('data-theme', sectionTheme);
        await page.screenshot({ path: resolve(OUT, `party-${tag}.png`) });
        shots++;
      }

      // ---- the phone menu; the header's links on desktop (clicked through)
      await page.goto('/cinema');
      await expect(page.locator('.cin-header')).toBeVisible();
      if (sizeName === 'phone') {
        await page.getByRole('button', { name: 'Open cinema menu' }).click();
        await page.waitForTimeout(600);
        await page.screenshot({ path: resolve(OUT, `menu-${tag}.png`) });
        shots++;
        await page.keyboard.press('Escape');
      } else {
        const nav = page.locator('.cin-nav-link');
        const n = await nav.count();
        for (let i = 0; i < n; i++) {
          await page.locator('.cin-nav-link').nth(i).click();
          await page.waitForTimeout(500);
          await expect(root).toHaveAttribute('data-theme', sectionTheme);
        }
        report(`${tag} header links clicked: ${n}`);
      }
      expect(errors, `${tag}: no page errors`).toEqual([]);
      await c.close();
    }
  }
  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { cinema: 'sideroom' } });
  report(`shots ${shots}`);
  await setStudySurfacesWith(o, false);
  await octx.close();
});
