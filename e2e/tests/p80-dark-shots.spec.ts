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
 * SEC-AUD-319 (wave 80; D-820), on the real stack: a Cinema surface kept dark (hero, title head, quick actions, the
 * player, the CRT, the watch-party theatre) is whole in its status colours too. Two variants per D-797: the shipped
 * theme (`sectionThemes.cinema = 'sideroom'`, set explicitly) and a light one (`'lounge-light'`), `data-theme` read
 * back before each shot. Local only (SR_P80_DARK=1); shots go to SR_P80_DARK_OUT. SR_P80_BASE=1 runs on the base (the
 * four CSS files held back): it measures and reports instead of asserting the fix, and writes `computed-base-*.json`;
 * the run after writes `computed-after-*.json` and asserts the shipped theme's computed colours equal the base's, and
 * that the light Cinema's equal the shipped Cinema's.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_DARK_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-dark';
const HLS = process.env.SR_P80_HLS ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/p80-dark/scratch-hls';
const BASE = process.env.SR_P80_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const VARIANTS = [
  ['shipped', 'sideroom'],
  ['light', 'lounge-light'],
] as const;
/** The Side Room values every dark block states (themes.ts `sideroom`), as the browser computes them. */
const SIDE_ROOM = {
  danger: 'rgb(180, 80, 63)',
  dangerInk: 'rgb(255, 246, 242)',
  dangerText: 'rgb(217, 122, 104)',
  success: 'rgb(140, 154, 106)',
  warning: 'rgb(194, 176, 143)',
  info: 'rgb(154, 168, 160)',
  olive: 'rgb(140, 154, 106)',
};
const DARK_BLOCKS = [
  '.cin-hero',
  '.cin-title-head',
  '.cin-qa',
  '.cpl-player',
  '.tv-crt',
  '.wp-root',
];

test.skip(process.env.SR_P80_DARK !== '1', 'local visual check only');
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
    if ('poster_url' in o && !o.poster_url) o.poster_url = `/__p80/poster-${n.i++}.svg`;
    if ('backdrop_url' in o && !o.backdrop_url) o.backdrop_url = `/__p80/backdrop-${n.i++}.svg`;
    return o;
  }
  return v;
}

const DISCOVER: { value: unknown } = { value: null };

type PlayMode = 'none' | 'error' | 'good';

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
  play: PlayMode = 'none',
  discover: unknown = DISCOVER.value,
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
  await c.route('**/__p80/*.svg', (r: Route) =>
    r.fulfill({
      status: 200,
      contentType: 'image/svg+xml',
      body: SVG(Number(/-(\d+)\.svg/.exec(r.request().url())?.[1] ?? 0)),
    }),
  );
  if (discover)
    await c.route(/\/api\/v1\/cinema\/discover/, (r: Route) =>
      r.fulfill({ status: 200, contentType: 'application/json', json: pictured(discover) }),
    );
  await c.route(/\/api\/v1\/cinema\/(titles|facets|collections|search|my)/, async (r: Route) => {
    const res = await r.fetch();
    const ct = res.headers()['content-type'] ?? '';
    if (!ct.includes('json')) return r.fulfill({ response: res });
    return r.fulfill({ response: res, json: pictured(await res.json()) });
  });
  if (play !== 'none') {
    // The real server has no media behind the fixture titles: the playback grant is faked. 'good' points at a real
    // 20 s VP9 HLS clip (ffmpeg testsrc, served from the scratch folder); 'error' at a playlist that answers 404.
    await c.route(/\/api\/v1\/cinema\/assets\/[^/]+\/playback/, (r: Route) =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        json: {
          master_url: `${ORIGIN}/__p80/hls/${play === 'good' ? 'master.m3u8' : 'missing.m3u8'}`,
          expires_at: Math.floor(Date.now() / 1000) + 3600,
          refresh_at: null,
          duration_secs: 20,
          subtitles: [],
          audio_tracks: [],
          markers: [],
        },
      }),
    );
    await c.route('**/__p80/hls/*', (r: Route) => {
      const name = new URL(r.request().url()).pathname.split('/').pop() ?? '';
      const file = resolve(HLS, name);
      if (!existsSync(file) || play === 'error') return r.fulfill({ status: 404, body: 'nope' });
      return r.fulfill({
        status: 200,
        contentType: name.endsWith('.m3u8')
          ? 'application/vnd.apple.mpegurl'
          : name.endsWith('.mp4')
            ? 'video/mp4'
            : 'video/iso.segment',
        headers: { 'access-control-allow-origin': '*' },
        body: readFileSync(file),
      });
    });
  }
  return c;
}

