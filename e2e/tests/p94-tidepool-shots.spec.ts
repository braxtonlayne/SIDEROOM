import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-428 (wave 94; D-934): the Tidepool player skin passes the 44 px hit test between 625 and 720 px of
 * viewport, where its wide layout shows (the player wider than 38rem), as well as at 375. The Owner on the visual
 * sweep's seeded world (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is
 * never edited, and this spec NEEDS that world), the account's main theme set by PATCH and localStorage and read
 * back, touch, the player playing a queue of three tone tracks whose first title is 57 characters long, the skin
 * chosen through the dev handle and read back (`.sr-ply[data-skin]`).
 *
 * Passes, each on a fresh page: BASE (the skin's phone block, the `@media (max-width: 720px)` rules of
 * `tidepool.css`, is deleted from the live style sheet in the page) at 700, 640, 620 and 375 x 812; AFTER (the
 * sheet as built) at the same four widths; W93 at 375 (if SR_P94_TIDEPOOL_W93CSS names a copy of the wave 93
 * sheet, it replaces the live one, to compare every box at 375 with wave 93's). Each pass: the layout that shows
 * (the grid's columns), the player's width against 38rem (608 px), sideways scroll of the document, the dock and
 * the skin, the scrollers inside the dock (D-927), the skin's height; `openCollapsed` then `hitTestAll` (the
 * shared helper, `e2e/support/hit-test.ts`) on the skin's root, every control's box, status and the taker of each
 * failing point; the gap from each control's box to its neighbours. Full passes (375 and 700) also count running
 * animations under reduced motion; AFTER at 700 and 375 taps each kind of control with a real touch at its centre
 * and 20 px above and below (Seek and Volume by the input's own `input` event, D-935). Then 1280 x 800 (no
 * touch): every control's box, base and after. Shots (4): `base-700`, `after-700`, `after-640`, `after-375`;
 * `measure.json`.
 *
 * Local only (SR_P94_TIDEPOOL_ON=1). SR_P94_TIDEPOOL_OUT is the shots folder. SR_P94_TIDEPOOL_BASE=1 runs the
 * base passes only. SR_P94_TIDEPOOL_THEME is the account's main theme (default `lounge-dark`; the skin wears its
 * own fixed light theme).
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8723, Vite 5723, metrics 9423,
 * `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_E2E_BASE_URL=http://localhost:5723 SR_P94_TIDEPOOL_ON=1 \
 *   SR_P94_TIDEPOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-tidepool \
 *   npx playwright test tests/p94-tidepool-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (leave SR_P94_TIDEPOOL_W93CSS unset: the comparison with wave 93 is then skipped.)
 */
const ON = process.env.SR_P94_TIDEPOOL_ON === '1';
const BASE_ONLY = process.env.SR_P94_TIDEPOOL_BASE === '1';
const OUT =
  process.env.SR_P94_TIDEPOOL_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-tidepool';
const THEME = process.env.SR_P94_TIDEPOOL_THEME ?? 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p94-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const ROOT = '.sr-ply[data-skin="tidepool"]';

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

type W = {
  __srPlayerActions: { play(t: unknown): void; setSkin(k: string): void };
  __srPlayer: { audio: HTMLAudioElement };
};

/** BASE: delete the phone block from the skin's live style sheet. Returns how many rules went. */
async function stripPhoneBlock(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    for (const s of Array.from(document.styleSheets)) {
      const owner = s.ownerNode as HTMLElement | null;
      const id = owner?.getAttribute?.('data-vite-dev-id') ?? owner?.getAttribute?.('href') ?? '';
      if (!/tidepool\.css/.test(id)) continue;
      for (let i = s.cssRules.length - 1; i >= 0; i--) {
        const r = s.cssRules[i];
        if (r instanceof CSSMediaRule && /max-width:\s*720px/.test(r.conditionText)) {
          s.deleteRule(i);
          n++;
        }
      }
    }
    return n;
  });
}

/** W93: swap the live tidepool style sheet for a copy of wave 93's. */
async function useW93Sheet(page: Page, css: string): Promise<boolean> {
  return page.evaluate((text) => {
    let found = false;
    for (const n of Array.from(document.querySelectorAll('style'))) {
      if (/tidepool\.css/.test(n.getAttribute('data-vite-dev-id') ?? '')) {
        n.remove();
        found = true;
      }
    }
    const st = document.createElement('style');
    st.textContent = text;
    document.head.append(st);
    return found;
  }, css);
}

async function setAccountTheme(
  req: Parameters<typeof apiPatch>[0],
  theme: string,
): Promise<string | undefined> {
  await apiPatch(req, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
  const back = (await (await req.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  return back.settings?.user_prefs?.theme;
}

async function openPlayer(page: Page): Promise<void> {
  await page.goto('/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(() => (window as unknown as Partial<W>).__srPlayerActions);
  await page.evaluate((tracks) => (window as unknown as W).__srPlayerActions.play(tracks), TRACKS);
  await page.waitForFunction(() => (window as unknown as Partial<W>).__srPlayer);
  await expect
    .poll(() => page.evaluate(() => (window as unknown as W).__srPlayer.audio.currentTime), {
      timeout: 20_000,
    })
    .toBeGreaterThan(0.4);
  await page.evaluate(() => (window as unknown as W).__srPlayerActions.setSkin('tidepool'));
  await expect(page.locator(ROOT)).toBeVisible();
  await page.waitForTimeout(1500);
}

const audioCall = (page: Page, what: 'pause' | 'play') =>
  page.evaluate(async (w) => {
    const a = (window as unknown as W).__srPlayer.audio;
    if (w === 'pause') a.pause();
    else await a.play();
  }, what);

/** In the page: every visible control's box relative to the skin's root, and the gaps to its neighbours. */
function boxesInPage(): {
  name: string;
  box: number[];
  gapAbove: number | null;
  gapBelow: number | null;
  gapBeside: number | null;
}[] {
  const root = document.querySelector<HTMLElement>('.sr-ply')!;
  const rr = root.getBoundingClientRect();
  const sel = 'button, input:not([type="hidden"])';
  const items = Array.from(root.querySelectorAll<HTMLElement>(sel))
    .filter((e) => !e.closest('[aria-hidden="true"]'))
    .map((e) => ({ e, r: e.getBoundingClientRect() }))
    .filter(({ r }) => r.width > 4 && r.height > 4);
  const nm = (e: Element) =>
    e.getAttribute('aria-label') ||
    (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 24) ||
    `${e.tagName.toLowerCase()}.${String(e.getAttribute('class') ?? '').split(/\s+/)[0]}`;
  const near = (v: number[]) => (v.length ? Math.round(Math.min(...v)) : null);
  return items.map(({ e, r }, i) => {
    const above: number[] = [];
    const below: number[] = [];
    const beside: number[] = [];
    items.forEach(({ r: o }, j) => {
      if (i === j) return;
      const hov = Math.min(r.right, o.right) - Math.max(r.left, o.left) > 0;
      const vov = Math.min(r.bottom, o.bottom) - Math.max(r.top, o.top) > 0;
      if (hov && o.bottom <= r.top + 0.5) above.push(r.top - o.bottom);
      if (hov && o.top >= r.bottom - 0.5) below.push(o.top - r.bottom);
      if (vov && o.left >= r.right - 0.5) beside.push(o.left - r.right);
      if (vov && o.right <= r.left + 0.5) beside.push(r.left - o.right);
    });
    return {
      name: nm(e),
      box: [
        Math.round(r.left - rr.left),
        Math.round(r.top - rr.top),
        Math.round(r.width),
        Math.round(r.height),
      ],
      gapAbove: near(above),
      gapBelow: near(below),
      gapBeside: near(beside),
    };
  });
}

const summarise = (rows: ControlRow[]) =>
  rows.map((r) => ({
    name: r.name,
    path: r.path,
    status: r.status,
    reason: r.reason,
    box: r.result
      ? [
          Math.round(r.result.box.x),
          Math.round(r.result.box.y),
          Math.round(r.result.box.width),
          Math.round(r.result.box.height),
        ]
      : null,
    takers: (r.result?.points ?? [])
      .filter((p) => !p.ok)
      .map((p) => `${p.at}: ${p.taker?.tag}.${p.taker?.path ?? ''} "${p.taker?.name ?? ''}"`),
  }));

/** One touch tap at (centre + dx, centre + dy) of a control scrolled into the middle of the viewport. */
async function tapAt(page: Page, loc: Locator, dy: number, fx = 0.5): Promise<void> {
  await loc.first().evaluate((e) => e.scrollIntoView({ block: 'center', inline: 'center' }));
  await page.waitForTimeout(150);
  const b = (await loc.first().boundingBox())!;
  await page.touchscreen.tap(b.x + b.width * fx, b.y + b.height / 2 + dy);
  await page.waitForTimeout(350);
}

const sig = (loc: Locator) =>
  loc
    .first()
    .evaluate(
      (e) =>
        `${e.getAttribute('aria-pressed')}|${e.getAttribute('data-repeat')}|${e.getAttribute('data-on')}|${e.getAttribute('aria-label')}|${(e.textContent ?? '').trim()}`,
    );

interface TapRow {
  control: string;
  dy: number;
  ok: boolean;
  detail: string;
}

/** AFTER only: each kind of control, tapped at its centre and 20 px above and below, and what it did. */
async function tapAll(page: Page): Promise<TapRow[]> {
  const rows: TapRow[] = [];
  const DYS = [0, -20, 20];
  const rec = (control: string, dy: number, ok: boolean, detail: string) =>
    rows.push({ control, dy, ok, detail });
  const paused = () => page.evaluate(() => (window as unknown as W).__srPlayer.audio.paused);
  const current = () =>
    page.evaluate(() => {
      const li = Array.from(document.querySelectorAll('.tp-queue li'));
      return li.findIndex((l) => l.getAttribute('data-current') === 'true') + 1;
    });
  const rootL = page.locator(ROOT);

  // Play and pause.
  const orb = rootL.locator('.tp-orb');
  for (const dy of DYS) {
    const before = await paused();
    await tapAt(page, orb, dy);
    const after = await paused();
    rec('Pause orb', dy, before !== after, `paused ${before} -> ${after}`);
  }
  if (await paused()) await audioCall(page, 'play').catch(() => undefined);

  // A queue row plays: rows 2, 3, 2.
  const rowsSeq: [number, number][] = [
    [0, 2],
    [-20, 3],
    [20, 2],
  ];
  for (const [dy, row] of rowsSeq) {
    await tapAt(page, rootL.locator(`.tp-queue li:nth-child(${row}) button`), dy);
    const c = await current();
    rec(`Queue row ${row}`, dy, c === row, `current row ${c}`);
  }

  // Prev and Next (from row 2: next -> 3, prev -> 2, next -> 3).
  const prev = rootL.locator('.tp-cap-prev');
  const next = rootL.locator('.tp-cap-next');
  for (const [dy, loc, want, nm] of [
    [0, next, 3, 'Next'],
    [-20, prev, 2, 'Previous'],
    [20, next, 3, 'Next'],
  ] as const) {
    await tapAt(page, loc, dy);
    const c = await current();
    rec(nm, dy, c === want, `current row ${c}, wanted ${want}`);
  }
  await tapAt(page, rootL.locator('.tp-queue li:nth-child(1) button'), 0);

  // Seek and Volume: a tap at 25 %, 75 % and 50 % of the track.
  const seek = rootL.locator('input.tp-seek');
  await audioCall(page, 'play').catch(() => undefined);
  await page.waitForTimeout(1500);
  rec(
    'Audio before Seek',
    0,
    true,
    `currentTime ${await page.evaluate(() => (window as unknown as W).__srPlayer.audio.currentTime.toFixed(1))}, paused ${await paused()}`,
  );
  await seek.first().evaluate((e) => {
    (window as unknown as { __sk: number[] }).__sk = [];
    e.addEventListener('input', (ev) => {
      const i = ev.target as HTMLInputElement;
      (window as unknown as { __sk: number[] }).__sk.push(Number(i.value) / Number(i.max));
    });
  });
  for (const [dy, fx] of [
    [0, 0.25],
    [-20, 0.75],
    [20, 0.5],
  ] as const) {
    await tapAt(page, seek, dy, fx);
    const evs = await page.evaluate(() => (window as unknown as { __sk: number[] }).__sk.splice(0));
    const last = evs.length ? evs[evs.length - 1]! : -1;
    rec(
      'Seek',
      dy,
      evs.length > 0 && Math.abs(last - fx) < 0.12,
      `target ${fx}, the input fired ${evs.length} event(s), last value ${last.toFixed(2)} (the audio element snaps back to the start on this harness, on the base too: see baseSeek)`,
    );
  }
  const vol = rootL.locator('.sr-ply-volume input');
  for (const [dy, fx] of [
    [0, 0.25],
    [-20, 0.75],
    [20, 0.5],
  ] as const) {
    await tapAt(page, vol, dy, fx);
    const f = await vol.first().evaluate((e) => {
      const i = e as HTMLInputElement;
      return (Number(i.value) - Number(i.min || 0)) / (Number(i.max || 1) - Number(i.min || 0));
    });
    rec('Volume', dy, Math.abs(f - fx) < 0.2, `target ${fx}, value ${f.toFixed(2)}`);
  }

  // Toggles whose state shows on the control: each tap changes it.
  const toggles: [string, Locator][] = [
    ['Shuffle', rootL.locator('.tp-shuffle')],
    ['Mute', rootL.locator('.tp-ball')],
    ['Repeat', rootL.locator('.tp-chip[data-repeat]')],
    ['Drift off', rootL.locator('.tp-sleep')],
    ['EQ', rootL.locator('button.tp-pill:nth-of-type(2)')],
  ];
  for (const [nm, loc] of toggles) {
    for (const dy of DYS) {
      const before = await sig(loc);
      await tapAt(page, loc, dy);
      const after = await sig(loc);
      rec(nm, dy, before !== after, `${before} -> ${after}`);
    }
  }

  // Dock: move (the dock's data-dock changes), then the skin pill (the skin changes), then close and reopen.
  const dockOf = () => page.locator('.sr-ply-dock').first().getAttribute('data-dock');
  const gem = rootL.locator('.tp-gem-aqua');
  for (const dy of DYS) {
    const before = await dockOf();
    await tapAt(page, gem, dy);
    const after = await dockOf();
    rec('Move', dy, before !== after, `${before} -> ${after}`);
  }
  for (const dy of DYS) {
    await tapAt(page, rootL.locator('button.tp-pill:nth-of-type(3)'), dy);
    const skin = await page.locator('.sr-ply').first().getAttribute('data-skin');
    rec('Skin pill', dy, skin !== 'tidepool', `skin now ${skin}`);
    await page.evaluate(() => (window as unknown as W).__srPlayerActions.setSkin('tidepool'));
    await expect(page.locator(ROOT)).toBeVisible();
    await page.waitForTimeout(500);
  }
  for (const dy of DYS) {
    await tapAt(page, page.locator(`${ROOT} .tp-gem-red`), dy);
    const gone = (await page.locator('.sr-ply').count()) === 0;
    rec('Close', dy, gone, gone ? 'closed' : 'still open');
    await page.evaluate((t) => (window as unknown as W).__srPlayerActions.play(t), TRACKS);
    await page.evaluate(() => (window as unknown as W).__srPlayerActions.setSkin('tidepool'));
    await expect(page.locator(ROOT)).toBeVisible();
    await page.waitForTimeout(800);
    rec('Reopen', dy, true, 'player back');
  }
  // Stop last (it ends playback).
  for (const dy of DYS) {
    await audioCall(page, 'play').catch(() => undefined);
    await page.waitForTimeout(300);
    await tapAt(page, rootL.locator('button.tp-chip:has(svg)').first(), dy);
    const p = await paused();
    rec('Stop', dy, p, `paused ${p}`);
  }
  return rows;
}

type Mode = 'base' | 'after' | 'w93';
const W93_CSS = process.env.SR_P94_TIDEPOOL_W93CSS
  ? readFileSync(process.env.SR_P94_TIDEPOOL_W93CSS, 'utf8')
  : null;

async function pass(
  page: Page,
  mode: Mode,
  width: number,
  full: boolean,
  shot: string | null,
): Promise<Record<string, unknown>> {
  const e: Record<string, unknown> = { mode, width };
  if (mode === 'base') e.deletedRules = await stripPhoneBlock(page);
  if (mode === 'w93') e.w93Sheet = W93_CSS ? await useW93Sheet(page, W93_CSS) : 'no copy';
  await page.waitForTimeout(500);

  if (full) {
    const anims = () =>
      page.evaluate(
        () =>
          document
            .getAnimations()
            .filter(
              (a) =>
                a.playState === 'running' &&
                ((a.effect as KeyframeEffect | null)?.target as Element | null)?.closest?.(
                  '.sr-ply',
                ),
            ).length,
      );
    await audioCall(page, 'play').catch(() => undefined);
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForTimeout(500);
    e.animsAllowed = await anims();
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForTimeout(600);
    e.animsReduced = await anims();
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForTimeout(400);
  }
  await audioCall(page, 'pause');
  await page.waitForTimeout(400);

  e.identity = await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>('.sr-ply');
    return {
      skin: root?.getAttribute('data-skin') ?? null,
      themeOfPlayer: root?.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
      themeOfPage: document.documentElement.getAttribute('data-theme'),
    };
  });
  e.scroll = await page.evaluate(() => {
    const de = document.documentElement;
    const root = document.querySelector<HTMLElement>('.sr-ply')!;
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const shell = document.querySelector<HTMLElement>('.tp-shell')!;
    const r = root.getBoundingClientRect();
    const scrollers = Array.from(dock.querySelectorAll<HTMLElement>('*'))
      .concat([dock])
      .filter(
        (x) => x.scrollHeight > x.clientHeight + 2 && getComputedStyle(x).overflowY !== 'visible',
      )
      .map(
        (x) =>
          `${x.tagName.toLowerCase()}.${String(x.getAttribute('class') ?? '').split(/\s+/)[0]} ${x.scrollHeight}/${x.clientHeight}`,
      );
    return {
      document: [de.scrollWidth, de.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth],
      skin: [root.scrollWidth, root.clientWidth],
      skinBox: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      skinHeight: root.scrollHeight,
      dockVertical: [dock.scrollHeight, dock.clientHeight],
      scrollers,
      playerInline: r.width,
      layout: getComputedStyle(shell).gridTemplateColumns.split(' ').length > 1 ? 'wide' : 'narrow',
    };
  });
  e.boxes = await page.evaluate(boxesInPage);

  const root = page.locator(ROOT);
  e.opened = (await openCollapsed(page, root)).length;
  const rows = await hitTestAll(page, root);
  const sum = summarise(rows);
  e.controls = sum;
  e.counts = {
    total: rows.length,
    pass: rows.filter((r) => r.status === 'pass').length,
    fail: rows.filter((r) => r.status === 'fail').length,
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    skipped: rows.filter((r) => r.status === 'skipped').length,
  };
  const sc = e.scroll as {
    document: number[];
    skinHeight: number;
    layout: string;
    playerInline: number;
  };
  console.log(
    `REPORT ${mode} ${width}: ${JSON.stringify(e.counts)}; layout ${sc.layout} (player ${sc.playerInline}); doc ${JSON.stringify(sc.document)}; height ${sc.skinHeight}`,
  );
  for (const r of sum.filter((x) => x.status !== 'pass'))
    console.log(
      `REPORT   ${r.status} ${r.name} box ${JSON.stringify(r.box)} takers ${JSON.stringify(r.takers.slice(0, 2))}`,
    );
  if (shot) {
    await page.evaluate(() => {
      for (const el of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply-dock, .sr-ply')))
        el.scrollTop = 0;
    });
    await page.waitForTimeout(300);
    await page.screenshot({ path: resolve(OUT, `${shot}.png`) });
  }
  return e;
}

type Boxes = { name: string; box: number[] }[];
const maxDiff = (a: Boxes, b: Boxes): number => {
  let max = 0;
  a.forEach((x, i) =>
    x.box.forEach((v, k) => (max = Math.max(max, Math.abs(v - (b[i]?.box[k] ?? Infinity))))),
  );
  return max;
};

test('SEC-AUD-428: the Tidepool skin between 625 and 720 px, base and after', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const report: Record<string, unknown> = { theme: THEME, long: LONG_TITLE };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();

  const newCtx = async (w: number, h: number) => {
    const phone = w < 1000;
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: w, height: h },
      storageState: state,
      bypassCSP: true,
      ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
    });
    await ctx.addInitScript((th) => {
      try {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme: th, zoom: 100 } }, version: 2 }),
        );
      } catch {
        /* no storage */
      }
    }, THEME);
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await serveTone(page);
    await openPlayer(page);
    return { ctx, page };
  };

  const widths = [700, 640, 620, 375];
  const bases: Record<string, unknown> = {};
  for (const w of widths) {
    const { ctx, page } = await newCtx(w, 812);
    bases[String(w)] = await pass(
      page,
      'base',
      w,
      w === 700 || w === 375,
      w === 700 ? 'base-700' : null,
    );
    await ctx.close();
  }
  report.base = bases;
  const afters: Record<string, unknown> = {};
  const failing: string[] = [];
  let tapFails = 0;
  if (!BASE_ONLY) {
    for (const w of widths) {
      const { ctx, page } = await newCtx(w, 812);
      const full = w === 700 || w === 375;
      const r = await pass(page, 'after', w, full, w === 620 ? null : `after-${w}`);
      afters[String(w)] = r;
      const c = r.counts as { fail: number; notReached: number };
      if (c.fail || c.notReached) failing.push(`${w}: ${c.fail} fail, ${c.notReached} not reached`);
      if (full) {
        await audioCall(page, 'play').catch(() => undefined);
        const taps = await tapAll(page);
        r.taps = taps;
        const f = taps.filter((t) => !t.ok);
        tapFails += f.length;
        console.log(`REPORT taps ${w}: ${taps.length}, failed ${f.length}`);
        for (const t of f)
          console.log(`REPORT tap fail ${w}: ${t.control} dy ${t.dy}: ${t.detail}`);
      }
      await ctx.close();
    }
    report.after = afters;
    if (W93_CSS) {
      const { ctx, page } = await newCtx(375, 812);
      const w93 = await pass(page, 'w93', 375, false, null);
      const a = (afters['375'] as { boxes: Boxes }).boxes;
      const b = w93.boxes as Boxes;
      const max = a.length === b.length ? maxDiff(a, b) : -1;
      report.w93Compare = {
        sameCount: a.length === b.length,
        maxDiffPx: max,
        w93Counts: w93.counts,
        w93Height: (w93.scroll as { skinHeight: number }).skinHeight,
      };
      console.log(
        `REPORT 375 against wave 93: ${a.length}/${b.length} controls, max box diff ${max} px`,
      );
      await ctx.close();
    }
  }

  // 1280 x 800: every control's box, base and after.
  {
    const boxesOf = async (strip: boolean) => {
      const { ctx, page } = await newCtx(1280, 800);
      if (strip) await stripPhoneBlock(page);
      await page.waitForTimeout(400);
      const boxes: Boxes = await page.evaluate(boxesInPage);
      const height = await page.locator(ROOT).evaluate((el) => el.scrollHeight);
      await ctx.close();
      return { boxes, height };
    };
    const b = await boxesOf(true);
    if (!BASE_ONLY) {
      const a = await boxesOf(false);
      const same = a.boxes.length === b.boxes.length;
      const max = same ? maxDiff(a.boxes, b.boxes) : -1;
      report.desktop = {
        count: a.boxes.length,
        sameCount: same,
        maxDiffPx: max,
        heightBase: b.height,
        heightAfter: a.height,
      };
      console.log(`REPORT 1280: ${a.boxes.length} controls, max box diff ${max} px, same ${same}`);
    } else report.desktop = { count: b.boxes.length };
  }

  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ownerCtx.close();
  if (!BASE_ONLY) {
    console.log(`REPORT tap failures in total: ${tapFails}`);
    expect(failing, 'widths with failing or unreached controls').toEqual([]);
  }
});
