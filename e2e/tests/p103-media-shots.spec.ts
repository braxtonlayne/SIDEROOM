import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { apiPatch, apiPost, apiPut, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';
import { setStudySurfacesWith } from '../support/studySwitches';
import { apiDelete } from '../support/world';

/**
 * SEC-AUD-462, 464, 465; D-971, D-972 (wave 103, pod media: watch, media): the watch party's Speed and Season, the music
 * panel's Playlists "whose", Equalizer preset and a track row's "add to playlist", Live TV's reminders lead time move
 * to the kit's `NativeSelect` with `fit="content"`; the Study's Cover moves with no `fit`; the sheet rules that styled
 * selects by element name go (`watchparty.css`, `tv.css`, `.stu-field > select`). The dependants are measured too: the
 * People panel's two selects and the Start party dialog's (the rule's dark box is gone), the Study dialog's Title and
 * Note labels (`.stu-field > span` is narrowed). Per select: box, `fit` (the wrapper), the width of the selected
 * option's text and the chevron's box, the boxes of its row, its label, computed font-size, the document's
 * `scrollWidth` / `clientWidth`, the contrast of its text and border against the ground under it (the shared helper),
 * the hit-test rows of its section or dialog with takers. Selects are found by TAG inside a root found by role, label
 * or id, never by class. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited).
 *
 * Own data (D-942), all named `p103m ...` and removed in a `finally`: a movie and a series (published, a fake asset id),
 * a party on the movie, a Live TV channel, an own playlist. The storefront switches are put on for the run and off
 * again. Faked in the browser (page.route): the Study's shelf and files, `GET /music/library` (three tracks), the POST
 * that would add a track to the playlist, and the settings PATCH of the reminders lead time. Nothing else is saved: the
 * keyboard cases change a value and put it back.
 *
 * `EXPECT` says per select whether this wave moves it (`fill`: the kit's, full width; `content`: the kit's with
 * `fit="content"`) or leaves it `raw`. A select a code builder puts back is set to `raw` here before the last commit.
 *
 * Phases: `SR_P103_MEDIA_PHASE=base` records the unmerged tree (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same, checks each select against `EXPECT` and, when
 * `figures-base.json` is in the folder, compares with it (block size not under 40, 44 at 375 touch; a `fill` select not
 * narrower than the base; a `content` select whole text clear of the chevron and no wider than the base by more than
 * 36 px, Speed and Season not narrower either; a control below or beside moved by no more than the summed height
 * difference above it; the select's label not taller than the select's growth; a select stays in its row; nothing
 * overlapping; Study labels unchanged; the hit test equal in count and no row worse), writes `compare.json` and a
 * side-by-side picture `cmp-<scenario>-<config>.png` of base and after for each shot. Text contrast is at least 4.5:1
 * in the after phase. Configs: the Lounge's (1024 Lounge Dark, 1024 Lounge Light, 375 touch Lounge Light) for the music
 * panel; the sections' (1024 shipped, 375 touch shipped, 1024 light) for the Cinema (watch party), Live TV and the
 * Study (`sectionThemes` cinema / live-tv / library set explicitly and read back). `data-theme` is read back from the
 * page before each shot. `SR_P103_MEDIA_ONLY` is a regex on the scenario key (fix a spec error without running all).
 *
 * Local only (SR_P103_MEDIA_ON=1). SR_P103_MEDIA_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8810, Vite 5810, metrics 9510,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli). Base (the tree
 * before the two pod branches), then, on the merged tree, after:
 *   SR_E2E_BASE_URL=http://localhost:5810 SR_P103_MEDIA_ON=1 SR_P103_MEDIA_PHASE=base \
 *   SR_P103_MEDIA_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-media \
 *   npx playwright test tests/p103-media-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *   (the same with SR_P103_MEDIA_PHASE=after)
 * With no base, delete `figures-base.json` first if the folder holds one from another tree.
 */
const ON = process.env.SR_P103_MEDIA_ON === '1';
const PHASE = process.env.SR_P103_MEDIA_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_MEDIA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-media';
const ONLY = process.env.SR_P103_MEDIA_ONLY ? new RegExp(process.env.SR_P103_MEDIA_ONLY) : null;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);

