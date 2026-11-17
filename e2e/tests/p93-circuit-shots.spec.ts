import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { CONTROL_SELECTOR, hitTestAll, openCollapsed } from '../support/hit-test';
import { measureFit, serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-419 (wave 93; D-926): the Circuit player skin passes the 44 px hit test (D-856) on a phone. Needs the
 * visual sweep's seeded world (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that spec
 * is never edited): the signed-in Owner at `/channels/@me`, the account's main theme set by PATCH and localStorage
 * and read back, 375 x 812 with touch, the player playing three tone tracks whose first title is 57 characters, the
 * skin chosen through the dev handle and read back (`.sr-ply[data-skin="circuit"]`).
 *
 * Cases: the base case (SR_P93_CIRCUIT_BASE=1) removes the sheet's `@media (max-width: 720px)` rules from the live
 * document (the tree itself is the change, so the base is the same tree with that block switched off) and measures;
 * the after case measures the tree as it is. Each case, at 375: the skin and theme read back; sideways scroll of the
 * document, the dock and the skin; the skin's box and the dock's scroll height; every control's box (in the skin's own
 * coordinates) and its gap to the nearest control above, below and beside; `openCollapsed`, then `hitTestAll` on the
 * skin's root (`e2e/support/hit-test.ts`, no private probe: a `not-reached` row is a spec error); running animations
 * with motion allowed and with `prefers-reduced-motion: reduce`; two shots (the skin at its start, and scrolled to its
 * end). At 1280 x 800 every control's box. The after case also taps (real touch) every kind of control at its centre,
 * 20 px above and 20 px below and checks the control did its job. With BASE=1 the after case's 1280 boxes are compared
 * with the base's (0 px).
 *
 * Local only (SR_P93_CIRCUIT_ON=1). SR_P93_CIRCUIT_OUT is the shots folder, SR_P93_CIRCUIT_BASE=1 adds the base case.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8716, Vite 5716):
 *   SR_E2E_BASE_URL=http://localhost:5716 SR_P93_CIRCUIT_ON=1 SR_P93_CIRCUIT_BASE=1 \
 *   SR_P93_CIRCUIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-circuit \
 *   npx playwright test tests/p93-circuit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P93_CIRCUIT_ON === '1';
const BASE = process.env.SR_P93_CIRCUIT_BASE === '1';
const OUT =
  process.env.SR_P93_CIRCUIT_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-circuit';
