import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { apiPatch, expect, test } from '../support/fixtures';
import { CONTROL_SELECTOR, hitTestAll, openCollapsed } from '../support/hit-test';
import { measureFit, serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-417 (wave 94; D-930, D-934): the Valve player skin passes the 44 px hit test (D-856) on a phone, at 375 and
 * at 700 (where the skin's wide layout shows); SEC-AUD-425 (D-931): its title reaches 4.5. Needs the visual sweep's
 * seeded world (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never
 * edited): the signed-in Owner at `/channels/@me`, the account's main theme set by PATCH and localStorage and read
 * back, touch, the player playing three tone tracks whose first title is 57 characters, the skin chosen through the
 * dev handle and read back (`.sr-ply[data-skin="valve"]`).
 *
 * Cases: the base case (SR_P94_VALVE_BASE=1) removes the sheet's `@media (max-width: 720px)` rules and the title's own
 * rule (`.sr-ply-valve .v-plate .sr-ply-ticker`) from the live document (the tree itself is the change, so the base is
 * the same tree with that CSS switched off); the after case measures the tree as it is. Each case, at 375 and at 700:
 * the skin and theme read back; sideways scroll of the document, the dock and the skin; the skin's box and scroll
 * heights; every control's box and its gap to the nearest control above, below and beside; `openCollapsed`, then
 * `hitTestAll` on the skin's root (`e2e/support/hit-test.ts`, no private probe: a `not-reached` row is a spec error).
 * The knobs' range inputs are `pointer-events: none` by design (the knob's wrapper takes the finger and turns the
 * knob), so the table is taken twice: with the helper's defaults (`strict`) and with `wrapper: '.v-knob'` (the one
 * the assertions use). At 375 also: running animations with motion allowed and with `prefers-reduced-motion:
 * reduce`; the third queue row at the end of the scroll; the title's contrast (text hidden, ground from a
 * screenshot's pixels, 21 grid samples, `support/contrast.ts`); two shots. At 1280 x 800 every control's box and the
 * title's contrast. The after case taps (real touch) every kind of control at its centre, 20 px above and 20 px
 * below. Seek and the speed slider are checked by the input's own `input` event and value (D-935), never by
 * `audio.currentTime`. With BASE=1 the after case's 1280 boxes are compared with the base's (0 px).
 *
 * Local only (SR_P94_VALVE_ON=1). SR_P94_VALVE_OUT is the shots folder, SR_P94_VALVE_BASE=1 adds the base case.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8725, Vite 5725):
 *   SR_E2E_BASE_URL=http://localhost:5725 SR_P94_VALVE_ON=1 SR_P94_VALVE_BASE=1 \
 *   SR_P94_VALVE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-valve \
 *   npx playwright test tests/p94-valve-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P94_VALVE_ON === '1';
const BASE = process.env.SR_P94_VALVE_BASE === '1';
const OUT =
  process.env.SR_P94_VALVE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p94-valve';
const THEME = 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p94v-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const SKIN = '.sr-ply[data-skin="valve"]';
const TITLE_RULE = '.sr-ply-valve .v-plate .sr-ply-ticker';

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

type Win = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
  __p94In?: Record<string, number[]>;
};

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

async function openPage(
  browser: import('@playwright/test').Browser,
  state: Awaited<ReturnType<import('@playwright/test').BrowserContext['storageState']>>,
  width: number,
) {
  const phone = width < 1000;
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width, height: 812 } : { width: 1280, height: 800 },
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
  await page.goto('/channels/@me');
  await page.waitForFunction(() => document.readyState === 'complete');
  await page.mouse.click(2, 2);
  await page.waitForFunction(() => (window as unknown as Partial<Win>).__srPlayerActions);
  await page.evaluate((tracks) => {
    (window as unknown as Win).__srPlayerActions.play(tracks);
  }, TRACKS);
  await page.waitForFunction(() => (window as unknown as Partial<Win>).__srPlayer);
  await expect
    .poll(() => page.evaluate(() => (window as unknown as Win).__srPlayer.audio.currentTime), {
      timeout: 20_000,
    })
    .toBeGreaterThan(0.4);
  await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('valve'));
  await expect(page.locator(SKIN)).toBeVisible();
  await page.waitForTimeout(1500);
  return { ctx, page };
}

