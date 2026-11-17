import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { serveTone } from '../support/playerAudio';

/**
 * SEC-AUD-422 (wave 93; D-927): a tall player skin scrolls once, not twice. Needs the visual sweep's seeded world
 * (`e2e/.state/sweep/`: `owner.json`, `world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; that
 * spec is never edited). The Owner, 375 x 812 with touch, the account's main theme set by PATCH and localStorage
 * and read back, a queue of three tone tracks, the five backdrop skins, the player in its bottom dock, in two
 * states: a Lounge channel open (the composer's clearance applies) and Settings open. Plus: Study in a top dock
 * (`setDock('top-end')`, the store call the move button makes), and Study at 1280 x 800.
 *
 * Base and after in ONE run: the "base" case puts the sheet's old cap back with an injected rule
 * (`.sr-ply-bd { max-block-size: calc(100vh - 1rem) !important }`), the "after" case is the tree as built. Per skin
 * and case: the dock's and the skin box's scrollHeight / clientHeight / computed max-block-size, `--sr-ply-lift` on
 * the dock and on the skin's box, the dock's distance above the viewport's bottom (D-789), and the third queue
 * row's box with every scroller above it moved to its end (and with the dock alone at its end), plus the skin's top
 * edge and first buttons at the start. Shots go to SR_P93_DOCK_OUT; `measure.json` beside them.
 *
 * Local only (SR_P93_DOCK_ON=1). SR_P93_DOCK_BASE=1 runs the base case only. SR_P93_DOCK_THEME is the account's
 * main theme (default `lounge-dark`). SR_P93_DOCK_ONLY is a comma list of skins.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8715, Vite 5715, metrics 9415,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; seeding needs DATABASE_URL and SR_CLI_BIN set):
 *   SR_E2E_BASE_URL=http://localhost:5715 SR_P93_DOCK_ON=1 \
 *   SR_P93_DOCK_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-dock \
 *   npx playwright test tests/p93-dock-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P93_DOCK_ON === '1';
const BASE_ONLY = process.env.SR_P93_DOCK_BASE === '1';
const OUT =
  process.env.SR_P93_DOCK_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-dock';
const THEME = process.env.SR_P93_DOCK_THEME ?? 'lounge-dark';
const ONLY = process.env.SR_P93_DOCK_ONLY
  ? process.env.SR_P93_DOCK_ONLY.split(',').map((s) => s.trim())
  : null;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SKINS = ['valve', 'tidepool', 'circuit', 'study', 'stack'] as const;
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p93-${n}`,
  title: n === 1 ? 'Midnight Drive With The Windows Down (Extended Club Mix)' : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

/** The old cap, put back for the base case. */
const OLD_CAP = '.sr-ply-bd { max-block-size: calc(100vh - 1rem) !important; }';

