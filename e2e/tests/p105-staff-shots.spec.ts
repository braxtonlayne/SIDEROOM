import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 105, pod staff (D-1002 donate, D-1011 bridge): the Admin > Donations toolbars and the bridge's two forms.
 *   invoices      D-1002  State and Purpose are the kit's NativeSelect; the toolbar's rows, label and control tops, the
 *                         search field, the checkbox row and the table must not move (1280, 1024 light, 1024 dark, 375).
 *   entitlements  D-1002  the Kind filter is the kit's select (fit content); height 40 (44 at 375), at most 36 px wider.
 *   grant         D-1002  SEC-AUD-497: the grant dialog's Kind label reads like the other labels (1024 light).
 *   others        D-1002  Admin > Audit log, Bans, Storage (1024) and the Conflict log (1024, 375): nothing moves.
 *   bridgemap     D-1011  "Mirror another channel": three selects read 14 px, 400, none, normal (1024 light and dark, 375).
 *   plan          D-1011  the plan's "Link two by hand" fieldset: the same for its two selects (1024, 375).
 * The bridge (status, features, discord channels, conflicts, preflight, plan) is answered by `page.route` fakes, the same in
 * both phases; nothing is sent to a server. The spec makes no data and keeps nothing: the Admin pages are read as the
 * seeded world has them (no invoice is made: if the table has no row, the table figures say so). Elements are found by
 * role, label or tag inside a root found that way; the table by its role. It never imports a /src module.
 *
 * Phases: SR_P105_STAFF_PHASE=base records the base bundle (shots get a `base-` prefix, `figures-base.json`); `after`
 * records the tree and, when `figures-base.json` is in the folder, compares it with the base: every broken "must not
 * change" or "may change" limit of the donate and bridge tasks is a problem line with its figures.
 *
 * Needs the sweep's seeded world (`e2e/.state/sweep/`): the Owner's state and `world.json`. Local only.
 * The commander's command:
 *   bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_STAFF_ON=1 --env SR_P105_STAFF_PHASE=after --env SR_P105_STAFF_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-staff -- tests/p105-staff-shots.spec.ts
 */
const ON = process.env.SR_P105_STAFF_ON === '1';
const PHASE = process.env.SR_P105_STAFF_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_STAFF_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-staff';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
}
const L1280: Cfg = {
  tag: '1280-light',
  width: 1280,
  height: 800,
  touch: false,
  theme: 'lounge-light',
};
const L1024: Cfg = {
  tag: '1024-light',
  width: 1024,
  height: 768,
  touch: false,
  theme: 'lounge-light',
};
const D1024: Cfg = {
  tag: '1024-dark',
  width: 1024,
  height: 768,
  touch: false,
  theme: 'lounge-dark',
};
const L375: Cfg = { tag: '375-light', width: 375, height: 812, touch: true, theme: 'lounge-light' };

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Sty {
  fontSize: string;
  fontWeight: string;
  textTransform: string;
  letterSpacing: string;
  color: string;
}
interface Ctl {
  key: string;
  tag: string;
  box: Box;
  wrap: Box | null;
  label: Box | null;
  text: { box: Box; sty: Sty } | null;
  sty: Sty;
  gapY: number | null;
  gapX: number | null;
  clear: boolean | null;
  value: string;
}
interface Fig {
  root: Box;
  ctls: Ctl[];
  rowTops: number[];
  uiText: string;
  scroll: [number, number];
  theme: string;
}
interface TableFig {
  box: Box;
  firstRowTop: number | null;
  rows: number;
  wrapScroll: [number, number];
  shade: string;
}
interface HitCount {
  pass: number;
  fail: number;
  notReached: number;
  total: number;
  fails: string[];
}
interface Rec {
  fig: Fig;
  table?: TableFig | null;
  hit?: HitCount;
  keyboard?: string;
}

