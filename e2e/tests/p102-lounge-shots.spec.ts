import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-967 / SEC-AUD-453 (wave 102, lounge): the selects of the Notification dialog, the Timeout and Ban dialogs, the
 * custom status dialog, Create Event, Create Poll and the Scrapbook's "Choose a shelf" move to the kit's
 * `NativeSelect`, and `.lng-field select` leaves lounge.css. A record of every select's box, every control's box in
 * the dialog / page, the exact hit test, base then after, plus the assertions the comparison needs. NEEDS THE SWEEP'S
 * SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited). It creates
 * no data and saves nothing (D-942): every dialog is opened and closed without its save button; the keyboard cases
 * change a draft value and put it back. The Scrapbook is switched on for the run and off in a `finally`.
 *
 * Phases: `SR_P102_LOUNGE_PHASE=base` records the unedited files (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (block size not smaller than the base, at least 44 px at 375 touch, inline size not smaller, controls below moved by
 * no more than the summed height difference above them, nothing overlapping, hit test not worse). Configs: 1024 x 768
 * Lounge Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light. The account theme is set by PATCH and
 * localStorage and read back; `data-theme` is read from the page before each shot. Dialogs other than Create Poll are
 * opened through the dev server's module graph (`/src/lounge/store.ts`, `openDialog`), as page-sweep.spec.ts does for
 * the voice store; the poll is opened from the composer's "Add to message" menu.
 *
 * Local only (SR_P102_LOUNGE_ON=1). SR_P102_LOUNGE_OUT is the folder of the shots and the figures.
 */
const ON = process.env.SR_P102_LOUNGE_ON === '1';
const PHASE = process.env.SR_P102_LOUNGE_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
  /** Scenario names that get a shot in this config. */
  shots: string[];
}
const ALL = ['notif', 'timeout', 'ban', 'status', 'event', 'poll', 'shelf'];
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: ALL,
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: [],
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: ALL,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as {
    guildId: string;
    channelId: string;
  };
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
  root: Box | null;
  selects: { key: string; box: Box }[];
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: unknown[];
  opened: unknown[];
  keyboard?: Record<string, unknown>;
}

/** Boxes in the root's coordinates are page coordinates plus every scrolled ancestor's offset. */
async function measure(page: Page, rootSel: string): Promise<Measured> {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel);
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
    const els = root
      ? Array.from(
          root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
        ).filter((e) => e.getClientRects().length > 0)
      : [];
    const name = (e: HTMLElement) =>
      (
        e.getAttribute('aria-label') ??
        e.closest('label')?.firstElementChild?.textContent ??
        e.textContent ??
        ''
      )
        .trim()
        .slice(0, 30);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}:${name(e)}`,
      box: box(e),
    }));
    const selEls = els.filter((e) => e.tagName === 'SELECT');
    const selects = selEls.map((e) => ({
      key: `${els.indexOf(e)}:select:${name(e)}`,
      box: box(e),
    }));
    const overlaps: string[] = [];
    for (const s of selEls) {
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `select(${name(s)}) x ${o.tagName.toLowerCase()}(${name(o)}) ${Math.round(ix)}x${Math.round(iy)}`,
          );
      }
    }
    return {
      theme: {
        html: document.documentElement.getAttribute('data-theme'),
        first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      },
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      root: root ? box(root) : null,
      selects,
      controls,
      overlaps,
    };
  }, rootSel);
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function hit(page: Page, root: Locator, tag: string, problems: string[]) {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const h = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: hit ${JSON.stringify(h)}`);
  if (h.notReached) problems.push(`${tag}: ${h.notReached} controls not reached (probe error)`);
  return { h, hitFails, opened };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