type Family = 'lounge' | 'section';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  family: Family;
  main: string;
  /** sectionThemes the account gets (all three set explicitly). */
  sections: { cinema: string; library: string; 'live-tv': string };
}
const SHIPPED = { cinema: 'sideroom', library: 'sideroom', 'live-tv': 'sideroom-broadcast' };
const LIGHT = { cinema: 'lounge-light', library: 'lounge-light', 'live-tv': 'lounge-light' };
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    family: 'lounge',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    family: 'lounge',
    main: 'lounge-light',
    sections: SHIPPED,
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    family: 'lounge',
    main: 'lounge-light',
    sections: SHIPPED,
  },
  {
    tag: '1024-shipped',
    width: 1024,
    height: 768,
    touch: false,
    family: 'section',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '375-shipped',
    width: 375,
    height: 812,
    touch: true,
    family: 'section',
    main: 'lounge-dark',
    sections: SHIPPED,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    family: 'section',
    main: 'lounge-dark',
    sections: LIGHT,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

// ------------------------------------------------------------------------------------------------ the data

interface Ids {
  guildId: string;
  textCh: string;
  movieId: string | null;
  seriesId: string | null;
  seriesName: string;
  partyId: string | null;
  emptyPartyId: string | null;
  tvId: string | null;
  playlistId: string | null;
  playlistName: string;
}

const json = (route: Route, body: unknown, status = 200) =>
  route.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function makeData(o: BrowserContext['request'], notes: string[]): Promise<Ids> {
  const w = world();
  const ids: Ids = {
    guildId: w.guildId,
    textCh: w.channelId,
    movieId: null,
    seriesId: null,
    seriesName: `p103m series ${STAMP}`,
    partyId: null,
    emptyPartyId: null,
    tvId: null,
    playlistId: null,
    playlistName: `p103m playlist ${STAMP}`,
  };
  const attempt = async <T>(what: string, fn: () => Promise<T>): Promise<T | null> => {
    try {
      return await fn();
    } catch (e) {
      notes.push(`${what}: ${String(e).slice(0, 220)}`);
      return null;
    }
  };
  await attempt('storefront switches', () => setStudySurfacesWith(o, true));
  const title = (kind: 'movie' | 'series', name: string) =>
    attempt(`title ${name}`, async () => {
      const t = await apiPost<{ id: string }>(o, '/admin/cinema/titles', {
        kind,
        title: name,
        year: 2001,
        runtime_min: 118,
        content_rating: 'PG',
        synopsis: 'A fixture for the pod media select check.',
        genres: ['Drama'],
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
      return t.id;
    });
  ids.movieId = await title('movie', `p103m movie ${STAMP}`);
  ids.seriesId = await title('series', ids.seriesName);
  if (ids.movieId)
    ids.partyId = await attempt('party on the movie', async () => {
      const p = await apiPost<{ id: string }>(o, '/watch-parties', { title_id: ids.movieId });
      return p.id;
    });
  ids.emptyPartyId = await attempt('empty party', async () => {
    const p = await apiPost<{ id: string }>(o, '/watch-parties', {});
    return p.id;
  });
  ids.tvId = await attempt('tv channel', async () => {
    const c = await apiPost<{ id: string }>(o, '/admin/tv/channels', {
      number: 700 + (Date.now() % 250),
      name: `p103m TV ${STAMP}`,
      kind: 'manual',
      timezone: 'UTC',
      enabled: true,
    });
    return c.id;
  });
  ids.playlistId = await attempt('playlist', async () => {
    const p = await apiPost<{ id: string }>(o, '/users/@me/music/playlists', {
      name: ids.playlistName,
    });
    return p.id;
  });
  say(`data ${JSON.stringify(ids)} notes ${JSON.stringify(notes)}`);
  return ids;
}

async function removeData(o: BrowserContext['request'], ids: Ids) {
  const del = async (p: string) => apiDelete(o, p).catch(() => undefined);
  if (ids.partyId) await del(`watch-parties/${ids.partyId}`);
  if (ids.emptyPartyId) await del(`watch-parties/${ids.emptyPartyId}`);
  if (ids.tvId) await del(`admin/tv/channels/${ids.tvId}`);
  if (ids.playlistId) await del(`music/playlists/${ids.playlistId}`);
  for (const t of [ids.movieId, ids.seriesId])
    if (t) {
      await apiPost(o, `/admin/cinema/titles/${t}/unpublish`).catch(() => undefined);
      await del(`admin/cinema/titles/${t}`);
    }
  await setStudySurfacesWith(o, false).catch(() => undefined);
}

// ------------------------------------------------------------------------------------------------ the browser

async function setPrefs(ownerCtx: BrowserContext, c: Cfg) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: c.main, zoom: 100, sectionThemes: c.sections } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: {
      user_prefs?: { theme?: string; sectionThemes?: Record<string, string> };
    };
  };
  const p = back.settings?.user_prefs;
  expect(p?.theme).toBe(c.main);
  for (const k of ['cinema', 'library', 'live-tv'] as const)
    expect(p?.sectionThemes?.[k]).toBe(c.sections[k]);
}

async function newContext(browser: Browser, ownerCtx: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: c.main, zoom: 100, sectionThemes: c.sections },
  );
  return ctx;
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface SelFig {
  key: string;
  box: Box;
  label: Box | null;
  options: number;
  /** The kit's wrapper span (its class), or null for a raw select. */
  wrapper: string | null;
  wrapStyle: {
    display: string;
    width: number;
    maxWidth: string;
    textTransform: string;
    letterSpacing: string;
  } | null;
  disabled: boolean;
  fontSize: string;
  labelFontSize: string | null;
  /** Width of the selected option's text and of the longest option's text, in the select's own font. */
  textW: number;
  longestW: number;
  /** Where the text starts inside the select (border + padding), and the chevron's box (the kit's, 14 px from the end, 6 px). */
  textStart: number;
  chevron: Box | null;
  /** The controls that share the select's row (vertical overlap over half the smaller height). */
  row: { key: string; box: Box }[];
  /** CSS colours: the select's text, its border, and every background from the select up to the root element. */
  colors: { text: string; border: string; bgs: string[] };
  contrast?: {
    text: number;
    border: number;
    fill: number;
    ground: string;
    fillColor: string;
    textColor: string;
  };
}
interface LabelFig {
  text: string;
  fontSize: string;
  textTransform: string;
  letterSpacing: string;
  fontWeight: string;
  color: string;
  box: Box;
}
interface Measured {
  labels: LabelFig[];
  themes: string[];
  scroll: number[];
  rootScroll: number[];
  root: Box | null;
  selects: SelFig[];
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { status: string; control: string; takers: string[] }[];
  opened: unknown[];
  keyboard?: Record<string, unknown>;
  shot?: string;
}

