import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 107, listshots (pod lounge B: lists + heads; D-1022, D-1023, D-1024). Measures the channel list, the Home list,
 * the user panel, the member sheet, the Friends page header, the channel header search, the DM header, the phone search
 * sheet, the switcher, the forum and thread searches and the new-post textarea, base and after.
 *
 * Parts (SR_P107_LIST_PART): 1 = phone (375 x 812 touch, Lounge Light): server list, Home list, switcher, Friends and
 * its tabs, the channel's member sheet; 2 = phone: channel header, DM, phone search sheet, forum, new post, threads
 * popover; 3 = desktop 1024 x 768 Lounge Light (boxes, hit tests at box reach, the five fields) and boxes at 1280;
 * 4 = desktop 1024 x 768 Lounge Dark (the same fields and boxes).
 * Phone census: every control through `hitTestAll` (default reach), every row whole into `figures-<phase>.json`
 * (merged by part). Desktop: the box of every named element (after must equal base) and the field reads of cure B
 * (rest border, hover, keyboard focus ring, placeholder, ratios from contrast.ts, and the contract's exact values read
 * from the page's own tokens). After compares with `figures-base.json` in OUT and prints every broken limit as a
 * `PROBLEM` line. Own data (D-942): one forum `p107list-forum-<stamp>` with one post, removed in a `finally`.
 * Needs the seeded sweep world, never seeds, imports no `/src` module.
 *
 * Switches: SR_P107_LIST_ON=1, SR_P107_LIST_PHASE=base|after, SR_P107_LIST_PART, SR_P107_LIST_OUT=<shots folder>.
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_LIST_ON=1 --env SR_P107_LIST_PHASE=after --env SR_P107_LIST_PART=1 --env SR_P107_LIST_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-listshots -- tests/p107-list-shots.spec.ts
 * (parts 2, 3 and 4: the same with --env SR_P107_LIST_PART=<n>)
 */
const ON = process.env.SR_P107_LIST_ON === '1';
const PHASE = process.env.SR_P107_LIST_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P107_LIST_PART ?? '1';
const OUT =
  process.env.SR_P107_LIST_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-listshots';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);
const r1 = (n: number) => Math.round(n * 10) / 10;

interface World {
  guildId: string;
  channelId: string;
  dmId: string | null;
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Fig {
  page: string;
  state: string;
  index: number;
  tag: string;
  role: string | null;
  name: string;
  status: string;
  reason: string | null;
  box: { x: number; y: number; width: number; height: number } | null;
  takers: { at: string; tag: string; role: string | null; name: string; path: string }[];
  takerKind: 'control' | 'container' | null;
  path: string;
}
interface FieldFig {
  id: string;
  theme: string;
  dataTheme: string | null;
  viaTab: boolean;
  focusVisible: boolean;
  box: Box | null;
  borderWidth: string;
  borderStyle: string;
  restBorder: string;
  hoverBorder: string;
  focusBorder: string;
  outline: { style: string; width: string; color: string; offset: string };
  innerOutline: string;
  phColor: string;
  phOpacity: string;
  expected: { rest: string; hover: string; ring: string; faint: string };
  ratios: { borderOutside: number; borderField: number; ring: number; placeholder: number };
}
interface PartData {
  rows: Fig[];
  boxes: Record<string, Box[]>;
  fields: FieldFig[];
  extra: Record<string, number | string>;
  notes: string[];
  covered: string[];
  scroll: string[];
}
const D: PartData = {
  rows: [],
  boxes: {},
  fields: [],
  extra: {},
  notes: [],
  covered: [],
  scroll: [],
};

const CONTROL_TAGS = new Set(['button', 'a', 'input', 'select', 'textarea', 'summary', 'label']);
const CONTROL_ROLES = new Set([
  'button',
  'link',
  'combobox',
  'checkbox',
  'switch',
  'radio',
  'tab',
  'menuitem',
  'option',
  'slider',
]);

/** Every element whose box the pod may touch; read at every page, desktop after must equal base. */
const SELS = [
  '.lng-channel',
  '.lng-category-btn',
  '.lng-category-head',
  '.lng-find',
  '.lng-nav-item',
  '.lng-member',
  '.rng-btn',
  '.rng-tray-item .lng-icon-btn',
  '.lng-user-area',
  '.lng-user-panel',
  '.lng-user-id',
  '.lng-user-avatar',
  '.lng-user-panel .lng-icon-btn',
  '.lng-split',
  '.lng-left',
  '.lng-rail',
  'nav.lng-tabbar',
  '.lng-sidebar',
  '.lng-right',
  'header.lng-header',
  'header.lng-header .lng-icon-btn',
  'header.lng-header .lng-search',
  'header.lng-header input[type=search]',
  '.lng-dm-identity',
  '.lng-friends-tabs',
  '.lng-friends-body',
  '.lng-search-sheet',
  '.lng-search-sheet .lng-search',
  '.lng-search-sheet input',
  '.lng-forum-search',
  '.lng-forum-tools',
  '.lng-threads-search',
  '.lng-post-textarea',
  '.lng-switcher-input',
];
/** Rows that grow to 44 px on a phone (A.2). */
const ROW_SELS = [
  '.lng-channel',
  '.lng-nav-item',
  '.lng-member',
  '.lng-find',
  '.rng-tray-item .lng-icon-btn',
];
const HEAD_SELS = ['.lng-category-btn', '.lng-category-head'];
const SAME375 = [
  '.lng-user-area',
  '.lng-user-panel',
  '.lng-user-id',
  '.lng-user-avatar',
  '.lng-user-panel .lng-icon-btn',
  '.lng-split',
  '.lng-rail',
  'nav.lng-tabbar',
  'header.lng-header',
  'header.lng-header .lng-icon-btn',
  'header.lng-header .lng-search',
  '.lng-dm-identity',
  '.lng-search-sheet',
  '.lng-search-sheet .lng-search',
  '.rng-btn',
];
const NAMED =
  /lng-(channel|category-btn|category-head|find|nav-item|member|user-|split|dm-identity|search-sheet|forum-search)|lng-search(?!-)|rng-btn|rng-tray/;

async function boxSnap(page: Page, label: string) {
  const got = await page.evaluate((sels) => {
    const out: Record<string, { x: number; y: number; w: number; h: number }[]> = {};
    const rnd = (n: number) => Math.round(n * 10) / 10;
    for (const s of sels) {
      const list: { x: number; y: number; w: number; h: number }[] = [];
      for (const e of Array.from(document.querySelectorAll(s)).slice(0, 80)) {
        const r = e.getBoundingClientRect();
        if (r.width <= 0 || r.height <= 0 || !e.getClientRects().length) continue;
        list.push({ x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) });
      }
      if (list.length) out[s] = list;
    }
    return out;
  }, SELS);
  for (const [s, v] of Object.entries(got)) D.boxes[`${label}|${s}`] = v;
}

async function newCtx(
  browser: Browser,
  owner: BrowserContext,
  theme: string,
  w: number,
  h: number,
  touch: boolean,
) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme, 'account theme read back').toBe(theme);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: w, height: h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((t) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: t, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function settle(page: Page, ms = 4000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}
async function go(page: Page, url: string, ready?: Locator) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  if (ready)
    await ready
      .first()
      .waitFor({ timeout: 30_000 })
      .catch(() => D.notes.push(`${url}: ready locator not seen`));
  await settle(page);
}
const dataTheme = (page: Page) =>
  page.evaluate(
    () =>
      document.documentElement.getAttribute('data-theme') ??
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null,
  );

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  D.scroll.push(`${name}: ${sw}/${cw}`);
  if (sw > cw) D.notes.push(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  await page.mouse.move(2, 2);
  await page.waitForTimeout(250);
  const suffix = /-(375|1024|1280)(-|$)/.test(name) ? '' : '';
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}${suffix}.png`), timeout: 10_000 });
}

const tap = async (loc: Locator) => {
  await loc
    .first()
    .scrollIntoViewIfNeeded({ timeout: 5000 })
    .catch(() => undefined);
  await loc.first().click({ timeout: 6000 });
};
const openMore = async (page: Page) => {
  const more = page.getByRole('button', { name: 'More actions' });
  if ((await more.count()) > 0 && (await more.first().getAttribute('aria-expanded')) === 'false')
    await more.first().click({ timeout: 5000 });
};
const main = (page: Page) => page.locator('#lounge-main');
const dialog = (page: Page) => page.getByRole('dialog').last();

/** Opens what is closed, tests every control under `root`, writes the rows. */
async function census(
  page: Page,
  pageName: string,
  state: string,
  root: Locator,
  reach?: 'box',
): Promise<void> {
  if ((await root.count()) === 0) {
    D.notes.push(`${pageName} / ${state}: root not found`);
    return;
  }
  await openCollapsed(page, root.first());
  const rows: ControlRow[] = await hitTestAll(page, root.first(), reach ? { reach } : {});
  for (const r of rows) {
    const failing = (r.result?.points ?? []).filter((p) => !p.ok);
    const taker = failing[0]?.taker ?? null;
    D.rows.push({
      page: pageName,
      state,
      index: r.index,
      tag: r.tag,
      role: r.role,
      name: r.name,
      status: r.status,
      reason: r.reason ?? null,
      box: r.result?.box ?? null,
      takers: failing.map((p) => ({
        at: p.at,
        tag: p.taker?.tag ?? '',
        role: p.taker?.role ?? null,
        name: p.taker?.name ?? '',
        path: p.taker?.path ?? '',
      })),
      takerKind: taker
        ? CONTROL_TAGS.has(taker.tag) || (taker.role !== null && CONTROL_ROLES.has(taker.role))
          ? 'control'
          : 'container'
        : null,
      path: r.path,
    });
  }
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  say(
    `${pageName} / ${state}: ${rows.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}`,
  );
  for (const r of rows.filter((x) => x.status === 'fail' || x.status === 'not-reached')) {
    const b = r.result?.box;
    const t = (r.result?.points ?? []).find((p) => !p.ok)?.taker;
    say(
      `  ${r.status.toUpperCase()} ${r.tag} "${r.name}" ${b ? `${Math.round(b.width)}x${Math.round(b.height)}` : '-'} taker ${t ? `${t.tag} "${t.name}"` : '-'} ${r.path}`,
    );
  }
}

/** One page: census, box snapshot, shot. */
async function pageStep(
  page: Page,
  name: string,
  state: string,
  root: Locator,
  shotTag: string | null,
  reach?: 'box',
) {
  try {
    await census(page, name, state, root, reach);
    await boxSnap(page, `${name}/${state}`);
    if (shotTag) await shot(page, shotTag);
    D.covered.push(`${name} / ${state}`);
  } catch (e) {
    D.notes.push(`${name} / ${state}: failed (${msg(e)})`);
  }
}

async function overlay(
  page: Page,
  name: string,
  state: string,
  open: () => Promise<void>,
  root: () => Locator,
  shotTag: string,
  reload?: string,
  reach?: 'box',
) {
  try {
    await open();
    await settle(page, 2500);
    await root().first().waitFor({ timeout: 6000 });
    await pageStep(page, name, state, root(), shotTag, reach);
  } catch (e) {
    D.notes.push(`${name} / ${state}: not opened (${msg(e)})`);
  }
  if (reload) await go(page, reload);
  else {
    await page.keyboard.press('Escape').catch(() => undefined);
    await page.waitForTimeout(400);
  }
}

async function makeForum(owner: BrowserContext, w: World): Promise<string | null> {
  try {
    const f = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p107list-forum-${STAMP}`,
      type: 15,
    });
    await sleep(1200);
    await apiPost(owner.request, `/channels/${f.id}/threads`, {
      name: `p107list-post-${STAMP}`,
      message: { content: 'A post for the list shots.' },
    });
    return f.id;
  } catch (e) {
    D.notes.push(`forum not made: ${msg(e)}`);
    return null;
  }
}

