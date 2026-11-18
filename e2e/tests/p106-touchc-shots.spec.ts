import { deflateSync } from 'node:zlib';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import {
  CONTROL_SELECTOR,
  MENU_ITEM_SELECTOR,
  hitTestAll,
  openCollapsed,
  type ControlRow,
} from '../support/hit-test';

/**
 * Wave 106, census touchc (D-856, D-917, D-983; MEASURE ONLY): every control that fails the 44 px hit test at 375 x 812
 * with touch, Lounge Light, in Server settings, Admin, and the tools and rooms. No app file is changed.
 *
 * For each page: go there as a member does, settle, `openCollapsed`, `hitTestAll` over the page's `<main>` with the
 * default reach (21 px); then open up to three things a member opens with one tap (menu and kit select triggers, one
 * "Create / New / Add / Invite" dialog button; never a destructive one) and test each overlay with the menu items and
 * options added. Nothing is submitted. A few pages add a state of their own (one role, one channel, the member bulk bar,
 * one moderation case). Every row is written whole to `figures-part<n>.json`; one viewport shot per page.
 *
 * Parts (`SR_P106_TOUCHC_PART`): 1 Server settings, 2 Admin, 3 tools and
 * rooms. It needs the sweep's seeded world (`e2e/.state/sweep/`, given fresh by the job; never seeded here). Part 3 makes
 * one text channel through the API (`p106touchc-<stamp>`) and removes it in a `finally`.
 *
 * Local only (SR_P106_TOUCHC_ON=1). The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_TOUCHC_ON=1 --env SR_P106_TOUCHC_PART=1 --env SR_P106_TOUCHC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-touchc -- tests/p106-touchc-shots.spec.ts
 */
const ON = process.env.SR_P106_TOUCHC_ON === '1';
const PART = Number(process.env.SR_P106_TOUCHC_PART ?? '1');
const OUT =
  process.env.SR_P106_TOUCHC_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-touchc';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);
const BUDGET_MS = 420_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
  partyId: string | null;
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface PointFig {
  at: string;
  ok: boolean;
  relation: string | null;
  takerTag: string | null;
  takerRole: string | null;
  takerName: string | null;
  takerPath: string | null;
}
interface RowFig {
  page: string;
  state: string;
  index: number;
  name: string;
  tag: string;
  role: string | null;
  type: string | null;
  path: string;
  status: string;
  reason: string | null;
  box: { x: number; y: number; width: number; height: number } | null;
  offset: number | null;
  /** The first failing point's taker, when another node took a point. */
  taker: { tag: string; role: string | null; name: string; path: string; at: string } | null;
  points: PointFig[];
}
interface StateFig {
  page: string;
  state: string;
  rows: number;
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
  opened: number;
}
const rowsOut: RowFig[] = [];
const statesOut: StateFig[] = [];
const notes: string[] = [];
const notCovered: string[] = [];
const sideways: Record<string, number[]> = {};

// ---- the pages ------------------------------------------------------------------------------------------------------

interface PageDef {
  name: string;
  part: number;
  path: (w: World) => string | null;
  /** After the page itself is tested: sets up one more state of the same page and tests it. */
  extra?: (page: Page, def: PageDef, root: Locator) => Promise<void>;
  /** The shell's own controls (sidebar toggle, close) are tested on this page too. */
  chrome?: boolean;
}