function checkTheme(m: Measured, c: Cfg, tag: string, problems: string[]) {
  say(`${tag}: data-theme html=${m.theme.html} first=${m.theme.first}`);
  if (m.theme.html !== c.theme && m.theme.first !== c.theme)
    problems.push(`${tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);
}

/** Opens a Lounge dialog through the dev server's module graph and waits for it. */
async function openViaStore(page: Page, dialog: Record<string, unknown>, title: RegExp) {
  const w = world();
  await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await page
    .getByTestId('channel-sidebar')
    .first()
    .waitFor({ timeout: 20_000 })
    .catch(() => undefined);
  await page.waitForTimeout(1200);
  await page.evaluate(async (d) => {
    const m = (await import(/* @vite-ignore */ String('/src/lounge/store.ts'))) as {
      useLoungeUi: { getState: () => { openDialog: (x: unknown) => void } };
    };
    m.useLoungeUi.getState().openDialog(d);
  }, dialog);
  const dlg = page.getByRole('dialog', { name: title });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('select').first().waitFor({ timeout: 15_000 });
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.waitForTimeout(600);
  return dlg;
}

interface Scenario {
  name: string;
  selects: number;
  open: (page: Page, ids: { memberId: string }) => Promise<Locator>;
  /** The select driven by the keyboard (index among the root's selects). */
  keyboardIdx: number;
  root: string;
}

const SCENARIOS: Scenario[] = [
  {
    name: 'notif',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: (page) =>
      openViaStore(
        page,
        { kind: 'notifications', guildId: world().guildId },
        /Notification Settings/,
      ),
  },
  {
    name: 'timeout',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: (page, ids) =>
      openViaStore(
        page,
        { kind: 'memberAction', action: 'timeout', guildId: world().guildId, userId: ids.memberId },
        /Timeout/,
      ),
  },
  {
    name: 'ban',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: (page, ids) =>
      openViaStore(
        page,
        { kind: 'memberAction', action: 'ban', guildId: world().guildId, userId: ids.memberId },
        /Ban /,
      ),
  },
  {
    name: 'status',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: (page) => openViaStore(page, { kind: 'customStatus' }, /Set a custom status/),
  },
  {
    name: 'event',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: (page) =>
      openViaStore(page, { kind: 'createEvent', guildId: world().guildId }, /Create Event/),
  },
  {
    name: 'poll',
    selects: 1,
    root: '[role="dialog"]',
    keyboardIdx: 0,
    open: async (page) => {
      const w = world();
      await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
        waitUntil: 'domcontentloaded',
        timeout: 20_000,
      });
      await page
        .getByRole('button', { name: 'Add to message' })
        .first()
        .waitFor({ timeout: 20_000 });
      await page.waitForTimeout(800);
      await page.getByRole('button', { name: 'Add to message' }).first().click();
      await page.getByRole('menuitem', { name: 'Create Poll' }).click();
      const dlg = page.getByRole('dialog', { name: 'Create Poll' });
      await dlg.waitFor({ timeout: 15_000 });
      await dlg.locator('select').first().waitFor({ timeout: 15_000 });
      await page.waitForTimeout(600);
      return dlg;
    },
  },
  {
    name: 'shelf',
    selects: 4,
    root: 'main',
    keyboardIdx: 3,
    open: async (page) => {
      const w = world();
      await page.goto(`/channels/${w.guildId}/scrapbook`, {
        waitUntil: 'domcontentloaded',
        timeout: 20_000,
      });
      await page.locator('.lng-scr-filters select').first().waitFor({ timeout: 20_000 });
      await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
      await page.evaluate(() => document.fonts?.ready);
      await page.waitForTimeout(1200);
      return page.locator('main').first();
    },
  },
];

async function scenario(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  s: Scenario,
  ids: { memberId: string },
  problems: string[],
) {
  const tag = `${s.name} ${c.tag}`;
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  try {
    const dlg = await s.open(page, ids);
    const m = await measure(page, s.root);
    checkTheme(m, c, tag, problems);
    say(`${tag}: root ${JSON.stringify(m.root)} selects ${JSON.stringify(m.selects)}`);
    if (m.selects.length !== s.selects)
      problems.push(`${tag}: ${m.selects.length} selects, wanted ${s.selects}`);
    const h = await hit(page, dlg, tag, problems);
    const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
    await page.evaluate(() => window.scrollTo(0, 0));
    if (s.name === 'shelf') await page.locator('.lng-scr-actions').first().scrollIntoViewIfNeeded();
    else
      await dlg.evaluate((e) => {
        for (const el of Array.from(e.querySelectorAll('*'))) (el as HTMLElement).scrollTop = 0;
        (e as HTMLElement).scrollTop = 0;
      });
    if (c.shots.includes(s.name)) await shot(page, `${s.name}-${c.tag}`);

    // Keyboard on one select: focus, ArrowDown changes the value (when it has more than one option), ArrowUp puts it back.
    const sel = dlg.locator('select').nth(s.keyboardIdx);
    const n = await sel.locator('option').count();
    await page.keyboard.press('Shift');
    await sel.focus();
    const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
    const before = await sel.inputValue();
    await page.keyboard.press('ArrowDown');
    const changed = await sel.inputValue();
    await page.keyboard.press('ArrowUp');
    const back = await sel.inputValue();
    fig.keyboard = { focusVisible, options: n, before, changed, back };
    say(`${tag}: keyboard ${JSON.stringify(fig.keyboard)}`);
    if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
    if (n > 1 && changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
    if (back !== before) problems.push(`${tag}: ArrowUp did not restore the value`);
    await page.keyboard.press('Escape');
    return fig;
  } finally {
    await ctx.close();
  }
}

function compare(label: string, base: Figures, after: Figures, touch: boolean, problems: string[]) {
  const diffs: Record<string, unknown> = {};
  if (base.selects.length !== after.selects.length) {
    problems.push(`${label}: select count ${base.selects.length} -> ${after.selects.length}`);
    return diffs;
  }
  base.selects.forEach((b, i) => {
    const a = after.selects[i]!;
    diffs[b.key] = { base: b.box, after: a.box };
    if (a.box.h < b.box.h - 0.5)
      problems.push(`${label}: ${b.key} shorter than base (${a.box.h} < ${b.box.h})`);
    if (touch && a.box.h < 44)
      problems.push(`${label}: ${b.key} is ${a.box.h} px tall at 375 touch`);
    if (a.box.w < b.box.w - 0.5)
      problems.push(`${label}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
  });
  if (base.root && after.root)
    diffs['root'] = { base: base.root, after: after.root, dh: after.root.h - base.root.h };
  if (base.controls.length !== after.controls.length) {
    problems.push(`${label}: control count ${base.controls.length} -> ${after.controls.length}`);
    return diffs;
  }
  const moved: { key: string; dx: number; dy: number; allowed: number }[] = [];
  base.controls.forEach((bc, i) => {
    const ac = after.controls[i]!;
    let allowed = 0;
    base.selects.forEach((b, j) => {
      const a = after.selects[j]!;
      if (b.box.y + b.box.h <= bc.box.y + 0.5) allowed += a.box.h - b.box.h;
      else if (b.box.y < bc.box.y + bc.box.h && bc.box.y < b.box.y + b.box.h)
        allowed = Math.max(allowed, (a.box.h - b.box.h) / 2 + 1);
    });
    // A centred dialog grows by the summed difference and re-centres: its top edge moves by half of it.
    const rootDy =
      base.root && after.root && label.split(' ')[0] !== 'shelf' ? after.root.y - base.root.y : 0;
    const dy = Math.round((ac.box.y - bc.box.y - rootDy) * 10) / 10;
    const dx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
    moved.push({ key: bc.key, dx, dy, allowed: Math.round(allowed * 10) / 10 });
    if (dy > allowed + 1) problems.push(`${label}: ${bc.key} moved ${dy} px, allowed ${allowed}`);
    if (dy < -1) problems.push(`${label}: ${bc.key} moved up ${dy} px`);
  });
  diffs['moved'] = moved.filter((x) => x.dx !== 0 || x.dy !== 0 || x.allowed !== 0);
  if (
    after.hit.fail > base.hit.fail ||
    after.hit.notReached > 0 ||
    after.hit.pass + after.hit.fail !== base.hit.pass + base.hit.fail
  )
    problems.push(`${label}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}`);
  return diffs;
}

