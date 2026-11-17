import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { measureFit, serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-418 (wave 93; D-926): the Tidepool player skin passes the 44 px hit test on a phone. The Owner on the
 * visual sweep's seeded world (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that
 * spec is never edited, and this spec NEEDS that world), the account's main theme set by PATCH and localStorage
 * and read back, 375 x 812 with touch, the player playing a queue of three tone tracks whose first title is 57
 * characters long, the skin chosen through the dev handle and read back (`.sr-ply[data-skin]`).
 *
 * Two passes on the same tree, each on a fresh page: BASE (the skin's phone block, the `@media (max-width: 720px)`
 * rules of `tidepool.css`, is deleted from the live style sheet in the page: the base is the tree with the change
 * switched off) and AFTER (the sheet as built). Each pass: the skin and theme read back; sideways scroll of the
 * document, the dock and the skin; what leaves the skin's box; `openCollapsed` then `hitTestAll` (the shared
 * helper, `e2e/support/hit-test.ts`) on the skin's root, every control's box, status and the taker of each failing
 * point; the gap from each control's box to its nearest neighbour above, below and beside; the skin's height;
 * the contrast of the queue rows' titles (`support/contrast.ts`; text hidden, ground from the screenshot); running
 * animations under reduced motion. AFTER also taps each kind of control with a real touch at its centre and at 20
 * px above and below, and checks it did its job. Then 1280 x 800 (no touch): every control's box, base and after.
 * Shots (5): `base-first-375`, `base-end-375`, `after-first-375`, `after-end-375`, `after-1280`; `measure.json`.
 *
 * Local only (SR_P93_TIDEPOOL_ON=1). SR_P93_TIDEPOOL_OUT is the shots folder. SR_P93_TIDEPOOL_BASE=1 runs the base
 * pass only (the after pass and the taps are skipped). SR_P93_TIDEPOOL_THEME is the account's main theme (default
 * `lounge-dark`; the skin wears its own fixed light theme).
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8718, Vite 5718, metrics 9418,
 * `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_E2E_BASE_URL=http://localhost:5718 SR_P93_TIDEPOOL_ON=1 \
 *   SR_P93_TIDEPOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-tidepool \
 *   npx playwright test tests/p93-tidepool-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P93_TIDEPOOL_ON === '1';
const BASE_ONLY = process.env.SR_P93_TIDEPOOL_BASE === '1';
const OUT =
  process.env.SR_P93_TIDEPOOL_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-tidepool';
const THEME = process.env.SR_P93_TIDEPOOL_THEME ?? 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p93-${n}`,
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

const rgba = (p: number[]): Rgba => ({ r: p[0]!, g: p[1]!, b: p[2]!, a: (p[3] ?? 255) / 255 });