const sv = (s: string) => (w: World) => `/settings/server/${w.guildId}/${s}`;
const SERVER: PageDef[] = [
  { name: 'server-overview', part: 1, path: sv('overview'), chrome: true },
  { name: 'server-roles', part: 1, path: sv('roles'), extra: roleExtra },
  { name: 'server-support', part: 1, path: sv('support') },
  { name: 'server-tag', part: 1, path: sv('tag') },
  { name: 'server-channels', part: 1, path: sv('channels'), extra: channelExtra },
  { name: 'server-emoji', part: 1, path: sv('emoji') },
  { name: 'server-members', part: 1, path: sv('members'), extra: membersExtra },
  { name: 'server-invites', part: 1, path: sv('invites') },
  { name: 'server-bans', part: 1, path: sv('bans') },
  { name: 'server-features', part: 1, path: sv('features') },
  { name: 'server-extensions', part: 1, path: sv('extensions') },
  { name: 'server-watch-library', part: 1, path: sv('watch-library') },
  { name: 'server-storage', part: 1, path: sv('storage') },
  { name: 'server-integrations', part: 1, path: sv('integrations') },
  { name: 'server-bridge', part: 1, path: sv('bridge') },
  { name: 'server-gameservers', part: 1, path: sv('gameservers') },
  { name: 'server-moderation', part: 1, path: sv('moderation') },
  { name: 'server-audit-log', part: 1, path: sv('audit-log') },
  { name: 'server-community', part: 1, path: sv('community') },
  { name: 'server-delete', part: 1, path: sv('delete') },
  {
    name: 'server-channel-permissions',
    part: 1,
    path: (w) => `/settings/server/${w.guildId}/channels?channel=${w.channelId}&tab=permissions`,
  },
  {
    name: 'server-channel-voice',
    part: 1,
    path: (w) =>
      w.voiceChannelId
        ? `/settings/server/${w.guildId}/channels?channel=${w.voiceChannelId}&tab=voice`
        : null,
  },
];
const ADMIN_SECTIONS = [
  'dashboard',
  'crash-reports',
  'accounts',
  'bans',
  'moderation',
  'data-requests',
  'storage',
  'invites',
  'invite-graph',
  'servers',
  'staff',
  'review',
  'jobs',
  'ingest-sources',
  'library-catalog',
  'cinema-catalog',
  'cinema-collections',
  'cinema-transcoding',
  'cinema-live-tv',
  'donations',
  'donation-invoices',
  'donation-entitlements',
  'donation-pricing',
  'features',
  'arcade',
  'entrance',
];
const ADMIN: PageDef[] = ADMIN_SECTIONS.map((s, i) => ({
  name: `admin-${s}`,
  part: 2,
  path: () => `/admin/${s}`,
  chrome: i === 0,
  ...(s === 'moderation' ? { extra: moderationExtra } : {}),
}));

// ---- helpers --------------------------------------------------------------------------------------------------------

