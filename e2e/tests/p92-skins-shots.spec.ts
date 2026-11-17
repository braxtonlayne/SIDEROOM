import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { apiPatch, expect, test } from '../support/fixtures';
import { measureFit, serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-406 (wave 92; D-919): a look pass over the five built player skins on a phone. Nothing is built; D-855
 * stands. The Owner on the visual sweep's seeded world (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed
 * the sweep world"; that spec is never edited): the signed-in Owner at `/channels/@me`, the account's main theme
 * set by PATCH and localStorage and read back, 375 x 812 with touch (`isMobile`, `hasTouch`), the player playing a
 * queue of three tone tracks whose first title is longer than 40 characters, the skin set through the dev handle.
 *
 * Per skin, in the order the picker lists them (valve, tidepool, circuit, study, stack): (1) the skin and the
 * theme read back (`.sr-ply[data-skin]`, the nearest `data-theme`); (2) the sideways scroll of the document, the
 * dock and the player's root, and what leaves the player's box (`measureFit`); (3) every control hit-tested at its
 * box centre and 21 px above and below (D-856): what is closed is opened first, the control is scrolled to the
 * middle of the viewport, and a control with no box, or a point outside the viewport, is a probe error counted
 * apart from the faults; (4) contrast of the title and of every time readout, from `support/contrast.ts`: the text
 * is made transparent, the ground is read from a screenshot's pixels at the box's four corners (and a grid), the
 * text colour is laid over each and the lowest ratio is reported; (5) reduced motion: with `prefers-reduced-motion:
 * reduce` the running CSS animations and the pixels (time and title text hidden) 500 ms apart are compared, and
 * the same with motion allowed as the control. Shots: `<skin>-first-375.png` (paused, as a member first sees it),
 * `<skin>-expanded-375.png` (the dock scrolled to its end, where the skin scrolls). `measure.json` is written beside
 * them.
 *
 * Local only (SR_P92_SKINS_ON=1). Shots and measure.json go to SR_P92_SKINS_OUT. SR_P92_SKINS_ONLY is a comma list
 * of skin ids. SR_P92_SKINS_THEME is the account's main theme (default `lounge-dark`; the backdrop skins wear
 * their own fixed theme). SR_P92_SKINS_BASE=1 is accepted and changes nothing: the spec builds nothing, the base
 * is the tree.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8705, Vite 5705):
 *   SR_E2E_BASE_URL=http://localhost:5705 SR_P92_SKINS_ON=1 \
 *   SR_P92_SKINS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-skins \
 *   npx playwright test tests/p92-skins-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P92_SKINS_ON === '1';
const OUT =
  process.env.SR_P92_SKINS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-skins';
const ONLY = process.env.SR_P92_SKINS_ONLY
  ? process.env.SR_P92_SKINS_ONLY.split(',').map((s) => s.trim())
  : null;
const THEME = process.env.SR_P92_SKINS_THEME ?? 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
/** The picker's order (`PLAYER_SKINS`): the five backdrop skins. */
const SKINS = ['valve', 'tidepool', 'circuit', 'study', 'stack'] as const;
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const LONG_ARTIST = 'Neon Coast Collective';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p92-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? LONG_ARTIST : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

interface HitRow {
  name: string;
  kind: string;
  box: number[];
  /** 'ok', 'fail' (a point returned something else), 'probe' (no box, or a point outside the viewport). */
  verdict: 'ok' | 'fail' | 'probe';
  takers: string[];
  note?: string;
}

