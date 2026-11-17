import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { measureFit, serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-420 (D-926) and SEC-AUD-424 (D-928), wave 93: the Study player skin passes the 44 px hit test on a phone,
 * and its "0:14 / 0:40" readout reaches 4.5 at the lowest point of its ground. NEEDS THE SWEEP'S SEEDED WORLD
 * (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited): the
 * signed-in Owner at `/channels/@me`, the account's main theme set by PATCH and localStorage and read back, 375 x 812
 * with touch, the player playing three tone tracks (first title 57 characters), the skin chosen through the dev
 * handle and read back (`.sr-ply[data-skin]`, the nearest `data-theme`).
 *
 * At 375: `openCollapsed` then `hitTestAll` (the shared tool, `support/hit-test.ts`) on the skin's root: every control
 * with its box, status and the taker of each failing point; the effective 44 px targets of two controls that overlap;
 * `scrollWidth` against `clientWidth` for the document, the dock and the skin; the skin's box (height); what leaves
 * the player's box; reduced motion (running animations go to 0); the contrast (`support/contrast.ts`) of the title
 * and every time readout: the text hidden, the ground read from the shot at a 7 x 3 grid, the minimum, the 20th
 * percentile and the median. Then by a real touch tap at a control's centre, 20 px above and 20 px below: play and
 * pause, a queue row, the seek ribbon, the volume range, the skin button, the move button, close and reopen.
 * At 1280 x 800: every control's box (written to `boxes1280-<case>.json`; an after run compares with the base file
 * when it is in the folder), the contrast of the readouts, one shot.
 *
 * Local only (SR_P93_STUDY_ON=1). SR_P93_STUDY_OUT is the folder of shots and `measure-<case>.json`.
 * SR_P93_STUDY_BASE=1 is the base case: the phone block (the `@media (max-width: 720px)` rule that names `.st-dot`)
 * is deleted from the loaded sheets in the page and the readout's old colour is put back, so the same tree is
 * measured with the change switched off; shots are named `base-*`, the after case's `after-*`.
 * SR_P93_STUDY_THEME is the account's main theme (default `lounge-dark`; the Study wears its own light theme).
 * SR_P93_STUDY_NOTAP=1 skips the tap section.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8717, Vite 5717, metrics 9417,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli when it uses
 * the postgres login), the after case, then the base case:
 *   SR_E2E_BASE_URL=http://localhost:5717 SR_P93_STUDY_ON=1 \
 *   SR_P93_STUDY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-study \
 *   npx playwright test tests/p93-study-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 *   (the same with SR_P93_STUDY_BASE=1 for the base case)
 */
const ON = process.env.SR_P93_STUDY_ON === '1';
const BASE = process.env.SR_P93_STUDY_BASE === '1';
const NOTAP = process.env.SR_P93_STUDY_NOTAP === '1';
const OUT =
  process.env.SR_P93_STUDY_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-study';
const THEME = process.env.SR_P93_STUDY_THEME ?? 'lounge-dark';
const CASE = BASE ? 'base' : 'after';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const LONG_ARTIST = 'Neon Coast Collective';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p93-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? LONG_ARTIST : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const ROOT = '.sr-ply[data-skin="study"]';

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

type Actions = {
  play(t: unknown): void;
  setSkin(k: string): void;
};
const actions = (page: Page) => ({
  play: (t: unknown) =>
    page.evaluate(
      (x) => (window as unknown as { __srPlayerActions: Actions }).__srPlayerActions.play(x),
      t,
    ),
  setSkin: (k: string) =>
    page.evaluate(
      (x) => (window as unknown as { __srPlayerActions: Actions }).__srPlayerActions.setSkin(x),
      k,
    ),
});

const audio = (page: Page) =>
  page.evaluate(() => {
    const a = (window as unknown as { __srPlayer: { audio: HTMLAudioElement } }).__srPlayer.audio;
    return { paused: a.paused, t: a.currentTime, d: a.duration, v: a.volume };
  });