type Style = {
  bg: string;
  color: string;
  border: string;
  opacity: string;
  cursor: string;
} | null;
type Comp = Record<string, Style | Record<string, string> | null>;

/** Computed look of the first element matching `sel` (an optional text filter). */
async function look(page: Page, sel: string, text?: string): Promise<Style> {
  return page.evaluate(
    ([s, t]) => {
      const list = Array.from(document.querySelectorAll<HTMLElement>(s!));
      const e = t ? list.find((x) => (x.textContent ?? '').trim() === t) : list[0];
      if (!e) return null;
      const cs = getComputedStyle(e);
      return {
        bg: cs.backgroundColor,
        color: cs.color,
        border: cs.borderTopColor,
        opacity: cs.opacity,
        cursor: cs.cursor,
      };
    },
    [sel, text] as [string, string | undefined],
  );
}

/** The six status tokens (and the olive mapping) as resolved colours inside every dark block on the page. */
async function probes(page: Page): Promise<Record<string, Record<string, string>>> {
  return page.evaluate((blocks) => {
    const out: Record<string, Record<string, string>> = {};
    for (const b of blocks) {
      const host = document.querySelector(b);
      if (!host) continue;
      const d = document.createElement('i');
      d.style.cssText =
        'position:absolute;width:0;height:0;background-color:var(--ui-danger);color:var(--ui-danger-ink);' +
        'border:1px solid var(--ui-danger-text);outline-color:var(--ui-success);' +
        'column-rule:1px solid var(--ui-warning);text-decoration-color:var(--ui-info);caret-color:var(--sr-olive-accent);';
      host.appendChild(d);
      const cs = getComputedStyle(d);
      out[b] = {
        danger: cs.backgroundColor,
        dangerInk: cs.color,
        dangerText: cs.borderTopColor,
        success: cs.outlineColor,
        warning: cs.columnRuleColor,
        info: cs.textDecorationColor,
        olive: cs.caretColor,
      };
      d.remove();
    }
    return out;
  }, DARK_BLOCKS);
}

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
    return {
      sw: document.documentElement.scrollWidth,
      cw: document.documentElement.clientWidth,
      past,
    };
  });
}

/** `data-theme` of the nearest themed ancestor of the first element matching `sel`. */
const themeOf = (page: Page, sel: string) =>
  page.evaluate(
    (s) => document.querySelector(s)?.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
    sel,
  );

