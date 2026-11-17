import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';

/**
 * D-946 (wave 98): Circuit and Study are shot and hit-tested at 700 x 812 with touch, as wave 95 did for Valve. A
 * record, not a gate: the spec asserts only what must hold for the record to be trusted (the skin is the one named,
 * the hit test reached every control, the shots were written). A fail at 700 is a finding written to `frames.json`;
 * the spec still passes. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; that spec is never edited). It creates no named data (D-942): the Owner at `/channels/@me`, the
 * account's main theme `lounge-dark` set by PATCH and localStorage and read back (the theme both wave 93 specs use),
 * three tone tracks, the skin chosen through the dev handle and read back.
 *
 * Per skin: the root (`.sr-ply[data-skin]`) scrolled to its top and to its end (`scrollTop` read back against the
 * maximum), a shot of the root at each; the frame layers (root `::before`/`::after`, `.sr-ply-bd-backdrop`, and the
 * skin's own art: Circuit `.nc-body`, `.nc-shell`, `.nc-shell-plate`; Study `.st-dots`, `.st-grain`, `.st-body`) as
 * boxes in the root's content coordinates against the root's `scrollHeight` and `clientHeight` (does a layer end
 * before the content does); `openCollapsed` then `hitTestAll` over the root (controls, pass, fail with box and taker,
 * not-reached); `scrollWidth` against `clientWidth` of the root, the dock and the document.
 *
 * Local only (SR_P98_FRAMES_ON=1). SR_P98_FRAMES_OUT is the folder of the four shots and `frames.json`.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8761, Vite 5761, metrics 9461,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5761 SR_P98_FRAMES_ON=1 \
 *   SR_P98_FRAMES_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p98-frames \
 *   npx playwright test tests/p98-frames-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P98_FRAMES_ON === '1';
const OUT =
  process.env.SR_P98_FRAMES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p98-frames';
const THEME = 'lounge-dark';
const WIDTH = 700;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p98f-${n}`,
  title: n === 1 ? 'Midnight Drive With The Windows Down (Extended Club Mix)' : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const FRAME_LAYERS: Record<string, Record<string, string>> = {
  circuit: {
    backdrop: '.sr-ply-bd-backdrop',
    body: '.nc-body',
    shell: '.nc-shell',
    plate: '.nc-shell-plate',
  },
  study: {
    backdrop: '.sr-ply-bd-backdrop',
    dots: '.st-dots',
    grain: '.st-grain',
    body: '.st-body',
  },
};

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Win = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
};

const rootSel = (skin: string) => `.sr-ply[data-skin="${skin}"]`;

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
  skin: string,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: WIDTH, height: 812 },
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
  await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
  await page.waitForTimeout(600);
  return { ctx, page };
}

async function scrollTo(page: Page, skin: string, where: 'start' | 'end') {
  const r = await page.evaluate(
    ([s, w]) => {
      const e = document.querySelector<HTMLElement>(s!)!;
      const max = e.scrollHeight - e.clientHeight;
      e.scrollTop = w === 'start' ? 0 : max;
      return { scrollTop: Math.round(e.scrollTop), max };
    },
    [rootSel(skin), where],
  );
  await page.waitForTimeout(300);
  return r;
}

/** The frame layers' boxes in the root's content coordinates (scroll added back) and the scroll figures. */
async function frameFigures(page: Page, skin: string) {
  return page.evaluate(
    ([sel, layers]) => {
      const root = document.querySelector<HTMLElement>(sel as string)!;
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
      const pseudo = (p: '::before' | '::after') => {
        const cs = getComputedStyle(root, p);
        return cs.content === 'none' || cs.content === 'normal'
          ? null
          : { display: cs.display, position: cs.position, height: cs.height, top: cs.top };
      };
      const dock = document.querySelector<HTMLElement>('.sr-ply-dock')!;
      const cs = getComputedStyle(root);
      const found: Record<string, ReturnType<typeof box>> = {};
      for (const [name, q] of Object.entries(layers as Record<string, string>))
        found[name] = box(root.querySelector(q));
      return {
        clientHeight: root.clientHeight,
        scrollHeight: root.scrollHeight,
        rootBg: { attachment: cs.backgroundAttachment, clip: cs.backgroundClip },
        before: pseudo('::before'),
        after: pseudo('::after'),
        layers: found,
        doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
        dock: [dock.scrollWidth, dock.clientWidth],
        root: [root.scrollWidth, root.clientWidth],
        theme: root.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
      };
    },
    [rootSel(skin), FRAME_LAYERS[skin]!] as const,
  );
}

test('D-946: Circuit and Study at 700 x 812, touch: shots, frame figures, hit test', async ({
  browser,
}) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  mkdirSync(OUT, { recursive: true });
  const back = await setAccountTheme(ownerCtx.request, THEME);
  expect(back).toBe(THEME);
  const state = await ownerCtx.storageState();
  const report: Record<string, unknown> = { width: WIDTH, theme: back };
  for (const skin of ['circuit', 'study']) {
    const { ctx, page } = await openPage(browser, state, skin);
    const root = page.locator(rootSel(skin));
    const readBack = await root.getAttribute('data-skin');
    expect(readBack).toBe(skin);
    const s: Record<string, unknown> = {};
    for (const where of ['start', 'end'] as const) {
      const sc = await scrollTo(page, skin, where);
      expect(sc.scrollTop, `${skin} ${where} scrollTop read back`).toBe(
        where === 'start' ? 0 : sc.max,
      );
      const fig = await frameFigures(page, skin);
      s[where] = { scroll: sc, ...fig };
      const file = `${skin}-${WIDTH}-${where}.png`;
      await root.screenshot({ path: resolve(OUT, file) });
      console.log(
        `REPORT ${skin} ${where}: scroll ${fig.clientHeight}/${fig.scrollHeight} top ${sc.scrollTop}/${sc.max} before ${JSON.stringify(fig.before)} after ${JSON.stringify(fig.after)} layers ${JSON.stringify(fig.layers)} width root ${fig.root.join('/')} dock ${fig.dock.join('/')} doc ${fig.doc.join('/')} theme ${fig.theme}`,
      );
    }
    await scrollTo(page, skin, 'start');
    s.opened = await openCollapsed(page, root);
    const rows = await hitTestAll(page, root);
    const by = (st: string) => rows.filter((r) => r.status === st);
    const fails = by('fail').map((r) => ({
      name: r.name,
      path: r.path,
      box: r.result?.box,
      takers: r.result?.points.filter((p) => !p.ok).map((p) => ({ at: p.at, taker: p.taker })),
    }));
    const notReached = by('not-reached').map((r) => ({
      name: r.name,
      path: r.path,
      why: r.reason,
    }));
    s.hit = {
      controls: rows.length,
      pass: by('pass').length,
      fail: fails,
      notReached,
      skipped: by('skipped').length,
    };
    console.log(
      `REPORT ${skin} hit test: pass ${by('pass').length} of ${rows.length}, fail ${fails.length}, not-reached ${notReached.length}, skipped ${by('skipped').length}`,
    );
    for (const f of fails) console.log(`REPORT ${skin} FAIL ${JSON.stringify(f)}`);
    report[skin] = s;
    expect(notReached, `${skin}: a not-reached row is a spec error`).toEqual([]);
    await ctx.close();
  }
  writeFileSync(resolve(OUT, 'frames.json'), JSON.stringify(report, null, 2));
});