/** In the page: open what is closed, then hit-test every control of the player. */
function hitTestInPage(): { rows: HitRow[]; opened: number } {
  const root = document.querySelector<HTMLElement>('.sr-ply');
  const dock = document.querySelector<HTMLElement>('.sr-ply-dock');
  const rows: HitRow[] = [];
  if (!root) return { rows, opened: 0 };
  // (1) What is closed: every <details>, every disclosure that says it is collapsed.
  let opened = 0;
  for (const d of Array.from(root.querySelectorAll('details:not([open])'))) {
    (d as HTMLDetailsElement).open = true;
    opened++;
  }
  const sel =
    'button, a[href], input:not([type="hidden"]), select, textarea, [role="slider"], [role="option"], [role="button"], [role="switch"], [role="tab"]';
  const label = (e: Element): string => {
    const al = e.getAttribute('aria-label');
    const tx = (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
    const ph = e.getAttribute('title');
    return (
      al ||
      tx ||
      ph ||
      `${e.tagName.toLowerCase()}.${String(e.getAttribute('class') ?? '').slice(0, 30)}`
    );
  };
  const kindOf = (e: Element): string => {
    const t = e.getAttribute('type');
    const role = e.getAttribute('role');
    return `${e.tagName.toLowerCase()}${t ? `[${t}]` : ''}${role ? `[role=${role}]` : ''}`;
  };
  const describe = (e: Element | null): string => {
    if (!e) return 'null';
    const al = e.getAttribute('aria-label');
    return `${e.tagName.toLowerCase()}.${String(e.getAttribute('class') ?? '')
      .split(/\s+/)
      .slice(0, 2)
      .join('.')}${al ? ` "${al}"` : ''}`;
  };
  const els = Array.from(root.querySelectorAll<HTMLElement>(sel)).filter(
    (e) => !e.closest('[aria-hidden="true"]'),
  );
  const vw = document.documentElement.clientWidth;
  const vh = document.documentElement.clientHeight;
  for (const el of els) {
    const cs = getComputedStyle(el);
    if (cs.display === 'none' || cs.visibility === 'hidden') continue;
    el.scrollIntoView({ block: 'center', inline: 'center' });
    const r = el.getBoundingClientRect();
    const box = [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)];
    const row: HitRow = {
      name: label(el),
      kind: kindOf(el),
      box,
      verdict: 'ok',
      takers: [],
    };
    // Screen-reader-only boxes (1 px) are not touch targets.
    if (r.width <= 1.5 || r.height <= 1.5) {
      row.verdict = 'probe';
      row.note = 'no box (sr-only or empty)';
      rows.push(row);
      continue;
    }
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const own = (hit: Element | null): boolean => {
      if (!hit) return false;
      if (hit === el || el.contains(hit)) return true;
      const lab = (hit as HTMLElement).closest?.('label');
      if (lab && ((lab as HTMLLabelElement).control === el || lab.contains(el))) return true;
      return false;
    };
    const pts: [string, number][] = [
      ['above', cy - 21],
      ['centre', cy],
      ['below', cy + 21],
    ];
    for (const [nm, y] of pts) {
      if (cx < 0 || cx >= vw || y < 0 || y >= vh) {
        row.verdict = 'probe';
        row.note = `${nm} point (${Math.round(cx)}, ${Math.round(y)}) outside the viewport ${vw}x${vh}`;
        continue;
      }
      const hit = document.elementFromPoint(cx, y);
      if (hit === null) {
        row.verdict = 'probe';
        row.note = `${nm} point returned null`;
        continue;
      }
      if (!own(hit)) {
        if (row.verdict === 'ok') row.verdict = 'fail';
        row.takers.push(`${nm}: ${describe(hit)}`);
      }
    }
    rows.push(row);
  }
  if (dock) dock.scrollTop = 0;
  return { rows, opened };
}

interface Probe {
  kind: 'title' | 'time';
  text: string;
  box: { x: number; y: number; w: number; h: number };
  color: string;
}

/** In the page: the title and every time readout, scrolled into view, with the text colour that is really painted. */
function probesInPage(): Probe[] {
  const root = document.querySelector<HTMLElement>('.sr-ply');
  const out: Probe[] = [];
  if (!root) return out;
  const TIME = /\d{1,2}:\d{2}/;
  const skip = '.sr-ply-q-len, .nc-c-len, .ss-pl-len, .sr-ply-bd-sr';
  const colorOf = (e: HTMLElement): string => {
    const cs = getComputedStyle(e);
    let fill = cs.getPropertyValue('-webkit-text-fill-color') || cs.color;
    // Fold the opacity of the element and its ancestors into the colour's alpha.
    let op = 1;
    for (let a: HTMLElement | null = e; a && a !== root.parentElement; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    if (op < 1) fill = `${fill}|opacity:${op}`;
    return fill;
  };
  const add = (e: HTMLElement, kind: Probe['kind']) => {
    if (e.matches(skip) || e.closest(skip)) return;
    const cs = getComputedStyle(e);
    if (cs.display === 'none' || cs.visibility === 'hidden') return;
    e.scrollIntoView({ block: 'center', inline: 'center' });
    const r = e.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) return;
    out.push({
      kind,
      text: (e.textContent ?? '').trim().slice(0, 60),
      box: { x: r.left, y: r.top, w: r.width, h: r.height },
      color: colorOf(e),
    });
  };
  // The ticker's clipping box is the visible part of the title; the inner text is as wide as the whole title.
  const title = root.querySelector<HTMLElement>('.sr-ply-ticker');
  if (title) add(title, 'title');
  for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
    if (e.children.length > 0) continue;
    const t = e.textContent ?? '';
    if (TIME.test(t) || e.classList.contains('v-nixie-digit')) add(e, 'time');
  }
  return out;
}