/** In the page: every figure of one skin in one case. `shot` is called between steps by the caller. */
function figures() {
  const dock = document.querySelector<HTMLElement>('.sr-ply-dock');
  const root = document.querySelector<HTMLElement>('.sr-ply');
  if (!dock || !root) return null;
  const px = (v: number) => Math.round(v * 10) / 10;
  const cs = (e: HTMLElement) => getComputedStyle(e);
  const dr = dock.getBoundingClientRect();
  const rr = root.getBoundingClientRect();
  const leaf = Array.from(root.querySelectorAll<HTMLElement>('*')).find(
    (e) => e.children.length === 0 && /^Tone 3/.test((e.textContent ?? '').trim()),
  );
  const rowBox = () => {
    if (!leaf) return null;
    const r = leaf.getBoundingClientRect();
    return [Math.round(r.top), Math.round(r.bottom)];
  };
  const out: Record<string, unknown> = {
    dataDock: dock.getAttribute('data-dock'),
    viewport: [document.documentElement.clientWidth, window.innerHeight],
    docScroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    composerClear: document.documentElement.style.getPropertyValue('--sr-composer-clear') || '0',
    dock: {
      scroll: [dock.scrollHeight, dock.clientHeight],
      maxBlock: cs(dock).maxBlockSize,
      lift: cs(dock).getPropertyValue('--sr-ply-lift').trim(),
      box: [Math.round(dr.left), Math.round(dr.top), Math.round(dr.width), Math.round(dr.height)],
      gapAboveViewportBottom: px(window.innerHeight - dr.bottom),
    },
    skin: {
      scroll: [root.scrollHeight, root.clientHeight],
      maxBlock: cs(root).maxBlockSize,
      overflowY: cs(root).overflowY,
      lift: cs(root).getPropertyValue('--sr-ply-lift').trim(),
      box: [Math.round(rr.left), Math.round(rr.top), Math.round(rr.width), Math.round(rr.height)],
      widthScroll: [root.scrollWidth, root.clientWidth],
    },
  };
  // Start: the skin's top edge and the first buttons.
  dock.scrollTop = 0;
  root.scrollTop = 0;
  const first = Array.from(root.querySelectorAll<HTMLElement>('button')).slice(0, 3);
  const rr0 = root.getBoundingClientRect();
  out.start = {
    skinTop: Math.round(rr0.top),
    skinTopWhole: rr0.top >= 0,
    firstButtons: first.map((b) => {
      const r = b.getBoundingClientRect();
      return {
        label: b.getAttribute('aria-label') ?? (b.textContent ?? '').trim().slice(0, 16),
        top: Math.round(r.top),
        bottom: Math.round(r.bottom),
        insideSkin: r.top >= rr0.top - 0.5 && r.bottom <= rr0.bottom + 0.5,
      };
    }),
  };
  // The dock alone at its end (the skin box left at its start).
  dock.scrollTop = dock.scrollHeight;
  const rd = dock.getBoundingClientRect();
  const alone = rowBox();
  out.dockAloneEnd = {
    row: alone,
    dockBox: [Math.round(rd.top), Math.round(rd.bottom)],
    inside: !!alone && alone[0]! >= rd.top - 0.5 && alone[1]! <= rd.bottom + 0.5,
  };
  // Every scroller above the row moved to its end, as a finger would.
  let moved = 0;
  const movedNames: string[] = [];
  for (let a: HTMLElement | null = leaf?.parentElement ?? null; a; a = a.parentElement) {
    if (a.scrollHeight > a.clientHeight + 2 && getComputedStyle(a).overflowY !== 'visible') {
      a.scrollTop = a.scrollHeight;
      moved++;
      movedNames.push(String(a.getAttribute('class') ?? a.tagName).split(/\s+/)[0]!);
    }
  }
  const rr1 = root.getBoundingClientRect();
  const row = rowBox();
  out.end = {
    row,
    skinBox: [Math.round(rr1.top), Math.round(rr1.bottom)],
    scrollersMoved: moved,
    movedNames,
    insideSkin: !!row && row[0]! >= rr1.top - 0.5 && row[1]! <= rr1.bottom + 0.5,
    insideViewport: !!row && row[0]! >= 0 && row[1]! <= window.innerHeight,
  };
  return out;
}

const resetScroll = (page: Page) =>
  page.evaluate(() => {
    for (const e of Array.from(document.querySelectorAll<HTMLElement>('.sr-ply-dock, .sr-ply')))
      e.scrollTop = 0;
  });

/** Leave the dock and the skin both at their ends for the "scrolled to the end" shot. */
const scrollEnds = (page: Page) =>
  page.evaluate(() => {
    const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
    const root = document.querySelector<HTMLElement>('.sr-ply')!;
    dock.scrollTop = dock.scrollHeight;
    root.scrollTop = root.scrollHeight;
  });