/** Runs in the page, on the root of a toolbar, dialog or fieldset: every control with its label's text and type. */
function describeInPage(root: HTMLElement): Fig {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const boxOf = (
    r: DOMRect | { left: number; top: number; width: number; height: number },
  ): Box => ({
    x: r1(r.left),
    y: r1(r.top),
    w: r1(r.width),
    h: r1(r.height),
  });
  const styOf = (e: Element): Sty => {
    const c = getComputedStyle(e);
    return {
      fontSize: c.fontSize,
      fontWeight: c.fontWeight,
      textTransform: c.textTransform,
      letterSpacing: c.letterSpacing,
      color: c.color,
    };
  };
  const textOf = (lab: Element, ctl: Element) => {
    const w = document.createTreeWalker(lab, NodeFilter.SHOW_TEXT);
    for (let n = w.nextNode(); n; n = w.nextNode()) {
      const s = n.textContent ?? '';
      if (!s.trim() || ctl.contains(n)) continue;
      const pe = n.parentElement;
      if (!pe || pe.closest('option')) continue;
      const rg = document.createRange();
      rg.setStart(n, s.search(/\S/));
      rg.setEnd(n, s.trimEnd().length);
      return {
        box: boxOf(rg.getBoundingClientRect()),
        sty: styOf(pe),
        text: s.trim().slice(0, 30),
      };
    }
    return null;
  };
  const canvas = document.createElement('canvas').getContext('2d')!;
  const seen = new Map<string, number>();
  const ctls: Ctl[] = [];
  for (const el of Array.from(
    root.querySelectorAll<HTMLElement>('select, input:not([type=hidden]), textarea, button'),
  )) {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const tag = el.tagName.toLowerCase();
    const type = tag === 'input' ? (el as HTMLInputElement).type : '';
    const lab =
      el.closest('label') ??
      (el.id ? document.querySelector(`label[for="${CSS.escape(el.id)}"]`) : null);
    const text = lab ? textOf(lab, el) : null;
    const name =
      text?.text ??
      el.getAttribute('aria-label') ??
      (tag === 'button' ? (el.textContent ?? '').trim().slice(0, 30) : '');
    let key = `${tag}${type ? `:${type}` : ''}:${name}`;
    const n = (seen.get(key) ?? 0) + 1;
    seen.set(key, n);
    if (n > 1) key += `#${n}`;
    const par = el.parentElement;
    const wrap =
      par && par.tagName === 'SPAN' && par !== lab ? boxOf(par.getBoundingClientRect()) : null;
    const field = wrap ?? boxOf(r);
    const cs = getComputedStyle(el);
    let clear: boolean | null = null;
    let value = '';
    if (el instanceof HTMLSelectElement) {
      value = el.options[el.selectedIndex]?.textContent ?? '';
      canvas.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const inner =
        r.width -
        parseFloat(cs.paddingLeft) -
        parseFloat(cs.paddingRight) -
        parseFloat(cs.borderLeftWidth) -
        parseFloat(cs.borderRightWidth);
      clear = canvas.measureText(value).width <= inner + 0.5;
    }
    ctls.push({
      key,
      tag,
      box: boxOf(r),
      wrap,
      label: lab ? boxOf(lab.getBoundingClientRect()) : null,
      text: text ? { box: text.box, sty: text.sty } : null,
      sty: styOf(el),
      gapY: text ? r1(field.y - (text.box.y + text.box.h)) : null,
      gapX: text ? r1(field.x - (text.box.x + text.box.w)) : null,
      clear,
      value,
    });
  }
  const kids = Array.from(root.children).filter((c) => c.getBoundingClientRect().height > 0);
  const rowTops = Array.from(
    new Set(kids.map((k) => Math.round(k.getBoundingClientRect().top / 4) * 4)),
  ).sort((a, b) => a - b);
  const probe = document.createElement('span');
  probe.style.color = 'var(--ui-text)';
  root.appendChild(probe);
  const uiText = getComputedStyle(probe).color;
  probe.remove();
  return {
    root: boxOf(root.getBoundingClientRect()),
    ctls,
    rowTops,
    uiText,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? 'no-theme',
  };
}

/** Runs in the page: the first table's box, its first row's top and the wrapper's shade. */
function tableInPage(): TableFig | null {
  const t = document.querySelector<HTMLElement>('main table, table');
  if (!t) return null;
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const b = t.getBoundingClientRect();
  const wrap = t.parentElement!;
  const cs = getComputedStyle(wrap);
  const row = t.querySelector('tbody tr');
  return {
    box: { x: r1(b.left), y: r1(b.top), w: r1(b.width), h: r1(b.height) },
    firstRowTop: row ? r1(row.getBoundingClientRect().top) : null,
    rows: t.querySelectorAll('tbody tr').length,
    wrapScroll: [wrap.scrollWidth, wrap.clientWidth],
    shade: [cs.backgroundImage, cs.maskImage, cs.boxShadow].join(' | ').slice(0, 200),
  };
}

