import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-958 (wave 101, lounge): the Invite dialog's three selects and the Scrapbook gallery's three filter selects move to
 * the kit's `NativeSelect` (the Command form's two are not reached: see the handoff). A record of every select's box,
 * of every control's box in the dialog / the gallery, the exact hit test, base then after, plus the assertions the
 * comparison needs. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited). It creates no data and saves nothing (D-942): the Invite dialog is opened and closed
 * without pressing "Generate a link"; the keyboard cases change a draft value and put it back.
 *
 * Phases: `SR_P101_LOUNGE_PHASE=base` records the unedited files (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (block size not smaller than the base, at least 44 px at 375 touch, inline size not smaller, controls below moved by
 * no more than the summed height difference above them, nothing overlapping, hit test equal in count and status).
 * Configs: 1024 x 768 Lounge Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light. The account theme is set by
 * PATCH and localStorage and read back, `data-theme` is read from the page before each shot.
 *
 * Local only (SR_P101_LOUNGE_ON=1). SR_P101_LOUNGE_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8787, Vite 5787, metrics 9487,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5787 SR_P101_LOUNGE_ON=1 \
 *   SR_P101_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-lounge \
 *   npx playwright test tests/p101-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * After the merge there is no base to compare with: delete `figures-base.json` first if the folder holds one from
 * another tree.
 */
const ON = process.env.SR_P101_LOUNGE_ON === '1';
const PHASE = process.env.SR_P101_LOUNGE_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P101_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-lounge';
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
  shots: Record<string, string | undefined>;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: { invite: 'invite-1024-lounge-light', gallery: 'gallery-1024-lounge-light' },
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: { invite: 'invite-1024-lounge-dark', inviteFocus: 'invite-1024-focus-lounge-dark' },
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: { invite: 'invite-375-lounge-light', gallery: 'gallery-375-lounge-light' },
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
  lastControlReachable?: boolean;
  keyboard?: Record<string, unknown>;
}

/** Boxes in the root's coordinates are page coordinates plus every scrolled ancestor's offset (a scrolled page compares with a top one). */
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

async function openInvite(page: Page, c: Cfg) {
  const w = world();
  const guild = (await (await page.request.get(`/api/v1/guilds/${w.guildId}`)).json()) as {
    name: string;
  };
  await page.goto(c.touch ? `/channels/${w.guildId}` : `/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  const sidebar = page.getByTestId('channel-sidebar').first();
  await sidebar.getByRole('button', { name: guild.name }).first().click();
  await page.getByRole('menuitem', { name: 'Invite to Server' }).click();
  const dlg = page.getByRole('dialog', { name: /Invite people to/ });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('select').first().waitFor({ timeout: 15_000 });
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.waitForTimeout(800);
  return dlg;
}

async function inviteScenario(browser: Browser, owner: BrowserContext, c: Cfg, problems: string[]) {
  const tag = `invite ${c.tag}`;
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  try {
    const dlg = await openInvite(page, c);
    const ROOT = '[role="dialog"]';
    const m = await measure(page, ROOT);
    checkTheme(m, c, tag, problems);
    say(`${tag}: dialog ${JSON.stringify(m.root)} selects ${JSON.stringify(m.selects)}`);
    if (m.selects.length !== 3) problems.push(`${tag}: ${m.selects.length} selects, wanted 3`);
    const h = await hit(page, dlg, tag, problems);
    const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
    // Does the dialog's own scroller reach its last control?
    const last = dlg.locator('button, input, select').last();
    await last.scrollIntoViewIfNeeded();
    fig.lastControlReachable = await last.evaluate((e) => {
      const r = e.getBoundingClientRect();
      return r.bottom <= window.innerHeight + 1 && r.top >= -1;
    });
    if (!fig.lastControlReachable) problems.push(`${tag}: the last control is not reachable`);
    await dlg.evaluate((e) => {
      for (const el of Array.from(e.querySelectorAll('*'))) (el as HTMLElement).scrollTop = 0;
      (e as HTMLElement).scrollTop = 0;
    });
    await page.evaluate(() => window.scrollTo(0, 0));
    const name = c.shots['invite'];
    if (name) await shot(page, name);

    // Keyboard on the expiry select: focus, ArrowDown, the value changes; put it back. No invite is generated.
    const sel = dlg.locator('select').nth(1);
    await page.keyboard.press('Shift');
    await sel.focus();
    const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
    const ring = await sel.evaluate((e) => {
      const cs = getComputedStyle(e);
      return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} off ${cs.outlineOffset}`;
    });
    const before = await sel.inputValue();
    if (c.shots['inviteFocus']) await shot(page, c.shots['inviteFocus']);
    await page.keyboard.press('ArrowDown');
    const changed = await sel.inputValue();
    await page.keyboard.press('ArrowUp');
    const back = await sel.inputValue();
    fig.keyboard = { focusVisible, ring, before, changed, back };
    say(`${tag}: keyboard ${JSON.stringify(fig.keyboard)}`);
    if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
    if (changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
    if (back !== before) problems.push(`${tag}: ArrowUp did not restore the value`);
    await page.keyboard.press('Escape');
    return fig;
  } finally {
    await ctx.close();
  }
}