// ---- phone parts ---------------------------------------------------------------------------------------------------

async function part1(browser: Browser, owner: BrowserContext, w: World) {
  const { ctx, page } = await newCtx(browser, owner, 'lounge-light', 375, 812, true);
  try {
    const left = () => page.locator('.lng-left, nav.lng-tabbar');
    await go(page, '/channels/@me', page.locator('.lng-left'));
    await pageStep(page, 'home-list', 'page', left(), 'home-list-375');
    await overlay(
      page,
      'home-list',
      'switcher',
      async () => {
        await tap(page.locator('.lng-find'));
      },
      () => dialog(page),
      'switcher-375',
    );

    try {
      await go(page, '/channels/@me', page.locator('.lng-left'));
      await tap(
        page
          .getByRole('button', { name: /^Friends$/ })
          .or(page.getByRole('link', { name: /^Friends$/ })),
      );
      await settle(page, 2500);
      await page.locator('section.lng-friends').first().waitFor({ timeout: 8000 });
      await pageStep(page, 'friends', 'page', page.locator('section.lng-friends'), 'friends-375');
      for (const tab of ['Online', 'Pending', 'Add Friend']) {
        await overlay(
          page,
          'friends',
          `tab ${tab}`,
          async () => {
            await tap(
              page
                .getByRole('tab', { name: tab, exact: true })
                .or(page.getByRole('button', { name: tab, exact: true })),
            );
          },
          () => page.locator('section.lng-friends'),
          `friends-tab-${tab.toLowerCase().replace(/ /g, '-')}-375`,
        );
      }
    } catch (e) {
      D.notes.push(`friends: not opened (${msg(e)})`);
    }

    const curl = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, curl, page.getByRole('log'));
    await tap(page.getByRole('button', { name: 'Back to channels' }));
    await settle(page, 2500);
    await pageStep(page, 'server-list', 'page', left(), 'server-list-375');

    await go(page, curl, page.getByRole('log'));
    await overlay(
      page,
      'channel',
      'member list sheet',
      async () => {
        await openMore(page);
        await tap(page.getByRole('button', { name: 'Member List' }));
      },
      () => page.locator('aside.lng-right'),
      'member-sheet-375',
    );
  } finally {
    await ctx.close();
  }
}