// ------------------------------------------------------------------------------------------------ plumbing

async function setMain(owner: BrowserContext, theme: string) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function newCtx(browser: Browser, owner: BrowserContext, c: Cfg) {
  await setMain(owner, c.theme);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((theme) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, c.theme);
  return ctx;
}

const settled = async (page: Page, ms = 4000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};

async function goAdmin(page: Page, label: RegExp) {
  await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await settled(page);
  await page
    .locator(NAV)
    .filter({ hasText: label })
    .first()
    .evaluate((el) => (el as HTMLElement).click());
  await settled(page, 3000);
}

async function shot(page: Page, root: Locator, name: string) {
  mkdirSync(OUT, { recursive: true });
  await root.evaluate((e) => (e as HTMLElement).scrollIntoView({ block: 'center' }));
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function hit(page: Page, scope: Locator, c: Cfg): Promise<HitCount> {
  await openCollapsed(page, scope);
  const rows: ControlRow[] = await hitTestAll(page, scope, c.touch ? {} : { reach: 'box' });
  const fails = rows
    .filter((r) => r.status === 'fail')
    .map(
      (r) =>
        `${r.tag} "${r.name}": ${(r.result?.points ?? [])
          .filter((p) => !p.ok)
          .map((p) => `${p.at}:${p.taker?.tag}.${p.taker?.name}`)
          .join(',')}`,
    );
  return {
    pass: rows.filter((r) => r.status === 'pass').length,
    fail: fails.length,
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    total: rows.length,
    fails,
  };
}

const figure = (root: Locator) => root.evaluate(describeInPage);

// ------------------------------------------------------------------------------------------------ the bridge fakes

async function fakeBridge(page: Page, g: string, mode: 'whole_guild' | 'selective') {
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/discord-channels$`), (route) =>
    route.fulfill({
      json: [
        { id: '1600000000000000001', name: 'p105s-disc-one' },
        { id: '1600000000000000002', name: 'p105s-disc-two' },
      ],
    }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${g}/sr/features$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as { id: string; enabled: boolean }[];
    await route.fulfill({
      response: res,
      json: list.map((m) => (m.id === 'bridge.enabled' ? { ...m, enabled: true } : m)),
    });
  });
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/status$`), (route) =>
    route.fulfill({
      json: {
        platform_on: true,
        module_on: true,
        connected: true,
        bridge: {
          discord_guild_id: '1100000000000000001',
          discord_guild_name: 'p105s-friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'p105s-relay',
          mode,
          status: 'connected',
          intents_granted: (1 << 15) | 1,
          created_by: '1',
          created_at: 0,
          invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot',
        },
        maps: [],
        queued: 0,
        dead_letters: 0,
        leftovers: [],
        erasure_leftovers: [],
        is_owner: true,
      },
    }),
  );
  const conflict = (id: string, name: string) => ({
    id,
    kind: 'cap',
    object_kind: 'channel',
    sr_object_id: '1400000000000000001',
    discord_object_id: '1400000000000000002',
    field: 'name',
    discord_value: 'p105s-new',
    sr_value: 'p105s-old',
    discord_actor_name: 'p105s-actor',
    count: 1,
    status: 'open',
    first_at: Date.now() - 3_600_000,
    last_at: Date.now() - 60_000,
    object_name: name,
  });
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/conflicts(\\?.*)?$`), (route) =>
    route.fulfill({
      json: {
        conflicts: [
          conflict('1500000000000000001', 'p105s-general'),
          conflict('1500000000000000002', 'p105s-off-topic'),
        ],
        open: 2,
      },
    }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/preflight$`), (route) =>
    route.fulfill({
      json: {
        checks: [
          { id: 'bot_admin', ok: true, blocker: true },
          { id: 'intents', ok: true, blocker: true },
        ],
        invite_url: null,
      },
    }),
  );
  const item = (action: string, name: string, extra: Record<string, unknown>) => ({
    kind: 'channel',
    action,
    name,
    sr_permissions: '0',
    discord_permissions: '0',
    dangerous_bits: '0',
    import: false,
    ...extra,
  });
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/plan$`), (route) =>
    route.fulfill({ json: { plan_id: '1700000000000000001' } }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/plan/1700000000000000001$`), (route) =>
    route.fulfill({
      json: {
        id: '1700000000000000001',
        status: 'ready',
        seed_structure: 'sr',
        plan_hash: 'p105s',
        expires_at: Date.now() + 900_000,
        blockers: [],
        items: [
          item('create_on_discord', 'p105s-only-here', { sr_id: '1800000000000000001' }),
          item('create_here', 'p105s-only-there', { discord_id: '1800000000000000002' }),
        ],
        not_mirrored: [],
        estimated_writes: 2,
      },
    }),
  );
}