test('SEC-AUD-319: dark Cinema surfaces keep the status colours', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);

  // ---- the world: three published titles, a TV channel, a watch party (the owner is its host)
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
      synopsis: 'A fixture film for the colour check. Two friends cross a city in one night.',
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
  DISCOVER.value = {
    hero: {
      key: 'hero',
      kind: 'curated',
      display: 'hero',
      title: 'Featured',
      items: real.slice(0, 2),
    },
    rows: [
      {
        key: 'trending',
        kind: 'generated',
        display: 'row',
        title: 'Trending now',
        eyebrow: 'This week',
        items: real,
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
  expect(partyId, 'a party was started').toBeTruthy();
  expect(tvId, 'a TV channel exists').toBeTruthy();

  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  let shots = 0;
  /** Measure the sideways scroll, then shoot the viewport. */
  const shoot = async (page: Page, name: string, tag: string) => {
    const m = await measure(page);
    report(`${tag} ${name} ${JSON.stringify(m)}`);
    if (!BASE)
      expect(m.sw, `${tag} ${name}: no sideways scroll (${m.past.join(' ')})`).toBeLessThanOrEqual(
        m.cw,
      );
    await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
    shots++;
  };

  /** The Live TV watch page with its tune answered 404: the failed state and its "Try again". */
  const liveFailure = async (page: Page, tag: string, expectTheme: string, click: boolean) => {
    let tunes = 0;
    await page.route(/\/api\/v1\/tv\/channels\/[^/]+\/tune/, (r: Route) => {
      tunes++;
      return r.fulfill({ status: 404, contentType: 'application/json', json: { code: 404 } });
    });
    await page.goto(`/cinema/tv/watch/${tvId}`);
    const again = page.getByRole('button', { name: 'Try again' });
    await expect(again).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(1200);
    expect(await themeOf(page, '.cpl-player'), `${tag}: data-theme`).toBe(expectTheme);
    const comp = await look(page, '.cpl-player button', 'Try again');
    report(`${tag} live try-again ${JSON.stringify(comp)}`);
    report(`${tag} live probes ${JSON.stringify(await probes(page))}`);
    await shoot(page, 'live-tryagain', tag);
    if (click) {
      const before = tunes;
      await again.click();
      await expect.poll(() => tunes, { message: 'Try again tunes again' }).toBeGreaterThan(before);
      await expect(again).toBeVisible();
      report(`${tag} try again clicked: tune requests ${before} -> ${tunes}`);
    }
    await page.unroute(/\/api\/v1\/tv\/channels\/[^/]+\/tune/);
    return { comp, probe: await probes(page).catch(() => ({})) };
  };

  for (const [variant, sectionTheme] of VARIANTS) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: sectionTheme },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs, 'error');
      const page = await c.newPage();
      page.setDefaultTimeout(25_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      page.on('response', (r) => {
        if (r.status() >= 400 && /cinema|tv|__p80/.test(r.url()))
          report(`${tag} HTTP ${r.status()} ${r.url().replace(ORIGIN, '')}`);
      });
      const comp: Comp = {};

      // ---- the theatre as host: "End party" and the chat's Send, disabled then enabled
      await page.goto(`/cinema/party/${partyId}`);
      await expect(page.locator('.wp-root')).toBeVisible({ timeout: 60_000 });
      await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', sectionTheme);
      await page.waitForTimeout(1500);
      const end = page.getByRole('button', { name: 'End party' });
      await expect(end, 'the host sees End party').toBeVisible();
      comp['end party'] = await look(page, '.wp-root header button', 'End party');
      comp['send disabled'] = await look(page, '.wp-chat-form button[type="submit"]');
      comp['probes party'] = (await probes(page))['.wp-root'] ?? null;
      report(`${tag} end party ${JSON.stringify(comp['end party'])}`);
      report(`${tag} send disabled ${JSON.stringify(comp['send disabled'])}`);
      await shoot(page, 'party-disabled', tag);
      const msg = page.getByPlaceholder('Message the party');
      await msg.fill('Starting in a minute');
      await page.waitForTimeout(300);
      comp['send enabled'] = await look(page, '.wp-chat-form button[type="submit"]');
      report(`${tag} send enabled ${JSON.stringify(comp['send enabled'])}`);
      await shoot(page, 'party-enabled', tag);
      if (variant === 'light') {
        await page.getByRole('button', { name: 'Send', exact: true }).click();
        await expect(page.locator('.wp-line', { hasText: 'Starting in a minute' })).toBeVisible();
        report(`${tag} chat line sent and shown`);
      }

      // ---- Live TV watch page, the tune failing: "Try again"
      const live = await liveFailure(page, tag, sectionTheme, variant === 'light');
      comp['try again'] = live.comp;
      comp['probes live'] =
        (live.probe as Record<string, Record<string, string>>)['.cpl-player'] ?? null;

      // ---- the title player with a stream that fails: its error state
      const trace = (r: import('@playwright/test').Request) => {
        if (/\/api\/v1\/(cinema|tv)|__p80\/hls/.test(r.url()))
          report(`${tag} player req ${r.method()} ${r.url().replace(ORIGIN, '')}`);
      };
      page.on('request', trace);
      await page.goto(`/cinema/watch/${first}`);
      const stopped = page.getByRole('button', { name: 'Try again' });
      const reached = await stopped
        .waitFor({ state: 'visible', timeout: 45_000 })
        .then(() => true)
        .catch(() => false);
      page.off('request', trace);
      report(`${tag} player error state reached: ${reached}`);
      if (reached) {
        await page.waitForTimeout(800);
        expect(await themeOf(page, '.cpl-player'), `${tag}: player data-theme`).toBe(sectionTheme);
        comp['player try again'] = await look(page, '.cpl-player button', 'Try again');
        await shoot(page, 'player-error', tag);
      } else {
        await shoot(page, 'player-notreached', tag);
      }

      if (sizeName === 'desktop') {
        // ---- hero and title head (nothing moved)
        await page.goto('/cinema');
        await expect(page.locator('.cin-hero').first()).toBeVisible({ timeout: 60_000 });
        await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', sectionTheme);
        await page.waitForTimeout(1500);
        comp['probes home'] = null;
        Object.assign(
          comp,
          Object.fromEntries(
            Object.entries(await probes(page)).map(([k, v]) => [`probes home ${k}`, v]),
          ),
        );
        await shoot(page, 'home', tag);
        await page.goto(`/cinema/t/${first}`);
        await expect(page.locator('.cin-title-head').first()).toBeVisible({ timeout: 60_000 });
        await page.waitForTimeout(1200);
        Object.assign(
          comp,
          Object.fromEntries(
            Object.entries(await probes(page)).map(([k, v]) => [`probes title ${k}`, v]),
          ),
        );
        await shoot(page, 'title', tag);
        await page.goto('/cinema/tv');
        await expect(page.locator('.tv-crt').first()).toBeVisible({ timeout: 60_000 });
        Object.assign(
          comp,
          Object.fromEntries(
            Object.entries(await probes(page)).map(([k, v]) => [`probes tv ${k}`, v]),
          ),
        );
      }

      // ---- the claims
      const sideRoom = Object.entries(comp).filter(([k]) => k.startsWith('probes') && comp[k]);
      writeFileSync(resolve(OUT, `computed-${MODE}-${tag}.json`), JSON.stringify(comp, null, 2));
      if (!BASE) {
        for (const [k, v] of sideRoom)
          expect(v, `${tag} ${k}: the dark block states the Side Room status colours`).toEqual(
            SIDE_ROOM,
          );
        const shippedPath = resolve(OUT, `computed-after-shipped-${sizeName}.json`);
        if (variant === 'light' && existsSync(shippedPath)) {
          expect(comp, `${tag}: the light Cinema's colours equal the shipped Cinema's`).toEqual(
            JSON.parse(readFileSync(shippedPath, 'utf8')) as Comp,
          );
          report(`${tag} equals shipped (${Object.keys(comp).length} entries)`);
        }
        if (variant === 'shipped') {
          const basePath = resolve(OUT, `computed-base-${tag}.json`);
          if (existsSync(basePath)) {
            expect(comp, `${tag}: computed colours equal the base's`).toEqual(
              JSON.parse(readFileSync(basePath, 'utf8')) as Comp,
            );
            report(`${tag} equals the base`);
          }
        }
      } else if (variant === 'light') {
        const shippedPath = resolve(OUT, `computed-base-shipped-${sizeName}.json`);
        const sh = JSON.parse(readFileSync(shippedPath, 'utf8')) as Comp;
        for (const k of Object.keys(comp))
          report(
            `${tag} base ${k}: light ${JSON.stringify(comp[k])} | shipped ${JSON.stringify(sh[k])} | ${
              JSON.stringify(comp[k]) === JSON.stringify(sh[k]) ? 'same' : 'DIFFERENT'
            }`,
          );
      }
      expect(errors, `${tag}: no page errors`).toEqual([]);
      await c.close();
    }
  }

  // ---- Lounge Dark chosen for the Cinema: the theatre and "Try again" (desktop)
  {
    const prefs: Prefs = {
      theme: 'lounge-light',
      zoom: 100,
      sectionThemes: { cinema: 'lounge-dark' },
    };
    await setPrefs(prefs);
    const c = await context(browser, octx, SIZES.desktop, prefs, 'none');
    const page = await c.newPage();
    page.setDefaultTimeout(25_000);
    await page.goto(`/cinema/party/${partyId}`);
    await expect(page.locator('.wp-root')).toBeVisible({ timeout: 60_000 });
    await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', 'lounge-dark');
    await page.waitForTimeout(1500);
    report(
      `lounge-dark end party ${JSON.stringify(await look(page, '.wp-root header button', 'End party'))}`,
    );
    report(`lounge-dark probes ${JSON.stringify(await probes(page))}`);
    await shoot(page, 'party', 'loungedark-desktop');
    await liveFailure(page, 'loungedark-desktop', 'lounge-dark', false);
    await c.close();
  }

  // ---- the mini player (outside the Cinema), under Lounge Light and Lounge Dark as the main theme
  for (const main of ['lounge-light', 'lounge-dark'] as const) {
    const prefs: Prefs = { theme: main, zoom: 100, sectionThemes: { cinema: 'sideroom' } };
    await setPrefs(prefs);
    const c = await context(browser, octx, SIZES.desktop, prefs, 'good');
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    const step = async (name: string, fn: () => Promise<unknown>) => {
      try {
        await fn();
        report(`mini ${main}: ${name} ok`);
      } catch (e) {
        report(`mini ${main}: ${name} FAILED ${String(e).split('\n')[0]}`);
      }
    };
    await page.goto(`/cinema/watch/${first}`);
    await step('player opens', () =>
      expect(page.locator('.cpl-player')).toBeVisible({ timeout: 30_000 }),
    );
    await step('video loaded', () =>
      expect
        .poll(() => page.locator('video').evaluate((v: HTMLVideoElement) => v.readyState), {
          timeout: 30_000,
        })
        .toBeGreaterThanOrEqual(2),
    );
    // the control bar's Play is a real click (user activation)
    await step('play clicked', async () => {
      const big = page.locator('.cpl-bigplay');
      if (await big.isVisible().catch(() => false)) await big.click();
      else
        await page
          .getByRole('button', { name: /^play$/i })
          .first()
          .click();
    });
    await step('playing', () =>
      expect
        .poll(() => page.locator('video').evaluate((v: HTMLVideoElement) => v.currentTime), {
          timeout: 15_000,
        })
        .toBeGreaterThan(0.5),
    );
    // leave the player (history back to where it was opened from), then move to the Lounge by the app's own link
    await step('back', async () => {
      await page.locator('.cpl-top button').first().click();
      await page.waitForTimeout(800);
    });
    report(`mini ${main}: after back url ${page.url()}`);
    await step('to the lounge', async () => {
      await page
        .locator('.cin-sectionbar a', { hasText: /lounge|home|channels/i })
        .first()
        .click();
      await page.waitForTimeout(2500);
    });
    report(`mini ${main}: url ${page.url()}`);
    const mini = page.locator('.cpl-mini-root');
    const shown = await mini.isVisible().catch(() => false);
    report(`mini ${main}: shown ${shown}`);
    if (shown) {
      report(
        `mini ${main}: data-theme ${await themeOf(page, '.cpl-mini-root')} on ${await page.evaluate(() => document.documentElement.getAttribute('data-theme'))}`,
      );
      report(`mini ${main}: look ${JSON.stringify(await look(page, '.cpl-mini'))}`);
    }
    await shoot(page, shown ? 'mini' : 'mini-notshown', main);
    await c.close();
  }

  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { cinema: 'sideroom' } });
  report(`shots ${shots}`);
  await setStudySurfacesWith(o, false);
  await octx.close();
});