/** Contrast of each queue row's title: text hidden, the ground read from the screenshot, laid under the colour. */
async function queueContrast(page: Page) {
  const n = await page.locator('.tp-queue .sr-ply-q-title').count();
  const out: { text: string; box: number[]; min: number; p20: number; color: string }[] = [];
  for (let i = 0; i < n; i++) {
    const info = await page.evaluate((k) => {
      const e = document.querySelectorAll<HTMLElement>('.tp-queue .sr-ply-q-title')[k]!;
      e.scrollIntoView({ block: 'center' });
      const r = e.getBoundingClientRect();
      return {
        text: (e.textContent ?? '').trim().slice(0, 30),
        x: r.left,
        y: r.top,
        w: r.width,
        h: r.height,
        color: getComputedStyle(e).color,
      };
    }, i);
    const sty = await page.addStyleTag({
      content:
        '.tp-queue button, .tp-queue button * { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }',
    });
    const clip = {
      x: Math.max(0, Math.floor(info.x)),
      y: Math.max(0, Math.floor(info.y)),
      width: Math.max(3, Math.min(375 - Math.max(0, Math.floor(info.x)), Math.ceil(info.w))),
      height: Math.max(3, Math.min(812 - Math.max(0, Math.floor(info.y)), Math.ceil(info.h))),
    };
    const png = await page.screenshot({ clip });
    await sty.evaluate((el) => (el as Element).remove());
    const px = await page.evaluate(
      async ([b64, color]) => {
        const img = new Image();
        img.src = `data:image/png;base64,${b64}`;
        await img.decode();
        const c = document.createElement('canvas');
        c.width = img.naturalWidth;
        c.height = img.naturalHeight;
        const g = c.getContext('2d')!;
        g.drawImage(img, 0, 0);
        const grid: number[][] = [];
        for (let j = 0; j < 3; j++)
          for (let ii = 0; ii < 7; ii++)
            grid.push(
              Array.from(
                g.getImageData(
                  Math.min(c.width - 1, Math.round((ii * (c.width - 1)) / 6)),
                  Math.min(c.height - 1, Math.round((j * (c.height - 1)) / 2)),
                  1,
                  1,
                ).data,
              ),
            );
        const k = document.createElement('canvas');
        k.width = k.height = 1;
        const kg = k.getContext('2d')!;
        kg.fillStyle = color!;
        kg.fillRect(0, 0, 1, 1);
        const d = kg.getImageData(0, 0, 1, 1).data;
        return { grid, srgb: `rgba(${d[0]}, ${d[1]}, ${d[2]}, ${(d[3]! / 255).toFixed(3)})` };
      },
      [png.toString('base64'), info.color],
    );
    const t = parseColor(px.srgb);
    const rs = px.grid.map((p) => {
      const g = rgba(p);
      return ratio(over(t, g), g);
    });
    const sorted = [...rs].sort((a, b) => a - b);
    out.push({
      text: info.text,
      box: [Math.round(info.x), Math.round(info.y), Math.round(info.w), Math.round(info.h)],
      min: Number(sorted[0]!.toFixed(2)),
      p20: Number(sorted[Math.floor(sorted.length * 0.2)]!.toFixed(2)),
      color: info.color,
    });
  }
  return out;
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
  for (const [dy, fx] of [
    [0, 0.25],
    [-20, 0.75],
    [20, 0.5],
  ] as const) {
    await tapAt(page, seek, dy, fx);
    const frac = await page.evaluate(() => {
      const a = (window as unknown as W).__srPlayer.audio;
      return a.currentTime / a.duration;
    });
    rec('Seek', dy, Math.abs(frac - fx) < 0.12, `target ${fx}, position ${frac.toFixed(2)}`);
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

async function phonePass(
  page: Page,
  label: 'base' | 'after',
  strip: boolean,
): Promise<Record<string, unknown>> {
  const e: Record<string, unknown> = {};
  e.deletedRules = strip ? await stripPhoneBlock(page) : 0;
  if (!strip)
    e.phoneRulesPresent = await page.evaluate(() => {
      let n = 0;
      for (const s of Array.from(document.styleSheets)) {
        const o = s.ownerNode as HTMLElement | null;
        if (!/tidepool\.css/.test(o?.getAttribute?.('data-vite-dev-id') ?? '')) continue;
        for (const r of Array.from(s.cssRules))
          if (r instanceof CSSMediaRule && /max-width:\s*720px/.test(r.conditionText)) n++;
      }
      return n;
    });
  await page.waitForTimeout(400);

  // Motion: reduced motion stops every running animation of the player.
  const anims = () =>
    page.evaluate(
      () =>
        document
          .getAnimations()
          .filter(
            (a) =>
              a.playState === 'running' &&
              ((a.effect as KeyframeEffect | null)?.target as Element | null)?.closest?.('.sr-ply'),
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
    const r = root.getBoundingClientRect();
    return {
      document: [de.scrollWidth, de.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth],
      skin: [root.scrollWidth, root.clientWidth],
      skinBox: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      skinHeight: root.scrollHeight,
      dockVertical: [dock.scrollHeight, dock.clientHeight],
    };
  });
  e.fit = await measureFit(page);
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
  e.queueContrast = await queueContrast(page);
  console.log(
    `REPORT ${label}: ${JSON.stringify(e.counts)}; doc ${JSON.stringify((e.scroll as { document: number[] }).document)}; height ${(e.scroll as { skinHeight: number }).skinHeight}`,
  );

  // Shots: the start, then the end (every scroller above the last queue row moved to its end).
  const shotStart = async () => {
    await page.evaluate(() => {
      for (const el of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply-dock, .sr-ply')))
        el.scrollTop = 0;
    });
    await page.waitForTimeout(300);
  };
  await shotStart();
  await page.screenshot({ path: resolve(OUT, `${label}-first-375.png`) });
  e.endReach = await page.evaluate(() => {
    const leaf = Array.from(document.querySelectorAll<HTMLElement>('.sr-ply *')).find(
      (x) => x.children.length === 0 && /^Tone 3/.test((x.textContent ?? '').trim()),
    );
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    let moved = 0;
    for (let a: HTMLElement | null = leaf?.parentElement ?? null; a; a = a.parentElement) {
      if (a.scrollHeight > a.clientHeight + 2 && getComputedStyle(a).overflowY !== 'visible') {
        a.scrollTop = a.scrollHeight;
        moved++;
      }
    }
    const r = leaf?.getBoundingClientRect();
    const d = dock.getBoundingClientRect();
    return {
      scrollersMoved: moved,
      row: r ? [Math.round(r.top), Math.round(r.bottom)] : null,
      dock: [Math.round(d.top), Math.round(d.bottom)],
      inside: r ? r.top >= d.top && r.bottom <= d.bottom : null,
    };
  });
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${label}-end-375.png`) });
  await shotStart();
  return e;
}

test('SEC-AUD-418: the Tidepool skin on a phone, base and after', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const report: Record<string, unknown> = { theme: THEME, long: LONG_TITLE };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();

  const newCtx = async (phone: boolean) => {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
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

  // Phone: base, then after (a fresh page each).
  {
    const { ctx, page } = await newCtx(true);
    report.base = await phonePass(page, 'base', true);
    await ctx.close();
  }
  let afterFails = -1;
  if (!BASE_ONLY) {
    const { ctx, page } = await newCtx(true);
    const after = await phonePass(page, 'after', false);
    report.after = after;
    await audioCall(page, 'play').catch(() => undefined);
    const taps = await tapAll(page);
    report.taps = taps;
    afterFails = taps.filter((t) => !t.ok).length;
    console.log(`REPORT taps: ${taps.length}, failed ${afterFails}`);
    for (const t of taps.filter((x) => !x.ok))
      console.log(`REPORT tap fail: ${t.control} dy ${t.dy}: ${t.detail}`);
    await ctx.close();
  }

  // 1280 x 800: every control's box, base and after.
  {
    const boxesOf = async (strip: boolean, shot: boolean) => {
      const { ctx, page } = await newCtx(false);
      if (strip) await stripPhoneBlock(page);
      await page.waitForTimeout(400);
      const boxes = await page.evaluate(boxesInPage);
      const root = page.locator(ROOT);
      const height = await root.evaluate((el) => el.scrollHeight);
      if (shot) {
        await audioCall(page, 'pause');
        await page.screenshot({ path: resolve(OUT, 'after-1280.png') });
      }
      await ctx.close();
      return { boxes, height };
    };
    const b = await boxesOf(true, false);
    if (!BASE_ONLY) {
      const a = await boxesOf(false, true);
      let max = 0;
      const same = a.boxes.length === b.boxes.length;
      if (same)
        a.boxes.forEach((x, i) =>
          x.box.forEach((v, k) => (max = Math.max(max, Math.abs(v - b.boxes[i]!.box[k]!)))),
        );
      report.desktop = {
        controls: a.boxes.map((x) => ({ name: x.name, box: x.box })),
        sameCount: same,
        maxDiffPx: max,
        heightBase: b.height,
        heightAfter: a.height,
      };
      console.log(
        `REPORT 1280: ${a.boxes.length} controls, max box diff ${max} px, same count ${same}`,
      );
    } else report.desktop = { controls: b.boxes.map((x) => ({ name: x.name, box: x.box })) };
  }

  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ownerCtx.close();
  if (!BASE_ONLY) expect(afterFails, 'taps that did not do their job').toBe(0);
});