// ------------------------------------------------------------------------------------------------ comparing

const PICK: Sty = {
  fontSize: '14px',
  fontWeight: '400',
  textTransform: 'none',
  letterSpacing: 'normal',
  color: '',
};
const near = (a: number, b: number, tol = 0.5) => Math.abs(a - b) <= tol;
const bx = (b: Box | null) => (b ? `${b.x},${b.y} ${b.w}x${b.h}` : 'none');
const sy = (s: Sty) =>
  `${s.fontSize}/${s.fontWeight}/${s.textTransform}/${s.letterSpacing}/${s.color}`;
const sameBox = (a: Box | null, b: Box | null, axes = 'xywh') =>
  a === b || (!!a && !!b && [...axes].every((k) => near(a[k as keyof Box], b[k as keyof Box])));
const sameSty = (a: Sty, b: Sty) => sy(a) === sy(b);
const find = (f: Fig, key: string) => f.ctls.find((c) => c.key === key);

/** Every control of the base is in the after with the same boxes and type, except the keys named in `moved`. */
function strict(tag: string, b: Fig, a: Fig, problems: string[], moved: string[] = []) {
  if (!sameBox(b.root, a.root)) problems.push(`${tag}: root box ${bx(b.root)} -> ${bx(a.root)}`);
  if (b.rowTops.join() !== a.rowTops.join())
    problems.push(`${tag}: row tops ${b.rowTops.join()} -> ${a.rowTops.join()}`);
  for (const bc of b.ctls) {
    const ac = find(a, bc.key);
    if (!ac) {
      problems.push(`${tag}: control "${bc.key}" is gone`);
      continue;
    }
    if (!sameBox(bc.box, ac.box))
      problems.push(`${tag} "${bc.key}": box ${bx(bc.box)} -> ${bx(ac.box)}`);
    if (!sameBox(bc.label, ac.label))
      problems.push(`${tag} "${bc.key}": label box ${bx(bc.label)} -> ${bx(ac.label)}`);
    if (!sameBox(bc.text?.box ?? null, ac.text?.box ?? null))
      problems.push(
        `${tag} "${bc.key}": label text box ${bx(bc.text?.box ?? null)} -> ${bx(ac.text?.box ?? null)}`,
      );
    if (bc.text && ac.text && !sameSty(bc.text.sty, ac.text.sty))
      problems.push(`${tag} "${bc.key}": label text type ${sy(bc.text.sty)} -> ${sy(ac.text.sty)}`);
    if (!moved.includes(bc.key) && !sameSty(bc.sty, ac.sty))
      problems.push(`${tag} "${bc.key}": type ${sy(bc.sty)} -> ${sy(ac.sty)}`);
  }
  if (a.scroll[0] > a.scroll[1])
    problems.push(`${tag}: page scrolls sideways ${a.scroll.join('/')}`);
}

function expectPick(tag: string, a: Fig, keys: string[], problems: string[]) {
  for (const k of keys) {
    const c = find(a, k);
    if (!c) {
      problems.push(`${tag}: "${k}" not found`);
      continue;
    }
    const want = { ...PICK, color: a.uiText };
    if (!sameSty(c.sty, want))
      problems.push(`${tag} "${k}": type ${sy(c.sty)}, wanted ${sy(want)}`);
    if (c.clear === false) problems.push(`${tag} "${k}": selected text runs into the chevron`);
  }
}