/** Boxes in the root's coordinates are page coordinates plus every scrolled ancestor's offset (a scrolled page compares with a top one). */
async function measure(root: Locator): Promise<Measured> {
  return root.evaluate((rootEl) => {
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      let sx = 0;
      let sy = 0;
      for (let p: Element | null = e.parentElement; p; p = p.parentElement) {
        sx += p.scrollLeft;
        sy += p.scrollTop;
      }
      return {
        x: Math.round((r.left + sx) * 10) / 10,
        y: Math.round((r.top + sy) * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const els = Array.from(
      rootEl.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    // The accessible-ish name: aria-label, else the enclosing label's own text with every select taken out, else the text.
    const name = (e: HTMLElement) => {
      const al = e.getAttribute('aria-label');
      if (al) return al.trim().slice(0, 40);
      const lab = e.closest('label');
      if (lab) {
        const c = lab.cloneNode(true) as HTMLElement;
        c.querySelectorAll('select').forEach((s) => s.remove());
        const tx = (c.textContent ?? '').trim();
        if (tx) return tx.slice(0, 40);
      }
      return (e.id || e.textContent || '').trim().slice(0, 40);
    };
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}:${name(e)}`,
      box: box(e),
    }));
    const selEls = els.filter((e) => e.tagName === 'SELECT') as HTMLSelectElement[];
    const canvas = document.createElement('canvas').getContext('2d')!;
    const selects = selEls.map((e) => {
      const lab = e.closest('label');
      const par = e.parentElement;
      const cs = getComputedStyle(e);
      const wrapped = !!par && par.tagName === 'SPAN' && par !== lab;
      const wcs = wrapped && par ? getComputedStyle(par) : null;
      canvas.font = cs.font;
      const textOf = (o: HTMLOptionElement | undefined) => (o?.textContent ?? '').trim();
      const sel = e.selectedOptions[0];
      const textW = canvas.measureText(textOf(sel)).width;
      const longestW = Math.max(
        0,
        ...Array.from(e.options).map((o) => canvas.measureText(textOf(o)).width),
      );
      const b = box(e);
      const wb = wrapped && par ? par.getBoundingClientRect() : null;
      const chevron = wb
        ? (() => {
            const wr = box(par as Element);
            return {
              x: Math.round((wr.x + wr.w - 20) * 10) / 10,
              y: wr.y + wr.h / 2 - 3,
              w: 6,
              h: 6,
            };
          })()
        : null;
      const bgs: string[] = [];
      for (let q: Element | null = e; q; q = q.parentElement)
        bgs.push(getComputedStyle(q).backgroundColor);
      const row = els
        .filter((o) => o !== e && !e.contains(o) && !o.contains(e))
        .map((o) => ({
          key: `${els.indexOf(o)}:${o.tagName.toLowerCase()}:${name(o)}`,
          box: box(o),
        }))
        .filter((o) => {
          const top = Math.max(o.box.y, b.y);
          const bot = Math.min(o.box.y + o.box.h, b.y + b.h);
          return bot - top > 0.5 * Math.min(o.box.h, b.h);
        });
      return {
        key: `${els.indexOf(e)}:select:${name(e)}`,
        box: b,
        label: lab ? box(lab) : null,
        options: e.options.length,
        wrapper: wrapped && par ? par.className || 'span' : null,
        wrapStyle: wcs
          ? {
              display: wcs.display,
              width: Math.round((wb?.width ?? 0) * 10) / 10,
              maxWidth: wcs.maxWidth,
              textTransform: wcs.textTransform,
              letterSpacing: wcs.letterSpacing,
            }
          : null,
        disabled: e.disabled,
        fontSize: cs.fontSize,
        labelFontSize: lab ? getComputedStyle(lab).fontSize : null,
        textW: Math.round(textW * 10) / 10,
        longestW: Math.round(longestW * 10) / 10,
        textStart:
          Math.round((parseFloat(cs.paddingLeft) + parseFloat(cs.borderLeftWidth)) * 10) / 10,
        chevron,
        row,
        colors: { text: cs.color, border: cs.borderTopColor, bgs },
      };
    });
    const labels = Array.from(rootEl.querySelectorAll<HTMLElement>('label'))
      .filter((l) => l.getClientRects().length > 0)
      .map((l) => {
        const sp = l.firstElementChild as HTMLElement | null;
        if (!sp || sp.tagName !== 'SPAN') return null;
        const c = getComputedStyle(sp);
        return {
          text: (sp.textContent ?? '').trim().slice(0, 30),
          fontSize: c.fontSize,
          textTransform: c.textTransform,
          letterSpacing: c.letterSpacing,
          fontWeight: c.fontWeight,
          color: c.color,
          box: box(sp),
        };
      })
      .filter((x): x is NonNullable<typeof x> => !!x && x.text !== '');
    const overlaps: string[] = [];
    for (const s of selEls) {
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `select(${name(s)}) x ${o.tagName.toLowerCase()}(${name(o)}) ${Math.round(ix)}x${Math.round(iy)}`,
          );
      }
    }
    const themes = Array.from(
      new Set(
        [
          document.documentElement,
          ...Array.from(document.querySelectorAll('[data-theme]')).slice(0, 6),
        ]
          .map((e) => e.getAttribute('data-theme'))
          .filter((x): x is string => !!x),
      ),
    );
    return {
      labels,
      themes,
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      rootScroll: [(rootEl as HTMLElement).scrollWidth, (rootEl as HTMLElement).clientWidth],
      root: box(rootEl),
      selects,
      controls,
      overlaps,
    };
  });
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function hit(page: Page, root: Locator, tag: string, problems: string[]) {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const h = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status as string,
      control: `${r.tag} "${r.name}"`,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => `${p.at}:${p.taker ? `${p.taker.tag}"${p.taker.name}"` : 'null'}`),
    }));
  say(`${tag}: hit ${JSON.stringify(h)}`);
  if (h.notReached) problems.push(`${tag}: ${h.notReached} controls not reached (probe error)`);
  return { h, hitFails, opened };
}

async function shot(page: Page, root: Locator, firstSelect: Locator | null, name: string) {
  mkdirSync(OUT, { recursive: true });
  if (firstSelect) await firstSelect.scrollIntoViewIfNeeded().catch(() => undefined);
  await page.waitForTimeout(500);
  const vp = page.viewportSize() ?? { width: 1024, height: 768 };
  const b = await root.boundingBox().catch(() => null);
  let clip: { x: number; y: number; width: number; height: number } | undefined;
  if (b) {
    const x = Math.max(0, b.x);
    const y = Math.max(0, b.y);
    const width = Math.min(vp.width, b.x + b.width) - x;
    const height = Math.min(vp.height, b.y + b.height) - y;
    if (width >= 200 && height >= 120) clip = { x, y, width, height };
  }
  const file = resolve(OUT, `${shotName(name)}.png`);
  await page.screenshot({ path: file, timeout: 10_000, ...(clip ? { clip } : {}) });
  return `${shotName(name)}.png`;
}

/** base | after side by side, drawn in a scratch page. */
async function composite(browser: Browser, name: string) {
  const a = resolve(OUT, `base-${name}.png`);
  const b = resolve(OUT, `${name}.png`);
  if (!existsSync(a) || !existsSync(b)) return;
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 800 } });
  try {
    const p = await ctx.newPage();
    const img = (f: string) => `data:image/png;base64,${readFileSync(f).toString('base64')}`;
    await p.setContent(
      `<body style="margin:0;background:#777;font:14px sans-serif;color:#fff"><div style="display:flex;gap:10px;align-items:flex-start;padding:6px">` +
        `<div><div>BASE</div><img src="${img(a)}"></div><div><div>AFTER</div><img src="${img(b)}"></div></div></body>`,
    );
    await p.waitForFunction(() => Array.from(document.images).every((i) => i.complete));
    await p.screenshot({ path: resolve(OUT, `cmp-${name}.png`), fullPage: true });
  } finally {
    await ctx.close();
  }
}

// ------------------------------------------------------------------------------------------------ scenarios

interface Kbd {
  pick: (root: Locator) => Locator;
  /** The select's value does not change by itself (a "pick one to add" select): a request is the effect. */
  noChange?: boolean;
  settleMs?: number;
}
interface Scenario {
  key: string;
  files: string[];
  family: Family;
  /** The section whose theme `data-theme` must show (section family). */
  section?: 'cinema' | 'library' | 'live-tv';
  /** Scenarios of one group share one page per config. */
  group: string;
  minSelects: number;
  skip?: (c: Cfg) => string | null;
  open: (page: Page, c: Cfg, ids: Ids, first: boolean, log: string[]) => Promise<Locator>;
  /** The select to scroll to before the shot. */
  focus?: (root: Locator) => Locator;
  kbd?: Kbd;
}

const SIGNAL = { timeout: 25_000 };
const goto = (page: Page, path: string) =>
  page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};

const MUSIC_TRACKS = [1, 2, 3].map((n) => ({
  id: `p103m-t${n}`,
  mount_id: '1',
  title: n === 1 ? 'Midnight Drive With The Windows Down' : `Tone ${n}`,
  artist: 'E2E',
  album: 'Fixtures',
  duration_ms: 90_000,
  genres: [],
  tags: [],
}));
const PLAY_TRACKS = [1, 2, 3].map((n) => ({
  id: `p103m-p${n}`,
  title: `Tone ${n}`,
  artist: 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
type PlayerWin = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
};

async function openPlayer(page: Page, skin: string, ids: Ids) {
  await serveTone(page);
  await page.route('**/api/v1/music/library*', (r) =>
    r.request().method() === 'GET'
      ? json(r, { items: MUSIC_TRACKS, total: MUSIC_TRACKS.length })
      : r.fallback(),
  );
  await page.route(/\/api\/v1\/music\/playlists\/[^/]+\/items$/, (r) =>
    r.request().method() === 'POST' ? json(r, {}) : r.fallback(),
  );
  void ids;
  await goto(page, '/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(() => (window as unknown as Partial<PlayerWin>).__srPlayerActions, {
    timeout: 30_000,
  });
  await page.evaluate(
    (t) => (window as unknown as PlayerWin).__srPlayerActions.play(t),
    PLAY_TRACKS,
  );
  await page.waitForFunction(() => (window as unknown as Partial<PlayerWin>).__srPlayer);
  await page.evaluate((k) => (window as unknown as PlayerWin).__srPlayerActions.setSkin(k), skin);
  await page.locator(`.sr-ply[data-skin="${skin}"]`).waitFor(SIGNAL);
  await page.waitForTimeout(1200);
}

async function musicTab(page: Page, c: Cfg, ids: Ids, first: boolean, tab: string) {
  if (first) await openPlayer(page, 'library', ids);
  await page.locator(`#sr-mlib-tab-${tab}`).click();
  await page.waitForTimeout(500);
  void c;
  return page.locator('.sr-mlib').first();
}

const STUDY_FILES = [
  { key: 'Films/heat.mp4', size: 1000 },
  { key: 'Music/blue.mp3', size: 2000 },
  { key: 'Covers/one.png', size: 3000 },
  { key: 'Covers/two.png', size: 4000 },
];
async function fakeStudy(page: Page) {
  const API = '**/api/v1';
  await page.route(`${API}/shelves/visible`, (r) =>
    json(r, {
      shelves: [{ shelf_id: '9501', label: 'p103m Home', via: 'own', object_count: 4 }],
    }),
  );
  await page.route(`${API}/storage/mounts/*/objects*`, (r) =>
    json(r, {
      folders: [],
      objects: STUDY_FILES.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: true,
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
    json(r, {
      objects: STUDY_FILES.map((f) => ({
        object_hash: 'a'.repeat(64),
        path: f.key,
        size: f.size,
        class: 'other',
        title: '',
        cover: '',
        note: '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: true,
    }),
  );
  await page.route('https://bucket.invalid/**', (r) => r.fulfill({ status: 404 }));
}

/** A second visit finds the host out of the party ("You are not in this party any more"): rejoin. */
async function ensureIn(page: Page) {
  const rejoin = page.getByRole('button', { name: 'Rejoin the party' });
  await rejoin.waitFor({ state: 'visible', timeout: 3000 }).catch(() => undefined);
  if (await rejoin.isVisible().catch(() => false)) {
    await rejoin.click();
    await page.waitForTimeout(1500);
  }
}

const dialogOf = (page: Page, name: string | RegExp) => page.getByRole('dialog', { name }).last();

const SCENARIOS: Scenario[] = [
  {
    key: 'watch-theatre',
    files: ['watchparty/PeoplePanel.tsx (x2)', 'watchparty/ControlBar.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-theatre',
    minSelects: 3,
    async open(page, _c, ids) {
      if (!ids.partyId) throw new Error('not reached: no party on the movie could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      await page.getByRole('tab', { name: 'People' }).click();
      await page.locator('.wp-settings select').first().waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-root');
    },
    focus: (root) => root.locator('.wp-settings select').first(),
    kbd: { pick: (root) => root.locator('.wp-settings select').first(), settleMs: 900 },
  },
  {
    key: 'watch-speed',
    files: ['watchparty/ControlBar.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-speed',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.partyId) throw new Error('not reached: no party on the movie could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      const bar = page.getByRole('combobox', { name: 'Speed' });
      await bar.waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-stage-wrap');
    },
    focus: (root) => root.getByRole('combobox', { name: 'Speed' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Speed' }), settleMs: 900 },
  },
  {
    key: 'watch-picker',
    files: ['watchparty/TitlePicker.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-picker',
    minSelects: 1,
    async open(page, _c, ids) {
      if (!ids.partyId || !ids.seriesId)
        throw new Error('not reached: no party or series could be made');
      await goto(page, `/cinema/party/${ids.partyId}`);
      await page.locator('.wp-root').waitFor({ timeout: 40_000 });
      await ensureIn(page);
      await page.getByRole('tab', { name: /Queue/ }).click();
      await page.getByRole('button', { name: 'Add to queue' }).first().click();
      const box = page.getByRole('textbox', { name: 'Search the cinema' });
      await box.waitFor({ timeout: 40_000 });
      await box.fill(ids.seriesName);
      const ep = page.getByRole('button', { name: /episode/i }).first();
      await ep.waitFor(SIGNAL);
      await ep.click();
      await page.getByRole('combobox', { name: 'Season' }).waitFor(SIGNAL);
      await settled(page);
      return page.locator('.wp-picker').first();
    },
    focus: (root) => root.getByRole('combobox', { name: 'Season' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Season' }) },
  },
  {
    key: 'watch-start',
    files: ['watchparty/StartPartyDialog.tsx'],
    family: 'section',
    section: 'cinema',
    group: 'watch-start',
    minSelects: 1,
    async open(page, _c, ids) {
      const id = ids.movieId ?? ids.seriesId;
      if (!id) throw new Error('not reached: no title could be made');
      await goto(page, `/cinema/t/${id}`);
      await page.getByRole('button', { name: 'Watch together' }).first().click({ timeout: 40_000 });
      const dlg = dialogOf(page, 'Watch together');
      await dlg.getByRole('combobox', { name: 'Where to watch' }).waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.getByRole('combobox', { name: 'Where to watch' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Where to watch' }) },
  },
  {
    key: 'tv-reminders',
    files: ['cinema/tv/RemindersCard.tsx'],
    family: 'section',
    section: 'live-tv',
    group: 'tv-reminders',
    minSelects: 1,
    async open(page, _c, _ids, _first, log) {
      await page.route('**/api/v1/users/@me/settings', async (r) => {
        if (r.request().method() !== 'PATCH') return r.fallback();
        const body = r.request().postDataJSON() as {
          settings?: { tv?: { reminder_lead_minutes?: number } };
        };
        log.push('reminders PATCH faked');
        return json(r, { version: 9, settings: { tv: body.settings?.tv ?? {} } });
      });
      await goto(page, '/cinema/tv');
      await page.locator('.tv-reminders select').first().waitFor({ timeout: 40_000 });
      await settled(page);
      return page.locator('.tv-reminders').first();
    },
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first(), settleMs: 1600 },
  },
  {
    key: 'study-edit',
    files: ['study/StudyDialogs.tsx'],
    family: 'section',
    section: 'library',
    group: 'study-edit',
    minSelects: 1,
    async open(page) {
      await fakeStudy(page);
      await goto(page, '/study');
      const edit = page.getByRole('button', { name: /^Edit details of / }).first();
      await edit.waitFor({ timeout: 40_000 });
      await edit.click();
      const dlg = dialogOf(page, 'Edit details');
      await dlg.getByRole('combobox', { name: 'Cover' }).waitFor(SIGNAL);
      await settled(page);
      return dlg;
    },
    focus: (root) => root.getByRole('combobox', { name: 'Cover' }),
    kbd: { pick: (root) => root.getByRole('combobox', { name: 'Cover' }) },
  },
  {
    key: 'music-playlists',
    files: ['music/panel/sections.tsx (Playlists, whose)'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    open: (page, c, ids, first) => musicTab(page, c, ids, first, 'playlists'),
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'music-eq',
    files: ['music/panel/sections.tsx (Equalizer, preset)'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    open: (page, c, ids, first) => musicTab(page, c, ids, first, 'eq'),
    focus: (root) => root.locator('select').first(),
    kbd: { pick: (root) => root.locator('select').first() },
  },
  {
    key: 'music-tracks',
    files: ['music/panel/TrackList.tsx'],
    family: 'lounge',
    group: 'music',
    minSelects: 1,
    async open(page, c, ids, first) {
      if (!ids.playlistId) throw new Error('not reached: no own playlist could be made');
      const root = await musicTab(page, c, ids, first, 'search');
      const more = root.getByRole('button', { name: /^More for / }).first();
      await more.waitFor(SIGNAL);
      await more.click();
      await root.getByRole('combobox', { name: /^Add .* to a playlist/ }).waitFor(SIGNAL);
      await page.waitForTimeout(400);
      return root;
    },
    focus: (root) => root.getByRole('combobox', { name: /^Add .* to a playlist/ }).first(),
    kbd: {
      pick: (root) => root.getByRole('combobox', { name: /^Add .* to a playlist/ }).first(),
      noChange: true,
      settleMs: 600,
    },
  },
];

// ------------------------------------------------------------------------------------------------ expectations

type Mode = 'fill' | 'content' | 'raw';
interface Expect {
  scenario: string;
  name: RegExp;
  /** What this wave does to it. Set to 'raw' when a code builder puts a select back. */
  mode: Mode;
  file: string;
  /** Speed and Season: not narrower than the base (the wave 102 fault). */
  notNarrower?: boolean;
}
const EXPECT: Expect[] = [
  {
    scenario: 'watch-theatre',
    name: /Who can play/,
    mode: 'fill',
    file: 'watchparty/PeoplePanel.tsx',
  },
  {
    scenario: 'watch-theatre',
    name: /Who can add/,
    mode: 'fill',
    file: 'watchparty/PeoplePanel.tsx',
  },
  {
    scenario: 'watch-theatre',
    name: /:Playback speed$/i,
    mode: 'content',
    file: 'watchparty/ControlBar.tsx',
    notNarrower: true,
  },
  {
    scenario: 'watch-speed',
    name: /:Playback speed$/i,
    mode: 'content',
    file: 'watchparty/ControlBar.tsx',
    notNarrower: true,
  },
  {
    scenario: 'watch-picker',
    name: /:Season$/,
    mode: 'content',
    file: 'watchparty/TitlePicker.tsx',
    notNarrower: true,
  },
  {
    scenario: 'watch-start',
    name: /Where to watch/,
    mode: 'fill',
    file: 'watchparty/StartPartyDialog.tsx',
  },
  { scenario: 'tv-reminders', name: /./, mode: 'content', file: 'cinema/tv/RemindersCard.tsx' },
  { scenario: 'study-edit', name: /Cover/, mode: 'fill', file: 'study/StudyDialogs.tsx' },
  { scenario: 'music-playlists', name: /./, mode: 'content', file: 'music/panel/sections.tsx' },
  { scenario: 'music-eq', name: /./, mode: 'content', file: 'music/panel/sections.tsx' },
  {
    scenario: 'music-tracks',
    name: /:Add .*/,
    mode: 'content',
    file: 'music/panel/TrackList.tsx',
  },
];
const expectFor = (scenario: string, key: string) =>
  EXPECT.find((e) => e.scenario === scenario && e.name.test(key));

/** The ground under a select: its ancestors' backgrounds laid over white; then fill, text and border over it. */
function addContrast(m: Measured, problems: string[], tag: string) {
  for (const sel of m.selects) {
    try {
      const layers = sel.colors.bgs.map(parseColor);
      const white: Rgba = { r: 255, g: 255, b: 255, a: 1 };
      const fold = (ls: Rgba[]) => ls.reduceRight((acc, l) => over(l, acc), white);
      const ground = fold(layers.slice(1));
      const fill = over(layers[0]!, ground);
      const text = over(parseColor(sel.colors.text), fill);
      const border = over(parseColor(sel.colors.border), fill);
      const hex = (c: Rgba) =>
        '#' + [c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('');
      sel.contrast = {
        text: Math.round(ratio(text, fill) * 100) / 100,
        border: Math.round(ratio(border, ground) * 100) / 100,
        fill: Math.round(ratio(fill, ground) * 100) / 100,
        ground: hex(ground),
        fillColor: hex(fill),
        textColor: hex(text),
      };
    } catch (e) {
      problems.push(`${tag}: ${sel.key}: colour unreadable: ${String(e).slice(0, 120)}`);
    }
  }
}

/** What this wave expects of each select, on its own (no base needed). */
function checkExpect(scenario: string, c: Cfg, m: Measured, problems: string[], tag: string) {
  for (const sel of m.selects) {
    const ex = expectFor(scenario, sel.key);
    if (!ex) {
      problems.push(`${tag}: ${sel.key} has no entry in EXPECT`);
      continue;
    }
    if (ex.mode === 'raw') {
      if (sel.wrapper) problems.push(`${tag}: ${sel.key} should be raw but has the kit's wrapper`);
      continue;
    }
    if (!sel.wrapper) {
      problems.push(
        `${tag}: ${sel.key} (${ex.file}) is still raw, expected the kit's ${ex.mode} select`,
      );
      continue;
    }
    if (sel.box.h < 40 - 0.5)
      problems.push(`${tag}: ${sel.key} is ${sel.box.h} px tall (under 40)`);
    if (c.touch && sel.box.h < 44 - 0.5)
      problems.push(`${tag}: ${sel.key} is ${sel.box.h} px tall at 375 touch (under 44)`);
    if (
      sel.wrapStyle &&
      (sel.wrapStyle.textTransform === 'uppercase' || sel.wrapStyle.letterSpacing !== 'normal')
    )
      problems.push(
        `${tag}: ${sel.key}: the kit's wrapper is styled as a label (${sel.wrapStyle.textTransform}, ${sel.wrapStyle.letterSpacing})`,
      );
    if (ex.mode === 'content' && sel.chevron) {
      const room = sel.chevron.x - sel.box.x - (sel.textStart + sel.textW);
      if (room < 4)
        problems.push(
          `${tag}: ${sel.key}: the selected text (${sel.textW} px) is not clear of the chevron (room ${room.toFixed(1)} px, select ${sel.box.w} px)`,
        );
    }
    if (!sel.disabled && sel.contrast && sel.contrast.text < 4.5)
      problems.push(`${tag}: ${sel.key}: text contrast ${sel.contrast.text}:1 (under 4.5)`);
    if (scenario === 'watch-speed') {
      const cy = sel.box.y + sel.box.h / 2;
      for (const n of sel.row) {
        if (n.box.h < 24) continue;
        const d = Math.abs(n.box.y + n.box.h / 2 - cy);
        if (d > 2.5)
          problems.push(
            `${tag}: the bar's row is not centred: ${n.key} centre is ${d.toFixed(1)} px off the select's`,
          );
      }
    }
  }
}

// ------------------------------------------------------------------------------------------------ one scenario

async function keyboard(
  page: Page,
  root: Locator,
  k: Kbd,
  tag: string,
  requests: string[],
  problems: string[],
) {
  const sel = k.pick(root);
  const settle = k.settleMs ?? 350;
  const n = await sel.locator('option').count();
  await page.keyboard.press('Shift');
  await sel.focus();
  const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
  const ring = await sel.evaluate((e) => {
    const cs = getComputedStyle(e);
    return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} off ${cs.outlineOffset} shadow ${cs.boxShadow}`;
  });
  const before = await sel.inputValue();
  const reqBefore = requests.length;
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(settle);
  const changed = await sel.inputValue();
  const reqAfter = requests.length - reqBefore;
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(settle);
  const back = await sel.inputValue();
  const out = { options: n, focusVisible, ring, before, changed, back, requests: reqAfter };
  say(`${tag}: keyboard ${JSON.stringify(out)}`);
  if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
  if (PHASE === 'after' && !/^2px solid .* shadow none$/.test(ring))
    problems.push(`${tag}: the focus ring is not a single 2px ring: ${ring}`);
  if (k.noChange) {
    if (reqAfter < 1) problems.push(`${tag}: ArrowDown made no request (the page did not react)`);
  } else {
    if (n > 1 && changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
    if (back !== before) problems.push(`${tag}: ArrowUp did not restore the value`);
  }
  return out;
}

async function runScenario(
  s: Scenario,
  c: Cfg,
  page: Page,
  ids: Ids,
  first: boolean,
  problems: string[],
  reached: Record<string, string>,
  requests: string[],
): Promise<Figures | null> {
  const tag = `${s.key} ${c.tag}`;
  const skip = s.skip?.(c);
  if (skip) {
    reached[`${s.key}/${c.tag}`] = `skipped: ${skip}`;
    say(`${tag}: skipped (${skip})`);
    return null;
  }
  const log: string[] = [];
  let root: Locator;
  try {
    root = await s.open(page, c, ids, first, log);
  } catch (e) {
    const msg = String(e).split('\n')[0]!.slice(0, 300);
    reached[`${s.key}/${c.tag}`] = `not reached: ${msg}`;
    say(`${tag}: not reached: ${msg}`);
    if (!msg.includes('not reached:')) problems.push(`${tag}: spec could not open it: ${msg}`);
    return null;
  }
  const m = await measure(root);
  addContrast(m, problems, tag);
  if (PHASE === 'after') checkExpect(s.key, c, m, problems, tag);
  say(`${tag}: root ${JSON.stringify(m.root)} selects ${JSON.stringify(m.selects)}`);
  reached[`${s.key}/${c.tag}`] = `reached: ${m.selects.length} selects`;
  if (m.selects.length < s.minSelects)
    problems.push(`${tag}: ${m.selects.length} selects, wanted ${s.minSelects}`);
  // data-theme read back
  const wantTheme = s.family === 'lounge' ? c.main : c.sections[s.section ?? 'cinema'];
  say(`${tag}: data-theme ${JSON.stringify(m.themes)} wanted ${wantTheme}`);
  if (s.section !== 'library' && !m.themes.includes(wantTheme))
    problems.push(`${tag}: data-theme is ${JSON.stringify(m.themes)}, wanted ${wantTheme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);
  const h = await hit(page, root, tag, problems);
  const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
  // The shot: the select in view, the pointer resting on nothing.
  await page.mouse.move(1, 1);
  fig.shot = await shot(page, root, s.focus ? s.focus(root) : null, `${s.key}-${c.tag}`);
  if (s.kbd && (c.tag === '1024-lounge-dark' || c.tag === '1024-shipped')) {
    try {
      fig.keyboard = await keyboard(page, root, s.kbd, tag, requests, problems);
    } catch (e) {
      problems.push(`${tag}: keyboard step failed: ${String(e).split('\n')[0]!.slice(0, 240)}`);
    }
  }
  if (log.length) say(`${tag}: ${log.join('; ')}`);
  return fig;
}

// ------------------------------------------------------------------------------------------------ compare

function compare(label: string, base: Figures, after: Figures, problems: string[]) {
  const diffs: Record<string, unknown> = {};
  if (base.selects.length !== after.selects.length) {
    problems.push(`${label}: select count ${base.selects.length} -> ${after.selects.length}`);
    return diffs;
  }
  base.selects.forEach((b, i) => {
    const a = after.selects[i]!;
    diffs[b.key] = {
      base: b.box,
      after: a.box,
      wrapper: a.wrapper,
      labelBase: b.label,
      labelAfter: a.label,
    };
    const ex = expectFor(label.split('/')[0]!, b.key);
    if (ex?.mode === 'raw') {
      if (Math.abs(a.box.w - b.box.w) > 0.5 || Math.abs(a.box.h - b.box.h) > 0.5)
        problems.push(
          `${label}: ${b.key} is raw and changed (${b.box.w}x${b.box.h} -> ${a.box.w}x${a.box.h})`,
        );
    } else {
      if (a.box.h < b.box.h - 0.5)
        problems.push(`${label}: ${b.key} shorter than base (${a.box.h} < ${b.box.h})`);
      if (ex?.mode === 'content') {
        if (a.box.w > b.box.w + 36 + 0.5)
          problems.push(
            `${label}: ${b.key} wider than base by more than 36 px (${b.box.w} -> ${a.box.w})`,
          );
        if (ex.notNarrower && a.box.w < b.box.w - 0.5)
          problems.push(`${label}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
      } else if (a.box.w < b.box.w - 0.5)
        problems.push(`${label}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
    }
    if (b.row.length > 0 && a.row.length === 0)
      problems.push(
        `${label}: ${b.key} left its row (base row: ${b.row.map((r) => r.key).join(', ')})`,
      );
    diffs[b.key] = {
      ...(diffs[b.key] as object),
      rowBase: b.row.length,
      rowAfter: a.row.length,
      contrastBase: b.contrast,
      contrastAfter: a.contrast,
      textW: [b.textW, a.textW],
    };
    if (b.label && a.label) {
      const grew = a.label.h - b.label.h;
      const selGrew = a.box.h - b.box.h;
      if (grew > Math.max(0, selGrew) + 2)
        problems.push(
          `${label}: ${b.key} label is taller by ${grew.toFixed(1)} px, the select by ${selGrew.toFixed(1)} px (the label now wraps?)`,
        );
    }
  });
  if (base.labels.length === after.labels.length) {
    base.labels.forEach((bl, i) => {
      const al = after.labels[i]!;
      for (const k of [
        'fontSize',
        'textTransform',
        'letterSpacing',
        'fontWeight',
        'color',
      ] as const)
        if (bl[k] !== al[k])
          problems.push(`${label}: label "${bl.text}" ${k} ${bl[k]} -> ${al[k]}`);
    });
  } else problems.push(`${label}: label count ${base.labels.length} -> ${after.labels.length}`);
  if (base.root && after.root)
    diffs['root'] = { base: base.root, after: after.root, dh: after.root.h - base.root.h };
  if (base.controls.length !== after.controls.length) {
    problems.push(`${label}: control count ${base.controls.length} -> ${after.controls.length}`);
  } else {
    const moved: { key: string; dx: number; dy: number; allowed: number }[] = [];
    base.controls.forEach((bc, i) => {
      const ac = after.controls[i]!;
      let allowed = 0;
      base.selects.forEach((b, j) => {
        const a = after.selects[j]!;
        if (b.box.y + b.box.h <= bc.box.y + 0.5) allowed += a.box.h - b.box.h;
        else if (b.box.y < bc.box.y + bc.box.h && bc.box.y < b.box.y + b.box.h)
          allowed = Math.max(allowed, (a.box.h - b.box.h) / 2 + 1);
      });
      const rootDy = base.root && after.root ? after.root.y - base.root.y : 0;
      const dy = Math.round((ac.box.y - bc.box.y - rootDy) * 10) / 10;
      const dx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
      moved.push({ key: bc.key, dx, dy, allowed: Math.round(allowed * 10) / 10 });
      if (dy > allowed + 1) problems.push(`${label}: ${bc.key} moved ${dy} px, allowed ${allowed}`);
      // A bottom-anchored bar grows upward: a control may move up by the summed height the selects gained.
      const grew = after.selects.reduce(
        (n, a, j) => n + Math.max(0, a.box.h - base.selects[j]!.box.h),
        0,
      );
      if (
        dy <
        -1 - grew - Math.max(0, base.root && after.root ? (after.root.h - base.root.h) / 2 : 0)
      )
        problems.push(`${label}: ${bc.key} moved up ${dy} px`);
    });
    diffs['moved'] = moved.filter((x) => x.dx !== 0 || x.dy !== 0 || x.allowed !== 0);
  }
  const norm = (x: string) => x.replace(/p103m w+ w+/g, 'p103m #');
  const baseFail = new Set(base.hitFails.map((f) => norm(f.control)));
  const worse = after.hitFails.filter((f) => !baseFail.has(norm(f.control)));
  if (
    after.hit.fail > base.hit.fail ||
    after.hit.notReached > 0 ||
    after.hit.pass + after.hit.fail !== base.hit.pass + base.hit.fail ||
    worse.length
  )
    problems.push(
      `${label}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}; worse rows ${JSON.stringify(worse)}`,
    );
  return diffs;
}

// ------------------------------------------------------------------------------------------------ the test

test(`p103 media: the selects of the watch party, music panel, Study and Live TV, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const notes: string[] = [];
  const reached: Record<string, string> = {};
  const figs: Record<string, Figures> = {};
  const ids = await makeData(owner.request, notes);
  try {
    for (const c of CONFIGS) {
      const list = SCENARIOS.filter((s) => s.family === c.family && (!ONLY || ONLY.test(s.key)));
      if (!list.length) continue;
      await setPrefs(owner, c);
      const ctx = await newContext(browser, owner, c);
      const pages = new Map<string, Page>();
      const requests: string[] = [];
      try {
        for (const s of list) {
          let page = pages.get(s.group);
          const first = !page;
          if (!page) {
            page = await ctx.newPage();
            page.setDefaultTimeout(15_000);
            page.on('request', (r) => {
              if (r.method() === 'POST' && /\/music\/playlists\/[^/]+\/items/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'PATCH' && /\/users\/@me\/settings/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'PATCH' && /\/watch-parties\/[^/]+$/.test(r.url()))
                requests.push(r.url());
              if (r.method() === 'POST' && /\/watch-parties\/[^/]+\/playback/.test(r.url()))
                requests.push(r.url());
            });
            pages.set(s.group, page);
          }
          try {
            const f = await runScenario(s, c, page, ids, first, problems, reached, requests);
            if (f) figs[`${s.key}/${c.tag}`] = f;
          } catch (e) {
            problems.push(`${s.key} ${c.tag}: ${String(e).split('\n')[0]!.slice(0, 400)}`);
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await removeData(owner.request, ids).catch(() => undefined);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  writeFileSync(resolve(OUT, `reach-${PHASE}.json`), JSON.stringify({ reached, notes }, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const k of Object.keys(figs)) {
      if (base[k]) {
        cmp[k] = compare(k, base[k]!, figs[k]!, problems);
        await composite(browser, k.replace('/', '-')).catch(() => undefined);
      } else problems.push(`${k}: reached in the after phase only`);
    }
    for (const k of Object.keys(base))
      if (!figs[k] && !(ONLY && !ONLY.test(k)))
        problems.push(`${k}: reached in the base phase only`);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  await owner.close();
  say(`reach ${JSON.stringify(reached)}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
