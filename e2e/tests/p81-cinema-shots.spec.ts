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
 * SEC-AUD-329 and SEC-AUD-330 (wave 81; D-831, D-832), on the real stack.
 *  329: every dark Cinema block (hero, title head, quick actions, player, CRT, theatre) states the six colours
 *       `--ui-status-online/idle/dnd/offline`, `--text-link` and `--sr-olive-accent`: probed in each block under the
 *       shipped theme (`sideroom`), `sideroom-broadcast`, Lounge Light and Lounge Dark.
 *  330: the hero and the title head paint their own dark ground: the pixels behind the meta line (genres) and the
 *       synopsis are sampled (a second shot with the text hidden) and their contrast against the text colour is
 *       computed, with no picture, with a picture whose request answers 404, and with a picture.
 * Local only (SR_P81_CINEMA=1); shots go to SR_P81_CINEMA_OUT. SR_P81_BASE=1 runs on the base (the four CSS files
 * held back): it measures and reports instead of asserting the fix, and writes `computed-base-*.json` and
 * `pixels-base-*.json`; the run after asserts the fix and that the shipped theme's colours and pixels equal the base's.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_CINEMA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-cinema';
const BASE = process.env.SR_P81_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
type SizeName = keyof typeof SIZES;
/** The six colours as the shipped theme (`sideroom`, themes.ts) computes them. */
const SIDE_ROOM = {
  online: 'rgb(52, 194, 95)',
  idle: 'rgb(217, 164, 65)',
  dnd: 'rgb(204, 92, 71)',
  offline: 'rgb(111, 108, 96)',
  link: 'rgb(194, 176, 143)',
  olive: 'rgb(140, 154, 106)',
};
const BLOCKS = ['.cin-hero', '.cin-title-head', '.cin-qa', '.cpl-player', '.tv-crt', '.wp-root'];

test.skip(process.env.SR_P81_CINEMA !== '1', 'local visual check only');
test.setTimeout(2_400_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };
type Pic = 'none' | 'e404' | 'pic';

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
    if ('poster_url' in o && !o.poster_url) o.poster_url = `/__p81/poster-${n.i++}.svg`;
    if ('backdrop_url' in o && !o.backdrop_url) o.backdrop_url = `/__p81/backdrop-${n.i++}.svg`;
    return o;
  }
  return v;
}

const DISCOVER: { value: unknown } = { value: null };

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
  pic: Pic,
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
  await c.route('**/__p81/*.svg', (r: Route) =>
    pic === 'e404'
      ? r.fulfill({ status: 404, body: 'nope' })
      : r.fulfill({
          status: 200,
          contentType: 'image/svg+xml',
          body: SVG(Number(/-(\d+)\.svg/.exec(r.request().url())?.[1] ?? 0)),
        }),
  );
  const fake = pic !== 'none';
  if (DISCOVER.value)
    await c.route(/\/api\/v1\/cinema\/discover/, (r: Route) =>
      r.fulfill({
        status: 200,
        contentType: 'application/json',
        json: fake ? pictured(DISCOVER.value) : DISCOVER.value,
      }),
    );
  await c.route(/\/api\/v1\/cinema\/(titles|facets|collections|search|my)/, async (r: Route) => {
    const res = await r.fetch();
    const ct = res.headers()['content-type'] ?? '';
    if (!ct.includes('json') || !fake) return r.fulfill({ response: res });
    return r.fulfill({ response: res, json: pictured(await res.json()) });
  });
  return c;
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

const themeOf = (page: Page, sel: string) =>
  page.evaluate(
    (s) => document.querySelector(s)?.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
    sel,
  );

type Six = Record<'online' | 'idle' | 'dnd' | 'offline' | 'link' | 'olive', string>;

