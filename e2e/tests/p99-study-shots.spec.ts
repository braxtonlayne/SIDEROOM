import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { serveTone } from '../support/playerAudio';

/**
 * D-947 (wave 99, SEC-AUD-439): Study's dots and flecks as the root's own `local` background layers at 720 px and
 * under. A record plus the assertions the record needs to be trusted. NEEDS THE SWEEP'S SEEDED WORLD
 * (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited). It creates no named
 * data (D-942): the Owner at `/channels/@me`, the account theme `lounge-dark` set by PATCH and localStorage and read
 * back, three tone tracks, the skin chosen through the dev handle and read back.
 *
 * At 700 x 812 and 375 x 812 (touch): the page as built ("on"), then the block switched off by an injected style that
 * restores the base's two-layer background and the two spans ("off"). Per state: root clientHeight / scrollHeight,
 * the spans' boxes, every control's box (off against on: 0 moved), a shot of the root at scroll start and at scroll
 * end, and from the shots the number of pixels that differ in the lowest 80 px strip at the end and in the first
 * screen at the start (count and largest channel step; animations stopped by an injected style in both states). At 700 `openCollapsed` then `hitTestAll` in both states.
 * At 1024 x 768 (no touch): the root's computed `background-image` and the spans' `display` against the 700 "off"
 * state (the block does not apply there).
 *
 * Local only (SR_P99_STUDY_ON=1). SR_P99_STUDY_OUT is the folder of the shots and `study.json`.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8771, Vite 5771, metrics 9471,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5771 SR_P99_STUDY_ON=1 \
 *   SR_P99_STUDY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p99-study \
 *   npx playwright test tests/p99-study-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P99_STUDY_ON === '1';
const OUT =
  process.env.SR_P99_STUDY_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p99-study';
const THEME = 'lounge-dark';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const TRACKS = [1, 2, 3].map((n) => ({
  id: `p99s-${n}`,
  title: n === 1 ? 'Midnight Drive With The Windows Down (Extended Club Mix)' : `Tone ${n}`,
  artist: n === 1 ? 'Neon Coast Collective' : 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));
const SKIN = 'study';
const ROOT = `.sr-ply[data-skin="${SKIN}"]`;
/** The base's rules for the root and the spans, forced over the block. */
const OFF_CSS = `
.sr-ply-study {
  background:
    linear-gradient(var(--st-cream-hi), var(--st-cream)) padding-box,
    linear-gradient(var(--st-pink-hi), var(--st-pink) 30%, var(--st-pink-lo)) border-box !important;
}
.sr-ply-study .st-dots, .sr-ply-study .st-grain { display: block !important; }`;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Win = {
  __srPlayerActions: { setSkin(k: string): void; play(t: unknown): void };
  __srPlayer: { audio: HTMLAudioElement };
};
type Ctx = Awaited<ReturnType<import('@playwright/test').Browser['newContext']>>;