async function newPage(browser: Browser, owner: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  await ctx.addInitScript(() => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: 'lounge-light', zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function settle(page: Page, ms = 6000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page
    .waitForFunction(() => document.querySelectorAll('[aria-busy="true"]').length === 0, null, {
      timeout: 8000,
    })
    .catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const s = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  sideways[name] = s;
  if (s[0]! > s[1]!) notes.push(`${name}: the page scrolls sideways (${s[0]} > ${s[1]})`);
  await page.screenshot({ path: resolve(OUT, `${name}-375.png`), timeout: 15_000 });
}

function toRow(pageName: string, state: string, r: ControlRow): RowFig {
  const points: PointFig[] = (r.result?.points ?? []).map((p) => ({
    at: p.at,
    ok: p.ok,
    relation: p.relation,
    takerTag: p.taker?.tag ?? null,
    takerRole: p.taker?.role ?? null,
    takerName: p.taker?.name ?? null,
    takerPath: p.taker?.path ?? null,
  }));
  const bad = (r.result?.points ?? []).find((p) => !p.ok);
  return {
    page: pageName,
    state,
    index: r.index,
    name: r.name,
    tag: r.tag,
    role: r.role,
    type: r.type,
    path: r.path,
    status: r.status,
    reason: r.reason ?? null,
    box: r.result?.box ?? null,
    offset: r.result?.offset ?? null,
    taker: bad?.taker
      ? {
          tag: bad.taker.tag,
          role: bad.taker.role,
          name: bad.taker.name,
          path: bad.taker.path,
          at: bad.at,
        }
      : null,
    points,
  };
}

/** Tests one root: openCollapsed then hitTestAll; keeps every row. `keep` filters rows (the bulk bar's new ones). */
async function testRoot(
  page: Page,
  root: Locator,
  pageName: string,
  state: string,
  also?: string,
  keep?: (index: number) => Promise<boolean>,
) {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root, also ? { also } : {});
  const kept: ControlRow[] = [];
  for (const r of rows) if (!keep || (await keep(r.index))) kept.push(r);
  for (const r of kept) rowsOut.push(toRow(pageName, state, r));
  const n = (s: string) => kept.filter((r) => r.status === s).length;
  statesOut.push({
    page: pageName,
    state,
    rows: kept.length,
    pass: n('pass'),
    fail: n('fail'),
    notReached: n('not-reached'),
    skipped: n('skipped'),
    opened: opened.length,
  });
  say(
    `${pageName} [${state}]: ${kept.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}, skipped ${n('skipped')}`,
  );
  for (const r of kept)
    if (r.status === 'fail' || r.status === 'not-reached') {
      const b = r.result?.box;
      const bad = r.result?.points.find((p) => !p.ok);
      say(
        `  ${r.status.toUpperCase()} ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" ${b ? `${Math.round(b.width)}x${Math.round(b.height)}` : '-'} ${
          bad?.taker
            ? `taker ${bad.at}: ${bad.taker.tag} "${bad.taker.name}" ${bad.taker.path}`
            : (r.reason ?? '')
        }`,
      );
    }
}

const DANGER =
  /delete|remove|ban|kick|suspend|revoke|reset|purge|prune|leave|disconnect|sign out|log out/i;
const OVERLAY_ALSO = `${MENU_ITEM_SELECTOR},[role=option]`;

/** The overlay (menu, listbox or dialog) that is not the page shell: visible, with no <main> inside it. */
const overlayOf = (page: Page) =>
  page
    .locator('[role=menu],[role=listbox],[role=dialog]')
    .filter({ hasNot: page.locator('main') })
    .filter({ visible: true });

/** Opens up to three things one tap opens inside `root` and tests each overlay. */
async function overlays(page: Page, root: Locator, pageName: string, url: string) {
  const cands = root.locator(
    'button[aria-haspopup]:not([aria-haspopup="false"]),[role=combobox],button',
  );
  const n = Math.min(await cands.count(), 80);
  const picked: { loc: Locator; name: string }[] = [];
  const seen = new Set<string>();
  let dialogButtons = 0;
  for (let i = 0; i < n && picked.length < 3; i++) {
    const loc = cands.nth(i);
    const info = await loc
      .evaluate((el) => {
        const popup = el.getAttribute('aria-haspopup');
        const aria = (el.getAttribute('aria-label') ?? '').trim();
        const txt = (el.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40);
        const r = el.getBoundingClientRect();
        return {
          popup: popup !== null && popup !== 'false',
          combo: el.getAttribute('role') === 'combobox',
          name: aria || txt || el.getAttribute('title') || '',
          shown: r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden',
          disabled: (el as HTMLButtonElement).disabled === true,
          expanded: el.getAttribute('aria-expanded'),
          controls: el.getAttribute('aria-controls'),
        };
      })
      .catch(() => null);
    if (!info || !info.shown || info.disabled || info.expanded === 'true') continue;
    if (DANGER.test(info.name)) continue;
    const isPopup = info.popup || info.combo;
    const isDialogBtn = !isPopup && /^(create|new|add|invite|upload)\b/i.test(info.name);
    if (!isPopup && !(isDialogBtn && dialogButtons === 0)) continue;
    if (isDialogBtn) dialogButtons++;
    if (seen.has(info.name)) continue;
    seen.add(info.name);
    picked.push({ loc, name: info.name });
  }
  if (picked.length === 0) notes.push(`${pageName}: nothing to open with one tap`);
  for (const p of picked) {
    try {
      const before = await overlayOf(page).count();
      await p.loc.scrollIntoViewIfNeeded();
      await p.loc.evaluate((el) => (el as HTMLElement).click());
      await page.waitForTimeout(600);
      const ov = overlayOf(page);
      const count = await ov.count();
      if (count <= before) {
        notes.push(`${pageName}: "${p.name}" opened no overlay`);
        continue;
      }
      await testRoot(page, ov.last(), pageName, `overlay: ${p.name}`, OVERLAY_ALSO);
      await page.keyboard.press('Escape');
      await page.waitForTimeout(400);
      if (
        (await overlayOf(page).count()) > before ||
        !page.url().includes(new URL(url, ORIGIN).pathname)
      ) {
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await settle(page);
      }
    } catch (e) {
      notes.push(`${pageName}: overlay "${p.name}" failed: ${String(e).slice(0, 200)}`);
      await page
        .goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 })
        .catch(() => undefined);
      await settle(page);
    }
  }
}