async function part2(browser: Browser, owner: BrowserContext, w: World) {
  const forumId = await makeForum(owner, w);
  const { ctx, page } = await newCtx(browser, owner, 'lounge-light', 375, 812, true);
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    await pageStep(page, 'channel', 'page', main(page), 'channel-375');
    await overlay(
      page,
      'channel',
      'header more strip',
      async () => {
        await openMore(page);
      },
      () => page.locator('.lng-dm-extra'),
      'channel-more-375',
      url,
    );
    await overlay(
      page,
      'channel',
      'phone search sheet',
      async () => {
        await tap(page.getByRole('button', { name: 'Search this conversation' }));
      },
      () => page.locator('.lng-search-sheet'),
      'search-sheet-375',
      url,
    );
    await overlay(
      page,
      'channel',
      'threads popover',
      async () => {
        await openMore(page);
        await tap(page.getByRole('button', { name: 'Threads', exact: true }));
      },
      () => page.locator('.lng-threads-pop'),
      'threads-375',
      url,
    );

    if (forumId) {
      const furl = `/channels/${w.guildId}/${forumId}`;
      await go(page, furl, page.getByRole('combobox', { name: 'Sort by' }));
      await pageStep(page, 'forum', 'page', main(page), 'forum-375');
      await overlay(
        page,
        'forum',
        'new post',
        async () => {
          await tap(page.getByRole('button', { name: /new post|create post/i }));
        },
        () => dialog(page),
        'new-post-375',
      );
    }

    if (w.dmId) {
      const durl = `/channels/@me/${w.dmId}`;
      await go(page, durl);
      await tap(main(page).getByRole('button', { name: /^Accept/ }));
      await page.getByRole('log').first().waitFor({ timeout: 20_000 });
      await settle(page);
      await pageStep(page, 'dm', 'page', main(page), 'dm-375');
      await overlay(
        page,
        'dm',
        'header more strip',
        async () => {
          await openMore(page);
        },
        () => page.locator('.lng-dm-extra'),
        'dm-more-375',
        durl,
      );
    } else D.notes.push('dm: the world has no DM');
  } finally {
    await ctx.close();
    if (forumId)
      await apiDelete(owner.request, `/channels/${forumId}`).catch((e) =>
        D.notes.push(`forum ${forumId} (p107list-forum-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
}

// ---- desktop parts: fields -----------------------------------------------------------------------------------------

/** Moves focus by the keyboard to `loc`; true when Tab / Shift+Tab got there. */
async function kbFocus(page: Page, loc: Locator): Promise<boolean> {
  const is = () => loc.evaluate((e) => e === document.activeElement).catch(() => false);
  if (await is()) {
    await page.keyboard.press('Tab');
    await page.keyboard.press('Shift+Tab');
    if (await is()) return true;
  }
  for (const key of ['Tab', 'Shift+Tab']) {
    for (let i = 0; i < 160; i++) {
      await page.keyboard.press(key);
      if (await is()) return true;
    }
  }
  await loc.focus();
  return false;
}

interface Read {
  borderColor: string;
  borderWidth: string;
  borderStyle: string;
  bg: string;
  outlineStyle: string;
  outlineWidth: string;
  outlineColor: string;
  outlineOffset: string;
  stack: string[];
  expected: { rest: string; hover: string; ring: string; faint: string };
  phColor: string;
  phOpacity: string;
  focusVisible: boolean;
  box: Box | null;
}

/** What the browser computes for the box-drawing element `el` (the field or its wrapper) and the input's placeholder. */
async function readState(target: Locator, input: Locator): Promise<Read> {
  const ph = await input.evaluate((i) => {
    const cs = getComputedStyle(i, '::placeholder');
    return { color: cs.color, opacity: cs.opacity, fv: i.matches(':focus-visible') };
  });
  const r = await target.evaluate((el) => {
    const cs = getComputedStyle(el);
    const probe = document.createElement('span');
    (el.parentElement ?? document.body).appendChild(probe);
    const col = (v: string) => {
      probe.style.color = '';
      probe.style.color = v;
      return getComputedStyle(probe).color;
    };
    const expected = {
      rest: col('color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)'),
      hover: col('var(--ui-text-faint)'),
      ring: col('var(--ui-focus)'),
      faint: col('var(--ui-text-faint)'),
    };
    probe.remove();
    const stack: string[] = [];
    for (let x: Element | null = el.parentElement; x; x = x.parentElement) {
      const s = getComputedStyle(x);
      stack.unshift(s.backgroundColor + (s.backgroundImage !== 'none' ? '|image' : ''));
    }
    const b = el.getBoundingClientRect();
    return {
      borderColor: cs.borderTopColor,
      borderWidth: cs.borderTopWidth,
      borderStyle: cs.borderTopStyle,
      bg: cs.backgroundColor,
      outlineStyle: cs.outlineStyle,
      outlineWidth: cs.outlineWidth,
      outlineColor: cs.outlineColor,
      outlineOffset: cs.outlineOffset,
      stack,
      expected,
      box: {
        x: Math.round(b.left * 10) / 10,
        y: Math.round(b.top * 10) / 10,
        w: Math.round(b.width * 10) / 10,
        h: Math.round(b.height * 10) / 10,
      },
    };
  });
  return { ...r, phColor: ph.color, phOpacity: ph.opacity, focusVisible: ph.fv };
}

const WHITE: Rgba = { r: 255, g: 255, b: 255, a: 1 };
function ground(stack: string[]): Rgba {
  let g = WHITE;
  for (const s of stack) {
    const c = parseColor(s.split('|')[0]!);
    if (c.a > 0) g = over(c, g);
  }
  return g;
}
const colorsEqual = (a: string, b: string) => {
  try {
    const x = parseColor(a);
    const y = parseColor(b);
    return (
      Math.abs(x.r - y.r) <= 1.5 &&
      Math.abs(x.g - y.g) <= 1.5 &&
      Math.abs(x.b - y.b) <= 1.5 &&
      Math.abs(x.a - y.a) <= 0.02
    );
  } catch {
    return a === b;
  }
};

/**
 * Reads one field at rest, hovered, and with keyboard focus. `target` draws the box (the input, or the wrapper), `input`
 * holds the placeholder and the focus.
 */
async function readField(
  page: Page,
  id: string,
  theme: string,
  target: Locator,
  input: Locator,
  innerFocusProbe = false,
) {
  try {
    await target.first().waitFor({ timeout: 8000 });
    await page.mouse.move(2, 2);
    await page.waitForTimeout(200);
    const rest = await readState(target.first(), input.first());
    await target.first().hover({ timeout: 4000 });
    await page.waitForTimeout(250);
    const hov = await readState(target.first(), input.first());
    await page.mouse.move(2, 2);
    const viaTab = await kbFocus(page, input.first());
    await page.waitForTimeout(250);
    const foc = await readState(target.first(), input.first());
    const inner = innerFocusProbe
      ? await input.first().evaluate((i) => {
          const c = getComputedStyle(i);
          return `${c.outlineStyle} ${c.outlineWidth}`;
        })
      : '';
    const outside = ground(rest.stack);
    const fieldGround = parseColor(rest.bg).a > 0 ? over(parseColor(rest.bg), outside) : outside;
    const bc = over(parseColor(rest.borderColor), outside);
    const ring = over(parseColor(foc.outlineColor), outside);
    const phc = parseColor(rest.phColor);
    const ph = over({ ...phc, a: phc.a * Number(rest.phOpacity) }, fieldGround);
    D.fields.push({
      id,
      theme,
      dataTheme: await dataTheme(page),
      viaTab,
      focusVisible: foc.focusVisible,
      box: rest.box,
      borderWidth: rest.borderWidth,
      borderStyle: rest.borderStyle,
      restBorder: rest.borderColor,
      hoverBorder: hov.borderColor,
      focusBorder: foc.borderColor,
      outline: {
        style: foc.outlineStyle,
        width: foc.outlineWidth,
        color: foc.outlineColor,
        offset: foc.outlineOffset,
      },
      innerOutline: inner,
      phColor: rest.phColor,
      phOpacity: rest.phOpacity,
      expected: rest.expected,
      ratios: {
        borderOutside: r1(ratio(bc, outside)),
        borderField: r1(ratio(bc, fieldGround)),
        ring: r1(ratio(ring, outside)),
        placeholder: r1(ratio(ph, fieldGround)),
      },
    });
    say(
      `field ${id} ${theme}: border ${rest.borderColor} hover ${hov.borderColor} outline ${foc.outlineStyle} ${foc.outlineWidth} ${foc.outlineColor} +${foc.outlineOffset} viaTab ${viaTab} fv ${foc.focusVisible}`,
    );
  } catch (e) {
    D.notes.push(`field ${id} ${theme}: not read (${msg(e)})`);
  }
}

async function setMembers(page: Page, open: boolean) {
  const btn = page.getByRole('button', { name: /member list/i }).first();
  await btn.waitFor({ timeout: 20_000 });
  if (((await btn.getAttribute('aria-pressed')) === 'true') !== open) {
    await btn.click();
    await page.waitForTimeout(800);
  }
  await settle(page);
}

async function desktop(
  browser: Browser,
  owner: BrowserContext,
  w: World,
  theme: string,
  tag: string,
) {
  const forumId = await makeForum(owner, w);
  const { ctx, page } = await newCtx(browser, owner, theme, 1024, 768, false);
  const reach = tag === 'light' ? ('box' as const) : undefined;
  try {
    const curl = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, curl, page.getByRole('log'));
    const dt = await dataTheme(page);
    say(`data-theme ${dt}`);
    if (!dt?.includes(tag)) D.notes.push(`data-theme is ${dt}, wanted ${theme}`);

    // The header search, members closed then open.
    for (const open of [false, true]) {
      await setMembers(page, open);
      const st = open ? 'members-open' : 'members-closed';
      await boxSnap(page, `channel/${st}`);
      if (tag === 'light' && !open)
        await pageStep(page, 'channel', 'left pane', page.locator('.lng-left'), null, reach);
      if (tag === 'light' && open && (await page.locator('aside.lng-right').count()) > 0)
        await pageStep(
          page,
          'channel',
          'member list',
          page.locator('aside.lng-right'),
          null,
          reach,
        );
      const header = page.locator('header.lng-header').first();
      await readField(
        page,
        `header-search-${st}`,
        theme,
        header.locator('.lng-search').first(),
        header.locator('input[type=search]').first(),
        true,
      );
      await shot(page, open ? `channel-members-1024-${tag}` : `channel-1024-${tag}`);
      await page.keyboard.press('Escape');
      await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    }
    await setMembers(page, false);

    // Threads popover.
    try {
      await tap(page.getByRole('button', { name: 'Threads', exact: true }));
      const s = page.locator('.lng-threads-search');
      await s.first().waitFor({ timeout: 6000 });
      await boxSnap(page, 'threads');
      await readField(page, 'threads-search', theme, s, s);
      await shot(page, `threads-1024-${tag}`);
    } catch (e) {
      D.notes.push(`threads popover: not opened (${msg(e)})`);
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);

    // The switcher (Ctrl/Cmd+K).
    try {
      await go(page, curl, page.getByRole('log'));
      await expect(async () => {
        await page.keyboard.press('ControlOrMeta+k');
        await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
          timeout: 1000,
        });
      }).toPass({ timeout: 12_000 });
      await page.waitForTimeout(500);
      const s = page.locator('.lng-switcher-input');
      const openFv = await s.first().evaluate((i) => ({
        active: i === document.activeElement,
        fv: i.matches(':focus-visible'),
      }));
      D.extra['switcher-opens-focused'] = String(openFv.active);
      D.extra['switcher-opens-focus-visible'] = String(openFv.fv);
      await boxSnap(page, 'switcher');
      await readField(page, 'switcher', theme, s, s);
      await shot(page, `switcher-1024-${tag}`);
    } catch (e) {
      D.notes.push(`switcher: not opened (${msg(e)})`);
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);

    // Forum and the new-post textarea.
    if (forumId) {
      const furl = `/channels/${w.guildId}/${forumId}`;
      await go(page, furl, page.getByRole('combobox', { name: 'Sort by' }));
      await boxSnap(page, 'forum');
      const fs = page.locator('.lng-forum-search');
      await readField(page, 'forum-search', theme, fs, fs);
      await shot(page, `forum-1024-${tag}`);
      try {
        await tap(page.getByRole('button', { name: /new post|create post/i }));
        const ta = page.locator('.lng-post-textarea');
        await ta.first().waitFor({ timeout: 6000 });
        await page.waitForTimeout(900);
        await boxSnap(page, 'new-post');
        await readField(page, 'post-textarea', theme, ta, ta);
        await shot(page, `new-post-1024-${tag}`);
      } catch (e) {
        D.notes.push(`new post: not opened (${msg(e)})`);
      }
      await page.keyboard.press('Escape');
    }

    // Home list and Friends.
    await go(page, '/channels/@me', page.locator('.lng-left'));
    if (tag === 'light')
      await pageStep(
        page,
        'home-list',
        'page',
        page.locator('.lng-left'),
        `home-list-1024-${tag}`,
        reach,
      );
    else {
      await boxSnap(page, 'home-list/page');
      await shot(page, `home-list-1024-${tag}`);
    }
    try {
      await tap(
        page
          .getByRole('button', { name: /^Friends$/ })
          .or(page.getByRole('link', { name: /^Friends$/ })),
      );
      await settle(page, 2500);
      await page.locator('section.lng-friends').first().waitFor({ timeout: 8000 });
      await boxSnap(page, 'friends/page');
      await shot(page, `friends-1024-${tag}`);
    } catch (e) {
      D.notes.push(`friends: not opened (${msg(e)})`);
    }

    // DM header.
    if (w.dmId) {
      await go(page, `/channels/@me/${w.dmId}`);
      await tap(main(page).getByRole('button', { name: /^Accept/ }));
      await page.getByRole('log').first().waitFor({ timeout: 20_000 });
      await settle(page);
      await boxSnap(page, 'dm/page');
      await shot(page, `dm-1024-${tag}`);
    }

    // 1280, boxes only.
    if (tag === 'light') {
      await page.setViewportSize({ width: 1280, height: 800 });
      await go(page, curl, page.getByRole('log'));
      await boxSnap(page, 'channel-1280');
      await go(page, '/channels/@me', page.locator('.lng-left'));
      await boxSnap(page, 'home-list-1280');
    }
  } finally {
    await ctx.close();
    if (forumId)
      await apiDelete(owner.request, `/channels/${forumId}`).catch((e) =>
        D.notes.push(`forum ${forumId} (p107list-forum-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
}

// ---- comparison ----------------------------------------------------------------------------------------------------

const figFile = resolve(OUT, `figures-${PHASE}.json`);
const baseFile = resolve(OUT, 'figures-base.json');
const near = (a: number, b: number, t = 0.6) => Math.abs(a - b) <= t;
const bx = (b: Box | null | undefined) => (b ? `${b.x},${b.y} ${b.w}x${b.h}` : 'none');

function rowKey(f: Fig, seen: Map<string, number>) {
  const k = `${f.page}|${f.state}|${f.tag}|${f.name}|${f.path}`;
  const n = seen.get(k) ?? 0;
  seen.set(k, n + 1);
  return `${k}#${n}`;
}

function compare(base: PartData, after: PartData, problems: string[]) {
  const phone = PART === '1' || PART === '2';
  const info: string[] = [];
  const bRows = new Map<string, Fig>();
  const bSeen = new Map<string, number>();
  for (const f of base.rows) bRows.set(rowKey(f, bSeen), f);
  const aSeen = new Map<string, number>();
  let failAfter = 0;
  for (const f of after.rows) {
    const key = rowKey(f, aSeen);
    const b = bRows.get(key);
    if (f.status === 'not-reached')
      problems.push(`probe error (not-reached): ${key} ${f.reason ?? ''}`);
    if (f.status !== 'fail') continue;
    failAfter++;
    const t = f.takers[0];
    const tk = t ? `${t.tag}[${t.role ?? ''}] "${t.name}" (${t.path}) ${f.takerKind}` : '-';
    const named =
      NAMED.test(f.path) || (f.page === 'friends' && /lng-header|lng-back/.test(f.path));
    if (b && b.status === 'pass')
      problems.push(
        `REGRESSION (passed in base, fails after): ${key} ${f.box?.height}px high, taker ${tk}`,
      );
    else if (named && phone)
      problems.push(
        `NAMED control still failing: ${key} ${f.box?.width}x${f.box?.height}, taker ${tk}`,
      );
    else info.push(`still failing, not named: ${key} taker ${tk}`);
    if (f.takerKind === 'control' && phone)
      problems.push(`a tap area or box takes another control's point: ${key} taker ${tk}`);
  }
  const baseFails = base.rows.filter((r) => r.status === 'fail').length;
  say(
    `fails: base ${baseFails}, after ${failAfter} (part ${PART}, ${after.rows.length} rows, base ${base.rows.length})`,
  );
  D.extra['fails-base'] = baseFails;
  D.extra['fails-after'] = failAfter;

  // Boxes.
  for (const [k, aBoxes] of Object.entries(after.boxes)) {
    const bBoxes = base.boxes[k];
    const [, sel] = k.split('|') as [string, string];
    if (!bBoxes) {
      info.push(`box ${k}: no base twin`);
      continue;
    }
    if (bBoxes.length !== aBoxes.length) {
      problems.push(`count of ${k}: base ${bBoxes.length}, after ${aBoxes.length}`);
      continue;
    }
    const isRow = ROW_SELS.includes(sel);
    const isHead = HEAD_SELS.includes(sel);
    for (let i = 0; i < aBoxes.length; i++) {
      const a = aBoxes[i]!;
      const b = bBoxes[i]!;
      if (phone) {
        if (
          sel === '.rng-btn' ||
          (sel.startsWith('header.lng-header') && /^(friends|home-list)/.test(k))
        ) {
          // Rows above grow, so the ring moves down; the Friends header is cured to 44 px (its tools move 8 px down).
          if (!near(a.w, b.w) || (sel === '.rng-btn' && !near(a.h, b.h)))
            problems.push(`size changed: ${k}[${i}] base ${bx(b)} after ${bx(a)}`);
          if (sel !== '.rng-btn')
            info.push(`Friends header ${k}[${i}] base ${bx(b)} after ${bx(a)}`);
        } else if (SAME375.includes(sel)) {
          if (!near(a.x, b.x) || !near(a.y, b.y) || !near(a.w, b.w) || !near(a.h, b.h))
            problems.push(`must not change at 375: ${k}[${i}] base ${bx(b)} after ${bx(a)}`);
        } else if (isRow || isHead) {
          if (!near(a.w, b.w, 1)) problems.push(`row width changed: ${k}[${i}] ${b.w} -> ${a.w}`);
          const grow = a.h - b.h;
          const max = isHead ? 30 : 18;
          if (grow < -0.6 || grow > max)
            problems.push(
              `row height out of limit: ${k}[${i}] ${b.h} -> ${a.h} (may grow 0 to ${max})`,
            );
          if (a.h < 43.5 && sel !== '.lng-category-head')
            problems.push(`row below 44 px after: ${k}[${i}] ${a.h}`);
        } else if (sel === '.lng-friends-body') {
          if (a.y - b.y > 18 || a.y - b.y < -0.6)
            problems.push(`Friends body moved ${r1(a.y - b.y)} px (about 13 to 16 expected)`);
        } else if (sel === '.lng-friends-tabs') {
          info.push(`${k}[${i}] y ${b.y} -> ${a.y}`);
        } else if (sel === '.lng-search-sheet input') {
          info.push(`phone search input ${k}: base ${bx(b)} after ${bx(a)}`);
        } else if (!near(a.x, b.x) || !near(a.y, b.y) || !near(a.w, b.w) || !near(a.h, b.h)) {
          info.push(`moved at 375 (not a named row): ${k}[${i}] ${bx(b)} -> ${bx(a)}`);
        }
      } else if (!near(a.x, b.x) || !near(a.y, b.y) || !near(a.w, b.w) || !near(a.h, b.h)) {
        problems.push(`MOVED at desktop width: ${k}[${i}] base ${bx(b)} after ${bx(a)}`);
      }
    }
  }
  if (phone) {
    const forms = after.boxes['channel/phone search sheet|.lng-search-sheet .lng-search'];
    const inputs = after.boxes['channel/phone search sheet|.lng-search-sheet input'];
    if (forms?.[0] && inputs?.[0]) {
      if (Math.abs(forms[0].h - inputs[0].h) > 1.5)
        problems.push(
          `phone search input ${inputs[0].h} px, form ${forms[0].h} px: the input does not fill the form`,
        );
    } else if (PART === '2') info.push('phone search sheet boxes missing');
    const fs = after.boxes['forum/page|.lng-forum-search']?.[0];
    if (fs && fs.h < 43.5) problems.push(`forum search is ${fs.h} px high on a phone`);
    const ts = after.boxes['channel/threads popover|.lng-threads-search']?.[0];
    const ta = after.boxes['forum/new post|.lng-post-textarea']?.[0];
    if (ts) info.push(`thread search ${ts.h} px at 375 (heads did not cure it, "may change")`);
    if (ta) info.push(`post textarea ${ta.h} px at 375 (heads did not cure it)`);
  }

  // Fields.
  for (const f of after.fields) {
    const tag = `${f.id} ${f.theme}`;
    const b = base.fields.find((x) => x.id === f.id && x.theme === f.theme);
    if (f.dataTheme && !f.dataTheme.includes(f.theme.replace('lounge-', '')))
      problems.push(`${tag}: data-theme read ${f.dataTheme}`);
    if (b && f.box && b.box && (!near(f.box.w, b.box.w) || !near(f.box.h, b.box.h)))
      problems.push(`${tag}: field box changed ${bx(b.box)} -> ${bx(f.box)}`);
    const isHeader = f.id.startsWith('header-search');
    const sw = f.id === 'switcher';
    const P = (s: string) =>
      problems.push(
        sw ? `${tag} (inside the tools Drawer, tools.css overrides it): ${s}` : `${tag}: ${s}`,
      );
    if (!f.viaTab) info.push(`${tag}: focus was by focus(), Tab did not reach it`);
    if (!f.focusVisible) P(':focus-visible did not match with the keyboard focus');
    const ringOk =
      f.outline.style === 'solid' &&
      f.outline.width === '2px' &&
      colorsEqual(f.outline.color, f.expected.ring) &&
      f.outline.offset === '1px';
    if (!ringOk)
      P(
        `ring is ${f.outline.style} ${f.outline.width} ${f.outline.color} offset ${f.outline.offset}, wanted solid 2px ${f.expected.ring} offset 1px`,
      );
    if (f.ratios.ring < 3) P(`ring contrast ${f.ratios.ring} (3.0 or more)`);
    if (f.id === 'post-textarea') {
      info.push(`${tag}: the textarea has no placeholder attribute, placeholder not read`);
    } else if (!colorsEqual(f.phColor, f.expected.faint) || Number(f.phOpacity) !== 1)
      P(`placeholder is ${f.phColor} opacity ${f.phOpacity}, wanted ${f.expected.faint} at 1`);
    if (f.id !== 'post-textarea' && f.ratios.placeholder < 4.5)
      P(`placeholder contrast ${f.ratios.placeholder} (4.5 or more)`);
    if (isHeader) {
      if (f.borderWidth !== '0px') P(`the pill has a border (${f.borderWidth}); it keeps none`);
      if (f.innerOutline !== '' && !f.innerOutline.startsWith('none'))
        info.push(`${tag}: input's own outline ${f.innerOutline}`);
    } else {
      if (f.borderWidth !== '1px' || f.borderStyle !== 'solid')
        P(`border is ${f.borderWidth} ${f.borderStyle}`);
      if (!colorsEqual(f.restBorder, f.expected.rest))
        P(`rest border ${f.restBorder}, wanted the 65 mix ${f.expected.rest}`);
      if (!colorsEqual(f.hoverBorder, f.expected.hover))
        P(`hover border ${f.hoverBorder}, wanted ${f.expected.hover}`);
      if (f.ratios.borderOutside < 3)
        P(`rest border against the ground outside ${f.ratios.borderOutside} (3.0 or more)`);
      if (f.ratios.borderField < 3)
        P(`rest border against the field's ground ${f.ratios.borderField} (3.0 or more)`);
    }
  }
  for (const i of info) say(`INFO ${i}`);
}

function load(file: string): Record<string, PartData> | null {
  if (!existsSync(file)) return null;
  try {
    return (JSON.parse(readFileSync(file, 'utf8')) as { parts: Record<string, PartData> }).parts;
  } catch {
    return null;
  }
}

test(`wave 107 list shots: ${PHASE}, part ${PART}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    if (PART === '1') await part1(browser, ownerCtx, w);
    if (PART === '2') await part2(browser, ownerCtx, w);
    if (PART === '3') await desktop(browser, ownerCtx, w, 'lounge-light', 'light');
    if (PART === '4') await desktop(browser, ownerCtx, w, 'lounge-dark', 'dark');
  } finally {
    const prev = load(figFile) ?? {};
    prev[PART] = D;
    writeFileSync(figFile, JSON.stringify({ phase: PHASE, parts: prev }, null, 1));
    for (const n of D.notes) say(`note: ${n}`);
    await ownerCtx.close();
  }
  const problems: string[] = [];
  if (PHASE === 'after') {
    const base = load(baseFile)?.[PART];
    if (base) compare(base, D, problems);
    else say('no base figures for this part: nothing compared');
    for (const n of D.notes) if (/sideways|data-theme/.test(n)) problems.push(n);
  }
  for (const p of problems) say(`PROBLEM ${p}`);
  say(
    `part ${PART} ${PHASE}: ${D.rows.length} rows, ${D.fields.length} fields, ${problems.length} problem lines`,
  );
});