/** In the page: the four corners (and a 7 x 3 grid) of a PNG's pixels, as rgba. */
async function pixelsOf(
  page: Page,
  png: Buffer,
): Promise<{ corners: number[][]; grid: number[][]; w: number; h: number }> {
  return page.evaluate(async (b64) => {
    const img = new Image();
    img.src = `data:image/png;base64,${b64}`;
    await img.decode();
    const c = document.createElement('canvas');
    c.width = img.naturalWidth;
    c.height = img.naturalHeight;
    const g = c.getContext('2d')!;
    g.drawImage(img, 0, 0);
    const px = (x: number, y: number) =>
      Array.from(
        g.getImageData(
          Math.min(c.width - 1, Math.max(0, Math.round(x))),
          Math.min(c.height - 1, Math.max(0, Math.round(y))),
          1,
          1,
        ).data,
      );
    const w = c.width;
    const h = c.height;
    // Inset past a 1-3 px border (the text's own box often has one), as far as the box allows.
    const i = Math.max(1, Math.min(4, Math.floor(w / 4), Math.floor(h / 4)));
    const corners = [px(i, i), px(w - 1 - i, i), px(i, h - 1 - i), px(w - 1 - i, h - 1 - i)];
    const inset = i;
    const grid: number[][] = [];
    for (let j = 0; j < 3; j++)
      for (let ii = 0; ii < 7; ii++)
        grid.push(
          px(inset + (ii * (w - 1 - 2 * inset)) / 6, inset + (j * (h - 1 - 2 * inset)) / 2),
        );
    return { corners, grid, w, h };
  }, png.toString('base64'));
}

/** Compare two PNGs in the page: the number of differing pixels and the total. */
async function diffPixels(
  page: Page,
  a: Buffer,
  b: Buffer,
): Promise<{ diff: number; total: number }> {
  return page.evaluate(
    async ([x, y]) => {
      const load = async (s: string) => {
        const img = new Image();
        img.src = `data:image/png;base64,${s}`;
        await img.decode();
        const c = document.createElement('canvas');
        c.width = img.naturalWidth;
        c.height = img.naturalHeight;
        const g = c.getContext('2d')!;
        g.drawImage(img, 0, 0);
        return g.getImageData(0, 0, c.width, c.height).data;
      };
      const da = await load(x!);
      const db = await load(y!);
      let diff = 0;
      for (let i = 0; i < da.length; i += 4)
        if (
          Math.abs(da[i]! - db[i]!) > 2 ||
          Math.abs(da[i + 1]! - db[i + 1]!) > 2 ||
          Math.abs(da[i + 2]! - db[i + 2]!) > 2
        )
          diff++;
      return { diff, total: da.length / 4 };
    },
    [a.toString('base64'), b.toString('base64')],
  );
}

const rgba = (p: number[]): Rgba => ({ r: p[0]!, g: p[1]!, b: p[2]!, a: (p[3] ?? 255) / 255 });

/** The text colour (with a possible `|opacity:n` suffix from `probesInPage`) laid over a ground pixel. */
function ratioOn(color: string, ground: Rgba): number {
  const [c, op] = color.split('|opacity:');
  const t = parseColor(c!);
  if (op) t.a *= Number(op);
  return ratio(over(t, ground), ground);
}