async function testPage(page: Page, def: PageDef, url: string) {
  const res = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 45_000 });
  if ((res?.status() ?? 200) >= 400) notes.push(`${def.name}: HTTP ${res?.status()}`);
  const main = page.getByRole('main').first();
  await main.waitFor({ timeout: 30_000 }).catch(() => notes.push(`${def.name}: no <main>`));
  await settle(page);
  const root = (await main.count()) ? main : page.locator('body');
  await shot(page, def.name);
  if (def.chrome) {
    const dlg = page.locator('[role=dialog][aria-label]').first();
    const sidebar = dlg.locator('> div').first();
    const topline = dlg.locator('> div').nth(1).locator('> div').first();
    await testRoot(page, sidebar, def.name, 'shell: sections list');
    await testRoot(page, topline, def.name, 'shell: close');
  }
  await testRoot(page, root, def.name, 'page');
  await overlays(page, root, def.name, url);
  if (def.extra) await def.extra(page, def, root);
}

// ---- per-page extras ------------------------------------------------------------------------------------------------

async function roleExtra(page: Page, def: PageDef, root: Locator) {
  const b = root.locator('li button').first();
  if (!(await b.count())) return void notes.push(`${def.name}: no role to open`);
  await b.evaluate((el) => (el as HTMLElement).click());
  await settle(page, 3000);
  await shot(page, `${def.name}-one`);
  await testRoot(page, root, def.name, 'one role open');
}

async function channelExtra(page: Page, def: PageDef, root: Locator) {
  const b = root.locator('li button').first();
  if (!(await b.count())) return void notes.push(`${def.name}: no channel to open`);
  await b.evaluate((el) => (el as HTMLElement).click());
  await settle(page, 3000);
  await shot(page, `${def.name}-one`);
  await testRoot(page, root, def.name, 'one channel open');
}

async function membersExtra(page: Page, def: PageDef, root: Locator) {
  // Mark every control now, tick two rows, then keep only the controls that are new (the bulk bar).
  await root.locator(CONTROL_SELECTOR).evaluateAll((els) => {
    for (const e of els) e.setAttribute('data-p106-seen', '1');
  });
  const boxes = root.locator('table tbody tr input[type=checkbox]');
  const c = await boxes.count();
  if (c === 0) return void notes.push(`${def.name}: no row checkbox to tick`);
  for (let i = 0; i < Math.min(2, c); i++)
    await boxes.nth(i).evaluate((el) => (el as HTMLElement).click());
  await page.waitForTimeout(700);
  await shot(page, `${def.name}-selected`);
  await testRoot(
    page,
    root,
    def.name,
    'bulk bar (rows ticked; new controls only)',
    undefined,
    async (i) => {
      const seen = await root.locator(CONTROL_SELECTOR).nth(i).getAttribute('data-p106-seen');
      return seen === null;
    },
  );
}

async function moderationExtra(page: Page, def: PageDef, root: Locator) {
  const b = root.getByRole('button', { name: /^Open/ }).first();
  if (!(await b.count())) return void notes.push(`${def.name}: no case to open`);
  await b.evaluate((el) => (el as HTMLElement).click());
  await settle(page, 4000);
  await shot(page, `${def.name}-case`);
  await testRoot(page, root, def.name, 'one case open');
}

// ---- tools and rooms ------------------------------------------------------------------------------------------------