async function setAccountTheme(req: Parameters<typeof apiPatch>[0], theme: string) {
  await apiPatch(req, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
  const back = (await (await req.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  return back.settings?.user_prefs?.theme;
}

async function openPage(
  browser: import('@playwright/test').Browser,
  state: Awaited<ReturnType<Ctx['storageState']>>,
  width: number,
  height: number,
  touch: boolean,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width, height },
    storageState: state,
    bypassCSP: true,
    ...(touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  await page.evaluate((s) => (window as unknown as Win).__srPlayerActions.setSkin(s), SKIN);
  await expect(page.locator(ROOT)).toBeVisible();
  await page.waitForTimeout(1500);
  await page.evaluate(() => (window as unknown as Win).__srPlayer.audio.pause());
  await page.waitForTimeout(600);
  return { ctx, page };
}

async function scrollTo(page: Page, where: 'start' | 'end') {
  const r = await page.evaluate(
    ([s, w]) => {
      const e = document.querySelector<HTMLElement>(s!)!;
      const max = e.scrollHeight - e.clientHeight;
      e.scrollTop = w === 'start' ? 0 : max;
      return { scrollTop: Math.round(e.scrollTop), max };
    },
    [ROOT, where],
  );
  await page.waitForTimeout(300);
  return r;
}

/** Scroll figures, the spans' boxes and display, the root's computed background, every control's box (content coordinates). */
async function figures(page: Page) {
  return page.evaluate((sel) => {
    const root = document.querySelector<HTMLElement>(sel)!;
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
    const cs = getComputedStyle(root);
    const dots = root.querySelector('.st-dots');
    const grain = root.querySelector('.st-grain');
    const controls = Array.from(
      root.querySelectorAll('button, a[href], input, select, textarea, [role="button"]'),
    ).map((e) => {
      const r = e.getBoundingClientRect();
      return [
        Math.round((r.left - o.left) * 10) / 10,
        Math.round((r.top - o.top + root.scrollTop) * 10) / 10,
        Math.round(r.width * 10) / 10,
        Math.round(r.height * 10) / 10,
      ];
    });
    return {
      clientHeight: root.clientHeight,
      scrollHeight: root.scrollHeight,
      backgroundImage: cs.backgroundImage,
      dots: dots ? { display: getComputedStyle(dots).display, box: box(dots) } : null,
      grain: grain ? { display: getComputedStyle(grain).display, box: box(grain) } : null,
      controls,
      doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      root: [root.scrollWidth, root.clientWidth],
    };
  }, ROOT);
}

/** Pixels that differ between two PNGs of the same size (optionally only the lowest rows), and the largest channel step. */
async function diffPng(page: Page, a: Buffer, b: Buffer, fromBottom?: number) {
  return page.evaluate(
    async ([x, y, fb]) => {
      const load = async (s: string) => {
        const bin = Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
        const bmp = await createImageBitmap(new Blob([bin], { type: 'image/png' }));
        const c = new OffscreenCanvas(bmp.width, bmp.height);
        const g = c.getContext('2d')!;
        g.drawImage(bmp, 0, 0);
        return g.getImageData(0, 0, bmp.width, bmp.height);
      };
      const A = await load(x as string);
      const B = await load(y as string);
      if (A.width !== B.width || A.height !== B.height) return { error: 'size differs' };
      const y0 = fb ? Math.max(0, A.height - (fb as number)) : 0;
      let n = 0;
      let max = 0;
      for (let row = y0; row < A.height; row++)
        for (let col = 0; col < A.width; col++) {
          const i = (row * A.width + col) * 4;
          let d = 0;
          for (let k = 0; k < 3; k++) d = Math.max(d, Math.abs(A.data[i + k]! - B.data[i + k]!));
          if (d > 0) n++;
          if (d > max) max = d;
        }
      return { width: A.width, height: A.height, rows: [y0, A.height], differ: n, max };
    },
    [a.toString('base64'), b.toString('base64'), fromBottom ?? 0] as const,
  );
}

async function hit(page: Page) {
  const root = page.locator(ROOT);
  await scrollTo(page, 'start');
  await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const by = (st: string) => rows.filter((r) => r.status === st);
  return {
    controls: rows.length,
    pass: by('pass').length,
    fail: by('fail').map((r) => ({
      name: r.name,
      takers: r.result?.points.filter((p) => !p.ok).map((p) => ({ at: p.at, taker: p.taker })),
    })),
    notReached: by('not-reached').map((r) => ({ name: r.name, why: r.reason })),
    skipped: by('skipped').length,
  };
}

test('D-947: Study texture at 700 and 375, on against off; 1024 unchanged', async ({ browser }) => {
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  mkdirSync(OUT, { recursive: true });
  expect(await setAccountTheme(ownerCtx.request, THEME)).toBe(THEME);
  const state = await ownerCtx.storageState();
  const report: Record<string, unknown> = { theme: THEME };
  let off700: Awaited<ReturnType<typeof figures>> | null = null;

  for (const width of [700, 375]) {
    const { ctx, page } = await openPage(browser, state, width, 812, true);
    const root = page.locator(ROOT);
    expect(await root.getAttribute('data-skin')).toBe(SKIN);
    const r: Record<string, unknown> = {};
    // Animations stopped in both states: a pixel that differs is the texture, not a turning record.
    await page.addStyleTag({
      content:
        '*, *::before, *::after { animation: none !important; transition: none !important; }',
    });
    await page.waitForTimeout(300);
    // ON (as built)
    const onStart = await figures(page);
    const sOn = await scrollTo(page, 'start');
    expect(sOn.scrollTop).toBe(0);
    const shotOnStart = await root.screenshot({ path: resolve(OUT, `study-${width}-start.png`) });
    const eOn = await scrollTo(page, 'end');
    expect(eOn.scrollTop).toBe(eOn.max);
    const onEnd = await figures(page);
    const shotOnEnd = await root.screenshot({ path: resolve(OUT, `study-${width}-end.png`) });
    expect(onEnd.dots?.display, 'the block switches the span off').toBe('none');
    expect(onEnd.grain?.display).toBe('none');
    const hitOn = width === 700 ? await hit(page) : null;
    // OFF (the base)
    await page.addStyleTag({ content: OFF_CSS });
    await page.waitForTimeout(300);
    await scrollTo(page, 'start');
    const offStart = await figures(page);
    const shotOffStart = await root.screenshot({
      path: resolve(OUT, `study-${width}-start-before.png`),
    });
    await scrollTo(page, 'end');
    const offEnd = await figures(page);
    const shotOffEnd = await root.screenshot({
      path: resolve(OUT, `study-${width}-end-before.png`),
    });
    expect(offEnd.dots?.display, 'the injected style restores the span').toBe('block');
    const hitOff = width === 700 ? await hit(page) : null;
    if (width === 700) off700 = offStart;

    const moved = onStart.controls.filter(
      (c, i) => JSON.stringify(c) !== JSON.stringify(offStart.controls[i]),
    ).length;
    r.heights = {
      on: [onStart.clientHeight, onStart.scrollHeight],
      off: [offStart.clientHeight, offStart.scrollHeight],
    };
    r.spansOff = { dots: offEnd.dots, grain: offEnd.grain };
    r.controls = {
      on: onStart.controls.length,
      off: offStart.controls.length,
      moved,
      movedAtEnd: onEnd.controls.filter(
        (c, i) => JSON.stringify(c) !== JSON.stringify(offEnd.controls[i]),
      ).length,
    };
    r.stripEnd80 = await diffPng(page, shotOffEnd, shotOnEnd, 80);
    r.wholeEnd = await diffPng(page, shotOffEnd, shotOnEnd);
    r.firstScreenStart = await diffPng(page, shotOffStart, shotOnStart);
    r.width = { on: [onStart.root, onStart.doc], off: [offStart.root, offStart.doc] };
    r.hit = { on: hitOn, off: hitOff };
    report[String(width)] = r;
    console.log(`REPORT ${width}: ${JSON.stringify(r)}`);
    expect(moved, `${width}: no control moved`).toBe(0);
    expect(onStart.scrollHeight).toBe(offStart.scrollHeight);
    expect(onStart.clientHeight).toBe(offStart.clientHeight);
    if (hitOn) expect(hitOn.notReached).toEqual([]);
    if (hitOff) expect(hitOff.notReached).toEqual([]);
    await ctx.close();
  }

  const wide = await openPage(browser, state, 1024, 768, false);
  const f1024 = await figures(wide.page);
  report['1024'] = { backgroundImage: f1024.backgroundImage, dots: f1024.dots, grain: f1024.grain };
  console.log(`REPORT 1024: dots ${f1024.dots?.display} grain ${f1024.grain?.display}`);
  expect(f1024.backgroundImage).toBe(off700!.backgroundImage);
  expect(f1024.dots?.display).toBe('block');
  expect(f1024.grain?.display).toBe('block');
  await wide.ctx.close();
  writeFileSync(resolve(OUT, 'study.json'), JSON.stringify(report, null, 2));
});