const audioCall = (page: Page, what: 'pause' | 'play') =>
  page.evaluate(async (w) => {
    const a = (window as unknown as { __srPlayer: { audio: HTMLAudioElement } }).__srPlayer.audio;
    if (w === 'pause') a.pause();
    else await a.play();
  }, what);

/** The base case: take the phone block out of the loaded sheets, put the readout's old colour back. */
async function switchOff(page: Page) {
  return page.evaluate(() => {
    let removed = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      let rules: CSSRuleList;
      try {
        rules = sheet.cssRules;
      } catch {
        continue;
      }
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i];
        if (
          r instanceof CSSMediaRule &&
          r.conditionText.replace(/\s+/g, '') === '(max-width:720px)' &&
          r.cssText.includes('.st-dot::after')
        ) {
          sheet.deleteRule(i);
          removed++;
        }
      }
    }
    const st = document.createElement('style');
    st.setAttribute('data-p93-base', '1');
    st.textContent = '.sr-ply-study .st-lcd .sr-ply-time { color: var(--st-olive) !important; }';
    document.head.appendChild(st);
    return removed;
  });
}

interface Probe {
  kind: 'title' | 'time';
  text: string;
  color: string;
}

/** In the page: the title and every time readout of the skin, with the text colour that is really painted. */
function probesInPage(): Probe[] {
  const root = document.querySelector<HTMLElement>('.sr-ply[data-skin="study"]');
  const out: Probe[] = [];
  if (!root) return out;
  const TIME = /\d{1,2}:\d{2}/;
  const skip = '.sr-ply-q-len, .sr-ply-bd-sr';
  const colorOf = (e: HTMLElement): string => {
    const cs = getComputedStyle(e);
    let fill = cs.getPropertyValue('-webkit-text-fill-color') || cs.color;
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
    out.push({ kind, text: (e.textContent ?? '').trim().slice(0, 60), color: colorOf(e) });
  };
  const title = root.querySelector<HTMLElement>('.sr-ply-ticker');
  if (title) add(title, 'title');
  for (const e of Array.from(root.querySelectorAll<HTMLElement>('*'))) {
    if (e.children.length > 0) continue;
    if (TIME.test(e.textContent ?? '')) add(e, 'time');
  }
  return out;
}

async function pixelsOf(page: Page, png: Buffer): Promise<{ grid: number[][] }> {
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
    const i = Math.max(1, Math.min(4, Math.floor(w / 4), Math.floor(h / 4)));
    const grid: number[][] = [];
    for (let j = 0; j < 3; j++)
      for (let ii = 0; ii < 7; ii++)
        grid.push(px(i + (ii * (w - 1 - 2 * i)) / 6, i + (j * (h - 1 - 2 * i)) / 2));
    return { grid };
  }, png.toString('base64'));
}

const rgba = (p: number[]): Rgba => ({ r: p[0]!, g: p[1]!, b: p[2]!, a: (p[3] ?? 255) / 255 });

function ratioOn(color: string, ground: Rgba): number {
  const [c, op] = color.split('|opacity:');
  const t = parseColor(c!);
  if (op) t.a *= Number(op);
  return ratio(over(t, ground), ground);
}

