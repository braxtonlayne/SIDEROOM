import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-950 (wave 100, select): the kit's `NativeSelect` in Server settings > Overview. A record of the four selects'
 * boxes (and of every control's box in the section) with the exact hit test, base then after, plus the assertions the
 * comparison needs. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited). It creates no named data (D-942) and saves nothing: the keyboard case changes the
 * inactive-timeout draft and puts it back before the page is left (the section's unsaved-changes bar is read).
 *
 * Phases: `SR_P100_SELECT_PHASE=base` records the unedited tree (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (block size not smaller than the base, at least 44 px at 375 touch, inline size not smaller, controls below moved by
 * no more than the summed height difference above them, nothing overlapping, hit test equal in count and status).
 * Configs: 1024 x 768 Lounge Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light. The account theme is set by
 * PATCH and localStorage and read back, `data-theme` is read from the page before each shot.
 *
 * Local only (SR_P100_SELECT_ON=1). SR_P100_SELECT_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8782, Vite 5782, metrics 9482,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5782 SR_P100_SELECT_ON=1 \
 *   SR_P100_SELECT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-select \
 *   npx playwright test tests/p100-select-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P100_SELECT_ON === '1';
const PHASE = process.env.SR_P100_SELECT_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P100_SELECT_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-select';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const IDS = ['ss-system-channel', 'ss-afk-channel', 'ss-afk-timeout', 'ss-widget-channel'];

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
  shot: string;
  focusShot?: string;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shot: 'overview-1024-lounge-light',
    focusShot: 'overview-1024-focus-lounge-light',
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shot: 'overview-1024-lounge-dark',
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shot: 'overview-375-lounge-light',
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function newPage(browser: Browser, ownerCtx: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  }, c.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  theme: { html: string | null; first: string | null };
  scroll: number[];
  selects: Record<string, Box | null>;
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: unknown[];
  opened: unknown[];
  keyboard?: Record<string, unknown>;
}

/** Boxes in section coordinates: every scrolled ancestor's offset added, so a scrolled page compares with a top one. */
async function measure(page: Page): Promise<Measured> {
  return page.evaluate((ids) => {
    const main = document.querySelector('main')!;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      let sx = 0;
      let sy = 0;
      for (let p: Element | null = e.parentElement; p; p = p.parentElement) {
        sx += p.scrollLeft;
        sy += p.scrollTop;
      }
      return {
        x: Math.round((r.left + sx) * 10) / 10,
        y: Math.round((r.top + sy) * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const selects: Record<string, ReturnType<typeof box> | null> = {};
    for (const id of ids) {
      const e = document.getElementById(id);
      selects[id] = e ? box(e) : null;
    }
    const els = Array.from(
      main.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 30)}`,
      box: box(e),
    }));
    const overlaps: string[] = [];
    for (const id of ids) {
      const s = document.getElementById(id);
      if (!s) continue;
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `${id} x ${o.tagName.toLowerCase()}#${o.id} (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
    }
    return {
      theme: {
        html: document.documentElement.getAttribute('data-theme'),
        first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      },
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      overlaps,
    };
  }, IDS);
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function shotSelects(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.evaluate(() =>
    document.getElementById('ss-system-channel')?.scrollIntoView({ block: 'start' }),
  );
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function runConfig(browser: Browser, owner: BrowserContext, c: Cfg, problems: string[]) {
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  await page.goto(`/settings/server/${world().guildId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await page.waitForSelector('#ss-system-channel', { timeout: 20_000 });
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready);
  await page.waitForTimeout(1500);

  const main = page.locator('main').first();
  // Boxes first, with the page as it loaded (a hit test scrolls).
  const m = await measure(page);
  say(`${c.tag}: data-theme html=${m.theme.html} first=${m.theme.first}`);
  if (m.theme.html !== c.theme && m.theme.first !== c.theme)
    problems.push(`${c.tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${c.tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  say(`${c.tag}: select boxes ${JSON.stringify(m.selects)}`);
  if (m.overlaps.length) problems.push(`${c.tag}: overlaps ${m.overlaps.join('; ')}`);

  const opened = await openCollapsed(page, main);
  const rows = await hitTestAll(page, main);
  const hit = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${c.tag}: hit ${JSON.stringify(hit)}`);
  if (hit.notReached)
    problems.push(`${c.tag}: ${hit.notReached} controls not reached (probe error)`);
  const fig: Figures = { ...m, hit, hitFails, opened };

  // The ring: focus by keyboard so :focus-visible matches, then read it back.
  await page.locator('#ss-afk-timeout').scrollIntoViewIfNeeded();
  await page.keyboard.press('Shift');
  await page.locator('#ss-afk-timeout').focus();
  const focusVisible = await page.evaluate(() =>
    document.getElementById('ss-afk-timeout')!.matches(':focus-visible'),
  );
  const ring = await page.evaluate(() => {
    const cs = getComputedStyle(document.getElementById('ss-afk-timeout')!);
    return {
      outline: `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor}`,
      offset: cs.outlineOffset,
    };
  });
  fig.keyboard = { focusVisible, ring };
  if (!focusVisible) problems.push(`${c.tag}: :focus-visible did not match after a key`);
  await page.locator('#ss-afk-timeout').blur();
  await shotSelects(page, c.shot);
  if (c.focusShot) {
    await page.evaluate(() =>
      document.getElementById('ss-afk-timeout')?.scrollIntoView({ block: 'center' }),
    );
    await page.keyboard.press('Shift');
    await page.locator('#ss-afk-timeout').focus();
    await page.waitForTimeout(400);
    await page.screenshot({
      path: resolve(OUT, `${shotName(c.focusShot)}.png`),
      timeout: 10_000,
    });
  }

  // Keyboard: ArrowDown changes the value, the unsaved-changes bar appears; put it back (nothing is saved).
  if (c.tag === '1024-lounge-light') {
    const sel = page.locator('#ss-afk-timeout');
    await sel.focus();
    const before = await sel.inputValue();
    await page.keyboard.press('ArrowDown');
    const changed = await sel.inputValue();
    const bar = await page
      .getByText('Careful, you have unsaved changes!')
      .isVisible()
      .catch(() => false);
    await page.keyboard.press('ArrowUp');
    const back = await sel.inputValue();
    fig.keyboard = { ...fig.keyboard, before, changed, back, unsavedBarShown: bar };
    say(`${c.tag}: keyboard ${JSON.stringify(fig.keyboard)}`);
    if (changed === before) problems.push(`${c.tag}: ArrowDown did not change the value`);
    if (!bar) problems.push(`${c.tag}: the unsaved-changes bar did not show after ArrowDown`);
    if (back !== before) problems.push(`${c.tag}: ArrowUp did not restore the value`);
  }
  await ctx.close();
  return fig;
}

function compare(c: Cfg, base: Figures, after: Figures, problems: string[]) {
  const diffs: Record<string, unknown> = {};
  for (const id of IDS) {
    const b = base.selects[id];
    const a = after.selects[id];
    if (!b || !a) {
      if (!!b !== !!a) problems.push(`${c.tag}: ${id} present in only one phase`);
      continue;
    }
    diffs[id] = { base: b, after: a };
    if (a.h < b.h - 0.5) problems.push(`${c.tag}: ${id} shorter than base (${a.h} < ${b.h})`);
    if (c.touch && a.h < 44) problems.push(`${c.tag}: ${id} is ${a.h} px tall at 375 touch`);
    if (a.w < b.w - 0.5) problems.push(`${c.tag}: ${id} narrower than base (${a.w} < ${b.w})`);
  }
  if (base.controls.length !== after.controls.length) {
    problems.push(`${c.tag}: control count ${base.controls.length} -> ${after.controls.length}`);
    return diffs;
  }
  const moved: { key: string; moved: number; allowed: number }[] = [];
  base.controls.forEach((bc, i) => {
    const ac = after.controls[i]!;
    let allowed = 0;
    for (const id of IDS) {
      const b = base.selects[id];
      const a = after.selects[id];
      if (b && a && b.y + b.h <= bc.box.y + 0.5) allowed += a.h - b.h;
    }
    const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
    moved.push({ key: bc.key, moved: mv, allowed: Math.round(allowed * 10) / 10 });
    if (mv > allowed + 1) problems.push(`${c.tag}: ${bc.key} moved ${mv} px, allowed ${allowed}`);
    if (mv < -1) problems.push(`${c.tag}: ${bc.key} moved up ${mv} px`);
  });
  diffs['moved'] = moved.filter((x) => x.moved !== 0 || x.allowed !== 0);
  if (JSON.stringify(base.hit) !== JSON.stringify(after.hit))
    problems.push(`${c.tag}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}`);
  return diffs;
}

test(`p100 select: Overview's four selects, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  for (const c of CONFIGS) {
    try {
      figs[c.tag] = await runConfig(browser, owner, c, problems);
    } catch (e) {
      problems.push(`${c.tag}: ${String(e).slice(0, 500)}`);
    }
  }
  await owner.close();
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const c of CONFIGS)
      if (base[c.tag] && figs[c.tag]) cmp[c.tag] = compare(c, base[c.tag]!, figs[c.tag]!, problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