async function galleryScenario(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  problems: string[],
) {
  const tag = `gallery ${c.tag}`;
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  try {
    const w = world();
    await page.goto(`/channels/${w.guildId}/scrapbook`, {
      waitUntil: 'domcontentloaded',
      timeout: 20_000,
    });
    await page.locator('.lng-scr-filters select').first().waitFor({ timeout: 20_000 });
    await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
    await page.evaluate(() => document.fonts?.ready);
    await page.waitForTimeout(1200);
    const ROOT = 'main';
    const m = await measure(page, ROOT);
    checkTheme(m, c, tag, problems);
    say(`${tag}: selects ${JSON.stringify(m.selects)}`);
    const nf = await page.locator('.lng-scr-filters select').count();
    if (nf !== 3) problems.push(`${tag}: ${nf} filter selects, wanted 3`);
    const h = await hit(page, page.locator(ROOT).first(), tag, problems);
    const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.locator('.lng-scr-filters').scrollIntoViewIfNeeded();
    const name = c.shots['gallery'];
    if (name) await shot(page, name);

    if (c.tag === '1024-lounge-light') {
      // Keyboard on the Tag select: ArrowDown picks the first tag, the gallery reacts (its filter is in the URL or the
      // draft); put it back. With no tags in the world the select only holds "Any" and nothing can change.
      const sel = page.locator('.lng-scr-filters select').nth(1);
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
    }
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
    if (touch && a.box.h < 44 && !b.key.includes('Choose a shelf'))
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
      // A neighbour in the same row stretches with the select (the filter row's date fields).
      else if (b.box.y < bc.box.y + bc.box.h && bc.box.y < b.box.y + b.box.h)
        allowed = Math.max(allowed, (a.box.h - b.box.h) / 2 + 1);
    });
    // A centred dialog grows by the summed difference and re-centres: its top edge moves up by half of it.
    const rootDy = base.root && after.root ? after.root.y - base.root.y : 0;
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
    reason: 'p101 lounge shots',
  });
  await apiPatch(owner.request, `/guilds/${w.guildId}/sr/features/lounge.scrapbook`, {
    enabled: on,
  });
}

test(`p101 lounge: Invite dialog and Scrapbook gallery selects, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  await scrapbookSwitch(owner, true);
  try {
    for (const c of CONFIGS) {
      for (const [kind, fn] of [
        ['invite', inviteScenario],
        ['gallery', galleryScenario],
      ] as const) {
        if (kind === 'gallery' && !c.shots['gallery']) continue;
        try {
          figs[`${kind} ${c.tag}`] = await fn(browser, owner, c, problems);
        } catch (e) {
          problems.push(`${kind} ${c.tag}: ${String(e).slice(0, 500)}`);
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