/** The ground is read from a screenshot of the box with its text hidden; the colour goes through the helper. */
async function contrastOf(page: Page, probe: Probe, vw: number, vh: number) {
  const found = await page.evaluate(
    ([kind, text]) => {
      const root = document.querySelector('.sr-ply[data-skin="study"]')!;
      const all = Array.from(root.querySelectorAll<HTMLElement>('*')).filter(
        (e) => e.children.length === 0 || e.matches('.sr-ply-ticker'),
      );
      const el = all.find(
        (e) =>
          (e.textContent ?? '').trim().slice(0, 60) === text &&
          (kind === 'title' ? e.matches('.sr-ply-ticker') : true),
      );
      if (!el) return false;
      el.setAttribute('data-p93-probe', '1');
      return true;
    },
    [probe.kind, probe.text],
  );
  if (!found) return null;
  const style = await page.addStyleTag({
    content:
      '[data-p93-probe="1"], [data-p93-probe="1"] * { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }',
  });
  const box = await page.evaluate(() => {
    const e = document.querySelector<HTMLElement>('[data-p93-probe="1"]')!;
    e.scrollIntoView({ block: 'center', inline: 'center' });
    const r = e.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  });
  const x0 = Math.max(0, Math.floor(box.x));
  const y0 = Math.max(0, Math.floor(box.y));
  const png = await page.screenshot({
    clip: {
      x: x0,
      y: y0,
      width: Math.max(3, Math.min(vw - x0, Math.ceil(box.w))),
      height: Math.max(3, Math.min(vh - y0, Math.ceil(box.h))),
    },
  });
  await page.evaluate(() =>
    document.querySelector('[data-p93-probe="1"]')?.removeAttribute('data-p93-probe'),
  );
  await style.evaluate((el) => (el as Element).remove());
  const px = await pixelsOf(page, png);
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
  const forHelper = opacity ? `${srgb}|opacity:${opacity}` : srgb;
  const gr = px.grid.map((p) => ratioOn(forHelper, rgba(p)));
  const sorted = [...gr].sort((a, b) => a - b);
  return {
    kind: probe.kind,
    text: probe.text,
    color: probe.color,
    colorAsSrgb: srgb,
    gridMin: Number(sorted[0]!.toFixed(2)),
    gridP20: Number(sorted[Math.floor(gr.length * 0.2)]!.toFixed(2)),
    gridMedian: Number(sorted[Math.floor(gr.length / 2)]!.toFixed(2)),
  };
}