function hitCmp(tag: string, b: HitCount | undefined, a: HitCount | undefined, problems: string[]) {
  if (!b || !a) return;
  if (a.notReached) problems.push(`${tag}: hit test not reached ${a.notReached} (probe error)`);
  if (a.fail > b.fail || a.total !== b.total)
    problems.push(
      `${tag}: hit test base ${b.pass}/${b.fail}/${b.total} -> after ${a.pass}/${a.fail}/${a.total} ${a.fails.join(' ; ')}`,
    );
}

type Figs = Record<string, Rec>;

function compare(cfgTags: Record<string, Cfg[]>, base: Figs, after: Figs, problems: string[]) {
  const want = (c: Cfg) => (c.touch ? 44 : 40);
  for (const [name, cfgs] of Object.entries(cfgTags)) {
    for (const c of cfgs) {
      const id = `${name}-${c.tag}`;
      const b = base[id];
      const a = after[id];
      const tag = `${name} ${c.tag}`;
      if (!a) {
        problems.push(`${tag}: no figures in the after phase (scenario failed)`);
        continue;
      }
      if (!b) {
        problems.push(`${tag}: no base figures (scenario failed in the base phase)`);
        continue;
      }
      if (name === 'invoices') {
        strict(tag, b.fig, a.fig, problems, ['select:State', 'select:Purpose']);
        expectPick(tag, a.fig, ['select:State', 'select:Purpose'], problems);
        if (!sameBox(b.table?.box ?? null, a.table?.box ?? null))
          problems.push(
            `${tag}: table box ${bx(b.table?.box ?? null)} -> ${bx(a.table?.box ?? null)}`,
          );
        if (b.table?.firstRowTop !== a.table?.firstRowTop)
          problems.push(`${tag}: first row top ${b.table?.firstRowTop} -> ${a.table?.firstRowTop}`);
        if (b.table?.shade !== a.table?.shade)
          problems.push(`${tag}: table shade ${b.table?.shade} -> ${a.table?.shade}`);
        for (const k of ['select:State', 'select:Purpose']) {
          const h = find(a.fig, k)?.box.h;
          if (h !== undefined && !near(h, want(c)))
            problems.push(`${tag} "${k}": height ${h}, wanted ${want(c)}`);
        }
        hitCmp(tag, b.hit, a.hit, problems);
        if (a.keyboard && !a.keyboard.startsWith('ok'))
          problems.push(`${tag}: keyboard ${a.keyboard}`);
      } else if (name === 'entitlements') {
        const bk = find(b.fig, 'select:Kind');
        const ak = find(a.fig, 'select:Kind');
        if (!bk || !ak) {
          problems.push(`${tag}: Kind not found`);
        } else {
          const grow = ak.box.h - bk.box.h;
          if (!near(ak.box.h, want(c)))
            problems.push(`${tag} Kind: height ${bk.box.h} -> ${ak.box.h}, wanted ${want(c)}`);
          if (ak.box.w > bk.box.w + 36)
            problems.push(`${tag} Kind: width ${bk.box.w} -> ${ak.box.w} (limit base + 36)`);
          if (ak.clear === false) problems.push(`${tag} Kind: selected text runs into the chevron`);
          if (b.fig.root.h > a.fig.root.h + 0.5 || a.fig.root.h > b.fig.root.h + grow + 0.5)
            problems.push(
              `${tag}: toolbar height ${b.fig.root.h} -> ${a.fig.root.h} (Kind grew ${grow})`,
            );
          if (bk.text && ak.text && !sameSty(bk.text.sty, ak.text.sty))
            problems.push(`${tag} Kind label type ${sy(bk.text.sty)} -> ${sy(ak.text.sty)}`);
          say(
            `${tag}: Kind box ${bx(bk.box)} -> ${bx(ak.box)}; gap x ${bk.gapX} -> ${ak.gapX}; style ${sy(bk.sty)} -> ${sy(ak.sty)}`,
          );
        }
        const bc = find(b.fig, 'input:checkbox:Only active');
        const ac = find(a.fig, 'input:checkbox:Only active');
        if (bc && ac) {
          const grow = (ak?.box.h ?? 0) - (bk?.box.h ?? 0);
          if (!sameBox(bc.label, ac.label, 'wh'))
            problems.push(`${tag}: checkbox label ${bx(bc.label)} -> ${bx(ac.label)}`);
          if (Math.abs((ac.label?.y ?? 0) - (bc.label?.y ?? 0)) > grow + 0.5)
            problems.push(
              `${tag}: checkbox label y ${bc.label?.y} -> ${ac.label?.y} (Kind grew ${grow})`,
            );
          if (!sameSty(bc.sty, ac.sty)) problems.push(`${tag}: checkbox type changed`);
        } else problems.push(`${tag}: the checkbox row not found`);
        hitCmp(tag, b.hit, a.hit, problems);
        if (a.keyboard && !a.keyboard.startsWith('ok'))
          problems.push(`${tag}: keyboard ${a.keyboard}`);
      } else if (name === 'grant') {
        const k = find(a.fig, 'select:Kind');
        const others = a.fig.ctls.filter(
          (x) => x.text && x.key !== 'select:Kind' && /^(input:text|textarea)/.test(x.key),
        );
        if (!k?.text) problems.push(`${tag}: Kind label not found`);
        else {
          for (const o of others)
            if (o.text && !sameSty(k.text.sty, o.text.sty))
              problems.push(
                `${tag}: Kind label ${sy(k.text.sty)} differs from "${o.key}" label ${sy(o.text.sty)}`,
              );
          if (!others.length)
            problems.push(`${tag}: no other labelled field to compare the Kind label with`);
          if (k.sty.fontSize !== '14px' || k.sty.textTransform !== 'none')
            problems.push(`${tag}: Kind select type ${sy(k.sty)} (wanted 14px, none)`);
          if (k.clear === false) problems.push(`${tag}: Kind selected text runs into the chevron`);
        }
        const bk = find(b.fig, 'select:Kind');
        say(
          `${tag}: Kind label base ${bk?.text ? sy(bk.text.sty) : 'none'} -> after ${k?.text ? sy(k.text.sty) : 'none'}`,
        );
        for (const o of others)
          say(`${tag}: other label "${o.key}" ${o.text ? sy(o.text.sty) : 'none'}`);
      } else if (name === 'audit' || name === 'bans' || name === 'storage') {
        strict(tag, b.fig, a.fig, problems);
      } else if (name === 'conflicts') {
        strict(tag, b.fig, a.fig, problems);
        const k = find(a.fig, 'select:Kind') ?? a.fig.ctls.find((x) => x.tag === 'select');
        const kb = find(b.fig, 'select:Kind') ?? b.fig.ctls.find((x) => x.tag === 'select');
        if (k && kb) {
          if (k.sty.fontSize !== '14px' || k.sty.fontWeight !== '600')
            problems.push(`${tag}: Kind ${sy(k.sty)} (wanted 14px, 600)`);
          if (k.gapX !== kb.gapX || k.gapY !== kb.gapY)
            problems.push(`${tag}: Kind gap ${kb.gapX}/${kb.gapY} -> ${k.gapX}/${k.gapY}`);
        }
      } else if (name === 'bridgemap' || name === 'plan') {
        const keys = a.fig.ctls.filter((x) => x.tag === 'select').map((x) => x.key);
        strict(tag, b.fig, a.fig, problems, keys);
        expectPick(tag, a.fig, keys, problems);
        for (const k of keys) {
          const bc = find(b.fig, k);
          const ac = find(a.fig, k);
          if (bc && ac && (bc.gapY !== ac.gapY || bc.gapX !== ac.gapX))
            problems.push(`${tag} "${k}": gap ${bc.gapX}/${bc.gapY} -> ${ac.gapX}/${ac.gapY}`);
          if (ac && !near(ac.box.h, want(c)))
            problems.push(`${tag} "${k}": height ${ac.box.h}, wanted ${want(c)}`);
        }
        if (!keys.length) problems.push(`${tag}: no selects found`);
        if (name === 'plan' && keys.length !== 2)
          problems.push(`${tag}: ${keys.length} selects, wanted 2`);
        if (name === 'bridgemap' && keys.length !== 3)
          problems.push(`${tag}: ${keys.length} selects, wanted 3`);
      }
    }
  }
}