function makePng(w = 240, h = 120): Buffer {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b: Buffer) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const o = y * (w * 3 + 1) + 1 + x * 3;
      raw[o] = Math.round((x / w) * 255);
      raw[o + 1] = Math.round((y / h) * 255);
      raw[o + 2] = 160;
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const drawerOf = (page: Page) => page.getByRole('dialog', { name: 'Drawer', exact: true });

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawerOf(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click();
  await page.waitForTimeout(900);
}

async function closeTool(page: Page) {
  for (
    let i = 0;
    i < 3 &&
    (await drawerOf(page)
      .isVisible()
      .catch(() => false));
    i++
  ) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

interface ToolDef {
  name: string;
  query: string;
  pick: RegExp;
  room?: boolean;
  setup?: (page: Page, drawer: Locator) => Promise<void>;
}
const TOOLS: ToolDef[] = [
  { name: 'tool-calculator', query: 'calculator', pick: /^calculator/i },
  { name: 'tool-units', query: 'unit', pick: /unit converter/i },
  { name: 'tool-invite', query: 'invite card', pick: /invite card/i },
  {
    name: 'tool-tierlist',
    query: 'tier list',
    pick: /tier list/i,
    room: true,
    setup: async (_page, drawer) => {
      await drawer.getByRole('button', { name: 'Start a new one' }).click();
      const rows = drawer.getByRole('button', { name: 'Add the rows S to D' });
      await rows.waitFor({ timeout: 15_000 }).catch(() => undefined);
      if (await rows.isVisible().catch(() => false)) await rows.click();
      for (const item of ['p106 a', 'p106 b', 'p106 c']) {
        await drawer.getByLabel('Item', { exact: true }).fill(item);
        await drawer.getByRole('button', { name: 'Add item' }).click();
      }
    },
  },
  {
    name: 'tool-poll',
    query: 'poll',
    pick: /poll builder/i,
    room: true,
    setup: async (_page, drawer) => {
      await drawer.getByRole('button', { name: 'Start a new one' }).click();
    },
  },
  {
    name: 'tool-studio',
    query: 'image studio',
    pick: /image studio/i,
    setup: async (_page, drawer) => {
      await drawer
        .getByLabel('Choose an image')
        .setInputFiles({ name: 'p106.png', mimeType: 'image/png', buffer: makePng() });
      await drawer
        .getByRole('button', { name: 'Caption', exact: true })
        .waitFor({ timeout: 20_000 });
    },
  },
];

async function toolsPart(browser: Browser, owner: BrowserContext, w: World, started: number) {
  const o = owner.request;
  let roomId: string | null = null;
  try {
    const create = async () =>
      apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
        name: `p106touchc-${STAMP}`,
        type: 0,
      });
    try {
      roomId = (await create()).id;
    } catch (e) {
      if (!String(e).includes('429')) throw e;
      say('429 on channel create: one retry after 5 s');
      await new Promise((r) => setTimeout(r, 5000));
      roomId = (await create()).id;
    }
    const { ctx, page } = await newPage(browser, owner);
    page.on('pageerror', (e) => notes.push(`pageerror: ${String(e).slice(0, 200)}`));
    try {
      for (const t of TOOLS) {
        if (Date.now() - started > BUDGET_MS) {
          notCovered.push(`${t.name}: the part's time budget was used up`);
          continue;
        }
        try {
          const url = `/channels/${w.guildId}/${t.room ? roomId : w.channelId}`;
          await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
          await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
          await settle(page, 4000);
          await openTool(page, t.query, t.pick);
          const drawer = drawerOf(page);
          await expect(drawer).toBeVisible();
          await t.setup?.(page, drawer);
          await settle(page, 3000);
          await shot(page, t.name);
          await testRoot(page, drawer, t.name, 'tool drawer');
          await overlays(page, drawer, t.name, url);
          await closeTool(page);
        } catch (e) {
          notCovered.push(`${t.name}: ${String(e).slice(0, 240)}`);
          say(`NOT COVERED ${t.name}: ${String(e).slice(0, 240)}`);
          await closeTool(page).catch(() => undefined);
        }
      }

      // The watch-together strip and the listening room card in the Lounge.
      const strip = async (
        name: string,
        url: string | null,
        find: (p: Page) => Locator,
        why: string,
      ) => {
        if (!url) return void notCovered.push(`${name}: ${why}`);
        await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        await settle(page, 4000);
        const el = find(page);
        if (!(await el.count())) return void notCovered.push(`${name}: not on the page (${why})`);
        await el.first().scrollIntoViewIfNeeded();
        await shot(page, name);
        await testRoot(page, el.first(), name, 'card');
      };
      try {
        await strip(
          'room-watch-party-strip',
          `/channels/${w.guildId}/${w.channelId}`,
          (p) =>
            p
              .locator('aside[aria-label]')
              .filter({ has: p.locator('strong') })
              .filter({ hasText: /watching|paused|playing/i }),
          'the seeded party may not be in this channel',
        );
      } catch (e) {
        notCovered.push(`room-watch-party-strip: ${String(e).slice(0, 200)}`);
      }
      try {
        await strip(
          'room-listening-card',
          w.voiceChannelId ? `/channels/${w.guildId}/${w.voiceChannelId}` : null,
          (p) => p.getByTestId('room-card'),
          'the sweep world configures no 24/7 listening room (only the voice channel and a playlist)',
        );
      } catch (e) {
        notCovered.push(`room-listening-card: ${String(e).slice(0, 200)}`);
      }
    } finally {
      await ctx.close();
    }
  } finally {
    if (roomId)
      await apiDelete(o, `/channels/${roomId}`).catch((e) =>
        say(`channel ${roomId} (p106touchc-${STAMP}) not deleted: ${String(e).slice(0, 160)}`),
      );
  }
}