/** The six colours as resolved inside every dark block that is on the page. */
async function probes(page: Page): Promise<Record<string, Six & { ground?: string }>> {
  return page.evaluate((blocks) => {
    const out: Record<string, Record<string, string>> = {};
    for (const b of blocks) {
      const host = document.querySelector<HTMLElement>(b);
      if (!host) continue;
      const d = document.createElement('i');
      d.style.cssText =
        'position:absolute;width:0;height:0;background-color:var(--ui-status-online);' +
        'color:var(--ui-status-idle);border:1px solid var(--ui-status-dnd);' +
        'outline-color:var(--ui-status-offline);column-rule:1px solid var(--text-link);' +
        'caret-color:var(--sr-olive-accent);';
      host.appendChild(d);
      const cs = getComputedStyle(d);
      out[b] = {
        online: cs.backgroundColor,
        idle: cs.color,
        dnd: cs.borderTopColor,
        offline: cs.outlineColor,
        link: cs.columnRuleColor,
        olive: cs.caretColor,
        ground: getComputedStyle(host).backgroundColor,
      };
      d.remove();
    }
    return out;
  }, BLOCKS) as Promise<Record<string, Six & { ground?: string }>>;
}

// ---- pixels: the viewport shot is decoded in a blank page with a canvas (no image library is installed)
let decoder: Page | null = null;
async function pixels(
  browser: Browser,
  png: Buffer,
  pts: [number, number][],
): Promise<[number, number, number][]> {
  if (!decoder) decoder = await browser.newPage();
  return decoder.evaluate(
    async ([b64, points]) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const cv = document.createElement('canvas');
      cv.width = img.width;
      cv.height = img.height;
      const cx = cv.getContext('2d')!;
      cx.drawImage(img, 0, 0);
      return (points as [number, number][]).map(([x, y]) => {
        const d = cx.getImageData(Math.round(x), Math.round(y), 1, 1).data;
        return [d[0]!, d[1]!, d[2]!] as [number, number, number];
      });
    },
    [png.toString('base64'), pts] as const,
  );
}

const parse = (c: string): [number, number, number] => {
  const m = /rgba?\(([^)]+)\)/.exec(c);
  const p = (m?.[1] ?? '0,0,0').split(/[ ,/]+/).map(Number);
  return [p[0]!, p[1]!, p[2]!];
};
const lum = ([r, g, b]: [number, number, number]) => {
  const f = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
};
const contrast = (a: [number, number, number], b: [number, number, number]) => {
  const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
};

