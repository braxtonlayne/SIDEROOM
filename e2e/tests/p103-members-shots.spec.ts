import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { textRatio } from '../support/contrast';
import { hitTest, hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-977 and D-975 (wave 103, members): SEC-AUD-457 (Server settings > Members at 1024: the table is wider than its column,
 * so its own sideways scroll cuts the Actions column) and SEC-AUD-458 (the phone's "select all" box has no visible label).
 *
 * 457: at 1024, 900 and 1280 (Light; Dark at 1024) it reads the table wrapper's scrollWidth / clientWidth / scrollLeft,
 * every column head's box, where the Actions head starts, the Actions buttons' boxes against the wrapper's right edge, and
 * every body row's height. The large case (30 members, 32-character names, five roles each) is faked in the browser as
 * p102-settings-shots does (the five roles are made through the API under `p103-mem-role-*` names and removed in a
 * `finally`). 458: at 375 touch the head's checkbox line, its label (text, box, contrast, hit test), a tap on the label text
 * ticks every row, `getByRole('checkbox', { name })` finds exactly one box at 375 and at 1024; at 1024 the label's box is
 * 1 px and clipped (no text shows) and the head cell's box equals the base.
 *
 * Base then after, in one round on one database: `SR_P103_MEMBERS_PHASE=base` records the tree with `Members.tsx` and
 * `server-settings.css` put back to the base's text (shots get a `base-` prefix, figures go to `figures-base.json`); the
 * default `after` records the same, asserts the contract, and compares with `figures-base.json` when it is in the folder.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited).
 * It registers no account, saves nothing (selection is client state; the box is set back), and makes only the five roles.
 *
 * Local only (SR_P103_MEMBERS_ON=1). SR_P103_MEMBERS_OUT is the folder of the shots and the figures. From e2e/, on a stack
 * with the sweep's world (API 8801, Vite 5801, metrics 9501, `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN`
 * set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5801 SR_P103_MEMBERS_ON=1 \
 *   SR_P103_MEMBERS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-members \
 *   npx playwright test tests/p103-members-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * After the merge there is no base: delete `figures-base.json` first if the folder holds one from another tree.
 */
const ON = process.env.SR_P103_MEMBERS_ON === '1';
const PHASE = process.env.SR_P103_MEMBERS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_MEMBERS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-members';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SELECT_ALL = 'Select all members on this page';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
  large: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    large: true,
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    large: false,
  },
  {
    tag: '900-lounge-light',
    width: 900,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    large: true,
  },
  {
    tag: '1280-lounge-light',
    width: 1280,
    height: 800,
    touch: false,
    theme: 'lounge-light',
    large: false,
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    large: false,
  },
  {
    tag: '375-lounge-dark',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-dark',
    large: false,
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
interface Scan {
  theme: string | null;
  page: [number, number];
  wrapper: (Box & { sw: number; cw: number; sl: number }) | null;
  table: Box | null;
  /** every column head: text, box. */
  heads: { text: string; x: number; w: number }[];
  /** the Actions head's right edge against the wrapper's client right edge. */
  actionsHeadRight: number | null;
  wrapperRight: number | null;
  /** the right edge of the last Actions button of each of the first rows. */
  actionsRight: number[];
  rowHeights: number[];
  firstRow: Box | null;
  headTh: Box | null;
  headCheck: Box | null;
  label: {
    text: string;
    box: Box;
    display: string;
    color: string;
    ground: string;
    clipPath: string;
  } | null;
  names: { byRoleAll: number };
  rows: number;
}

function scan(page: Page): Promise<Scan> {
  return page.evaluate((SEL) => {
    const rd = (n: number) => Math.round(n * 10) / 10;
    const bx = (e: Element | null): Box | null => {
      if (!e) return null;
      const b = e.getBoundingClientRect();
      return { x: rd(b.x), y: rd(b.y), w: rd(b.width), h: rd(b.height) };
    };
    const main = document.querySelector('main');
    const wrapEl = main?.querySelector('.ss-table-scroll') ?? null;
    const table = main?.querySelector('table') ?? null;
    const wb = bx(wrapEl);
    const ths = Array.from(table?.querySelectorAll('thead th') ?? []);
    const heads = ths.map((th) => {
      const b = bx(th)!;
      return { text: (th.textContent ?? '').trim(), x: b.x, w: b.w };
    });
    const actionsTh = ths[ths.length - 1] ?? null;
    const rows = Array.from(table?.querySelectorAll('tbody tr') ?? []);
    const actionsRight = rows.slice(0, 3).map((r) => {
      const btns = Array.from(r.querySelectorAll('td:last-child button'));
      return rd(Math.max(...btns.map((b) => b.getBoundingClientRect().right)));
    });
    const label = main?.querySelector('thead th label') ?? null;
    let ground = 'rgba(0, 0, 0, 0)';
    for (let e: Element | null = label; e; e = e.parentElement) {
      const bg = getComputedStyle(e).backgroundColor;
      if (bg && bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {
        ground = bg;
        break;
      }
    }
    const roleAll = Array.from(main?.querySelectorAll('input[type=checkbox]') ?? []).filter((i) => {
      const input = i as HTMLInputElement;
      const name =
        input.getAttribute('aria-label') ??
        Array.from(input.labels ?? [])
          .map((l) => l.textContent ?? '')
          .join(' ');
      return name.trim() === SEL;
    }).length;
    return {
      theme:
        document.documentElement.getAttribute('data-theme') ??
        document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
        null,
      page: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      wrapper:
        wb && wrapEl
          ? { ...wb, sw: wrapEl.scrollWidth, cw: wrapEl.clientWidth, sl: wrapEl.scrollLeft }
          : null,
      table: bx(table),
      heads,
      actionsHeadRight: actionsTh ? bx(actionsTh)!.x + bx(actionsTh)!.w : null,
      wrapperRight: wrapEl ? wb!.x + wrapEl.clientWidth : null,
      actionsRight,
      rowHeights: rows.map((r) => bx(r)!.h),
      firstRow: bx(rows[0] ?? null),
      headTh: bx(main?.querySelector('table thead th:has(input[type=checkbox])') ?? null),
      headCheck: bx(main?.querySelector('table thead input[type=checkbox]') ?? null),
      label: label
        ? {
            text: (label.textContent ?? '').trim(),
            box: bx(label)!,
            display: getComputedStyle(label).display,
            color: getComputedStyle(label).color,
            ground,
            clipPath: getComputedStyle(label).clipPath,
          }
        : null,
      names: { byRoleAll: roleAll },
      rows: rows.length,
    };
  }, SELECT_ALL);
}

interface Fig {
  scan: Scan;
  role: number;
  ratio?: number;
  tap?: { before: number; after: number; rows: number };
  headPoints?: string[];
  hit?: { name: string; status: string; takers: string[] }[];
}

const SECTION = (section: string) => `/settings/server/${world().guildId}/${section}`;

async function openMembers(page: Page) {
  await page.goto(SECTION('members'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await page.waitForSelector('#ss-member-role', { timeout: 20_000 });
  await page.waitForSelector('main table tbody tr', { timeout: 20_000 });
  await idle(page, 4000);
  await page.evaluate(() => document.fonts?.ready);
}

async function run(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  large: boolean,
  roleIds: string[],
  problems: string[],
): Promise<Fig> {
  const { ctx, page } = await newPage(browser, owner, c);
  const gid = world().guildId;
  const tag = `${c.tag}${large ? '/large' : ''}`;
  try {
    if (large) {
      const members = Array.from({ length: 30 }, (_, i) => {
        const n = String(i + 1).padStart(2, '0');
        return {
          user: {
            id: `13030000000000010${n}`,
            username: `p103-mem-member-${n}-`.padEnd(32, 'x'),
            global_name: null,
          },
          nick: null,
          roles: roleIds,
          joined_at: new Date(Date.UTC(2026, 10, 1 + (i % 15))).toISOString(),
          communication_disabled_until: null,
          sr_join: { source: 'invite', invite_code: 'p103meminvite' },
          sr_signals: i % 3 === 0 ? ['new_account'] : [],
        };
      });
      await page.route(new RegExp(`/api/v1/guilds/${gid}/members(\\?.*)?$`), (route) =>
        route.fulfill({ json: members }),
      );
    }
    await openMembers(page);
    await page.evaluate(() => {
      const w = document.querySelector('main .ss-table-scroll');
      if (w) w.scrollLeft = 0;
      window.scrollTo(0, 0);
    });
    const s = await scan(page);
    const role = await page.getByRole('checkbox', { name: SELECT_ALL }).count();
    const fig: Fig = { scan: s, role };
    say(`${tag}: ${JSON.stringify(s)} role-name boxes ${role}`);
    if (s.theme !== c.theme) problems.push(`${tag}: data-theme is ${s.theme}`);
    if (role !== 1) problems.push(`${tag}: getByRole name "${SELECT_ALL}" finds ${role} boxes`);
    if (s.page[0] > s.page[1]) problems.push(`${tag}: page scrolls sideways ${s.page.join('>')}`);

    if (c.touch && !large) {
      const lab = s.label;
      if (PHASE === 'after') {
        if (!lab || lab.text !== SELECT_ALL) problems.push(`${tag}: head label text ${lab?.text}`);
        else {
          fig.ratio = textRatio(lab.color, lab.ground);
          if (fig.ratio < 4.5) problems.push(`${tag}: label contrast ${fig.ratio.toFixed(2)}`);
          if (lab.box.w < 20 || lab.box.h < 14)
            problems.push(`${tag}: label box is ${lab.box.w}x${lab.box.h}`);
          if (s.headTh && lab.box.y + lab.box.h > s.headTh.y + s.headTh.h + 0.5)
            problems.push(`${tag}: label leaves the head line`);
        }
      }
      // a tap on the text ticks every row
      const rowBoxes = page.locator('main table tbody input[type=checkbox]');
      const total = await rowBoxes.count();
      const checked = () =>
        page.evaluate(
          () => document.querySelectorAll('main table tbody input[type=checkbox]:checked').length,
        );
      const before = await checked();
      if (PHASE === 'after') {
        await page.locator('main thead th label').tap();
        await page.waitForTimeout(300);
        const after = await checked();
        fig.tap = { before, after, rows: total };
        if (after !== total)
          problems.push(`${tag}: a tap on the label ticked ${after} of ${total}`);
        await page.locator('main thead th label').tap();
        await page.waitForTimeout(300);
        if ((await checked()) !== 0) problems.push(`${tag}: second tap did not clear the rows`);
      }
      const main = page.locator('main').last();
      await openCollapsed(page, main);
      const rows: ControlRow[] = await hitTestAll(page, main);
      // The helper names a box by its aria-label, not by a <label>: the labelled head box is tested on its own too.
      const head = await hitTest(page, page.locator('main thead input[type=checkbox]'));
      fig.hit = [
        {
          name: 'head: ' + SELECT_ALL,
          status: head.pass ? 'pass' : 'fail',
          takers: head.points.filter((p) => !p.ok).map((p) => `${p.at}: ${p.taker?.path}`),
        },
        ...rows
          .filter((r) => /^select /i.test(r.name))
          .map((r) => ({
            name: r.name,
            status: r.status,
            takers: (r.result?.points ?? [])
              .filter((p) => !p.ok)
              .map((p) => `${p.at}: ${p.taker?.path}`),
          })),
      ];
      fig.headPoints = head.points.map((p) => `${p.at}:${p.relation}`);
      say(`${tag}: hit select rows ${JSON.stringify(fig.hit)}`);
      if (rows.some((r) => r.status === 'not-reached'))
        problems.push(`${tag}: a row was not reached (probe error)`);
      if (PHASE === 'after') {
        if (!fig.hit.length || !fig.hit[0]!.name.startsWith('head:'))
          problems.push(`${tag}: no head checkbox row in the hit test`);
        const bad = fig.hit.filter((r) => r.status !== 'pass');
        if (bad.length) problems.push(`${tag}: select rows not passing: ${JSON.stringify(bad)}`);
      }
      await page.evaluate(() => window.scrollTo(0, 0));
    } else if (!c.touch && PHASE === 'after' && !large) {
      // above a phone no text shows: the label is a 1 px clipped box, the name stays
      const lab = s.label;
      if (!lab)
        problems.push(`${tag}: no head label (the name must come from one label or aria-label)`);
      else if (lab.box.w > 1.5 || lab.box.h > 1.5)
        problems.push(`${tag}: head label shows ${lab.box.w}x${lab.box.h}`);
    }
    if (PHASE === 'after' && !c.touch) {
      if (s.wrapper && !large && c.width >= 1024 && s.wrapper.sw !== s.wrapper.cw)
        problems.push(`${tag}: table scrolls ${s.wrapper.sw} > ${s.wrapper.cw}`);
      if (s.actionsHeadRight !== null && s.wrapperRight !== null && !large && c.width >= 1024) {
        if (s.actionsHeadRight > s.wrapperRight + 0.5)
          problems.push(`${tag}: Actions head ends ${s.actionsHeadRight} past ${s.wrapperRight}`);
        for (const r of s.actionsRight)
          if (r > s.wrapperRight + 0.5)
            problems.push(`${tag}: Actions buttons end ${r} past ${s.wrapperRight}`);
      }
    }
    mkdirSync(OUT, { recursive: true });
    await page.evaluate(() => window.scrollTo(0, 0));
    const nm = large ? `members-${c.tag.replace('-lounge', '-large-lounge')}` : `members-${c.tag}`;
    await page.screenshot({ path: resolve(OUT, `${shotName(nm)}.png`), timeout: 10_000 });
    return fig;
  } finally {
    await ctx.close();
  }
}

function near(a: number, b: number, tol: number) {
  return Math.abs(a - b) <= tol;
}

test(`p103 members: wrap at tablet width and the phone's select-all label, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = owner.request;
  const gid = world().guildId;
  const problems: string[] = [];
  const madeRoles: string[] = [];
  const figs: Record<string, Fig> = {};
  try {
    for (let i = 1; i <= 5; i++) {
      const r = await apiPost<{ id: string }>(o, `/guilds/${gid}/roles`, {
        name: `p103-mem-role-${i}-with-a-long-name`,
      });
      madeRoles.push(r.id);
    }
    for (const c of CONFIGS) {
      await setMain(owner, c.theme);
      try {
        figs[c.tag] = await run(browser, owner, c, false, [], problems);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 400)}`);
      }
      if (c.large) {
        try {
          figs[`${c.tag}/large`] = await run(browser, owner, c, true, madeRoles, problems);
        } catch (e) {
          problems.push(`${c.tag}/large: ${String(e).slice(0, 400)}`);
        }
      }
    }
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Fig>;
      // 1280 and 375: the boxes equal the base (the 375 head line may not move the first row)
      for (const k of ['1280-lounge-light', '375-lounge-light', '375-lounge-dark']) {
        const b = base[k]?.scan;
        const a = figs[k]?.scan;
        if (!b || !a) {
          say(`${k}: no base or after figure`);
          continue;
        }
        if (b.table && a.table)
          for (const f of ['x', 'y', 'w', 'h'] as const)
            if (!near(b.table[f], a.table[f], 1))
              problems.push(`${k}: table.${f} ${b.table[f]} -> ${a.table[f]}`);
        b.rowHeights.forEach((h, i) => {
          if (!near(h, a.rowHeights[i] ?? -1, 1))
            problems.push(`${k}: row ${i} height ${h} -> ${a.rowHeights[i]}`);
        });
        if (k === '1280-lounge-light' && b.headTh && a.headTh && !near(b.headTh.w, a.headTh.w, 1))
          problems.push(`${k}: head cell ${b.headTh.w} -> ${a.headTh.w}`);
        if (b.firstRow && a.firstRow && !near(b.firstRow.y, a.firstRow.y, 1))
          problems.push(`${k}: first row moved ${b.firstRow.y} -> ${a.firstRow.y}`);
      }
      // 1024: the head checkbox's box equals the base (its cell is narrower: the padding gave)
      for (const k of ['1024-lounge-light', '1024-lounge-dark']) {
        const b = base[k]?.scan.headCheck;
        const a = figs[k]?.scan.headCheck;
        if (b && a)
          for (const f of ['w', 'h'] as const)
            if (!near(b[f], a[f], 1)) problems.push(`${k}: head checkbox ${f} ${b[f]} -> ${a[f]}`);
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