// ------------------------------------------------------------------------------------------------ the run

const PLAN: Record<string, Cfg[]> = {
  invoices: [L1280, L1024, D1024, L375],
  entitlements: [L1024, D1024, L375],
  grant: [L1024],
  audit: [L1024],
  bans: [L1024],
  storage: [L1024],
  conflicts: [L1024, L375],
  bridgemap: [L1024, D1024, L375],
  plan: [L1024, L375],
};
const SHOTS = new Set([
  'invoices-1280-light',
  'invoices-1024-light',
  'invoices-1024-dark',
  'invoices-375-light',
  'entitlements-1024-light',
  'entitlements-1024-dark',
  'entitlements-375-light',
  'grant-1024-light',
  'conflicts-1024-light',
  'bridgemap-1024-light',
  'bridgemap-1024-dark',
  'bridgemap-375-light',
  'plan-1024-light',
]);

test(`p105 staff: donations toolbars and the bridge forms, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Figs = {};
  const shots: string[] = [];
  const g = (JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string }).guildId;
  const all = new Map<string, Cfg>();
  for (const cs of Object.values(PLAN)) for (const c of cs) all.set(c.tag, c);

  const take = async (
    name: string,
    c: Cfg,
    page: Page,
    root: Locator,
    extra?: (r: Rec) => Promise<void>,
  ) => {
    const id = `${name}-${c.tag}`;
    const fig = await figure(root);
    if (fig.theme !== c.theme)
      problems.push(`${id}: data-theme is ${fig.theme}, wanted ${c.theme}`);
    if (fig.scroll[0] > fig.scroll[1])
      problems.push(`${id}: page scrolls sideways ${fig.scroll.join('/')}`);
    const rec: Rec = { fig };
    if (extra) await extra(rec);
    figs[id] = rec;
    if (SHOTS.has(id)) {
      await shot(page, root, id);
      shots.push(id);
    }
    for (const k of fig.ctls)
      say(
        `${PHASE} ${id} ${k.key}: box ${bx(k.box)} label text ${bx(k.text?.box ?? null)} type ${sy(k.sty)} label type ${k.text ? sy(k.text.sty) : '-'} gap ${k.gapX}/${k.gapY} clear ${k.clear}`,
      );
    say(`${PHASE} ${id}: root ${bx(fig.root)} rowTops ${fig.rowTops.join()} uiText ${fig.uiText}`);
  };
  const guard = async (what: string, f: () => Promise<void>) => {
    try {
      await f();
    } catch (e) {
      problems.push(
        `${what}: spec error / not reached: ${String(e).replace(/\s+/g, ' ').slice(0, 400)}`,
      );
    }
  };

  try {
    for (const c of all.values()) {
      const ctx = await newCtx(browser, owner, c);
      try {
        const page = await ctx.newPage();
        page.setDefaultTimeout(15_000);
        const on = (n: string) => (PLAN[n] ?? []).includes(c);

        if (on('invoices'))
          await guard(`invoices ${c.tag}`, async () => {
            await goAdmin(page, /^Invoices$/);
            const root = page
              .locator('form')
              .filter({ has: page.getByRole('button', { name: 'Search', exact: true }) })
              .filter({ has: page.locator('select') })
              .first();
            await root.waitFor({ timeout: 15_000 });
            await take('invoices', c, page, root, async (r) => {
              r.table = await page.evaluate(tableInPage);
              r.hit = await hit(page, root, c);
              if (c.tag === '1024-light') {
                const state = root.locator('select').first();
                const rows = () => page.getByRole('table').first().locator('tbody tr').count();
                const before = await rows();
                await state.focus();
                await page.keyboard.press('ArrowDown');
                await settled(page, 3000);
                const v = await state.inputValue();
                const during = await rows();
                await state.selectOption('');
                await settled(page, 3000);
                const back = await state.inputValue();
                r.keyboard =
                  v !== '' && back === ''
                    ? `ok value "${v}", rows ${before} -> ${during}`
                    : `ArrowDown gave "${v}", reset gave "${back}"`;
                say(`${PHASE} invoices keyboard: ${r.keyboard}`);
              }
            });
          });

        if (on('entitlements') || on('grant'))
          await guard(`entitlements ${c.tag}`, async () => {
            await goAdmin(page, /^Entitlements$/);
            const kindLabel = page
              .locator('label')
              .filter({ hasText: /^Kind/ })
              .filter({ has: page.locator('select') })
              .first();
            await kindLabel.waitFor({ timeout: 15_000 });
            const root = kindLabel.locator('xpath=..');
            if (on('entitlements'))
              await take('entitlements', c, page, root, async (r) => {
                r.hit = await hit(page, root, c);
                if (c.tag === '1024-light') {
                  const kind = root.locator('select').first();
                  await kind.focus();
                  await page.keyboard.press('ArrowDown');
                  await settled(page, 2500);
                  const v = await kind.inputValue();
                  await kind.selectOption('');
                  r.keyboard =
                    v !== '' ? `ok value "${v}"` : 'ArrowDown did not change the Kind filter';
                  say(`${PHASE} entitlements keyboard: ${r.keyboard}`);
                }
              });
            if (on('grant')) {
              await page.getByRole('button', { name: 'Grant', exact: true }).first().click();
              const dlg = page.getByRole('dialog').last();
              await dlg.waitFor({ timeout: 10_000 });
              await page.waitForTimeout(700);
              await take('grant', c, page, dlg);
              await page.keyboard.press('Escape');
            }
          });

        for (const [name, re, role] of [
          ['audit', /^Platform audit log$/, 'form'],
          ['bans', /^Platform bans$/, 'bans'],
          ['storage', /^Storage$/, 'storage'],
        ] as const)
          if (on(name))
            await guard(`${name} ${c.tag}`, async () => {
              await goAdmin(page, re);
              const root =
                role === 'form'
                  ? page.getByRole('search', { name: /audit log/i }).first()
                  : role === 'bans'
                    ? page.getByRole('group', { name: /filter bans/i }).first()
                    : page.getByRole('group', { name: /^filters$/i }).first();
              await root.waitFor({ timeout: 15_000 });
              await take(name, c, page, root);
            });

        const bridgePage = async (mode: 'whole_guild' | 'selective') => {
          const p = await ctx.newPage();
          p.setDefaultTimeout(20_000);
          await fakeBridge(p, g, mode);
          await p.goto(`/settings/server/${g}/bridge`, {
            waitUntil: 'domcontentloaded',
            timeout: 30_000,
          });
          return p;
        };

        if (on('conflicts'))
          await guard(`conflicts ${c.tag}`, async () => {
            const p = await bridgePage('whole_guild');
            const card = p.getByTestId('bridge-conflicts');
            await card.getByRole('combobox').waitFor({ timeout: 40_000 });
            await settled(p);
            await take('conflicts', c, p, card);
          });

        if (on('bridgemap'))
          await guard(`bridgemap ${c.tag}`, async () => {
            const p = await bridgePage('selective');
            const add = p.getByRole('group', { name: /Mirror another channel/ }).first();
            await add.waitFor({ timeout: 40_000 });
            await settled(p);
            await take('bridgemap', c, p, add);
          });

        if (on('plan'))
          await guard(`plan ${c.tag}`, async () => {
            const p = await bridgePage('selective');
            await p
              .getByRole('button', { name: 'Prepare the whole-server plan' })
              .click({ timeout: 40_000 });
            await p.getByRole('button', { name: 'Build the plan' }).click();
            await p.getByRole('tab', { name: /^Channels/ }).click({ timeout: 20_000 });
            const link = p.getByRole('group', { name: 'Link two by hand' });
            await link.waitFor({ timeout: 20_000 });
            await settled(p);
            await take('plan', c, p, link);
          });
      } finally {
        await ctx.close().catch(() => undefined);
      }
    }
  } finally {
    await owner.close().catch(() => undefined);
  }

  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Figs;
    compare(PLAN, base, figs, problems);
  } else if (PHASE === 'after') say('no figures-base.json in the folder: nothing compared');
  say(`shots ${shots.length}: ${shots.join(', ')}`);
  say(`problems ${JSON.stringify(problems)}`);
  writeFileSync(resolve(OUT, `problems-${PHASE}.json`), JSON.stringify(problems, null, 1));
  void sleep;
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