// ---- the test -------------------------------------------------------------------------------------------------------

test(`touchc census, part ${PART}: 44 px hit test at 375`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const started = Date.now();
  const w = world();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    expect(back.settings?.user_prefs?.theme).toBe('lounge-light');

    if (PART === 3) {
      await toolsPart(browser, ownerCtx, w, started);
    } else {
      const defs = [...SERVER, ...ADMIN].filter((d) => d.part === PART);
      const { ctx, page } = await newPage(browser, ownerCtx);
      page.on('pageerror', (e) => notes.push(`pageerror: ${String(e).slice(0, 200)}`));
      try {
        for (const def of defs) {
          const url = def.path(w);
          if (!url) {
            notCovered.push(`${def.name}: the seeded world has no such page`);
            continue;
          }
          if (Date.now() - started > BUDGET_MS) {
            notCovered.push(`${def.name}: the part's time budget was used up`);
            continue;
          }
          try {
            await testPage(page, def, url);
          } catch (e) {
            notCovered.push(`${def.name}: ${String(e).slice(0, 240)}`);
            say(`NOT COVERED ${def.name}: ${String(e).slice(0, 240)}`);
          }
          say(`elapsed ${Math.round((Date.now() - started) / 1000)} s after ${def.name}`);
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    writeFileSync(
      resolve(OUT, `figures-part${PART}.json`),
      JSON.stringify(
        { part: PART, states: statesOut, rows: rowsOut, notes, notCovered, sideways },
        null,
        1,
      ),
    );
    await ownerCtx.close();
  }
  const nr = rowsOut.filter((r) => r.status === 'not-reached');
  for (const n of notes) say(`note: ${n}`);
  for (const n of notCovered) say(`not covered: ${n}`);
  say(
    `part ${PART}: ${rowsOut.length} rows, fail ${rowsOut.filter((r) => r.status === 'fail').length}, not-reached ${nr.length}, ${Math.round((Date.now() - started) / 1000)} s`,
  );
  // A not-reached row is this spec's error (the brief): the run fails so it is read and fixed.
  expect(nr.map((r) => `${r.page} [${r.state}] ${r.name}: ${r.reason}`)).toEqual([]);
});