async function setAccountTheme(req: Parameters<typeof apiPatch>[0], theme: string) {
  await apiPatch(req, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
  const back = (await (await req.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  return back.settings?.user_prefs?.theme;
}

test('SEC-AUD-422: a tall skin scrolls once', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const world = JSON.parse(readFileSync(resolve(STATE, 'world.json'), 'utf8')) as {
    guildId: string;
    channelId: string;
  };
  const ownerCtx = await browser.newContext({
    baseURL: ORIGIN,
    storageState: resolve(STATE, 'owner.json'),
  });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  console.log(`REPORT account theme set ${THEME}, read back: ${back}`);
  expect(back).toBe(THEME);
  const storage = await ownerCtx.storageState();

  const report: Record<string, unknown> = { theme: THEME };

  const newPage = async (width: number, height: number, phone: boolean) => {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width, height },
      storageState: storage,
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
    return { ctx, page };
  };
  const start = async (page: Page, path: string) => {
    await page.goto(path);
    await page.waitForFunction(() => document.readyState === 'complete');
    await page.mouse.click(2, 2);
    await page.waitForFunction(
      () => (window as unknown as { __srPlayerActions?: unknown }).__srPlayerActions,
    );
    await page.evaluate((tracks) => {
      (
        window as unknown as { __srPlayerActions: { play(t: unknown): void } }
      ).__srPlayerActions.play(tracks);
    }, TRACKS);
    await page.waitForSelector('.sr-ply-dock');
    // Paused for a quiet look.
    await page
      .evaluate(() =>
        (
          window as unknown as { __srPlayer?: { audio: HTMLAudioElement } }
        ).__srPlayer?.audio.pause(),
      )
      .catch(() => undefined);
  };
  const setSkin = async (page: Page, skin: string) => {
    await page.evaluate(
      (s) =>
        (
          window as unknown as { __srPlayerActions: { setSkin(k: string): void } }
        ).__srPlayerActions.setSkin(s),
      skin,
    );
    await expect(page.locator(`.sr-ply[data-skin="${skin}"]`)).toBeVisible();
    await page.waitForTimeout(1200); // the skin's lazy chunk and fonts
  };
  const setDock = (page: Page, dock: string) =>
    page.evaluate(
      (d) =>
        (
          window as unknown as { __srPlayerActions: { setDock(k: string): void } }
        ).__srPlayerActions.setDock(d),
      dock,
    );
  const themeOf = (page: Page) =>
    page.evaluate(
      () =>
        (
          document.querySelector('.sr-ply')?.closest('[data-theme]') as HTMLElement | null
        )?.getAttribute('data-theme') ?? null,
    );

  const cases: Array<'base' | 'after'> = BASE_ONLY ? ['base'] : ['base', 'after'];
  const measure = async (
    page: Page,
    tag: string,
    skin: string,
    c: 'base' | 'after',
  ): Promise<Record<string, any>> => {
    await resetScroll(page);
    const f = await page.evaluate(figures);
    (f as Record<string, unknown>).themeOfPlayer = await themeOf(page);
    return { tag, skin, case: c, ...f };
  };

  // ---- 375 x 812 touch: a Lounge channel, then Settings; five skins; base and after ----
  const phone = await newPage(375, 812, true);
  const rows: unknown[] = [];
  report.phone = rows;
  for (const state of ['channel', 'settings'] as const) {
    const path =
      state === 'channel'
        ? `/channels/${world.guildId}/${world.channelId}`
        : '/settings/user/account';
    await start(phone.page, path);
    // Wait for the composer's clearance in the channel state.
    if (state === 'channel')
      await phone.page
        .waitForFunction(
          () => !!document.documentElement.style.getPropertyValue('--sr-composer-clear'),
          undefined,
          { timeout: 8000 },
        )
        .catch(() => console.log('REPORT no --sr-composer-clear in the channel state'));
    for (const skin of SKINS) {
      if (ONLY && !ONLY.includes(skin)) continue;
      await setSkin(phone.page, skin);
      for (const c of cases) {
        const style = c === 'base' ? await phone.page.addStyleTag({ content: OLD_CAP }) : null;
        await phone.page.waitForTimeout(250);
        const m = await measure(phone.page, state, skin, c);
        rows.push(m);
        const wantShot = state === 'channel' && ['study', 'valve', 'circuit'].includes(skin);
        if (wantShot) {
          await scrollEnds(phone.page);
          await phone.page.waitForTimeout(250);
          if (skin === 'study' || c === 'after')
            await phone.page.screenshot({ path: resolve(OUT, `${skin}-end-375-${c}.png`) });
          if (skin === 'study' && c === 'after') {
            await resetScroll(phone.page);
            await phone.page.waitForTimeout(250);
            await phone.page.screenshot({ path: resolve(OUT, `study-start-375-after.png`) });
          }
        }
        if (style) await style.evaluate((el) => (el as Element).remove());
        console.log(
          `REPORT ${state} ${skin} ${c}: dock ${JSON.stringify(m.dock.scroll)} skin ${JSON.stringify(m.skin.scroll)}`,
        );
      }
    }
    if (state === 'channel' && !ONLY) {
      // One top dock, Study.
      await setSkin(phone.page, 'study');
      await setDock(phone.page, 'top-end');
      await phone.page.waitForTimeout(400);
      for (const c of cases) {
        const style = c === 'base' ? await phone.page.addStyleTag({ content: OLD_CAP }) : null;
        await phone.page.waitForTimeout(250);
        rows.push(await measure(phone.page, 'channel-top-dock', 'study', c));
        if (style) await style.evaluate((el) => (el as Element).remove());
      }
      await setDock(phone.page, 'bottom-end');
    }
  }
  await phone.ctx.close();

  // ---- 1280 x 800: Study, a channel ----
  if (!ONLY || ONLY.includes('study')) {
    const wide = await newPage(1280, 800, false);
    const wideRows: unknown[] = [];
    report.wide = wideRows;
    await start(wide.page, `/channels/${world.guildId}/${world.channelId}`);
    await setSkin(wide.page, 'study');
    for (const c of cases) {
      const style = c === 'base' ? await wide.page.addStyleTag({ content: OLD_CAP }) : null;
      await wide.page.waitForTimeout(250);
      wideRows.push(await measure(wide.page, 'channel-1280', 'study', c));
      if (c === 'after') {
        await resetScroll(wide.page);
        await wide.page.screenshot({ path: resolve(OUT, 'study-1280-after.png') });
      }
      if (style) await style.evaluate((el) => (el as Element).remove());
    }
    await wide.ctx.close();
  }

  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(report, null, 2));
  await ownerCtx.close();
});
