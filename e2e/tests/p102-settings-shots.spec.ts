import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-964 (wave 102, settings): SEC-AUD-451 (Server settings > Members at 1024 sits under the sidebar), SEC-AUD-450 (the
 * Command permissions rule rows squeeze the names to 0), SEC-AUD-452 (the Members "Select all" checkbox fails the hit
 * test at 375).
 *
 * 451 confirm first: step 1 opens Members by navigation only (no scroll call of any kind) and reads, for `main` and every
 * ancestor up to `html`, scrollLeft / scrollWidth / clientWidth / overflow-x, the sidebar's box, the content's left edge, the
 * filter row's and the table's widths. Step 2 repeats it after the calls `p101-chan-shots.spec.ts` makes before its
 * `members-1024` shot, in its order (`openCollapsed` over `main`, `hitTestAll` over `main`, then
 * `#ss-member-role.scrollIntoView({block:'start'})`). Step 3 is the large case (30 members, 32-character names, five roles
 * each: the members list is faked in the browser, the five roles are made through the API under `p102-set-role-*` names and
 * removed in a `finally`) and repeats step 1 and 2's reads. 450: every rule row of the permissions dialog (the bot, its
 * commands and its rules are faked in the browser as `p101-mod-shots.spec.ts` does). 452: at 375 touch the hit test over
 * `main`; the rows whose name starts "Select" are reported with their takers.
 *
 * Base then after, in one round on one database: `SR_P102_SETTINGS_PHASE=base` records the tree with the three files put
 * back to the base's text (shots get a `base-` prefix, figures go to `figures-base.json`); the default `after` records the
 * same, asserts the contract, and when `figures-base.json` is in the folder compares the 375 boxes with it.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited).
 * It registers no account. Data of its own (D-942): five roles `p102-set-role-<n>`, removed in a `finally`; it saves nothing.
 *
 * Local only (SR_P102_SETTINGS_ON=1). SR_P102_SETTINGS_OUT is the folder of the shots and the figures. From e2e/, on a
 * stack with the sweep's world (API 8781, Vite 5781, metrics 9481, `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and
 * `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5781 SR_P102_SETTINGS_ON=1 \
 *   SR_P102_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-settings \
 *   npx playwright test tests/p102-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * After the merge there is no base: delete `figures-base.json` first if the folder holds one from another tree.
 */
const ON = process.env.SR_P102_SETTINGS_ON === '1';
const PHASE = process.env.SR_P102_SETTINGS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_SETTINGS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-settings';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const APP_ID = '1302000000000000001';
const BOT_NAME = 'p102-set-bot';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
  /** The chan spec's hit test over `main` too (slow): 1024 and 375 only. */
  hit: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    hit: true,
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    hit: true,
  },
  {
    tag: '900-lounge-light',
    width: 900,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    hit: false,
  },
  {
    tag: '1280-lounge-light',
    width: 1280,
    height: 800,
    touch: false,
    theme: 'lounge-light',
    hit: false,
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    hit: true,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(400);
};

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
interface ChainRow {
  el: string;
  sl: number;
  sw: number;
  cw: number;
  ox: string;
}
interface Scan {
  theme: string | null;
  chain: ChainRow[];
  sidebar: Box | null;
  main: Box | null;
  heading: Box | null;
  search: Box | null;
  toolbar: (Box & { sw: number; cw: number }) | null;
  table: (Box & { sw: number; cw: number }) | null;
  wrapper: (Box & { sw: number; cw: number; sl: number }) | null;
  nameTh: Box | null;
  filters: Record<string, Box | null>;
  firstRow: Box | null;
  headCheck: Box | null;
  headTh: Box | null;
  rowCheck: Box | null;
  rows: number;
  /** ancestors (not the table's own wrapper) with scrollWidth > clientWidth, or a scrollLeft other than 0. */
  over: string[];
  moved: string[];
  /** The heading or the search field starts left of the sidebar's end. */
  covered: string[];
}

function scan(page: Page): Promise<Scan> {
  return page.evaluate(() => {
    const rd = (n: number) => Math.round(n * 10) / 10;
    const bx = (e: Element | null): Box | null => {
      if (!e) return null;
      const b = e.getBoundingClientRect();
      return { x: rd(b.x), y: rd(b.y), w: rd(b.width), h: rd(b.height) };
    };
    const main = document.querySelector('main');
    const label = (e: Element) =>
      `${e.tagName.toLowerCase()}${
        typeof e.className === 'string' && e.className
          ? `.${e.className
              .trim()
              .split(/\s+/)[0]!
              .replace(/_[a-z0-9]{4,}$/i, '')}`
          : ''
      }`;
    const chain: ChainRow[] = [];
    for (let e: Element | null = main; e; e = e.parentElement) {
      chain.push({
        el: label(e),
        sl: e.scrollLeft,
        sw: e.scrollWidth,
        cw: e.clientWidth,
        ox: getComputedStyle(e).overflowX,
      });
    }
    let sidebar: Element | null = null;
    const rootEl = main?.parentElement?.parentElement ?? null;
    const nav = rootEl?.querySelector('nav') ?? null;
    for (let e: Element | null = nav; e && e.parentElement; e = e.parentElement) {
      if (e.parentElement === rootEl) {
        sidebar = e;
        break;
      }
    }
    const withScroll = (e: Element | null) => {
      const b = bx(e);
      return b ? { ...b, sw: e!.scrollWidth, cw: e!.clientWidth } : null;
    };
    const wrapEl = document.querySelector('main .ss-table-scroll');
    const wb = bx(wrapEl);
    const filters: Record<string, Box | null> = {};
    for (const id of ['ss-member-role', 'ss-member-after', 'ss-member-before', 'ss-member-sort'])
      filters[id] = bx(document.getElementById(id)?.closest('div') ?? null);
    const sb = bx(sidebar);
    const heading = bx(main?.querySelector('.ss-title') ?? null);
    const search = bx(main?.querySelector('input[type=search]') ?? null);
    const over: string[] = [];
    const moved: string[] = [];
    for (const r of chain) {
      if (r.sw > r.cw) over.push(`${r.el} ${r.sw}>${r.cw}`);
      if (r.sl !== 0) moved.push(`${r.el} scrollLeft ${r.sl}`);
    }
    const covered: string[] = [];
    // Above a phone the sidebar sits beside the content; on a phone it is a bar above it.
    const edge = sb && window.innerWidth > 720 ? sb.x + sb.w : 0;
    if (heading && heading.x < edge - 0.5) covered.push(`heading ${heading.x} < ${edge}`);
    if (search && search.x < edge - 0.5) covered.push(`search ${search.x} < ${edge}`);
    return {
      theme:
        document.documentElement.getAttribute('data-theme') ??
        document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
        null,
      chain,
      sidebar: sb,
      main: bx(main),
      heading,
      search,
      toolbar: withScroll(main?.querySelector('.ss-toolbar') ?? null),
      table: withScroll(main?.querySelector('table') ?? null),
      wrapper:
        wb && wrapEl
          ? { ...wb, sw: wrapEl.scrollWidth, cw: wrapEl.clientWidth, sl: wrapEl.scrollLeft }
          : null,
      nameTh: bx(main?.querySelector('table thead th:nth-child(2)') ?? null),
      filters,
      firstRow: bx(main?.querySelector('table tbody tr') ?? null),
      headCheck: bx(main?.querySelector('table thead input[type=checkbox]') ?? null),
      headTh: bx(main?.querySelector('table thead th:has(input[type=checkbox])') ?? null),
      rowCheck: bx(main?.querySelector('table tbody input[type=checkbox]') ?? null),
      rows: main?.querySelectorAll('table tbody tr').length ?? 0,
      over: over.filter((s) => !s.startsWith('div.ss-table-scroll')),
      moved: moved.filter((s) => !s.startsWith('div.ss-table-scroll')),
      covered,
    };
  });
}

interface MembersFig {
  nav: Scan;
  scrolled?: Scan;
  hit?: { pass: number; fail: number; notReached: number; skipped: number };
  selectRows?: { name: string; status: string; takers: string[] }[];
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

const SECTION = (section: string) => `/settings/server/${world().guildId}/${section}`;

async function openMembers(page: Page) {
  await page.goto(SECTION('members'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await page.waitForSelector('#ss-member-role', { timeout: 20_000 });
  await page.waitForSelector('main table tbody tr', { timeout: 20_000 });
  await idle(page, 4000);
  await page.evaluate(() => document.fonts?.ready);
}

function judge(tag: string, s: Scan, problems: string[], what: string) {
  if (s.over.length)
    problems.push(`${tag} ${what}: scrollWidth > clientWidth: ${s.over.join('; ')}`);
  if (s.moved.length) problems.push(`${tag} ${what}: scrolled sideways: ${s.moved.join('; ')}`);
  if (s.covered.length) problems.push(`${tag} ${what}: under the sidebar: ${s.covered.join('; ')}`);
}

/** The calls the chan spec makes before its members-1024 shot, in its order. */
async function chanCalls(page: Page, hit: boolean) {
  const main = page.locator('main').last();
  let rows: ControlRow[] = [];
  if (hit) {
    await openCollapsed(page, main);
    rows = await hitTestAll(page, main);
  }
  await page.evaluate(() =>
    document.getElementById('ss-member-role')?.scrollIntoView({ block: 'start' }),
  );
  await page.waitForTimeout(500);
  return rows;
}

async function membersRun(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  large: boolean,
  roleIds: string[],
  problems: string[],
): Promise<MembersFig> {
  const { ctx, page } = await newPage(browser, owner, c);
  const gid = world().guildId;
  const tag = `${c.tag}${large ? '/large' : ''}`;
  try {
    if (large) {
      const members = Array.from({ length: 30 }, (_, i) => {
        const n = String(i + 1).padStart(2, '0');
        return {
          user: {
            id: `13020000000000010${n}`,
            username: `p102-set-member-${n}-`.padEnd(32, 'x'),
            global_name: null,
          },
          nick: null,
          roles: roleIds,
          joined_at: new Date(Date.UTC(2026, 10, 1 + (i % 15))).toISOString(),
          communication_disabled_until: null,
          sr_join: { source: 'invite', invite_code: 'p102setinvite' },
          sr_signals: i % 3 === 0 ? ['new_account'] : [],
        };
      });
      await page.route(new RegExp(`/api/v1/guilds/${gid}/members(\\?.*)?$`), (route) =>
        route.fulfill({ json: members }),
      );
    }
    await openMembers(page);
    const fig: MembersFig = { nav: await scan(page) };
    say(`${tag} step 1 (navigation only): ${JSON.stringify(fig.nav)}`);
    if (fig.nav.theme !== c.theme) problems.push(`${tag}: data-theme is ${fig.nav.theme}`);
    mkdirSync(OUT, { recursive: true });
    const nm = large ? `members-${c.tag}-large` : `members-${c.tag}`;
    if (c.tag === '1024-lounge-light' || large)
      await page.screenshot({
        path: resolve(OUT, `${shotName(`${nm}-nav`)}.png`),
        timeout: 10_000,
      });
    if (PHASE === 'after') judge(tag, fig.nav, problems, 'step 1');

    const rows = await chanCalls(page, c.hit);
    fig.scrolled = await scan(page);
    say(`${tag} step 2 (the chan spec's calls): ${JSON.stringify(fig.scrolled)}`);
    if (c.hit) {
      fig.hit = count(rows);
      fig.selectRows = rows
        .filter((r) => /^select/i.test(r.name))
        .slice(0, 3)
        .map((r) => ({
          name: r.name,
          status: r.status,
          takers: (r.result?.points ?? [])
            .filter((p) => !p.ok)
            .map((p) => `${p.at}: ${p.taker?.path}`),
        }));
      say(`${tag}: hit ${JSON.stringify(fig.hit)} select rows ${JSON.stringify(fig.selectRows)}`);
      if (fig.hit.notReached)
        problems.push(`${tag}: ${fig.hit.notReached} not reached (probe error)`);
      if (PHASE === 'after' && c.touch) {
        const bad = rows.filter((r) => r.status === 'fail' && /^select/i.test(r.name));
        if (bad.length)
          problems.push(`${tag}: select checkboxes fail: ${bad.map((r) => r.name).join(', ')}`);
        if (!rows.some((r) => /^select all/i.test(r.name) && r.status === 'pass'))
          problems.push(`${tag}: "Select all" did not pass`);
      }
    }
    if (PHASE === 'after') judge(tag, fig.scrolled, problems, 'step 2');
    // As the chan spec leaves it (its hit test scrolls the table's own wrapper to the last control) ...
    if (c.tag === '1024-lounge-light' && !large)
      await page.screenshot({
        path: resolve(OUT, `${shotName('members-1024-scrolled')}.png`),
        timeout: 10_000,
      });
    // ... and the table's own scroll put back to its start: the shot of the page itself.
    await page.evaluate(() => {
      const w = document.querySelector('main .ss-table-scroll');
      if (w) w.scrollLeft = 0;
    });
    await page.screenshot({ path: resolve(OUT, `${shotName(nm)}.png`), timeout: 10_000 });
    return fig;
  } finally {
    await ctx.close();
  }
}