async function contrastAll(page: Page, vw: number, vh: number) {
  const probes = await page.evaluate(probesInPage);
  const out = [];
  for (const p of probes) {
    const c = await contrastOf(page, p, vw, vh).catch((e: unknown) => ({
      kind: p.kind,
      text: p.text,
      error: String(e),
    }));
    if (c) out.push(c);
  }
  return out;
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

/** In the page: every control's box (viewport coordinates, every scroller at its start), and the effective 44 px targets that overlap. */
function boxesInPage(): {
  boxes: { name: string; box: number[] }[];
  overlaps: string[];
  gaps: { name: string; above: number | null; below: number | null; beside: number | null }[];
} {
  const root = document.querySelector<HTMLElement>('.sr-ply[data-skin="study"]')!;
  for (let a: HTMLElement | null = root; a; a = a.parentElement) {
    if (a.scrollTop) a.scrollTop = 0;
  }
  const sel = 'button, input:not([type="hidden"]), textarea, a[href]';
  const els = Array.from(root.querySelectorAll<HTMLElement>(sel)).filter((e) => {
    const r = e.getBoundingClientRect();
    return r.width > 4 && r.height > 4 && getComputedStyle(e).display !== 'none';
  });
  const name = (e: Element) =>
    (
      e.getAttribute('aria-label') ||
      (e.textContent ?? '').trim().replace(/\s+/g, ' ') ||
      String(e.getAttribute('class') ?? '')
    ).slice(0, 30);
  const rects = els.map((e) => {
    const r = e.getBoundingClientRect();
    return { e, n: name(e), l: r.left, t: r.top, r: r.right, b: r.bottom };
  });
  const boxes = rects.map((x) => ({
    name: x.n,
    box: [Math.round(x.l), Math.round(x.t), Math.round(x.r - x.l), Math.round(x.b - x.t)],
  }));
  // Effective target: at least 44 px high and wide, centred on the drawn box (the ::after).
  const tg = rects.map((x) => {
    const w = Math.max(44, x.r - x.l);
    const h = Math.max(44, x.b - x.t);
    const cx = (x.l + x.r) / 2;
    const cy = (x.t + x.b) / 2;
    return { n: x.n, e: x.e, l: cx - w / 2, r: cx + w / 2, t: cy - h / 2, b: cy + h / 2 };
  });
  const overlaps: string[] = [];
  for (let i = 0; i < tg.length; i++)
    for (let j = i + 1; j < tg.length; j++) {
      const a = tg[i]!;
      const b = tg[j]!;
      if (a.e.contains(b.e) || b.e.contains(a.e)) continue;
      const w = Math.min(a.r, b.r) - Math.max(a.l, b.l);
      const h = Math.min(a.b, b.b) - Math.max(a.t, b.t);
      if (w > 0.5 && h > 0.5) overlaps.push(`${a.n} x ${b.n} (${Math.round(w)}x${Math.round(h)})`);
    }
  const gaps = rects.map((x) => {
    let above: number | null = null;
    let below: number | null = null;
    let beside: number | null = null;
    for (const y of rects) {
      if (y === x || x.e.contains(y.e) || y.e.contains(x.e)) continue;
      const hOver = Math.min(x.r, y.r) - Math.max(x.l, y.l) > 0;
      const vOver = Math.min(x.b, y.b) - Math.max(x.t, y.t) > 0;
      if (hOver && y.b <= x.t) above = Math.min(above ?? 1e9, x.t - y.b);
      if (hOver && y.t >= x.b) below = Math.min(below ?? 1e9, y.t - x.b);
      if (vOver && y.l >= x.r) beside = Math.min(beside ?? 1e9, y.l - x.r);
      if (vOver && y.r <= x.l) beside = Math.min(beside ?? 1e9, x.l - y.r);
    }
    const f = (v: number | null) => (v === null ? null : Math.round(v));
    return { name: x.n, above: f(above), below: f(below), beside: f(beside) };
  });
  return { boxes, overlaps, gaps };
}

/** A real touch tap on a locator at (centre + dx, centre + dy), after the control is scrolled into view. */
async function tap(page: Page, loc: Locator, fx = 0.5, dy = 0) {
  await loc.scrollIntoViewIfNeeded();
  await page.waitForTimeout(500); // a scroller may still be settling
  const b = (await loc.boundingBox())!;
  const x = b.x + b.width * fx;
  const y = b.y + b.height / 2 + dy;
  await page.touchscreen.tap(x, y);
  await page.waitForTimeout(250);
  return { x: Math.round(x), y: Math.round(y) };
}

test('SEC-AUD-420 / 424: the Study skin on a phone, hit test and readout contrast', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const report: Record<string, unknown> = { case: CASE, theme: THEME, long: LONG_TITLE };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();
  await ownerCtx.close();

  const initTheme = (th: string) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: th, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  };

  // ---------------------------------------------------------------- 375 x 812 with touch
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: state,
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  await ctx.addInitScript(initTheme, THEME);
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  await serveTone(page);
  await page.goto('/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(
    () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
  );
  const act = actions(page);
  await act.play(TRACKS);
  await page.waitForFunction(() => (window as unknown as { __srPlayer?: unknown }).__srPlayer);
  await expect.poll(async () => (await audio(page)).t, { timeout: 20_000 }).toBeGreaterThan(0.4);
  await act.setSkin('study');
  await expect(page.locator(ROOT)).toBeVisible();
  await page.waitForTimeout(1200);
  if (BASE) report.baseRulesRemoved = await switchOff(page);
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await audioCall(page, 'play').catch(() => undefined);
  await page.waitForTimeout(500);

  // Reduced motion: running animations with motion allowed, then reduced.
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
  const motionAllowed = await anims();
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.waitForTimeout(600);
  const motionReduced = await anims();
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.waitForTimeout(400);
  report.motion = { allowed: motionAllowed, reduced: motionReduced };
  await audioCall(page, 'pause');
  await page.waitForTimeout(500);

  report.identity = await page.evaluate(() => {
    const root = document.querySelector<HTMLElement>('.sr-ply');
    const themed = root?.closest('[data-theme]') as HTMLElement | null;
    return {
      skin: root?.getAttribute('data-skin') ?? null,
      themeOfPlayer: themed?.getAttribute('data-theme') ?? null,
      themeOfPage: document.documentElement.getAttribute('data-theme'),
    };
  });
  expect((report.identity as { skin: string }).skin).toBe('study');

  report.scroll375 = await page.evaluate(() => {
    const de = document.documentElement;
    const root = document.querySelector<HTMLElement>('.sr-ply')!;
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const r = root.getBoundingClientRect();
    return {
      document: [de.scrollWidth, de.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth],
      skin: [root.scrollWidth, root.clientWidth],
      skinBox: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
      skinScrollHeight: root.scrollHeight,
      dockVertical: [dock.scrollHeight, dock.clientHeight],
    };
  });
  report.fit = await measureFit(page);

  // The hit test: the shared tool, on the skin's root.
  const root = page.locator(ROOT);
  const opened = await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root);
  const slim = rows.map((r) => ({
    name: r.name,
    tag: r.tag,
    type: r.type,
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
      .map((p) => `${p.at}: ${p.taker?.tag}.${p.taker?.path} "${p.taker?.name}"`),
  }));
  report.opened = opened;
  report.controls = slim;
  const count = (s: ControlRow['status']) => rows.filter((r) => r.status === s).length;
  report.hit = {
    controls: rows.length,
    pass: count('pass'),
    fail: count('fail'),
    notReached: count('not-reached'),
    skipped: count('skipped'),
  };
  console.log(`REPORT hit ${JSON.stringify(report.hit)}`);
  for (const r of slim.filter((x) => x.status === 'fail' || x.status === 'not-reached'))
    console.log(
      `REPORT ${r.status} ${r.name} ${JSON.stringify(r.box)} ${r.reason ?? ''} ${r.takers.join(' | ')}`,
    );
  report.targets375 = await page.evaluate(boxesInPage);

  // Contrast, 375.
  report.contrast375 = await contrastAll(page, 375, 812);
  console.log(`REPORT contrast375 ${JSON.stringify(report.contrast375)}`);

  // Shots: the skin at its start and scrolled to its end (every scroller above the last queue row).
  await page.evaluate(() => {
    for (const e of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply, .sr-ply-dock')))
      e.scrollTop = 0;
  });
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${CASE}-start-375.png`) });
  report.queueTail = await page.evaluate(() => {
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const leaf = Array.from(document.querySelectorAll<HTMLElement>('.sr-ply *')).find(
      (e) => e.children.length === 0 && /^Tone 3/.test((e.textContent ?? '').trim()),
    );
    if (!leaf) return null;
    let moved = 0;
    for (let a: HTMLElement | null = leaf.parentElement; a; a = a.parentElement) {
      if (a.scrollHeight > a.clientHeight + 2 && getComputedStyle(a).overflowY !== 'visible') {
        a.scrollTop = a.scrollHeight;
        moved++;
      }
    }
    const r = leaf.getBoundingClientRect();
    const d = dock.getBoundingClientRect();
    return {
      row: [Math.round(r.top), Math.round(r.bottom)],
      dock: [Math.round(d.top), Math.round(d.bottom)],
      scrollersMoved: moved,
      inside: r.top >= d.top && r.bottom <= d.bottom,
    };
  });
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${CASE}-end-375.png`) });
  await page.evaluate(() => {
    for (const e of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply, .sr-ply-dock')))
      e.scrollTop = 0;
  });

  // Taps: each kind of control at its centre, 20 px above and 20 px below.
  const taps: Record<string, unknown> = {};
  report.taps = taps;
  if (!NOTAP) {
    const R = page.locator(ROOT);
    const offs = [0, -20, 20];
    // play and pause
    {
      const res: unknown[] = [];
      const btn = R.locator('.st-keys .st-key-btn').nth(1);
      // The store and the element can disagree after the spec's own pause: click until the audio is paused.
      for (let k = 0; k < 3; k++) {
        const label = (await btn.getAttribute('aria-label')) ?? '';
        if (!/pause/i.test(label) && (await audio(page)).paused) break;
        await btn.click();
        await page.waitForTimeout(300);
      }
      for (const dy of offs) {
        const before = (await audio(page)).paused;
        await tap(page, btn, 0.5, dy);
        await page.waitForTimeout(200);
        res.push({ dy, pausedBefore: before, pausedAfter: (await audio(page)).paused });
      }
      taps.playPause = res;
      for (let k = 0; k < 3; k++) {
        const label = (await btn.getAttribute('aria-label')) ?? '';
        if (!/pause/i.test(label) && (await audio(page)).paused) break;
        await btn.click();
        await page.waitForTimeout(300);
      }
    }
    // a queue row plays
    {
      const res: unknown[] = [];
      for (const dy of offs) {
        const idx = () =>
          R.locator('.st-queue li[data-current="true"]').evaluate((li) =>
            Array.from(li.parentElement!.children).indexOf(li),
          );
        await tap(page, R.locator('.st-queue li button').nth(0), 0.5, dy);
        const first = await idx();
        await tap(page, R.locator('.st-queue li button').nth(2), 0.5, dy);
        res.push({ dy, afterFirstRow: first, afterThirdRow: await idx() });
      }
      taps.queueRow = res;
      await tap(page, R.locator('.st-queue li button').nth(0), 0.5, 0);
      await audioCall(page, 'pause');
    }
    // seek
    {
      const res: unknown[] = [];
      const seek = R.locator('.st-seek');
      const targets: [number, number][] = [
        [0.3, 0],
        [0.6, -20],
        [0.85, 20],
      ];
      for (const [fx, dy] of targets) {
        await tap(page, seek, fx, dy);
        const a = await audio(page);
        res.push({
          fx,
          dy,
          position: Number(a.t.toFixed(1)),
          duration: Number(a.d.toFixed(1)),
          expected: Number((fx * a.d).toFixed(1)),
        });
      }
      taps.seek = res;
    }
    // volume
    {
      const res: unknown[] = [];
      const vol = R.locator('.st-tile-vol input[type="range"]');
      const targets: [number, number][] = [
        [0.25, 0],
        [0.5, -20],
        [0.75, 20],
      ];
      for (const [fx, dy] of targets) {
        await tap(page, vol, fx, dy);
        res.push({ fx, dy, rangeValue: Number(await vol.inputValue()) });
      }
      taps.volume = res;
    }
    // the skin button (cycles to the next skin, then back to Study)
    {
      const res: unknown[] = [];
      // Warm the next skin's lazy chunk once (the dev server compiles it on first use), then come back.
      await act.setSkin('stack');
      await page.waitForFunction(
        () => document.querySelector('.sr-ply')?.getAttribute('data-skin') === 'stack',
        undefined,
        { timeout: 30_000 },
      );
      await act.setSkin('study');
      await expect(page.locator(ROOT)).toBeVisible();
      await page.waitForTimeout(500);
      for (const dy of offs) {
        await tap(page, R.locator('.st-dot-a'), 0.5, dy);
        const to = await page.evaluate(() =>
          document.querySelector('.sr-ply')?.getAttribute('data-skin'),
        );
        res.push({ dy, skinAfter: to });
        await act.setSkin('study');
        await expect(page.locator(ROOT)).toBeVisible();
        await page.waitForTimeout(500);
      }
      taps.skinButton = res;
    }
    // the move button (cycles the dock through four places)
    {
      const res: unknown[] = [];
      for (const dy of [0, -20, 20, 0]) {
        const before = await page.evaluate(() =>
          document.querySelector('.sr-ply-dock')?.getAttribute('data-dock'),
        );
        await tap(page, R.locator('.st-dot-b'), 0.5, dy);
        const after = await page.evaluate(() =>
          document.querySelector('.sr-ply-dock')?.getAttribute('data-dock'),
        );
        res.push({ dy, before, after });
      }
      taps.move = res;
    }
    // close and reopen
    {
      const res: unknown[] = [];
      for (const dy of offs) {
        await tap(page, R.locator('.st-dot-c'), 0.5, dy);
        await page.waitForTimeout(300);
        const closed = (await page.locator('.sr-ply').count()) === 0;
        await act.play(TRACKS);
        await act.setSkin('study');
        await expect(page.locator(ROOT)).toBeVisible();
        await page.waitForTimeout(500);
        res.push({ dy, closed, reopened: (await page.locator(ROOT).count()) > 0 });
      }
      taps.closeReopen = res;
    }
  }
  console.log(`REPORT taps ${JSON.stringify(taps)}`);
  await ctx.close();

  // ---------------------------------------------------------------- 1280 x 800
  const ctx2 = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 1280, height: 800 },
    storageState: state,
    bypassCSP: true,
  });
  await ctx2.addInitScript(initTheme, THEME);
  const p2 = await ctx2.newPage();
  p2.setDefaultTimeout(20_000);
  await serveTone(p2);
  await p2.goto('/channels/@me');
  await p2.waitForFunction(() => document.readyState === 'complete');
  await p2.mouse.click(2, 2);
  await p2.waitForFunction(
    () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
  );
  await actions(p2).play(TRACKS);
  await p2.waitForFunction(() => (window as unknown as { __srPlayer?: unknown }).__srPlayer);
  await actions(p2).setSkin('study');
  await expect(p2.locator(ROOT)).toBeVisible();
  await p2.waitForTimeout(1200);
  if (BASE) await switchOff(p2);
  await audioCall(p2, 'pause');
  await p2.waitForTimeout(500);
  report.scroll1280 = await p2.evaluate(() => {
    const de = document.documentElement;
    const root = document.querySelector<HTMLElement>('.sr-ply')!;
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const r = root.getBoundingClientRect();
    return {
      document: [de.scrollWidth, de.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth],
      skin: [root.scrollWidth, root.clientWidth],
      skinBox: [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)],
    };
  });
  const t1280 = await p2.evaluate(boxesInPage);
  report.boxes1280 = t1280.boxes;
  writeFileSync(resolve(OUT, `boxes1280-${CASE}.json`), JSON.stringify(t1280.boxes, null, 2));
  try {
    const baseBoxes = JSON.parse(
      (await import('node:fs')).readFileSync(resolve(OUT, 'boxes1280-base.json'), 'utf8'),
    ) as { name: string; box: number[] }[];
    let maxDiff = 0;
    let same = baseBoxes.length === t1280.boxes.length;
    if (same)
      baseBoxes.forEach((b, i) => {
        const a = t1280.boxes[i]!;
        if (a.name !== b.name) same = false;
        for (let k = 0; k < 4; k++) maxDiff = Math.max(maxDiff, Math.abs(a.box[k]! - b.box[k]!));
      });
    report.boxes1280VsBase = { sameControls: same, maxDiffPx: maxDiff, count: t1280.boxes.length };
  } catch {
    report.boxes1280VsBase = 'no base file in the folder';
  }
  report.contrast1280 = await contrastAll(p2, 1280, 800);
  console.log(`REPORT contrast1280 ${JSON.stringify(report.contrast1280)}`);
  await p2.evaluate(() => {
    for (const e of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply, .sr-ply-dock')))
      e.scrollTop = 0;
  });
  await p2.waitForTimeout(300);
  await p2.screenshot({ path: resolve(OUT, `${CASE}-1280.png`) });
  console.log(
    `REPORT scroll375 ${JSON.stringify(report.scroll375)} scroll1280 ${JSON.stringify(report.scroll1280)}`,
  );
  writeFileSync(resolve(OUT, `measure-${CASE}.json`), JSON.stringify(report, null, 2));
  await ctx2.close();

  if (!BASE) {
    expect(rows.filter((r) => r.status === 'fail' || r.status === 'not-reached')).toEqual([]);
  }
});
