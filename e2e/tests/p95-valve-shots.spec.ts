import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-432 (wave 95; D-940): when a skin scrolls inside itself (D-927) its frame is whole at every scroll position.
 * Valve's panel bevel (`.sr-ply-valve::before`) and its cabinet art (`svg.v-cabinet` in `.sr-ply-bd-backdrop`) are sized
 * to the skin's box, so on a phone they ended at the first screen's height. D-939: the knobs are tested through their
 * wrapper (`wrapper: '.v-knob'`); the range input stays `pointer-events: none`. Needs the visual sweep's seeded world
 * (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited): the Owner at
 * `/channels/@me`, the account's main theme set by PATCH and localStorage and read back, touch, three tone tracks, the
 * skin chosen through the dev handle and read back.
 *
 * The base case (SR_P95_VALVE_BASE=1) switches my rule off in the live document: the media block that sets
 * `--v-frame-whole` is deleted from the loaded sheet (the tree is the change). Each case, at 375 and at 700: the
 * root's clientHeight and scrollHeight; the bevel's computed box and the cabinet's and backdrop's boxes in the skin's
 * content coordinates; the frame by pixels (a screenshot of the skin's box decoded in the page) at the start, the
 * middle and the end of the scroll: the bevel's dark side column (1 px inside its inset) against the column 7 px inside
 * it at 8 rows (the drop, 0 or less: the bevel is gone), and the cabinet's noise as the pixel spread of a bare patch of
 * panel in the padding band at 5 heights; document, dock and skin `scrollWidth` against `clientWidth`; the after case
 * runs `openCollapsed` and `hitTestAll` with `wrapper: '.v-knob'` (no private probe). Shots: Valve 375 and 700, start and
 * end. With SR_P95_VALVE_LOOK=1 also Circuit and Study at 375, start and end (looked at, nothing asserted).
 *
 * Local only (SR_P95_VALVE_ON=1). The commander's command, from e2e/, on a stack with the sweep's world (API 8733,
 * Vite 5733):
 *   SR_E2E_BASE_URL=http://localhost:5733 SR_P95_VALVE_ON=1 SR_P95_VALVE_BASE=1 SR_P95_VALVE_LOOK=1 \
 *   SR_P95_VALVE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-valve \
 *   npx playwright test tests/p95-valve-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P95_VALVE_ON === '1';
const BASE = process.env.SR_P95_VALVE_BASE === '1';
const LOOK = process.env.SR_P95_VALVE_LOOK === '1';
const OUT =
  process.env.SR_P95_VALVE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p95-valve';
const THEME = 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p95v-${n}`,
  title: n === 1 ? 'Midnight Drive With The Windows Down (Extended Club Mix)' : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

type Win = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
};

async function setAccountTheme(req: Parameters<typeof apiPatch>[0], theme: string) {
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
  skin: string,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width, height: 812 },
    storageState: state,
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
  await page.evaluate((s) => (window as unknown as Win).__srPlayerActions.setSkin(s), skin);
  await expect(page.locator(rootSel(skin))).toBeVisible();
  await page.waitForTimeout(1500);
  // Paused: the pixel probes must not see a moving needle or reel.
  await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
  await page.waitForTimeout(600);
  return { ctx, page };
}

/** Switches my rule off in the live document (the base case): the media block that sets --v-frame-whole. */
async function stripChange(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      const id = (sheet.ownerNode as HTMLElement | null)?.getAttribute?.('data-vite-dev-id') ?? '';
      if (!id.includes('valve.css')) continue;
      for (let i = sheet.cssRules.length - 1; i >= 0; i--) {
        const r = sheet.cssRules[i]!;
        if (r instanceof CSSMediaRule && r.cssText.includes('--v-frame-whole')) {
          sheet.deleteRule(i);
          n++;
        }
      }
    }
    return n;
  });
}

function rootSel(skin: string) {
  return `.sr-ply[data-skin="${skin}"]`;
}

async function scrollTo(page: Page, skin: string, where: 'start' | 'mid' | 'end') {
  await page.evaluate(
    ([s, w]) => {
      const e = document.querySelector<HTMLElement>(s!)!;
      const max = e.scrollHeight - e.clientHeight;
      e.scrollTop = w === 'start' ? 0 : w === 'mid' ? Math.round(max / 2) : max;
    },
    [rootSel(skin), where],
  );
  await page.waitForTimeout(300);
}

/** Boxes of the frame pieces in the skin's content coordinates (scroll added back), and the scroll figures. */
async function frameBoxes(page: Page) {
  return page.evaluate(() => {
    const root = document.querySelector<HTMLElement>('.sr-ply[data-skin="valve"]')!;
    const o = root.getBoundingClientRect();
    const box = (e: Element | null) => {
      if (!e) return null;
      const r = e.getBoundingClientRect();
      return {
        top: Math.round(r.top - o.top + root.scrollTop),
        bottom: Math.round(r.bottom - o.top + root.scrollTop),
        h: Math.round(r.height),
      };
    };
    const cs = getComputedStyle(root, '::before');
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    return {
      clientHeight: root.clientHeight,
      scrollHeight: root.scrollHeight,
      bevel: { display: cs.display, height: cs.height, top: cs.top },
      cabinet: box(root.querySelector('svg.v-cabinet')),
      backdrop: box(root.querySelector('.sr-ply-bd-backdrop')),
      cheeks: Array.from(root.querySelectorAll('.v-cheek')).map(box),
      doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      dock: [dock.scrollWidth, dock.clientWidth, dock.scrollHeight, dock.clientHeight],
      skinW: [root.scrollWidth, root.clientWidth],
    };
  });
}

/**
 * The frame by pixels at the current scroll, decoded in the page from a screenshot of the skin's box: the bevel's dark
 * side column against the column 6 px further in (drop = lighter minus darker, 8 rows), and the cabinet's noise as the
 * standard deviation of a 4 x 16 patch of bare panel just inside the bevel, at 5 heights.
 */
async function framePixels(page: Page, skin: string, name: string | null) {
  const shot = await page
    .locator(rootSel(skin))
    .screenshot(name ? { path: resolve(OUT, name) } : {});
  const b64 = shot.toString('base64');
  return page.evaluate(
    async ([data, narrow]) => {
      const bmp = await createImageBitmap(
        await (await fetch(`data:image/png;base64,${data}`)).blob(),
      );
      const k = document.createElement('canvas');
      k.width = bmp.width;
      k.height = bmp.height;
      const g = k.getContext('2d')!;
      g.drawImage(bmp, 0, 0);
      const px = g.getImageData(0, 0, bmp.width, bmp.height).data;
      const w = bmp.width;
      const h = bmp.height;
      const lum = (x: number, y: number) => {
        const i = (w * y + x) * 4;
        return 0.2126 * px[i]! + 0.7152 * px[i + 1]! + 0.0722 * px[i + 2]!;
      };
      const rem = parseFloat(getComputedStyle(document.documentElement).fontSize);
      const inset = Math.round((narrow ? 0.875 : 2.25) * rem);
      const drops = Array.from({ length: 8 }, (_, i) => {
        const y = Math.round(((i + 0.5) / 8) * h);
        const dark = (lum(inset + 1, y) + lum(w - inset - 2, y)) / 2;
        const light = (lum(inset + (narrow ? 5 : 7), y) + lum(w - inset - (narrow ? 6 : 8), y)) / 2;
        return Math.round((light - dark) * 10) / 10;
      });
      const sd = (y0: number) => {
        const v: number[] = [];
        for (let y = y0; y < y0 + 16 && y < h; y++)
          for (let x = inset + 2; x < inset + 5; x++) v.push(lum(x, y));
        const m = v.reduce((a, c) => a + c, 0) / v.length;
        return Math.sqrt(v.reduce((a, c) => a + (c - m) ** 2, 0) / v.length);
      };
      const noise = [0.1, 0.3, 0.5, 0.7, 0.9].map(
        (f) => Math.round(sd(Math.round(f * (h - 16))) * 100) / 100,
      );
      return { w, h, inset, drops, minDrop: Math.min(...drops), noise };
    },
    [b64, (await page.viewportSize())!.width <= 640] as const,
  );
}

test('SEC-AUD-432: Valve frame whole at every scroll position; Circuit and Study looked at', async ({
  browser,
}) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  mkdirSync(OUT, { recursive: true });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();
  const report: Record<string, unknown> = { theme: back };
  const cases = BASE ? ['base', 'after'] : ['after'];
  for (const width of [375, 700]) {
    for (const label of cases) {
      const { ctx, page } = await openPage(browser, state, width, 'valve');
      const w: Record<string, unknown> = {};
      if (label === 'base') w.stripped = await stripChange(page);
      await page.waitForTimeout(400);
      w.frameTheme = await page
        .locator(rootSel('valve'))
        .evaluate((e) => e.closest('[data-theme]')?.getAttribute('data-theme'));
      for (const where of ['start', 'mid', 'end'] as const) {
        await scrollTo(page, 'valve', where);
        const boxes = await frameBoxes(page);
        const px = await framePixels(
          page,
          'valve',
          where === 'mid' ? null : `${label}-${width}-${where}.png`,
        );
        w[where] = { boxes, px };
        console.log(
          `REPORT ${width} ${label} ${where}: scroll ${boxes.clientHeight}/${boxes.scrollHeight} bevel ${JSON.stringify(boxes.bevel)} cabinet ${JSON.stringify(boxes.cabinet)} backdrop ${JSON.stringify(boxes.backdrop)} cheeks ${JSON.stringify(boxes.cheeks)} minDrop ${px.minDrop} drops ${px.drops.join(',')} noise ${px.noise.join(',')} dock ${boxes.dock.join('/')}`,
        );
        expect.soft(boxes.doc[0], `${width} ${label} ${where} document`).toBe(boxes.doc[1]);
        expect.soft(boxes.dock[0], `${width} ${label} ${where} dock`).toBe(boxes.dock[1]);
        expect.soft(boxes.skinW[0], `${width} ${label} ${where} skin`).toBe(boxes.skinW[1]);
        if (label === 'after') {
          expect.soft(px.minDrop, `${width} ${where} bevel`).toBeGreaterThan(width === 375 ? 1 : 2);
        }
      }
      if (label === 'after') {
        await scrollTo(page, 'valve', 'start');
        const root = page.locator(rootSel('valve'));
        w.opened = await openCollapsed(page, root);
        const rows = await hitTestAll(page, root, { wrapper: '.v-knob' });
        const pass = rows.filter((r) => r.status === 'pass').length;
        const nr = rows.filter((r) => r.status === 'not-reached').length;
        w.hit = { controls: rows.length, pass, notReached: nr };
        console.log(
          `REPORT ${width} hit test (wrapper .v-knob): ${pass} of ${rows.length}, not-reached ${nr}`,
        );
        expect.soft(nr).toBe(0);
        expect.soft(pass).toBe(rows.length);
      }
      report[`${label}-${width}`] = w;
      await ctx.close();
    }
  }
  if (LOOK) {
    for (const skin of ['circuit', 'study']) {
      const { ctx, page } = await openPage(browser, state, 375, skin);
      await scrollTo(page, skin, 'end');
      const m = await page
        .locator(rootSel(skin))
        .evaluate((e) => ({ scroll: [e.clientHeight, e.scrollHeight] }));
      report[`${skin}-375`] = m;
      console.log(`REPORT ${skin} 375: scroll ${m.scroll.join('/')}`);
      await page.locator(rootSel(skin)).screenshot({ path: resolve(OUT, `${skin}-375-end.png`) });
      await scrollTo(page, skin, 'start');
      await page.locator(rootSel(skin)).screenshot({ path: resolve(OUT, `${skin}-375-start.png`) });
      await ctx.close();
    }
  }
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
});