// ---- the permissions dialog (the bot is faked in the browser, as p101-mod-shots does) ----

async function fakeBot(page: Page, guildId: string, channelId: string) {
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/integrations$`), (route) =>
    route.fulfill({
      json: [
        {
          id: APP_ID,
          name: BOT_NAME,
          type: 'discord',
          enabled: true,
          application: {
            id: APP_ID,
            name: BOT_NAME,
            icon: null,
            description: '',
            bot: { id: '1302000000000000002', username: BOT_NAME },
          },
          user: { id: '1302000000000000003', username: 'p102-set-owner' },
          scopes: ['bot'],
        },
      ],
    }),
  );
  await page.route(
    new RegExp(`/api/v1/applications/${APP_ID}/guilds/${guildId}/commands(\\?.*)?$`),
    (route) =>
      route.fulfill({
        json: [
          { id: '1302000000000000011', name: 'p102-set-ping' },
          { id: '1302000000000000012', name: 'p102-set-roll' },
        ],
      }),
  );
  await page.route(
    new RegExp(`/api/v1/applications/${APP_ID}/guilds/${guildId}/commands/permissions$`),
    (route) =>
      route.fulfill({
        json: [
          {
            id: APP_ID,
            application_id: APP_ID,
            guild_id: guildId,
            permissions: [
              { id: guildId, type: 1, permission: true },
              { id: channelId, type: 3, permission: false },
            ],
          },
        ],
      }),
  );
}

interface RuleFig {
  rows: {
    name: Box & { sw: number; cw: number; text: string };
    meta: (Box & { sw: number; cw: number; text: string }) | null;
    select: Box;
    remove: Box | null;
    li: Box;
  }[];
  dialog: Box | null;
  scroll: number[];
}

async function permissionsRun(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  channelId: string,
  problems: string[],
): Promise<RuleFig> {
  const { ctx, page } = await newPage(browser, owner, c);
  try {
    const gid = world().guildId;
    await fakeBot(page, gid, channelId);
    await page.goto(SECTION('integrations'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
    await page.getByRole('button', { name: `Command permissions of ${BOT_NAME}` }).click();
    await page.waitForSelector('#cmdperm-target');
    await page.waitForSelector('[role=dialog] select[aria-label^="Rule for"]');
    await page.waitForTimeout(800);
    const fig: RuleFig = await page.evaluate(() => {
      const rd = (n: number) => Math.round(n * 10) / 10;
      const bx = (e: Element) => {
        const b = e.getBoundingClientRect();
        return { x: rd(b.x), y: rd(b.y), w: rd(b.width), h: rd(b.height) };
      };
      const dlg = Array.from(document.querySelectorAll('[role=dialog]')).pop() ?? null;
      const lis = Array.from(dlg?.querySelectorAll('ul.ss-list > li') ?? []);
      return {
        dialog: dlg ? bx(dlg) : null,
        scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
        rows: lis.map((li) => {
          const nm = li.querySelector('.ss-name')!;
          const mt = li.querySelector('.ss-meta');
          const rm = li.querySelector('button');
          return {
            li: bx(li),
            name: {
              ...bx(nm),
              sw: nm.scrollWidth,
              cw: nm.clientWidth,
              text: (nm.textContent ?? '').trim(),
            },
            meta: mt
              ? {
                  ...bx(mt),
                  sw: mt.scrollWidth,
                  cw: mt.clientWidth,
                  text: (mt.textContent ?? '').trim(),
                }
              : null,
            select: bx(li.querySelector('select')!),
            remove: rm ? bx(rm) : null,
          };
        }),
      };
    });
    say(`${c.tag} permissions: ${JSON.stringify(fig)}`);
    mkdirSync(OUT, { recursive: true });
    await page.screenshot({
      path: resolve(OUT, `${shotName(`permissions-${c.tag}`)}.png`),
      timeout: 10_000,
    });
    if (PHASE === 'after' && !c.touch) {
      if (fig.rows.length < 3) problems.push(`${c.tag}: ${fig.rows.length} rule rows, wanted 3`);
      for (const r of fig.rows) {
        if (r.name.sw > r.name.cw)
          problems.push(`${c.tag}: name "${r.name.text}" cut ${r.name.sw}>${r.name.cw}`);
        if (r.name.w < 60) problems.push(`${c.tag}: name "${r.name.text}" is ${r.name.w} px wide`);
        if (r.meta && r.meta.sw > r.meta.cw) problems.push(`${c.tag}: meta "${r.meta.text}" cut`);
        if (r.select.w < 160) problems.push(`${c.tag}: select ${r.select.w} px wide, wanted 160`);
      }
    }
    if (fig.scroll[0]! > fig.scroll[1]!)
      problems.push(`${c.tag}: permissions: page scrolls sideways`);
    return fig;
  } finally {
    await ctx.close();
  }
}

function near(a: number, b: number, tol: number) {
  return Math.abs(a - b) <= tol;
}

test(`p102 settings: Members and Command permissions, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = owner.request;
  const gid = world().guildId;
  const problems: string[] = [];
  const madeRoles: string[] = [];
  const figs: Record<string, unknown> = {};
  try {
    for (let i = 1; i <= 5; i++) {
      const r = await apiPost<{ id: string }>(o, `/guilds/${gid}/roles`, {
        name: `p102-set-role-${i}-with-a-long-name`,
      });
      madeRoles.push(r.id);
    }
    const chans = (await (await o.get(`/api/v1/guilds/${gid}/channels`)).json()) as {
      id: string;
      type: number;
    }[];
    const channelId = chans.find((c) => c.type === 0)!.id;
    for (const c of CONFIGS) {
      await setMain(owner, c.theme);
      try {
        figs[`members/${c.tag}`] = await membersRun(browser, owner, c, false, [], problems);
      } catch (e) {
        problems.push(`members/${c.tag}: ${String(e).slice(0, 400)}`);
      }
      if (c.tag === '1024-lounge-light' || c.tag === '375-lounge-light') {
        try {
          figs[`members/${c.tag}/large`] = await membersRun(
            browser,
            owner,
            c,
            true,
            madeRoles,
            problems,
          );
        } catch (e) {
          problems.push(`members/${c.tag}/large: ${String(e).slice(0, 400)}`);
        }
        try {
          figs[`permissions/${c.tag}`] = await permissionsRun(
            browser,
            owner,
            c,
            channelId,
            problems,
          );
        } catch (e) {
          problems.push(`permissions/${c.tag}: ${String(e).slice(0, 400)}`);
        }
      }
      if (c.tag === '1280-lounge-light') {
        try {
          figs[`permissions/${c.tag}`] = await permissionsRun(
            browser,
            owner,
            c,
            channelId,
            problems,
          );
        } catch (e) {
          problems.push(`permissions/${c.tag}: ${String(e).slice(0, 400)}`);
        }
      }
    }
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, any>;
      const a = figs as Record<string, any>;
      // 375: the toolbar and the first row against the base (the first row may move by the head checkbox's growth).
      const b375 = base['members/375-lounge-light']?.nav as Scan | undefined;
      const a375 = a['members/375-lounge-light']?.nav as Scan | undefined;
      if (b375 && a375) {
        const growth = a375.firstRow!.y - b375.firstRow!.y;
        const headH = a375.headTh?.h ?? 0;
        say(
          `375: toolbar base ${JSON.stringify(b375.toolbar)} after ${JSON.stringify(a375.toolbar)}`,
        );
        say(`375: first row moved ${growth} px (the head checkbox is ${headH} px tall)`);
        if (
          !near(a375.toolbar!.y, b375.toolbar!.y, 1) ||
          !near(a375.toolbar!.h, b375.toolbar!.h, 1)
        )
          problems.push('375: the toolbar moved');
        if (growth > headH + 1)
          problems.push(`375: first row moved ${growth} px, the head is ${headH}`);
        if (!near(a375.firstRow!.x, b375.firstRow!.x, 1))
          problems.push('375: first row moved sideways');
      }
      const bp = base['permissions/375-lounge-light'] as RuleFig | undefined;
      const ap = a['permissions/375-lounge-light'] as RuleFig | undefined;
      if (bp && ap) {
        bp.rows.forEach((r, i) => {
          const x = ap.rows[i];
          if (!x) return problems.push(`375 permissions: row ${i} missing`);
          for (const k of ['name', 'select', 'li'] as const)
            for (const f of ['x', 'y', 'w', 'h'] as const)
              if (!near(r[k][f], x[k][f], 1))
                problems.push(`375 permissions: row ${i} ${k}.${f} ${r[k][f]} -> ${x[k][f]}`);
        });
      }
    } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  } finally {
    for (const id of madeRoles)
      await apiDelete(o, `guilds/${gid}/roles/${id}`).catch(() => undefined);
    await owner.close();
  }
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
});