/** Switches the phone block and the title's own rule off in the live document (the base case). */
async function stripChange(
  page: Page,
  titleRule: string,
): Promise<{ media: number; title: number }> {
  return page.evaluate((tr) => {
    let media = 0;
    let title = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      const owner = sheet.ownerNode as HTMLElement | null;
      const id = owner?.getAttribute?.('data-vite-dev-id') ?? '';
      if (!id.includes('valve.css')) continue;
      for (let i = sheet.cssRules.length - 1; i >= 0; i--) {
        const r = sheet.cssRules[i]!;
        if (r instanceof CSSMediaRule && /max-width:\s*720px/.test(r.conditionText)) {
          sheet.deleteRule(i);
          media++;
        } else if (r instanceof CSSStyleRule && r.selectorText === tr) {
          sheet.deleteRule(i);
          title++;
        }
      }
    }
    return { media, title };
  }, titleRule);
}

interface Geo {
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Every control's box in the skin's own coordinates (top-left of its content, scroll added back). */
async function geometry(page: Page): Promise<Geo[]> {
  return page.evaluate((sel) => {
    const skin = document.querySelector<HTMLElement>('.sr-ply[data-skin="valve"]')!;
    const o = skin.getBoundingClientRect();
    return Array.from(skin.querySelectorAll<HTMLElement>(sel))
      .filter((e) => {
        const cs = getComputedStyle(e);
        return cs.display !== 'none' && !(e as HTMLButtonElement).disabled;
      })
      .map((e) => {
        const r = e.getBoundingClientRect();
        const name = (e.getAttribute('aria-label') || e.textContent || e.className || e.tagName)
          .trim()
          .replace(/\s+/g, ' ')
          .slice(0, 32);
        return {
          name,
          x: Math.round((r.left - o.left + skin.scrollLeft) * 10) / 10,
          y: Math.round((r.top - o.top + skin.scrollTop) * 10) / 10,
          w: Math.round(r.width * 10) / 10,
          h: Math.round(r.height * 10) / 10,
        };
      });
  }, CONTROL_SELECTOR);
}

/** The gap in px from a control to the nearest other control above, below and beside it (null: none). */
function gaps(all: Geo[], a: Geo) {
  let above: number | null = null;
  let below: number | null = null;
  let beside: number | null = null;
  for (const b of all) {
    if (b === a) continue;
    const xo = Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const yo = Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    if (xo > 1) {
      if (b.y + b.h <= a.y + 0.5) {
        const g = a.y - (b.y + b.h);
        above = above === null ? g : Math.min(above, g);
      } else if (b.y >= a.y + a.h - 0.5) {
        const g = b.y - (a.y + a.h);
        below = below === null ? g : Math.min(below, g);
      }
    }
    if (yo > 1) {
      if (b.x + b.w <= a.x + 0.5) {
        const g = a.x - (b.x + b.w);
        beside = beside === null ? g : Math.min(beside, g);
      } else if (b.x >= a.x + a.w - 0.5) {
        const g = b.x - (a.x + a.w);
        beside = beside === null ? g : Math.min(beside, g);
      }
    }
  }
  const r = (v: number | null) => (v === null ? null : Math.round(v * 10) / 10);
  return { above: r(above), below: r(below), beside: r(beside) };
}

async function scrollState(page: Page) {
  return page.evaluate(() => {
    const de = document.documentElement;
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const skin = document.querySelector<HTMLElement>('.sr-ply[data-skin="valve"]')!;
    const r = skin.getBoundingClientRect();
    return {
      document: [de.scrollWidth, de.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth],
      skin: [skin.scrollWidth, skin.clientWidth],
      skinBox: [Math.round(r.width), Math.round(r.height)],
      skinScrollHeight: [skin.scrollHeight, skin.clientHeight],
      dockScrollHeight: [dock.scrollHeight, dock.clientHeight],
    };
  });
}

const runningAnimations = (page: Page) =>
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

async function toStart(page: Page) {
  await page.evaluate(() => {
    for (const s of ['.sr-ply-dock', '.sr-ply[data-skin="valve"]'])
      document.querySelector<HTMLElement>(s)!.scrollTop = 0;
    window.scrollTo(0, 0);
  });
  await page.waitForTimeout(250);
}

async function toEnd(page: Page) {
  await page.evaluate(() => {
    for (const s of ['.sr-ply[data-skin="valve"]', '.sr-ply-dock']) {
      const e = document.querySelector<HTMLElement>(s)!;
      e.scrollTop = e.scrollHeight;
    }
  });
  await page.waitForTimeout(250);
}

/** After a scroll to the end: is the third queue row whole, and does the queue scroll by itself (a second scroller)? */
async function queueTail(page: Page) {
  return page.evaluate(() => {
    const q = document.querySelector<HTMLElement>('.v-queue')!;
    const rows = Array.from(q.querySelectorAll<HTMLElement>('li'));
    const last = rows[rows.length - 1]!.getBoundingClientRect();
    const skin = document
      .querySelector<HTMLElement>('.sr-ply[data-skin="valve"]')!
      .getBoundingClientRect();
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!.getBoundingClientRect();
    return {
      rows: rows.length,
      lastBottom: Math.round(last.bottom),
      skinBottom: Math.round(skin.bottom),
      dockBottom: Math.round(dock.bottom),
      viewport: window.innerHeight,
      queueScroll: [q.scrollHeight, q.clientHeight],
    };
  });
}

async function tap(page: Page, selector: string, dy: number, fx = 0.5, fy: number | null = null) {
  const b = await page
    .locator(selector)
    .first()
    .evaluate((el) => {
      el.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
      const r = el.getBoundingClientRect();
      return { x: r.left, y: r.top, w: r.width, h: r.height };
    });
  const y = fy === null ? b.y + b.h / 2 + dy : b.y + b.h * fy;
  await page.touchscreen.tap(b.x + b.w * fx, y);
  return { x: b.x + b.w * fx, y };
}

const attr = (page: Page, selector: string, name: string) =>
  page.locator(selector).first().getAttribute(name);

/** Starts recording the `input` events of an element (the values at the moment they fire). */
async function recordInputs(page: Page, selector: string, key: string) {
  await page.evaluate(
    ([sel, k]) => {
      const el = document.querySelector<HTMLInputElement>(sel!)!;
      const w = window as unknown as Win;
      w.__p94In = w.__p94In ?? {};
      w.__p94In[k!] = [];
      el.addEventListener('input', () => w.__p94In![k!]!.push(el.valueAsNumber));
    },
    [selector, key],
  );
}
const recorded = (page: Page, key: string) =>
  page.evaluate((k) => (window as unknown as Win).__p94In?.[k] ?? [], key);

/** The after case's taps: every kind of control, at its centre, 20 px above and 20 px below. */
async function taps(page: Page): Promise<Record<string, unknown>> {
  const out: Record<string, unknown> = {};
  const DYS = [0, -20, 20];
  // Play and pause: three taps, each flips the transport's busy flag.
  const keys = '.v-keys';
  const play = '.v-keys .v-key:nth-child(2)';
  const seen: string[] = [];
  for (const dy of DYS) {
    const before = await attr(page, keys, 'data-busy');
    await tap(page, play, dy);
    await expect.poll(() => attr(page, keys, 'data-busy')).not.toBe(before);
    seen.push(`${dy}:${before}->${await attr(page, keys, 'data-busy')}`);
  }
  out.playPause = seen;
  if ((await attr(page, keys, 'data-busy')) !== 'true') await tap(page, play, 0);
  // Queue rows: each tap plays that row.
  const rows: string[] = [];
  for (const [i, dy] of [
    [1, 0],
    [2, -20],
    [0, 20],
  ] as const) {
    await tap(page, `.v-queue li:nth-child(${i + 1}) button`, dy);
    await expect(page.locator(`.v-queue li:nth-child(${i + 1}) button`)).toHaveAttribute(
      'aria-current',
      'true',
    );
    rows.push(`row ${i + 1} at ${dy}`);
  }
  out.rows = rows;
  // Seek: the input's own `input` event carries the tapped value (D-935: not the audio's time).
  const seek = '.v-seek';
  await expect(page.locator(seek)).toBeEnabled();
  await expect.poll(async () => Number(await attr(page, seek, 'max'))).toBeGreaterThan(10);
  const max = Number(await attr(page, seek, 'max'));
  await recordInputs(page, seek, 'seek');
  const seekWant: number[] = [];
  for (const [fx, dy] of [
    [0.8, 0],
    [0.1, -20],
    [0.6, 20],
  ] as const) {
    const before = (await recorded(page, 'seek')).length;
    await tap(page, seek, dy, fx);
    await expect.poll(async () => (await recorded(page, 'seek')).length).toBeGreaterThan(before);
    const got = (await recorded(page, 'seek')).at(-1)!;
    console.log(`REPORT seek tap ${fx}/${dy}: input event value ${got} of max ${max}`);
    seekWant.push(got);
    expect(Math.abs(got - fx * max), `seek ${fx}`).toBeLessThan(0.15 * max);
  }
  out.seek = { max, inputValues: seekWant };
  // Tape speed: the same, by the input event.
  const speed = '.v-speed-input';
  await recordInputs(page, speed, 'speed');
  const speedVals: number[] = [];
  for (const [fx, dy] of [
    [0.9, 0],
    [0.1, -20],
    [0.6, 20],
  ] as const) {
    const before = (await recorded(page, 'speed')).length;
    await tap(page, speed, dy, fx);
    await expect.poll(async () => (await recorded(page, 'speed')).length).toBeGreaterThan(before);
    const got = (await recorded(page, 'speed')).at(-1)!;
    speedVals.push(got);
    expect(Math.abs(got - (0.75 + fx * 0.5)), `speed ${fx}`).toBeLessThan(0.08);
  }
  out.speed = speedVals;
  // Knobs: a tap on the knob's art focuses its input (the wrapper's pointer-down), and a mouse drag turns it.
  const knobs: string[] = [];
  for (const [i, dy] of [
    [0, 0],
    [5, -20],
    [6, 20],
  ] as const) {
    const sel = `.v-knobs .v-knob:nth-child(${i + 1}) .v-knob-art`;
    const input = `.v-knobs .v-knob:nth-child(${i + 1}) .v-knob-input`;
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await tap(page, sel, dy);
    await expect
      .poll(() => page.evaluate((s) => document.activeElement === document.querySelector(s), input))
      .toBe(true);
    const before = Number(await page.locator(input).inputValue());
    const b = await page
      .locator(sel)
      .first()
      .evaluate((el) => {
        const r = el.getBoundingClientRect();
        return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
      });
    await page.mouse.move(b.x, b.y);
    await page.mouse.down();
    await page.mouse.move(b.x + 20, b.y, { steps: 4 });
    await page.mouse.move(b.x + 40, b.y, { steps: 4 });
    await page.mouse.up();
    const after = Number(await page.locator(input).inputValue());
    knobs.push(`knob ${i + 1} at ${dy}: focus ok, drag ${before} -> ${after}`);
    expect(after, `knob ${i + 1} drag`).not.toBe(before);
  }
  out.knobs = knobs;
  // Toggles: a tap flips aria-pressed (tone, shuffle, mute) or changes the label (loop, repeat).
  for (const [name, n, by] of [
    ['tone', 1, 'aria-pressed'],
    ['loop', 2, 'aria-label'],
    ['shuffle', 3, 'aria-pressed'],
    ['repeat', 4, 'aria-label'],
    ['mute', 5, 'aria-pressed'],
  ] as const) {
    const sel = `.v-bank .v-tg:nth-child(${n}) .v-lever-btn`;
    const flips: string[] = [];
    for (const dy of [...DYS, 0]) {
      const before = await attr(page, sel, by);
      await tap(page, sel, dy);
      await expect.poll(() => attr(page, sel, by)).not.toBe(before);
      flips.push(`${dy}:${before}->${await attr(page, sel, by)}`);
    }
    out[name] = flips;
  }
  // Move: four taps go round the four docks and back.
  const dock = '.sr-ply-dock';
  const docks: string[] = [String(await attr(page, dock, 'data-dock'))];
  for (const dy of [...DYS, 0]) {
    await tap(page, '.v-mains-btns .v-cap:nth-child(2)', dy);
    await page.waitForTimeout(250);
    docks.push(String(await attr(page, dock, 'data-dock')));
  }
  out.move = docks;
  expect(new Set(docks.slice(0, 4)).size).toBe(4);
  expect(docks[4]).toBe(docks[0]);
  await toStart(page);
  // Skin button: each tap changes the skin; the skin is put back by the handle.
  const skins: string[] = [];
  for (const dy of DYS) {
    await tap(page, '.v-mains-btns .v-cap:nth-child(1)', dy);
    await expect(page.locator(SKIN)).toHaveCount(0);
    skins.push(`${dy}:${await attr(page, '.sr-ply', 'data-skin')}`);
    await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('valve'));
    await expect(page.locator(SKIN)).toBeVisible();
    await page.waitForTimeout(600);
  }
  out.skinButton = skins;
  // Close: each tap closes the player; it is opened again with a queue.
  const closes: string[] = [];
  for (const dy of DYS) {
    await tap(page, '.v-mains-btns .v-cap:nth-child(3)', dy);
    await expect(page.locator('.sr-ply')).toHaveCount(0);
    closes.push(`${dy}: closed`);
    await page.evaluate((t) => (window as unknown as Win).__srPlayerActions.play(t), TRACKS);
    await expect(page.locator('.sr-ply')).toBeVisible();
    await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('valve'));
    await expect(page.locator(SKIN)).toBeVisible();
    await page.waitForTimeout(600);
  }
  out.close = closes;
  return out;
}

