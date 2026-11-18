import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { hitTest, hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * D-951 (wave 100, editor). Server settings > Channels: on a phone (720 px and under) picking a channel brings the
 * editor's heading into view and focus, and "Back to channels" returns to the picked row. Local only.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json` and `owner.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; that spec is never edited), as `p85-guild-shots.spec.ts` does. The spec makes channels named
 * `p100-editor-<n>` through the API until the server has eight channels (D-942: no other spec uses the prefix).
 *
 * The command the commander runs (from `e2e/`, on a fresh database, stack ports 8783 / 5783 / 9483, the sweep world seeded
 * first with `SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium
 * --workers=1 --retries=0 --reporter=line`; the stack needs `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_P100_EDITOR_ON=1 SR_E2E_BASE_URL=http://localhost:5783 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P100_EDITOR_OUT=<folder> npx playwright test tests/p100-editor-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 * Optional: SR_P100_EDITOR_TAG (`base` or `after`, default `after`) prefixes the files. `base` is run on a tree without
 * the change; when the other tag's `boxes-*.json` and `hit-*.json` are in the folder, the spec compares them (moved boxes).
 *
 * What it does (one JSON line per measure in `measure.json`): the editor heading's top against the viewport height, the
 * scroller's `scrollTop` and `document.activeElement` after a pick of the first and of the last row (375 touch, Lounge
 * Light; the last also in Lounge Dark for the shot); at 1024x768 the same with every control box of the section; after
 * only: "Back to channels" (height, hit test, activation), reduced motion, and the keyboard at 375 without touch.
 */
const ON = process.env.SR_P100_EDITOR_ON === '1';
const OUT =
  process.env.SR_P100_EDITOR_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-editor';
const TAG = process.env.SR_P100_EDITOR_TAG === 'base' ? 'base' : 'after';
const OTHER = TAG === 'base' ? 'after' : 'base';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const ROW = '.ss-tree-row button.ss-link';
const BACK = '.ss-editor-back';

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

interface World {
  guildId: string;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Snap {
  theme: string | null;
  headingTop: number | null;
  headingText: string | null;
  viewport: number;
  inView: boolean;
  scrollTop: number;
  active: string;
  activeIsHeading: boolean;
  activeIsPicked: boolean;
  activeInEditor: boolean;
  pickedTop: number | null;
  pickedInView: boolean;
  backShown: boolean;
  docScrollW: number;
  docClientW: number;
}

/** In the page: where the editor heading is, what scrolls, what has focus. */
function snapPage(): Snap {
  const h = document.querySelector<HTMLElement>('.ss-split h2.ss-title');
  const picked = document.querySelector<HTMLElement>('.ss-tree-row [aria-current="true"]');
  const ae = document.activeElement as HTMLElement | null;
  let sc: HTMLElement | null = h ? h.parentElement : null;
  while (
    sc &&
    !(sc.scrollHeight > sc.clientHeight + 1 && /(auto|scroll)/.test(getComputedStyle(sc).overflowY))
  )
    sc = sc.parentElement;
  const scrollTop = sc ? sc.scrollTop : (document.scrollingElement?.scrollTop ?? 0);
  const vh = window.innerHeight;
  const top = h ? Math.round(h.getBoundingClientRect().top) : null;
  const pt = picked ? Math.round(picked.getBoundingClientRect().top) : null;
  const desc = ae
    ? `${ae.tagName.toLowerCase()}${ae.className && typeof ae.className === 'string' ? '.' + ae.className.split(/\s+/)[0] : ''}:${(ae.textContent ?? '').trim().slice(0, 30)}`
    : 'none';
  const back = document.querySelector<HTMLElement>('.ss-editor-back');
  return {
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    headingTop: top,
    headingText: h ? (h.textContent ?? '').trim() : null,
    viewport: vh,
    inView: top !== null && top >= 0 && top < vh,
    scrollTop: Math.round(scrollTop),
    active: desc,
    activeIsHeading: !!h && ae === h,
    activeIsPicked: !!picked && ae === picked,
    activeInEditor: !!ae && !!ae.closest('.ss-split section'),
    pickedTop: pt,
    pickedInView: pt !== null && pt >= 0 && pt < vh,
    backShown: !!back && getComputedStyle(back).display !== 'none' && back.offsetHeight > 0,
    docScrollW: document.documentElement.scrollWidth,
    docClientW: document.documentElement.clientWidth,
  };
}

interface Box {
  key: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
/** In the page: every control box of the section, keyed by order and name. */
function boxesPage(): Box[] {
  const sel = 'button,a[href],input:not([type=hidden]),textarea,select,[role=tab],[role=switch]';
  return Array.from(
    document.querySelectorAll<HTMLElement>(`.ss-split ${sel.split(',').join(', .ss-split ')}`),
  )
    .map((el, i) => {
      const r = el.getBoundingClientRect();
      const name = (el.getAttribute('aria-label') ?? el.textContent ?? '').trim().slice(0, 24);
      return { key: `${i}:${name}`, x: r.x, y: r.y + window.scrollY, w: r.width, h: r.height };
    })
    .filter((b) => b.w > 0 && b.h > 0);
}

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

interface Cfg {
  width: number;
  height: number;
  touch: boolean;
  reduced?: boolean;
}
async function newPage(browser: Browser, cfg: Cfg, theme: string, owner: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: cfg.width, height: cfg.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    reducedMotion: cfg.reduced ? 'reduce' : 'no-preference',
    ...(cfg.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  }, theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

test('D-951: the channel editor on a phone', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const failed: string[] = [];
  let shots = 0;
  const log = (id: string, data: unknown) => {
    appendFileSync(jsonl, `${JSON.stringify({ id, tag: TAG, data })}\n`);
    console.log(`REPORT ${TAG} ${id} ${JSON.stringify(data).slice(0, 1500)}`);
  };
  const check = (what: string, ok: boolean) => {
    log(`check ${ok ? 'PASS' : 'FAIL'} ${what}`, ok);
    if (!ok && TAG === 'after') failed.push(what);
  };
  const setTheme = async (theme: string) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
  };
  const shoot = async (page: Page, name: string) => {
    await page.screenshot({ path: resolve(OUT, `${TAG}-${name}.png`), timeout: 10_000 });
    shots++;
  };

  // At least eight channels (names of our own).
  const list = (await (await o.get(`/api/v1/guilds/${world.guildId}/channels`)).json()) as {
    id: string;
  }[];
  for (let n = list.length; n < 8; n++) {
    await apiPost(o, `/guilds/${world.guildId}/channels`, { name: `p100-editor-${n}`, type: 0 });
  }

  const openChannels = async (page: Page) => {
    await page.goto(`/settings/server/${world.guildId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 4000);
    const labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    const i = labels.findIndex((l) => /^Channels$/i.test(l));
    if (i < 0) throw new Error(`no nav item Channels: ${labels.join(' | ')}`);
    await page
      .locator(NAV)
      .nth(i)
      .evaluate((el) => (el as HTMLElement).click());
    await page.locator(ROW).first().waitFor();
    await idle(page, 2500);
  };
  const pick = async (page: Page, which: 'first' | 'last', touch: boolean) => {
    const row = which === 'first' ? page.locator(ROW).first() : page.locator(ROW).last();
    await row.scrollIntoViewIfNeeded();
    if (touch) await row.tap();
    else await row.click();
    await page.waitForTimeout(1600); // smooth scroll
  };
  const guard = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn();
    } catch (e) {
      failed.push(`${what}: ${String(e).slice(0, 200)}`);
      console.log(`REPORT FAILED ${what}: ${String(e).slice(0, 300)}`);
    }
  };

  // ---- 375 touch, Lounge Light: first then last; Dark
  for (const theme of ['lounge-light', 'lounge-dark']) {
    await setTheme(theme);
    const { ctx, page } = await newPage(
      browser,
      { width: 375, height: 812, touch: true },
      theme,
      ownerCtx,
    );
    await guard(`375 ${theme}`, async () => {
      await openChannels(page);
      const before = await page.evaluate(snapPage);
      log(`375 ${theme} before pick`, before);
      check(`${theme} data-theme read back`, before.theme === theme);
      await pick(page, 'first', true);
      const first = await page.evaluate(snapPage);
      log(`375 ${theme} pick first`, first);
      if (TAG === 'after') {
        check(`375 ${theme} first: heading in view`, first.inView);
        check(`375 ${theme} first: heading focused`, first.activeIsHeading);
      }
      if (theme === 'lounge-light') {
        await shoot(page, 'channels-375-picked-lounge-light');
        await pick(page, 'last', true);
        const last = await page.evaluate(snapPage);
        log(`375 ${theme} pick last`, last);
        if (TAG === 'after') {
          check(`375 last: heading in view`, last.inView);
          check(`375 last: heading focused`, last.activeIsHeading);
        }
        check(`375 ${theme} no sideways page scroll`, last.docScrollW <= last.docClientW);
        await shoot(page, 'channels-375-picked-last-lounge-light');

        if (TAG === 'after') {
          const back = page.locator(BACK);
          check('375 back button is shown', last.backShown);
          const box = await back.boundingBox();
          log('375 back box', box);
          check('375 back button at least 44 px tall', (box?.height ?? 0) >= 44);
          const hit = await hitTest(page, back, { sides: true });
          check('375 back button passes the hit test', hit.pass);
          await shoot(page, 'channels-375-back-lounge-light');
          // Whole-section hit test (the base's list plus the button).
          await page.locator(ROW).first().scrollIntoViewIfNeeded();
          await openCollapsed(page, page.locator('.ss-split'));
          const rows = await hitTestAll(page, page.locator('.ss-split'));
          writeFileSync(
            resolve(OUT, `hit-375-${TAG}.json`),
            JSON.stringify(rows.map((r) => ({ name: r.name, path: r.path, status: r.status }))),
          );
          log('375 hit test statuses', {
            total: rows.length,
            pass: rows.filter((r) => r.status === 'pass').length,
            fail: rows.filter((r) => r.status === 'fail').map((r) => `${r.name} ${r.reason ?? ''}`),
            notReached: rows.filter((r) => r.status === 'not-reached').length,
          });
          check(
            '375 hit test: no fail, no not-reached',
            rows.every((r) => r.status === 'pass' || r.status === 'skipped'),
          );
          // Activate Back.
          await back.scrollIntoViewIfNeeded();
          await back.tap();
          await page.waitForTimeout(1600);
          const after = await page.evaluate(snapPage);
          log('375 after Back', after);
          check('375 back: picked row in view', after.pickedInView);
          check('375 back: picked row focused', after.activeIsPicked);
        } else {
          await openCollapsed(page, page.locator('.ss-split'));
          const rows = await hitTestAll(page, page.locator('.ss-split'));
          writeFileSync(
            resolve(OUT, `hit-375-${TAG}.json`),
            JSON.stringify(rows.map((r) => ({ name: r.name, path: r.path, status: r.status }))),
          );
          log('375 hit test base', {
            total: rows.length,
            pass: rows.filter((r) => r.status === 'pass').length,
            fail: rows.filter((r) => r.status === 'fail').map((r) => r.name),
          });
        }
      } else {
        await shoot(page, 'channels-375-picked-lounge-dark');
      }
    });
    await ctx.close();
  }

  // ---- 1024: the same three, and every control box
  await setTheme('lounge-light');
  {
    const { ctx, page } = await newPage(
      browser,
      { width: 1024, height: 768, touch: false },
      'lounge-light',
      ownerCtx,
    );
    await guard('1024', async () => {
      await openChannels(page);
      const emptyBoxes = await page.evaluate(boxesPage);
      const s0 = await page.evaluate(snapPage);
      log('1024 before pick', s0);
      await pick(page, 'first', false);
      const s1 = await page.evaluate(snapPage);
      log('1024 pick first', s1);
      const boxes = await page.evaluate(boxesPage);
      check('1024 back button not displayed', !s1.backShown);
      check('1024 focus stays on the row button', s1.activeIsPicked);
      check('1024 scrollTop unchanged by a pick', s1.scrollTop === s0.scrollTop);
      writeFileSync(resolve(OUT, `boxes-1024-${TAG}.json`), JSON.stringify({ emptyBoxes, boxes }));
      const otherFile = resolve(OUT, `boxes-1024-${OTHER}.json`);
      if (existsSync(otherFile)) {
        const other = JSON.parse(readFileSync(otherFile, 'utf8')) as {
          emptyBoxes: Box[];
          boxes: Box[];
        };
        const moved = (a: Box[], b: Box[]) => {
          const m = new Map(b.map((x) => [x.key, x]));
          let n = 0;
          for (const x of a) {
            const y = m.get(x.key);
            if (
              !y ||
              Math.abs(x.x - y.x) > 0.5 ||
              Math.abs(x.y - y.y) > 0.5 ||
              Math.abs(x.w - y.w) > 0.5 ||
              Math.abs(x.h - y.h) > 0.5
            )
              n++;
          }
          return { n, of: a.length, otherLen: b.length };
        };
        const r = { empty: moved(emptyBoxes, other.emptyBoxes), picked: moved(boxes, other.boxes) };
        log('1024 boxes moved against ' + OTHER, r);
        check(
          '1024 control boxes equal to the base (0 moved)',
          r.empty.n === 0 && r.picked.n === 0,
        );
      }
      await pick(page, 'last', false);
      log('1024 pick last', await page.evaluate(snapPage));
      await shoot(page, 'channels-1024-picked-lounge-light');
    });
    await ctx.close();
  }

  if (TAG === 'after') {
    await setTheme('lounge-light');
    // ---- reduced motion at 375
    {
      const { ctx, page } = await newPage(
        browser,
        { width: 375, height: 812, touch: true, reduced: true },
        'lounge-light',
        ownerCtx,
      );
      await guard('375 reduced motion', async () => {
        await openChannels(page);
        const rm = await page.evaluate(
          () => matchMedia('(prefers-reduced-motion: reduce)').matches,
        );
        check('reduced motion is emulated', rm);
        await pick(page, 'first', true);
        const a = await page.evaluate(snapPage);
        log('375 reduced pick first', a);
        check('375 reduced: first heading in view and focused', a.inView && a.activeIsHeading);
        await pick(page, 'last', true);
        const b = await page.evaluate(snapPage);
        log('375 reduced pick last', b);
        check('375 reduced: last heading in view and focused', b.inView && b.activeIsHeading);
        await page.locator(BACK).scrollIntoViewIfNeeded();
        await page.locator(BACK).tap();
        await page.waitForTimeout(500);
        const c = await page.evaluate(snapPage);
        log('375 reduced Back', c);
        check('375 reduced back: row in view and focused', c.pickedInView && c.activeIsPicked);
      });
      await ctx.close();
    }
    // ---- keyboard at 375, no touch
    {
      const { ctx, page } = await newPage(
        browser,
        { width: 375, height: 812, touch: false },
        'lounge-light',
        ownerCtx,
      );
      await guard('375 keyboard', async () => {
        await openChannels(page);
        await page.locator(ROW).nth(2).focus();
        await page.keyboard.press('Enter');
        await page.waitForTimeout(1600);
        const a = await page.evaluate(snapPage);
        log('375 kbd Enter on a row', a);
        check('375 kbd: Enter moves focus to the heading', a.activeIsHeading && a.inView);
        await page.keyboard.press('Tab');
        const b = await page.evaluate(snapPage);
        log('375 kbd Tab', b);
        check('375 kbd: Tab goes on into the editor', !b.activeIsHeading && b.activeInEditor);
        await page.locator(BACK).focus();
        await page.keyboard.press('Enter');
        await page.waitForTimeout(1600);
        const c = await page.evaluate(snapPage);
        log('375 kbd Back', c);
        check('375 kbd: Back returns to the row', c.activeIsPicked && c.pickedInView);
      });
      await ctx.close();
    }
  }

  console.log(`REPORT ${TAG} shots=${shots} failed=${JSON.stringify(failed)}`);
  expectNoFailures(failed);
});

function expectNoFailures(failed: string[]) {
  if (TAG === 'after' && failed.length) throw new Error(`failed: ${failed.join(' || ')}`);
}