const THEME = 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const LONG_TITLE = 'Midnight Drive With The Windows Down (Extended Club Mix)';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p93c-${n}`,
  title: n === 1 ? LONG_TITLE : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const SKIN = '.sr-ply[data-skin="circuit"]';

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

type Win = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
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
  phone: boolean,
) {
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
  await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('circuit'));
  await expect(page.locator(SKIN)).toBeVisible();
  await page.waitForTimeout(1500);
  return { ctx, page };
}

/** Switches the phone block off in the live document (the base case). Returns how many rules went. */
async function stripPhoneBlock(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    for (const sheet of Array.from(document.styleSheets)) {
      const owner = sheet.ownerNode as HTMLElement | null;
      const id = owner?.getAttribute?.('data-vite-dev-id') ?? '';
      if (!id.includes('circuit.css')) continue;
      for (let i = sheet.cssRules.length - 1; i >= 0; i--) {
        const r = sheet.cssRules[i]!;
        if (r instanceof CSSMediaRule && /max-width:\s*720px/.test(r.conditionText)) {
          sheet.deleteRule(i);
          n++;
        }
      }
    }
    return n;
  });
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
    const skin = document.querySelector<HTMLElement>('.sr-ply[data-skin="circuit"]')!;
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
    const skin = document.querySelector<HTMLElement>('.sr-ply[data-skin="circuit"]')!;
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
    for (const s of ['.sr-ply-dock', '.sr-ply[data-skin="circuit"]'])
      document.querySelector<HTMLElement>(s)!.scrollTop = 0;
    window.scrollTo(0, 0);
  });
  await page.waitForTimeout(250);
}

async function toEnd(page: Page) {
  await page.evaluate(() => {
    for (const s of ['.sr-ply[data-skin="circuit"]', '.sr-ply-dock']) {
      const e = document.querySelector<HTMLElement>(s)!;
      e.scrollTop = e.scrollHeight;
    }
  });
  await page.waitForTimeout(250);
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
}

const attr = (page: Page, selector: string, name: string) =>
  page.locator(selector).first().getAttribute(name);

/** The after case's taps: every kind of control, at its centre, 20 px above and 20 px below. */
async function taps(page: Page): Promise<Record<string, unknown>> {
  const out: Record<string, unknown> = {};
  const DYS = [0, -20, 20];
  // Play and pause: three taps, each flips the button.
  const play = '.nc-tx-play';
  const seen: string[] = [];
  for (const dy of DYS) {
    const before = await attr(page, play, 'data-busy');
    await tap(page, play, dy);
    await expect.poll(() => attr(page, play, 'data-busy')).not.toBe(before);
    seen.push(`${dy}:${before}->${await attr(page, play, 'data-busy')}`);
  }
  out.playPause = seen;
  // Make sure it plays again for the rest.
  if ((await attr(page, play, 'data-busy')) !== 'true') await tap(page, play, 0);
  // Queue rows: each tap plays that row.
  const rows: string[] = [];
  for (const [i, dy] of [
    [1, 0],
    [2, -20],
    [0, 20],
  ] as const) {
    await tap(page, `.nc-rows li:nth-child(${i + 1}) button`, dy);
    await expect(page.locator(`.nc-rows li:nth-child(${i + 1}) button`)).toHaveAttribute(
      'aria-current',
      'true',
    );
    rows.push(`row ${i + 1} at ${dy}`);
  }
  out.rows = rows;
  // Seek: a tap at 80 percent, then at 10 percent, moves the value.
  const seek = '.nc-seek';
  const val = async (s: string) => Number(await page.locator(s).first().inputValue());
  const max = Number(await attr(page, seek, 'max'));
  const seekRes: number[] = [];
  for (const [fx, dy] of [
    [0.8, 0],
    [0.1, -20],
    [0.6, 20],
  ] as const) {
    await tap(page, seek, dy, fx);
    await page.waitForTimeout(250);
    seekRes.push(await val(seek));
  }
  out.seek = { max, values: seekRes };
  expect(seekRes[0]!).toBeGreaterThan(max * 0.6);
  expect(seekRes[1]!).toBeLessThan(max * 0.3);
  expect(seekRes[2]!).toBeGreaterThan(max * 0.4);
  // Volume fader: three taps at three heights give three different values.
  const vol = '.nc-fader-input[aria-label]';
  const vols: number[] = [];
  for (const dy of [-20, 0, 20]) {
    await tap(page, vol, dy);
    await page.waitForTimeout(150);
    vols.push(await val(vol));
  }
  out.volume = vols;
  expect(new Set(vols).size).toBe(3);
  // Toggles: Mute, EQ, Shuffle flip aria-pressed on each tap; Repeat cycles its label.
  for (const [name, sel] of [
    ['mute', '.nc-mute'],
    ['eq', '.nc-eq'],
    ['shuffle', '.nc-flags button:nth-of-type(2)'],
  ] as const) {
    const flips: string[] = [];
    for (const dy of [...DYS, 0]) {
      const before = await attr(page, sel, 'aria-pressed');
      await tap(page, sel, dy);
      await expect.poll(() => attr(page, sel, 'aria-pressed')).not.toBe(before);
      flips.push(`${dy}:${before}->${await attr(page, sel, 'aria-pressed')}`);
    }
    out[name] = flips;
  }
  const rep: string[] = [];
  for (const dy of DYS) {
    const before = await attr(page, '.nc-flag[data-repeat]', 'data-repeat');
    await tap(page, '.nc-flag[data-repeat]', dy);
    await expect.poll(() => attr(page, '.nc-flag[data-repeat]', 'data-repeat')).not.toBe(before);
    rep.push(`${dy}:${before}->${await attr(page, '.nc-flag[data-repeat]', 'data-repeat')}`);
  }
  out.repeat = rep;
  // Move: four taps go round the four docks and back.
  const dock = '.sr-ply-dock';
  const docks: string[] = [await attr(page, dock, 'data-dock').then((v) => String(v))];
  for (const dy of [...DYS, 0]) {
    await tap(page, '.nc-win .nc-winbtn:nth-child(2)', dy);
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
    await tap(page, '.nc-win .nc-winbtn:nth-child(1)', dy);
    await expect(page.locator(SKIN)).toHaveCount(0);
    skins.push(`${dy}:${await attr(page, '.sr-ply', 'data-skin')}`);
    await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('circuit'));
    await expect(page.locator(SKIN)).toBeVisible();
    await page.waitForTimeout(600);
  }
  out.skinButton = skins;
  // Close: each tap closes the player; it is opened again with a queue.
  const closes: string[] = [];
  for (const dy of DYS) {
    await tap(page, '.nc-winbtn-x', dy);
    await expect(page.locator('.sr-ply')).toHaveCount(0);
    closes.push(`${dy}: closed`);
    await page.evaluate((t) => (window as unknown as Win).__srPlayerActions.play(t), TRACKS);
    await expect(page.locator('.sr-ply')).toBeVisible();
    await page.evaluate(() => (window as unknown as Win).__srPlayerActions.setSkin('circuit'));
    await expect(page.locator(SKIN)).toBeVisible();
    await page.waitForTimeout(600);
  }
  out.close = closes;
  return out;
}

test('SEC-AUD-419: the Circuit skin on a phone, hit-tested', async ({ browser }) => {
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

    // ---- 375 x 812, touch
    const { ctx, page } = await openPage(browser, state, true);
    if (label === 'base') entry.rulesRemoved = await stripPhoneBlock(page);
    else
      entry.phoneBlockRules = await page.evaluate(() =>
        Array.from(document.styleSheets)
          .filter((s) =>
            (s.ownerNode as HTMLElement | null)
              ?.getAttribute?.('data-vite-dev-id')
              ?.includes('circuit.css'),
          )
          .reduce(
            (n, s) =>
              n +
              Array.from(s.cssRules).filter(
                (r) => r instanceof CSSMediaRule && /max-width:\s*720px/.test(r.conditionText),
              ).length,
            0,
          ),
      );
    await page.waitForTimeout(400);
    entry.identity = await page.evaluate(() => {
      const root = document.querySelector<HTMLElement>('.sr-ply')!;
      return {
        skin: root.getAttribute('data-skin'),
        theme: (root.closest('[data-theme]') as HTMLElement | null)?.getAttribute('data-theme'),
        motion: root.getAttribute('data-motion'),
      };
    });
    expect((entry.identity as { skin: string }).skin).toBe('circuit');

    // Motion: running animations with motion allowed, then with reduced motion.
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForTimeout(500);
    const allowed = await runningAnimations(page);
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForTimeout(700);
    const reduced = await runningAnimations(page);
    entry.motion = { allowed, reduced };
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.waitForTimeout(400);
    await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
    await page.waitForTimeout(400);

    await toStart(page);
    entry.scroll = await scrollState(page);
    entry.fit = await measureFit(page);
    const geo = await geometry(page);
    entry.geometry = geo.map((g) => ({ ...g, gap: gaps(geo, g) }));

    // The shared tool: open what is closed, then hit-test every control of the skin.
    const root = page.locator(SKIN);
    entry.opened = await openCollapsed(page, root);
    const rows = await hitTestAll(page, root);
    entry.controls = rows.map((r) => ({
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
          (p) =>
            `${p.at}: ${p.taker ? `${p.taker.tag} ${p.taker.name} [${p.taker.path}]` : 'null'}`,
        ),
    }));
    const counts = {
      controls: rows.length,
      pass: rows.filter((r) => r.status === 'pass').length,
      fail: rows.filter((r) => r.status === 'fail').length,
      notReached: rows.filter((r) => r.status === 'not-reached').length,
      skipped: rows.filter((r) => r.status === 'skipped').length,
    };
    entry.counts = counts;
    console.log(
      `REPORT 375 ${label}: ${JSON.stringify(counts)} skin ${JSON.stringify((entry.scroll as { skinBox: number[] }).skinBox)}`,
    );

    // Shots (the hit test scrolled things around).
    await toStart(page);
    await page.screenshot({ path: resolve(OUT, `${label}-375-start.png`) });
    await toEnd(page);
    await page.screenshot({ path: resolve(OUT, `${label}-375-end.png`) });
    await toStart(page);
    entry.scrollAfter = await scrollState(page);

    if (label === 'after') {
      expect(counts.fail, JSON.stringify(entry.controls)).toBe(0);
      expect(counts.notReached).toBe(0);
      for (const k of ['document', 'dock', 'skin'] as const) {
        const [sw, cw] = (entry.scroll as Record<string, number[]>)[k]!;
        expect(sw, `${k} scrollWidth`).toBe(cw);
      }
      expect(reduced).toBe(0);
      await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.play());
      await page.waitForTimeout(400);
      entry.taps = await taps(page);
    }
    await ctx.close();

    // ---- 1280 x 800
    const wide = await openPage(browser, state, false);
    await wide.page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
    await wide.page.waitForTimeout(500);
    const gw = await geometry(wide.page);
    entry.desktopBoxes = gw;
    entry.desktopScroll = await scrollState(wide.page);
    if (label === 'base') for (const g of gw) baseDesktop[g.name] = g;
    else {
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
        expect(diffs, 'a control box at 1280 differs from the base').toEqual([]);
      }
    }
    await wide.ctx.close();
  }
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ownerCtx.close();
});