/** The title's pixels (text hidden): a 7 x 3 grid of the box, as rgba, from a screenshot of the box. */
async function gridOf(page: Page, png: Buffer): Promise<number[][]> {
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
    const inset = Math.max(1, Math.min(4, Math.floor(w / 4), Math.floor(h / 4)));
    const grid: number[][] = [];
    for (let j = 0; j < 3; j++)
      for (let ii = 0; ii < 7; ii++)
        grid.push(
          px(inset + (ii * (w - 1 - 2 * inset)) / 6, inset + (j * (h - 1 - 2 * inset)) / 2),
        );
    return grid;
  }, png.toString('base64'));
}

const rgba = (p: number[]): Rgba => ({ r: p[0]!, g: p[1]!, b: p[2]!, a: (p[3] ?? 255) / 255 });

/** Contrast of the title (the ticker's colour on the pixels under it, text hidden), as wave 92's spec took it. */
async function titleContrast(page: Page) {
  const probe = await page.evaluate(() => {
    const el = document.querySelector<HTMLElement>('.sr-ply[data-skin="valve"] .sr-ply-ticker')!;
    el.setAttribute('data-p94-probe', '1');
    return {
      color: getComputedStyle(el).color,
      text: (el.textContent ?? '').trim().slice(0, 60),
      vw: window.innerWidth,
      vh: window.innerHeight,
    };
  });
  const style = await page.addStyleTag({
    content:
      '[data-p94-probe="1"], [data-p94-probe="1"] * { color: transparent !important; -webkit-text-fill-color: transparent !important; text-shadow: none !important; }',
  });
  const box = await page.evaluate(() => {
    const e = document.querySelector<HTMLElement>('[data-p94-probe="1"]')!;
    e.scrollIntoView({ block: 'center', inline: 'center' });
    const r = e.getBoundingClientRect();
    return { x: r.left, y: r.top, w: r.width, h: r.height };
  });
  const x = Math.max(0, Math.floor(box.x));
  const y = Math.max(0, Math.floor(box.y));
  const png = await page.screenshot({
    clip: {
      x,
      y,
      width: Math.max(3, Math.min(probe.vw - x, Math.ceil(box.w))),
      height: Math.max(3, Math.min(probe.vh - y, Math.ceil(box.h))),
    },
  });
  await style.evaluate((n) => (n as Element).remove());
  await page.evaluate(() =>
    document.querySelector('[data-p94-probe="1"]')?.removeAttribute('data-p94-probe'),
  );
  const grid = await gridOf(page, png);
  // The browser's own canvas turns oklab()/color-mix() strings into sRGB first (the helper reads sRGB).
  const srgb = await page.evaluate((c) => {
    const k = document.createElement('canvas');
    k.width = k.height = 1;
    const g = k.getContext('2d')!;
    g.fillStyle = c;
    g.fillRect(0, 0, 1, 1);
    const d = g.getImageData(0, 0, 1, 1).data;
    return `rgba(${d[0]}, ${d[1]}, ${d[2]}, ${(d[3]! / 255).toFixed(3)})`;
  }, probe.color);
  const t = parseColor(srgb);
  const gr = grid.map((p) => ratio(over(t, rgba(p)), rgba(p)));
  const sorted = [...gr].sort((a, b) => a - b);
  return {
    text: probe.text,
    color: probe.color,
    colorAsSrgb: srgb,
    box: [Math.round(box.x), Math.round(box.y), Math.round(box.w), Math.round(box.h)],
    gridMin: Number(Math.min(...gr).toFixed(2)),
    gridP20: Number(sorted[Math.floor(gr.length * 0.2)]!.toFixed(2)),
    gridMedian: Number(sorted[Math.floor(gr.length / 2)]!.toFixed(2)),
  };
}