test('SEC-AUD-329/330: dark Cinema blocks state every colour and paint their own ground', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  await setStudySurfacesWith(o, true);

  // ---- the world: three published titles, a TV channel, a watch party
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
      synopsis:
        'A fixture film for the colour check. Two friends cross a city in one night, and by morning neither of them is quite the same person who set out.',
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

  const setPrefs = async (p: Prefs) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });
    return 200;
  };

  let shots = 0;
  const shoot = async (page: Page, name: string, tag: string) => {
    const m = await measure(page);
    report(`${tag} ${name} ${JSON.stringify(m)}`);
    expect(m.sw, `${tag} ${name}: no sideways scroll (${m.past.join(' ')})`).toBeLessThanOrEqual(
      m.cw,
    );
    const buf = await page.screenshot({ path: resolve(OUT, `${name}-${tag}-${MODE}.png`) });
    shots++;
    return buf;
  };

  /**
   * The ground and the contrast behind the text of a hero or a title head: the text colour, then a second shot with
   * the text hidden, sampled at the left, middle and right of the meta line and of the synopsis.
   */
  const ground = async (page: Page, surface: string, tag: string) => {
    const sel = `${surface} .cin-hero-meta, ${surface} .cin-hero-synopsis`;
    const info = await page.evaluate(
      ([s, sf]) => {
        const out: { name: string; rect: number[]; color: string }[] = [];
        for (const [cls, name] of [
          ['.cin-hero-meta', 'meta'],
          ['.cin-hero-synopsis', 'synopsis'],
        ] as const) {
          const e = document.querySelector<HTMLElement>(`${sf} ${cls}`);
          if (!e) continue;
          const r = e.getBoundingClientRect();
          out.push({
            name,
            rect: [r.left, r.top, r.width, r.height],
            color: getComputedStyle(e).color,
          });
        }
        const host = document.querySelector<HTMLElement>(sf!);
        return {
          out,
          ground: host ? getComputedStyle(host).backgroundColor : 'none',
          s,
        };
      },
      [sel, surface] as const,
    );
    await page.addStyleTag({
      content: `${surface} .cin-hero-body, ${surface} .cin-title-body { visibility: hidden !important; }`,
    });
    const png = await page.screenshot({ path: resolve(OUT, `${tag}-textless-${MODE}.png`) });
    const res: Record<string, unknown> = { ground: info.ground };
    let worst = 99;
    for (const it of info.out) {
      const [x, y, w, h] = it.rect as [number, number, number, number];
      const pts: [number, number][] = [];
      for (const fx of [0.02, 0.5, 0.98])
        for (const fy of [0.15, 0.5, 0.85]) pts.push([x + w * fx, y + h * fy]);
      const px = await pixels(browser, png, pts);
      const text = parse(it.color);
      const cs = px.map((p) => Math.round(contrast(text, p) * 100) / 100);
      const light = px.filter((p) => lum(p) > 0.25).length;
      res[it.name] = { text: it.color, pixels: px.map((p) => p.join(',')), contrast: cs };
      worst = Math.min(worst, ...cs);
      report(
        `${tag} ${it.name}: text ${it.color}, samples (left/mid/right x top/mid/bottom) ${px
          .map((p) => `rgb(${p.join(',')})`)
          .join(' ')}, contrast ${cs.join(' ')}, light samples ${light}`,
      );
    }
    report(`${tag} ground ${info.ground}, worst contrast ${worst}`);
    res.worst = worst;
    await page.reload();
    return res;
  };

  type Comp = Record<string, unknown>;
  const pixKey = (v: unknown) => JSON.stringify(v);

  /** The probes of every block page by page (desktop only), asserted against the shipped values. */
  const probePages = async (page: Page, tag: string, theme: string, comp: Comp) => {
    const grab = async (name: string) => {
      const pr = await probes(page);
      comp[name] = pr;
      for (const [b, v] of Object.entries(pr)) {
        const { ground: g, ...six } = v;
        report(`${tag} ${name} ${b} ${JSON.stringify(six)} ground ${g}`);
        if (!BASE)
          expect(six, `${tag} ${name} ${b}: the block states the six colours`).toEqual(SIDE_ROOM);
      }
      expect(Object.keys(pr).length, `${tag} ${name}: a block was probed`).toBeGreaterThan(0);
    };
    await page.goto('/cinema');
    await expect(page.locator('.cin-hero').first()).toBeVisible({ timeout: 60_000 });
    await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', theme);
    await page.waitForTimeout(1200);
    await grab('home');
    await page.goto(`/cinema/t/${first}`);
    await expect(page.locator('.cin-title-head').first()).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(1000);
    await grab('title');
    await page.goto('/cinema/tv');
    await expect(page.locator('.tv-crt').first()).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(800);
    report(`${tag} tv page data-theme ${await themeOf(page, '.tv-crt')}`);
    await grab('tv');
    await page.goto(`/cinema/party/${partyId}`);
    await expect(page.locator('.wp-root')).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(1200);
    await grab('party');
    await page.route(/\/api\/v1\/tv\/channels\/[^/]+\/tune/, (r: Route) =>
      r.fulfill({ status: 404, contentType: 'application/json', json: { code: 404 } }),
    );
    await page.goto(`/cinema/tv/watch/${tvId}`);
    await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible({ timeout: 60_000 });
    await page.waitForTimeout(800);
    await grab('live');
    await page.unroute(/\/api\/v1\/tv\/channels\/[^/]+\/tune/);
  };

  const variants = BASE
    ? ([
        ['shipped', 'sideroom'],
        ['light', 'lounge-light'],
      ] as const)
    : ([
        ['shipped', 'sideroom'],
        ['light', 'lounge-light'],
        ['dark', 'lounge-dark'],
        ['broadcast', 'sideroom-broadcast'],
      ] as const);

  for (const [variant, sectionTheme] of variants) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: sectionTheme },
    };
    const st = await setPrefs(prefs);
    report(`${variant}: PATCH settings ${st}`);
    const sizes: SizeName[] =
      variant === 'shipped' || variant === 'light' ? ['desktop', 'phone'] : ['desktop'];
    for (const sizeName of sizes) {
      const size = SIZES[sizeName];
      const tag = `${variant}-${sizeName}`;
      const comp: Comp = {};

      // ---- the six colours in each block (desktop; the phone only shoots the ground below)
      if (sizeName === 'desktop') {
        const c = await context(browser, octx, size, prefs, 'none');
        const page = await c.newPage();
        page.setDefaultTimeout(25_000);
        const errors: string[] = [];
        page.on('pageerror', (e) => errors.push(e.message));
        await probePages(page, tag, sectionTheme, comp);
        const th = await page.evaluate(
          () => document.querySelector('.cin-root')?.getAttribute('data-theme') ?? null,
        );
        report(`${tag} data-theme read back on the party page: ${th}`);
        writeFileSync(resolve(OUT, `computed-${MODE}-${tag}.json`), JSON.stringify(comp, null, 2));
        const basePath = resolve(OUT, `computed-base-${tag}.json`);
        if (!BASE && (variant === 'shipped' || variant === 'broadcast') && existsSync(basePath)) {
          const was = JSON.parse(readFileSync(basePath, 'utf8')) as Comp;
          // the six colours equal the base's (the ground of the hero and title head changes on purpose)
          const strip = (x: Comp) => JSON.stringify(x, (k, v) => (k === 'ground' ? undefined : v));
          expect(strip(comp)).toEqual(strip(was));
          report(`${tag}: ${Object.keys(was).length} probe sets compared with the base`);
        }
        expect(errors, `${tag}: no page errors`).toEqual([]);
        await c.close();
      }

      // ---- the ground (SEC-AUD-330): home hero and title head, three picture states
      if (variant !== 'shipped' && variant !== 'light') continue;
      const pics: Pic[] = BASE
        ? ['none']
        : variant === 'light'
          ? ['none', 'e404', 'pic']
          : ['none', 'pic'];
      for (const pic of pics) {
        const c = await context(browser, octx, size, prefs, pic);
        const page = await c.newPage();
        page.setDefaultTimeout(25_000);
        for (const [surface, path, name] of [
          ['.cin-hero', '/cinema', 'home'],
          ['.cin-title-head', `/cinema/t/${first}`, 'title'],
        ] as const) {
          await page.goto(path);
          await expect(page.locator(surface).first()).toBeVisible({ timeout: 60_000 });
          await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', sectionTheme);
          await page.waitForTimeout(1600);
          const imgs = await page.locator(`${surface} .cin-hero-img`).count();
          const full = `${name}-${pic}-${tag}`;
          report(`${full}: data-theme ${await themeOf(page, surface)}, backdrop images ${imgs}`);
          await shoot(page, name, `${pic}-${tag}`);
          const g = (await ground(page, surface, full)) as {
            ground: string;
            worst: number;
          } & Record<string, unknown>;
          const p = resolve(OUT, `pixels-${MODE}-${full}.json`);
          writeFileSync(p, JSON.stringify(g, null, 2));
          if (!BASE) {
            if (pic !== 'pic')
              expect(g.worst, `${full}: contrast behind the text`).toBeGreaterThanOrEqual(4.5);
            expect(g.ground, `${full}: the surface paints its own ground`).toBe('rgb(17, 17, 16)');
            const basePath = resolve(OUT, `pixels-base-${full}.json`);
            if (variant === 'shipped' && pic === 'none' && existsSync(basePath)) {
              expect(
                pixKey({ meta: g.meta, synopsis: g.synopsis }),
                `${full}: shipped pixels equal the base's`,
              ).toBe(
                pixKey({
                  meta: (JSON.parse(readFileSync(basePath, 'utf8')) as Record<string, unknown>)
                    .meta,
                  synopsis: (JSON.parse(readFileSync(basePath, 'utf8')) as Record<string, unknown>)
                    .synopsis,
                }),
              );
              report(`${full}: pixels equal the base's`);
            }
          }
        }
        await c.close();
      }
    }
  }

  if (!BASE) {
    // ---- the guide's own "Try again" (findings only) and the theatre and Live TV in light, then the click-through
    for (const [variant, sectionTheme] of [
      ['shipped', 'sideroom'],
      ['light', 'lounge-light'],
    ] as const) {
      const prefs: Prefs = {
        theme: 'lounge-dark',
        zoom: 100,
        sectionThemes: { cinema: sectionTheme },
      };
      await setPrefs(prefs);
      for (const sizeName of ['desktop', 'phone'] as const) {
        const tag = `${variant}-${sizeName}`;
        const c = await context(browser, octx, SIZES[sizeName], prefs, 'pic');
        const page = await c.newPage();
        page.setDefaultTimeout(25_000);
        let hits = 0;
        await page.route(/\/api\/v1\/tv\/guide/, (r: Route) => {
          hits++;
          return r.fulfill({ status: 500, contentType: 'application/json', json: { code: 500 } });
        });
        await page.goto('/cinema/tv');
        const again = page.getByRole('button', { name: 'Try again' });
        await expect(again).toBeVisible({ timeout: 60_000 });
        await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', sectionTheme);
        await page.waitForTimeout(800);
        const look = await page.evaluate(() => {
          const b = Array.from(document.querySelectorAll<HTMLElement>('.tv-state button')).find(
            (x) => /try again/i.test(x.textContent ?? ''),
          );
          const p = document.querySelector<HTMLElement>('.tv-state p');
          const bs = b ? getComputedStyle(b) : null;
          const ps = p ? getComputedStyle(p) : null;
          const st = document.querySelector<HTMLElement>('.tv-state');
          const root = st?.closest<HTMLElement>('[data-theme]');
          return {
            button: bs && { bg: bs.backgroundColor, color: bs.color, border: bs.borderTopColor },
            text: ps?.color,
            page: root ? getComputedStyle(root).backgroundColor : null,
          };
        });
        report(`${tag} guide try-again ${JSON.stringify(look)}`);
        await shoot(page, 'guide-tryagain', tag);
        const before = hits;
        await again.click();
        await expect.poll(() => hits).toBeGreaterThan(before);
        report(`${tag} guide try again clicked: requests ${before} -> ${hits}`);
        await c.close();
      }
    }

    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: 'lounge-light' },
    };
    await setPrefs(prefs);
    {
      const c = await context(browser, octx, SIZES.desktop, prefs, 'pic');
      const page = await c.newPage();
      page.setDefaultTimeout(25_000);
      await page.goto(`/cinema/party/${partyId}`);
      await expect(page.locator('.wp-root')).toBeVisible({ timeout: 60_000 });
      await expect(page.locator('.cin-root')).toHaveAttribute('data-theme', 'lounge-light');
      await page.waitForTimeout(1500);
      await shoot(page, 'theatre', 'light-desktop');
      await page.route(/\/api\/v1\/tv\/channels\/[^/]+\/tune/, (r: Route) =>
        r.fulfill({ status: 404, contentType: 'application/json', json: { code: 404 } }),
      );
      await page.goto(`/cinema/tv/watch/${tvId}`);
      await expect(page.getByRole('button', { name: 'Try again' })).toBeVisible({
        timeout: 60_000,
      });
      await page.waitForTimeout(800);
      await shoot(page, 'livetv', 'light-desktop');
      await c.close();
    }
    // click through, light, both sizes: a title from the home, "My list", back
    for (const sizeName of ['desktop', 'phone'] as const) {
      const tag = `light-${sizeName}`;
      const c = await context(browser, octx, SIZES[sizeName], prefs, 'pic');
      const page = await c.newPage();
      page.setDefaultTimeout(25_000);
      await page.goto('/cinema');
      await expect(page.locator('.cin-hero').first()).toBeVisible({ timeout: 60_000 });
      await page.waitForTimeout(1200);
      await page.locator('a[href^="/cinema/t/"]').first().click();
      await expect(page.locator('.cin-title-head')).toBeVisible({ timeout: 30_000 });
      const mylist = page.getByRole('button', { name: /my list/i }).first();
      await expect(mylist).toBeVisible();
      await mylist.click();
      await page.waitForTimeout(900);
      report(
        `${tag} click-through: title opened (${page.url().replace(ORIGIN, '')}), My list pressed`,
      );
      await shoot(page, 'clickthrough-mylist', tag);
      await page.goBack();
      await expect(page.locator('.cin-hero').first()).toBeVisible({ timeout: 30_000 });
      report(`${tag} click-through: back on ${page.url().replace(ORIGIN, '')}`);
      await c.close();
    }
  }

  await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { cinema: 'sideroom' } });
  report(`shots ${shots}`);
  await setStudySurfacesWith(o, false);
  await decoder?.close();
  await octx.close();
});