/** The Scrapbook is operator-gated and per server: switch it on for the run and off again in a finally. */
async function scrapbookSwitch(owner: BrowserContext, on: boolean) {
  const w = world();
  await apiPatch(owner.request, '/admin/features/lounge.scrapbook', {
    enabled: on,
    reason: 'p102 lounge shots',
  });
  await apiPatch(owner.request, `/guilds/${w.guildId}/sr/features/lounge.scrapbook`, {
    enabled: on,
  });
}

test(`p102 lounge: six dialogs' and the shelf panel's selects, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  const w = world();
  const me = (await (await owner.request.get('/api/v1/users/@me')).json()) as { id: string };
  const ms = (await (
    await owner.request.get(`/api/v1/guilds/${w.guildId}/members?limit=20`)
  ).json()) as { user?: { id: string } }[];
  const memberId = (Array.isArray(ms) ? ms : [])
    .map((m) => m.user?.id)
    .find((id) => id && id !== me.id);
  if (!memberId) throw new Error('the world has no second member');
  await scrapbookSwitch(owner, true);
  try {
    for (const c of CONFIGS) {
      for (const s of SCENARIOS) {
        try {
          figs[`${s.name} ${c.tag}`] = await scenario(browser, owner, c, s, { memberId }, problems);
        } catch (e) {
          problems.push(`${s.name} ${c.tag}: ${String(e).slice(0, 500)}`);
        }
      }
    }
  } finally {
    await scrapbookSwitch(owner, false).catch(() => undefined);
  }
  await owner.close();
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const k of Object.keys(figs))
      if (base[k]) cmp[k] = compare(k, base[k]!, figs[k]!, k.includes('375'), problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