function countRows(rows: Awaited<ReturnType<typeof hitTestAll>>) {
  return {
    controls: rows.length,
    pass: rows.filter((r) => r.status === 'pass').length,
    fail: rows.filter((r) => r.status === 'fail').length,
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    skipped: rows.filter((r) => r.status === 'skipped').length,
  };
}

function describeRows(rows: Awaited<ReturnType<typeof hitTestAll>>) {
  return rows.map((r) => ({
    name: r.name,
    path: r.path,
    status: r.status,
    reason: r.reason,
    box: r.result
      ? [r.result.box.x, r.result.box.y, r.result.box.width, r.result.box.height].map(
          (v) => Math.round(v * 10) / 10,
        )
      : null,
    takers: (r.result?.points ?? [])
      .filter((p) => !p.ok)
      .map(
        (p) => `${p.at}: ${p.taker ? `${p.taker.tag} ${p.taker.name} [${p.taker.path}]` : 'null'}`,
      ),
  }));
}

test('SEC-AUD-417 / 425: the Valve skin on a phone, hit-tested, and its title', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const report: Record<string, unknown> = { theme: THEME, long: LONG_TITLE };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();

  const baseDesktop: Record<string, Geo> = {};
  for (const label of BASE ? (['base', 'after'] as const) : (['after'] as const)) {
    const entry: Record<string, unknown> = {};
    report[label] = entry;

    for (const width of [375, 700] as const) {
      const w: Record<string, unknown> = {};
      entry[`w${width}`] = w;
      const { ctx, page } = await openPage(browser, state, width);
      if (label === 'base') w.removed = await stripChange(page, TITLE_RULE);
      await page.waitForTimeout(400);
      w.identity = await page.evaluate(() => {
        const root = document.querySelector<HTMLElement>('.sr-ply')!;
        return {
          skin: root.getAttribute('data-skin'),
          theme: (root.closest('[data-theme]') as HTMLElement | null)?.getAttribute('data-theme'),
          motion: root.getAttribute('data-motion'),
          layout: root.getBoundingClientRect().width > 608 ? 'wide' : 'narrow',
        };
      });
      expect((w.identity as { skin: string }).skin).toBe('valve');

      let reduced = -1;
      if (width === 375) {
        await page.emulateMedia({ reducedMotion: 'no-preference' });
        await page.waitForTimeout(500);
        const allowed = await runningAnimations(page);
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await page.waitForTimeout(700);
        reduced = await runningAnimations(page);
        w.motion = { allowed, reduced };
        await page.emulateMedia({ reducedMotion: 'no-preference' });
        await page.waitForTimeout(400);
      }
      await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
      await page.waitForTimeout(400);

      await toStart(page);
      w.scroll = await scrollState(page);
      w.fit = await measureFit(page);
      const geo = await geometry(page);
      w.geometry = geo.map((g) => ({ ...g, gap: gaps(geo, g) }));
      if (width === 375) w.title = await titleContrast(page);

      const root = page.locator(SKIN);
      w.opened = await openCollapsed(page, root);
      const strict = await hitTestAll(page, root);
      const rows = await hitTestAll(page, root, { wrapper: '.v-knob' });
      w.strictCounts = countRows(strict);
      w.strictControls = describeRows(strict);
      const counts = countRows(rows);
      w.counts = counts;
      w.controls = describeRows(rows);
      console.log(
        `REPORT ${width} ${label}: ${JSON.stringify(counts)} strict ${JSON.stringify(w.strictCounts)} skin ${JSON.stringify((w.scroll as { skinBox: number[] }).skinBox)} ${(w.identity as { layout: string }).layout}`,
      );

      await toEnd(page);
      w.tail = await queueTail(page);
      if (width === 375 || label === 'after') {
        await toStart(page);
        if (width === 375) await page.screenshot({ path: resolve(OUT, `${label}-375-start.png`) });
        else await page.screenshot({ path: resolve(OUT, 'after-700-start.png') });
        await toEnd(page);
        await page.screenshot({ path: resolve(OUT, `${label}-${width}-end.png`) });
      }
      await toStart(page);
      w.scrollAfter = await scrollState(page);

      if (label === 'after') {
        expect.soft(counts.fail, `${width}: ${JSON.stringify(w.controls)}`).toBe(0);
        expect.soft(counts.notReached).toBe(0);
        for (const k of ['document', 'dock', 'skin'] as const) {
          const [sw, cw] = (w.scroll as Record<string, number[]>)[k]!;
          expect.soft(sw, `${width} ${k} scrollWidth`).toBe(cw);
        }
        const tail = w.tail as Awaited<ReturnType<typeof queueTail>>;
        expect
          .soft(tail.queueScroll[0], 'the queue scrolls by itself')
          .toBeLessThanOrEqual(tail.queueScroll[1]! + 1);
        expect
          .soft(tail.lastBottom, 'third row whole')
          .toBeLessThanOrEqual(Math.min(tail.skinBottom, tail.viewport) + 1);
        if (width === 375) {
          expect.soft(reduced).toBe(0);
          expect
            .soft((w.title as { gridMin: number }).gridMin, 'title minimum at 375')
            .toBeGreaterThan(4.5);
          await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.play());
          await page.waitForTimeout(400);
          w.taps = await taps(page);
        }
      }
      writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
      await ctx.close();
    }

    // ---- 1280 x 800
    const wide = await openPage(browser, state, 1280);
    if (label === 'base') await stripChange(wide.page, TITLE_RULE);
    await wide.page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
    await wide.page.waitForTimeout(500);
    const gw = await geometry(wide.page);
    entry.desktopBoxes = gw;
    entry.desktopScroll = await scrollState(wide.page);
    entry.desktopTitle = await titleContrast(wide.page);
    if (label === 'base') for (const g of gw) baseDesktop[g.name] = g;
    else {
      expect
        .soft((entry.desktopTitle as { gridMin: number }).gridMin, 'title minimum at 1280')
        .toBeGreaterThan(4.5);
      await toStart(wide.page);
      await wide.page.screenshot({ path: resolve(OUT, 'after-1280.png') });
      if (BASE) {
        const diffs = gw
          .map((g) => {
            const b = baseDesktop[g.name];
            return b
              ? Math.max(
                  Math.abs(b.x - g.x),
                  Math.abs(b.y - g.y),
                  Math.abs(b.w - g.w),
                  Math.abs(b.h - g.h),
                )
              : 999;
          })
          .filter((d) => d > 0);
        entry.desktopDiffVsBase = {
          controls: gw.length,
          baseControls: Object.keys(baseDesktop).length,
          differing: diffs,
        };
        expect.soft(diffs, 'a control box at 1280 differs from the base').toEqual([]);
      }
    }
    await wide.ctx.close();
    writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  }
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ownerCtx.close();
});