/** What contrast the helper can read and what it cannot: the ground is read from pixels (text made transparent). */
async function contrastOf(page: Page, probe: Probe) {
  const sel = await page.evaluate(
    ([kind, text]) => {
      const root = document.querySelector('.sr-ply')!;
      const all = Array.from(root.querySelectorAll<HTMLElement>('*')).filter(
        (e) => e.children.length === 0 || e.matches('.sr-ply-ticker'),
      );
      const el = all.find(
        (e) =>
          (e.textContent ?? '').trim().slice(0, 60) === text &&
          (kind === 'title' ? e.matches('.sr-ply-ticker') : true),
      );
      if (!el) return false;
      el.setAttribute('data-p92-probe', '1');
      return true;
    },
    [probe.kind, probe.text],
  );
  if (!sel) return null;
  // Hide the text, keep the box; the screenshot then shows what lies under the text.
  await page.addStyleTag({
    content:
      '[data-p92-probe="1"], [data-p92-probe="1"] * { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }',
  });
  const box = await page.evaluate(() => {
    const e = document.querySelector<HTMLElement>('[data-p92-probe="1"]')!;
    e.scrollIntoView({ block: 'center', inline: 'center' });
    const r = e.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  });
  const vw = 375;
  const vh = 812;
  const clip = {
    x: Math.max(0, Math.floor(box.x)),
    y: Math.max(0, Math.floor(box.y)),
    width: Math.max(3, Math.min(vw - Math.max(0, Math.floor(box.x)), Math.ceil(box.w))),
    height: Math.max(3, Math.min(vh - Math.max(0, Math.floor(box.y)), Math.ceil(box.h))),
  };
  const png = await page.screenshot({ clip });
  await page.evaluate(() => {
    document.querySelector('[data-p92-probe="1"]')?.removeAttribute('data-p92-probe');
  });
  await page.evaluate(() => {
    for (const s of Array.from(document.querySelectorAll('style')))
      if ((s.textContent ?? '').startsWith('[data-p92-probe="1"]')) s.remove();
  });
  const px = await pixelsOf(page, png);
  // Chromium reports oklab()/oklch() for some computed colours, which the helper cannot read: the browser's own
  // canvas converts the string to sRGB first (the original string stays in the report).
  const [cssColor, opacity] = probe.color.split('|opacity:');
  const srgb = await page.evaluate((c) => {
    const k = document.createElement('canvas');
    k.width = k.height = 1;
    const g = k.getContext('2d')!;
    g.fillStyle = c;
    g.fillRect(0, 0, 1, 1);
    const d = g.getImageData(0, 0, 1, 1).data;
    return `rgba(${d[0]}, ${d[1]}, ${d[2]}, ${(d[3]! / 255).toFixed(3)})`;
  }, cssColor!);
  const colorForHelper = opacity ? `${srgb}|opacity:${opacity}` : srgb;
  const cr = px.corners.map((p) => ratioOn(colorForHelper, rgba(p)));
  const gr = px.grid.map((p) => ratioOn(colorForHelper, rgba(p)));
  return {
    kind: probe.kind,
    text: probe.text,
    color: probe.color,
    colorAsSrgb: srgb,
    box: [Math.round(box.x), Math.round(box.y), Math.round(box.w), Math.round(box.h)],
    corners: cr.map((v) => Number(v.toFixed(2))),
    cornersMin: Number(Math.min(...cr).toFixed(2)),
    gridMin: Number(Math.min(...gr).toFixed(2)),
    // Round 2: a corner can lie outside a rounded or clipped ground (an artefact); trust the median and 20th percentile.
    gridMedian: Number([...gr].sort((x, y) => x - y)[Math.floor(gr.length / 2)]!.toFixed(2)),
    gridP20: Number([...gr].sort((x, y) => x - y)[Math.floor(gr.length * 0.2)]!.toFixed(2)),
    ground: 'screenshot pixels (text hidden): the helper cannot read a gradient, an image or art',
  };
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

const audioCall = (page: Page, what: 'pause' | 'play') =>
  page.evaluate(async (w) => {
    const a = (window as unknown as { __srPlayer: { audio: HTMLAudioElement } }).__srPlayer.audio;
    if (w === 'pause') a.pause();
    else await a.play();
  }, what);

test('SEC-AUD-406: the five player skins on a phone, looked at', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const report: Record<string, unknown> = { theme: THEME, long: LONG_TITLE };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);

  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
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
  await page.goto('/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(
    () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
  );
  await page.evaluate((tracks) => {
    (window as unknown as { __srPlayerActions: { play(t: unknown): void } }).__srPlayerActions.play(
      tracks,
    );
  }, TRACKS);
  await page.waitForFunction(() => (window as unknown as { __srPlayer?: unknown }).__srPlayer);
  await expect
    .poll(
      () =>
        page.evaluate(
          () =>
            (window as unknown as { __srPlayer: { audio: HTMLAudioElement } }).__srPlayer.audio
              .currentTime,
        ),
      { timeout: 20_000 },
    )
    .toBeGreaterThan(0.4);

  const skins: Record<string, unknown> = {};
  report.skins = skins;
  for (const skin of SKINS) {
    if (ONLY && !ONLY.includes(skin)) continue;
    const entry: Record<string, unknown> = {};
    skins[skin] = entry;
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.evaluate(
      (s) =>
        (
          window as unknown as { __srPlayerActions: { setSkin(k: string): void } }
        ).__srPlayerActions.setSkin(s),
      skin,
    );
    await expect(page.locator(`.sr-ply[data-skin="${skin}"]`)).toBeVisible();
    await page.waitForTimeout(1200); // the skin's lazy chunk, its fonts, the first frames
    await audioCall(page, 'play').catch(() => undefined);
    await page.waitForTimeout(500);

    // Motion (normal): running CSS animations and the pixels 500 ms apart, with text that changes hidden.
    const quiet = () =>
      page.addStyleTag({
        content:
          '.sr-ply-ticker, .sr-ply-time, .sr-ply [class*="nixie"], .sr-ply [class*="lcd"], .sr-ply [class*="time"], .sr-ply [class*="counter"] { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }',
      });
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
    const dockShot = async () => page.locator('.sr-ply-dock').screenshot();
    const motion = async () => {
      const s = await quiet();
      const n = await anims();
      const a = await dockShot();
      await page.waitForTimeout(500);
      const b = await dockShot();
      const d = await diffPixels(page, a, b);
      await s.evaluate((el) => (el as Element).remove());
      return { runningAnimations: n, ...d, moving: d.diff > 0 };
    };
    entry.motionAllowed = await motion();
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForTimeout(600);
    const reduced = await motion();
    entry.motionReduced = reduced;
    entry.motionAttr = await page.evaluate(() =>
      document.querySelector('.sr-ply')?.getAttribute('data-motion'),
    );
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForTimeout(400);

    // Pause for the look.
    await audioCall(page, 'pause');
    await page.waitForTimeout(500);

    // (1) the skin and the theme read back
    entry.identity = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>('.sr-ply');
      const themed = root?.closest('[data-theme]') as HTMLElement | null;
      const outer = document.querySelector<HTMLElement>('.sr-ply-theme');
      return {
        skin: root?.getAttribute('data-skin') ?? null,
        status: root?.getAttribute('data-status') ?? null,
        motionAttr: root?.getAttribute('data-motion') ?? null,
        themeOfPlayer: themed?.getAttribute('data-theme') ?? null,
        themeOfOverlayRoot: outer?.getAttribute('data-theme') ?? null,
        themeOfPage: document.documentElement.getAttribute('data-theme'),
      };
    });

    // (2) sideways scroll, and what leaves the player's box
    entry.scroll = await page.evaluate(() => {
      const de = document.documentElement;
      const root = document.querySelector<HTMLElement>('.sr-ply')!;
      const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
      const r = root.getBoundingClientRect();
      return {
        document: [de.scrollWidth, de.clientWidth],
        dock: [dock.scrollWidth, dock.clientWidth],
        root: [root.scrollWidth, root.clientWidth],
        rootBox: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
        dockVertical: [dock.scrollHeight, dock.clientHeight],
      };
    });
    entry.fit = await measureFit(page);

    // (3) every control
    const hit = await page.evaluate(hitTestInPage);
    entry.opened = hit.opened;
    entry.controls = hit.rows;
    entry.controlCount = hit.rows.length;
    entry.faults = hit.rows.filter((r) => r.verdict === 'fail');
    entry.probeErrors = hit.rows.filter((r) => r.verdict === 'probe');

    // (4) contrast
    const probes = await page.evaluate(probesInPage);
    const contrast = [];
    for (const p of probes) {
      const c = await contrastOf(page, p).catch((e: unknown) => ({
        kind: p.kind,
        text: p.text,
        error: String(e),
      }));
      if (c) contrast.push(c);
    }
    entry.contrast = contrast;

    // (5) the eye: boxes of controls that overlap each other
    entry.overlaps = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>('.sr-ply')!;
      const sel = 'button, input:not([type="hidden"]), [role="slider"], [role="option"]';
      const els = Array.from(root.querySelectorAll<HTMLElement>(sel)).filter(
        (e) => !e.closest('[aria-hidden="true"]') && e.getBoundingClientRect().width > 4,
      );
      const name = (e: Element) =>
        e.getAttribute('aria-label') || (e.textContent ?? '').trim().slice(0, 20) || e.className;
      const out: string[] = [];
      for (let i = 0; i < els.length; i++)
        for (let j = i + 1; j < els.length; j++) {
          const a = els[i]!.getBoundingClientRect();
          const b = els[j]!.getBoundingClientRect();
          if (els[i]!.contains(els[j]!) || els[j]!.contains(els[i]!)) continue;
          const w = Math.min(a.right, b.right) - Math.max(a.left, b.left);
          const h = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
          if (w > 2 && h > 2)
            out.push(`${name(els[i]!)} x ${name(els[j]!)} (${Math.round(w)}x${Math.round(h)})`);
        }
      return out.slice(0, 40);
    });

    // Shots: first view (dock at the top), then the expanded view (the dock scrolled to its end).
    await page.evaluate(() => {
      const d = document.querySelector<HTMLElement>('.sr-ply-dock');
      if (d) d.scrollTop = 0;
    });
    await page.waitForTimeout(300);
    await page.screenshot({ path: resolve(OUT, `${skin}-first-375.png`) });
    const scrollable = await page.evaluate(() => {
      const d = document.querySelector<HTMLElement>('.sr-ply-dock');
      if (!d) return false;
      d.scrollTop = d.scrollHeight;
      return d.scrollHeight > d.clientHeight + 2;
    });
    // Is the last queue row (the third track) whole inside the dock at its end, and what clips it if not?
    entry.queueTail = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>('.sr-ply')!;
      const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
      const leaf = Array.from(root.querySelectorAll<HTMLElement>('*')).find(
        (e) => e.children.length === 0 && /^Tone 3/.test((e.textContent ?? '').trim()),
      );
      if (!leaf) return null;
      // Round 2: the player's own box can scroll inside the dock (a second scroller): measure the row at the end
      // of every scroller above it, as a finger would take it, and say how many scrollers had to move.
      let scrolled = 0;
      for (let a: HTMLElement | null = leaf.parentElement; a; a = a.parentElement) {
        if (a.scrollHeight > a.clientHeight + 2 && getComputedStyle(a).overflowY !== 'visible') {
          a.scrollTop = a.scrollHeight;
          scrolled++;
        }
      }
      const r = leaf.getBoundingClientRect();
      const d = dock.getBoundingClientRect();
      const clippers: string[] = [];
      for (let a = leaf.parentElement; a && a !== dock; a = a.parentElement) {
        const o = getComputedStyle(a).overflowY;
        if (o !== 'visible') {
          const ar = a.getBoundingClientRect();
          clippers.push(
            `${a.tagName.toLowerCase()}.${String(a.getAttribute('class') ?? '').split(/\s+/)[0]} overflow-y:${o} box ${Math.round(ar.top)}-${Math.round(ar.bottom)} scroll ${a.scrollHeight}/${a.clientHeight}`,
          );
        }
      }
      return {
        row: [Math.round(r.top), Math.round(r.bottom)],
        dock: [Math.round(d.top), Math.round(d.bottom)],
        scrollersMoved: scrolled,
        inside: r.top >= d.top && r.bottom <= d.bottom,
        clippers,
      };
    });
    entry.expandedShot = scrollable;
    if (scrollable) {
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `${skin}-expanded-375.png`) });
    }
    await page.evaluate(() => {
      const d = document.querySelector<HTMLElement>('.sr-ply-dock');
      if (d) d.scrollTop = 0;
    });
    const ctrls = entry.controls as HitRow[];
    console.log(
      `REPORT ${skin}: ${ctrls.length} controls, ${(entry.faults as HitRow[]).length} fail, ${(entry.probeErrors as HitRow[]).length} probe; doc ${JSON.stringify((entry.scroll as { document: number[] }).document)}`,
    );
  }
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ctx.close();
  await ownerCtx.close();
});
