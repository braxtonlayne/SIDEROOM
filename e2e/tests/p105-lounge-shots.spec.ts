import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiDelete } from '../support/world';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 105, pod lounge (D-1006 forum toolbar targets, D-1007 channel title before topic, D-1008 phone search sheet).
 *  - forum: 375 touch Light, 1024 Light and Dark: the toolbar's boxes (New Post, search, Sort, the two view buttons),
 *    its height and line count, the first tag chip and first post, scroll width, hit test over the toolbar.
 *  - title: 1024 and 1280 Light, members list open and closed, in a text channel of my own with a long topic; 375; the DM
 *    header, the Friends page and Message Requests titles at 1024.
 *  - sheet: the phone search sheet at 375 Light and Dark after a real search (chips row, each chip and how much of it is
 *    inside every ancestor, the count row and its Sort, the list, the hit test); the desktop search panel at 1024 Light.
 * Own data (D-942), named `p105l ...`, removed (and the topic of nothing else touched) in a `finally`: a forum channel
 * with a tag and two posts, a text channel with a long topic, a text channel with ten messages that hold one word.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`), never edited, never seeded here. It imports no `/src` module.
 *
 * Switches: SR_P105_LOUNGE_ON=1, SR_P105_LOUNGE_PHASE=base|after, SR_P105_LOUNGE_OUT=<shots folder>. Base shots have the
 * `base-` prefix and the figures go to `figures-<phase>.json`; after compares when `figures-base.json` is in OUT.
 * The commander's command:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_LOUNGE_ON=1 --env SR_P105_LOUNGE_PHASE=after --env SR_P105_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-lounge -- tests/p105-lounge-shots.spec.ts
 */
const ON = process.env.SR_P105_LOUNGE_ON === '1';
const PHASE = process.env.SR_P105_LOUNGE_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);
const WORD = `p105lq${STAMP}`;
const LONG_TOPIC =
  'p105l topic: a long line of text that goes on and on so that the header has more topic than room, ' +
  'with a second sentence about nothing in particular, to be cut by the header and never fit in one line.';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as {
    guildId: string;
    channelId: string;
    dmId: string;
  };

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
}
const L1024: Cfg = {
  tag: '1024-light',
  width: 1024,
  height: 768,
  touch: false,
  theme: 'lounge-light',
};
const D1024: Cfg = { ...L1024, tag: '1024-dark', theme: 'lounge-dark' };
const L1280: Cfg = { ...L1024, tag: '1280-light', width: 1280, height: 800 };
const L375: Cfg = { tag: '375-light', width: 375, height: 812, touch: true, theme: 'lounge-light' };
const D375: Cfg = { ...L375, tag: '375-dark', theme: 'lounge-dark' };

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
const near = (a: number, b: number, tol = 0.6) => Math.abs(a - b) <= tol;
const sameBox = (a: Box | null, b: Box | null, skipY = false) =>
  a === b ||
  (!!a && !!b && near(a.x, b.x) && (skipY || near(a.y, b.y)) && near(a.w, b.w) && near(a.h, b.h));
const bx = (b: Box | null) => (b ? `${b.x},${b.y} ${b.w}x${b.h}` : 'none');

/** One retry after a short wait on a 429 (the brief's rule). */
async function mk<T>(f: () => Promise<T>, what: string): Promise<T> {
  try {
    return await f();
  } catch (e) {
    if (!String(e).includes('429')) throw e;
    say(`429 on ${what}: one retry after 4 s`);
    await sleep(4000);
    return f();
  }
}

async function newCtx(browser: Browser, owner: BrowserContext, c: Cfg) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: c.theme, zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(c.theme);
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    ([theme, zoom]) => {
      try {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom } }, version: 2 }),
        );
      } catch {
        /* no storage */
      }
    },
    [c.theme, 100] as const,
  );
  return ctx;
}

const goto = (page: Page, url: string) =>
  page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};
const up = (loc: Locator, xpath: string) => loc.locator(`xpath=${xpath}`);

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

async function pageState(page: Page) {
  return page.evaluate(() => ({
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
  }));
}

async function hit(page: Page, root: Locator, reach: 'box' | undefined) {
  await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root, reach ? { reach } : {});
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  return {
    hit: { pass: n('pass'), fail: n('fail'), notReached: n('not-reached'), skipped: n('skipped') },
    hitFails: rows
      .filter((r) => r.status === 'fail')
      .map((r) => ({
        control: `${r.name || r.tag} (${r.path})`,
        takers: (r.result?.points ?? []).flatMap((p) => (p.taker ? [JSON.stringify(p.taker)] : [])),
      })),
  };
}

// ------------------------------------------------------------------------------------------------ data
interface Ids {
  guildId: string;
  forum: string;
  forumTitles: string[];
  titleChan: string;
  searchChan: string;
  all: string[];
}

async function makeData(octx: BrowserContext, ids: Ids) {
  const o = octx.request;
  const w = world();
  ids.guildId = w.guildId;
  const chan = async (name: string, type: number) => {
    const c = await mk(
      () => apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, { name, type }),
      `create ${name}`,
    );
    ids.all.push(c.id);
    return c.id;
  };
  ids.forum = await chan(`p105l-forum-${STAMP}`, 15);
  try {
    await apiPatch(o, `/channels/${ids.forum}`, { available_tags: [{ name: `p105l-tag` }] });
  } catch (e) {
    say(`no forum tag made: ${String(e).slice(0, 160)}`);
  }
  const post = async (name: string) =>
    mk(
      () =>
        apiPost(o, `/channels/${ids.forum}/threads`, {
          name,
          message: { content: `${name} body` },
        }),
      'forum post',
    );
  ids.forumTitles = [`p105l-post-one-${STAMP}`, `p105l-post-two-${STAMP}`];
  await post(ids.forumTitles[0]!);
  await sleep(1200);
  await post(ids.forumTitles[1]!);
  ids.titleChan = await chan(`p105l-title-${STAMP}`, 0);
  await apiPatch(o, `/channels/${ids.titleChan}`, { topic: LONG_TOPIC });
  ids.searchChan = await chan(`p105l-search-${STAMP}`, 0);
  for (let i = 1; i <= 10; i++) {
    await mk(
      () => apiPost(o, `/channels/${ids.searchChan}/messages`, { content: `${WORD} line m${i}` }),
      'message',
    );
    await sleep(700);
  }
}

async function removeData(octx: BrowserContext, ids: Ids) {
  for (const id of ids.all)
    await apiDelete(octx.request, `/channels/${id}`).catch((e) =>
      say(`channel ${id} (p105l-*-${STAMP}) not deleted: ${String(e).slice(0, 160)}`),
    );
}

// ------------------------------------------------------------------------------------------------ forum
const forumSort = (page: Page) => page.getByRole('combobox', { name: 'Sort by' });

interface ForumFig {
  theme: string | null;
  scroll: number[];
  tools: Box | null;
  newPost: Box | null;
  search: (Box & { fontSize: string }) | null;
  sort: Box | null;
  sortFont: string | null;
  views: Box[];
  firstTag: Box | null;
  firstPost: Box | null;
  rows: number;
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { control: string; takers: string[] }[];
}

async function forumScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
): Promise<ForumFig> {
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await goto(page, `/channels/${ids.guildId}/${ids.forum}`);
    await forumSort(page).waitFor({ timeout: 40_000 });
    await page.getByText(ids.forumTitles[0]!).first().waitFor({ timeout: 20_000 });
    await settled(page);
    const tools = up(forumSort(page), 'ancestor::label/..');
    const m = await tools.evaluate((root) => {
      const rnd = (n: number) => Math.round(n * 10) / 10;
      const box = (e: Element) => {
        const r = e.getBoundingClientRect();
        return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
      };
      const s = root.querySelector<HTMLInputElement>('input[type="search"]');
      const sel = root.querySelector('select');
      const newPost = root.querySelector('button:not([aria-pressed])');
      const views = Array.from(root.querySelectorAll<HTMLElement>('button[aria-pressed]')).map(box);
      const group = root.nextElementSibling;
      const chip = group?.getAttribute('role') === 'group' ? group.querySelector('button') : null;
      const post = document.querySelector('main li, [role="main"] li');
      const tops = new Set<number>();
      root.querySelectorAll('button, input, select').forEach((e) => {
        const r = e.getBoundingClientRect();
        if (e.getClientRects().length) tops.add(Math.round((r.top + r.bottom) / 2 / 12));
      });
      return {
        tools: box(root),
        newPost: newPost ? box(newPost) : null,
        search: s ? { ...box(s), fontSize: getComputedStyle(s).fontSize } : null,
        sort: sel ? box(sel) : null,
        sortFont: sel ? getComputedStyle(sel).fontSize : null,
        views,
        firstTag: chip ? box(chip) : null,
        firstPost: post ? box(post) : null,
        rows: tops.size,
      };
    });
    const h = await hit(page, tools, c.touch ? undefined : 'box');
    const st = await pageState(page);
    await shot(page, `forum-${c.tag}`);
    return { ...st, ...m, ...h };
  } finally {
    await ctx.close();
  }
}

// ------------------------------------------------------------------------------------------------ title
interface HeaderFig {
  theme: string | null;
  scroll: number[];
  header: Box | null;
  title: (Box & { sw: number; cw: number; text: string }) | null;
  topic: (Box & { sw: number; cw: number }) | null;
  sep: Box | null;
  tools: Box | null;
  toolBoxes: { name: string; box: Box }[];
  members: boolean | null;
}

/** The boxes of the header that holds the page's first level-1 heading. */
async function headerFig(page: Page, heading: Locator): Promise<HeaderFig> {
  const st = await pageState(page);
  const m = await heading.first().evaluate((h) => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
    };
    const header = h.closest('header') ?? h.parentElement!;
    const hh = h as HTMLElement;
    const topic = Array.from(header.querySelectorAll<HTMLElement>(':scope > button')).find(
      (b) => b.title && b.title.startsWith('p105l topic'),
    );
    const sep = header.querySelector(':scope > span[aria-hidden="true"]');
    const tools = Array.from(header.children).find(
      (e) => e.tagName === 'DIV' && e.querySelector('button'),
    );
    const name = (e: Element) =>
      e.getAttribute('aria-label') ??
      (e as HTMLInputElement).placeholder ??
      (e.textContent ?? '').trim().slice(0, 30);
    const toolBoxes = tools
      ? Array.from(tools.querySelectorAll('button, input')).flatMap((e) =>
          e.getClientRects().length ? [{ name: name(e), box: box(e) }] : [],
        )
      : [];
    const mem = header.querySelector('button[aria-pressed][aria-label]');
    return {
      header: box(header),
      title: {
        ...box(hh),
        sw: hh.scrollWidth,
        cw: hh.clientWidth,
        text: (hh.textContent ?? '').trim(),
      },
      topic: topic ? { ...box(topic), sw: topic.scrollWidth, cw: topic.clientWidth } : null,
      sep: sep ? box(sep) : null,
      tools: tools ? box(tools) : null,
      toolBoxes,
      members: mem ? mem.getAttribute('aria-pressed') === 'true' : null,
    };
  });
  return { ...st, ...m };
}

async function setMembers(page: Page, open: boolean) {
  const btn = page.getByRole('button', { name: /member list/i }).first();
  await btn.waitFor({ timeout: 20_000 });
  if (((await btn.getAttribute('aria-pressed')) === 'true') !== open) {
    await btn.click();
    await page.waitForTimeout(800);
  }
  await settled(page);
}

async function titleScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
  figs: Record<string, HeaderFig>,
  members: boolean[],
) {
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await goto(page, `/channels/${ids.guildId}/${ids.titleChan}`);
    const h1 = page.getByRole('heading', { level: 1, name: `p105l-title-${STAMP}` });
    await h1.waitFor({ timeout: 40_000 });
    for (const open of members) {
      if (!c.touch) await setMembers(page, open);
      else await settled(page);
      const f = await headerFig(page, h1);
      const key = c.touch ? `title ${c.tag}` : `title ${c.tag} members-${open ? 'open' : 'closed'}`;
      figs[key] = f;
      await shot(
        page,
        c.touch
          ? `title-${c.tag}`
          : `title-${c.tag.replace('-light', '')}-members-${open ? 'open' : 'closed'}-light`,
      );
    }
  } finally {
    await ctx.close();
  }
}

async function otherHeaders(
  browser: Browser,
  owner: BrowserContext,
  figs: Record<string, HeaderFig>,
) {
  const w = world();
  const ctx = await newCtx(browser, owner, L1024);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    const grab = async (key: string, name: RegExp | string) => {
      const h = page.getByRole('heading', { level: 1, name }).first();
      await h.waitFor({ timeout: 30_000 });
      await settled(page);
      figs[key] = await headerFig(page, h);
    };
    try {
      await goto(page, `/channels/@me/${w.dmId}`);
      await page.getByRole('heading', { level: 1 }).first().waitFor({ timeout: 30_000 });
      await settled(page);
      figs['dm 1024'] = await headerFig(page, page.getByRole('heading', { level: 1 }));
    } catch (e) {
      say(`dm header not measured: ${String(e).slice(0, 200)}`);
    }
    try {
      await goto(page, '/channels/@me');
      await grab('friends 1024', 'Friends');
      await page.getByText('Message Requests', { exact: true }).first().click({ timeout: 10_000 });
      await grab('requests 1024', 'Message Requests');
    } catch (e) {
      say(`friends / requests not measured: ${String(e).slice(0, 200)}`);
    }
  } finally {
    await ctx.close();
  }
}

// ------------------------------------------------------------------------------------------------ sheet
interface Anc {
  el: string;
  box: Box;
  overflowY: string;
}
interface ChipFig {
  name: string;
  box: Box;
  /** The part of the chip's height inside every clipping ancestor, 0..1. */
  visible: number;
}
interface Scrolled {
  scrollTop: number;
  scrollHeight: number;
  clientHeight: number;
  chips: ChipFig[];
  chipsRow: Box | null;
}
interface SheetFig {
  scrolled?: Scrolled;
  theme: string | null;
  scroll: number[];
  dialog: Box | null;
  head: Box | null;
  searchBox: Box | null;
  chipsRow: Box | null;
  chips: ChipFig[];
  ancestors: Anc[];
  count: Box | null;
  sort: Box | null;
  list: Box | null;
  countText: string;
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { control: string; takers: string[] }[];
}

async function runSearch(page: Page): Promise<Locator> {
  const sb = page.getByRole('searchbox').first();
  await sb.waitFor({ timeout: 40_000 });
  const panel = page.getByRole('region', { name: 'Search results' });
  let count = '';
  for (let i = 0; i < 12; i++) {
    await sb.fill(WORD);
    await sb.press('Enter');
    await panel.getByRole('combobox', { name: 'Sort by' }).waitFor({ timeout: 25_000 });
    count = ((await panel.locator('.lng-search-count').first().textContent()) ?? '').trim();
    if (/^10 /.test(count)) break;
    await page.waitForTimeout(1000);
  }
  expect(count, 'the index holds the ten messages').toMatch(/^10 /);
  await page.waitForTimeout(900);
  return panel;
}

async function sheetFigures(panel: Locator, page: Page, phone: boolean) {
  return panel
    .evaluate((p, isPhone) => {
      const rnd = (n: number) => Math.round(n * 10) / 10;
      const rect = (e: Element) => e.getBoundingClientRect();
      const box = (e: Element) => {
        const r = rect(e);
        return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
      };
      const wrap = p.parentElement!;
      const dialog = isPhone ? p.closest('[role="dialog"]') : null;
      const chipsRow = p.querySelector('[role="group"]');
      const chains: Element[] = [];
      for (let e: Element | null = chipsRow; e; e = e.parentElement) {
        chains.push(e);
        if (e === (dialog ?? wrap)) break;
      }
      const ancestors = chains.map((e) => ({
        el:
          e.tagName.toLowerCase() +
          (typeof e.className === 'string' && e.className ? `.${e.className.split(/\s+/)[0]}` : ''),
        box: box(e),
        overflowY: getComputedStyle(e).overflowY,
      }));
      const chips = chipsRow
        ? Array.from(chipsRow.querySelectorAll('button')).map((b) => {
            const r = rect(b);
            let top = r.top;
            let bottom = r.bottom;
            for (let e: Element | null = b.parentElement; e; e = e.parentElement) {
              const cs = getComputedStyle(e);
              if (cs.overflowY !== 'visible') {
                const er = rect(e);
                top = Math.max(top, er.top);
                bottom = Math.min(bottom, er.bottom);
              }
              if (e === (dialog ?? wrap)) break;
            }
            return {
              name: (b.textContent ?? '').trim(),
              box: box(b),
              visible: r.height ? rnd(Math.max(0, bottom - top) / r.height) : 0,
            };
          })
        : [];
      const sel = p.querySelector('select');
      const count = p.querySelector('.lng-search-count');
      const hitEl = p.querySelector('.lng-search-hit');
      const list = hitEl?.parentElement ?? null;
      const form = (dialog ?? wrap).querySelector('form[role="search"]');
      const head = dialog ? (dialog.firstElementChild as Element | null) : null;
      return {
        dialog: dialog ? box(dialog) : null,
        head: head ? box(head) : null,
        searchBox: form ? box(form) : null,
        chipsRow: chipsRow ? box(chipsRow) : null,
        chips,
        ancestors,
        count: count ? box(count) : null,
        sort: sel ? box(sel) : null,
        list: list ? box(list) : null,
        countText: (count?.textContent ?? '').trim(),
      };
    }, phone)
    .then(async (m) => ({ ...(await pageState(page)), ...m }));
}

async function sheetScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
): Promise<SheetFig> {
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await goto(page, `/channels/${ids.guildId}/${ids.searchChan}`);
    if (c.touch)
      await page
        .getByRole('button', { name: 'Search this conversation' })
        .click({ timeout: 40_000 });
    const panel = await runSearch(page);
    const m = await sheetFigures(panel, page, c.touch);
    const root = c.touch
      ? page.getByRole('dialog').filter({ has: page.getByRole('searchbox') })
      : panel;
    const h = c.touch
      ? await hit(page, root, undefined)
      : {
          hit: { pass: 0, fail: 0, notReached: 0, skipped: 0 },
          hitFails: [] as SheetFig['hitFails'],
        };
    await shot(page, c.touch ? `sheet-${c.tag}` : 'search-panel-1024-light');
    let scrolled: Scrolled | undefined;
    if (c.touch) {
      // the panel is the sheet's scroller: scroll it to its end and see where the chips row is (p103's sliver)
      const pos = await panel.evaluate((p) => {
        p.scrollTop = p.scrollHeight;
        return {
          scrollTop: p.scrollTop,
          scrollHeight: p.scrollHeight,
          clientHeight: p.clientHeight,
        };
      });
      await page.waitForTimeout(300);
      const m2 = await sheetFigures(panel, page, true);
      scrolled = { ...pos, chips: m2.chips, chipsRow: m2.chipsRow };
      await shot(page, `sheet-${c.tag}-scrolled`);
    }
    return { ...m, ...h, ...(scrolled ? { scrolled } : {}) } as SheetFig;
  } finally {
    await ctx.close();
  }
}

// ------------------------------------------------------------------------------------------------ the test
test(`p105 lounge: forum toolbar, channel title and search sheet, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const ids: Ids = {
    guildId: '',
    forum: '',
    forumTitles: [],
    titleChan: '',
    searchChan: '',
    all: [],
  };
  const figs: {
    forum: Record<string, ForumFig>;
    title: Record<string, HeaderFig>;
    sheet: Record<string, SheetFig>;
  } = { forum: {}, title: {}, sheet: {} };
  const run = async (what: string, f: () => Promise<void>) => {
    try {
      await f();
    } catch (e) {
      problems.push(`${what}: ${String(e).split('\n')[0]!.slice(0, 400)} (SPEC/PROBE ERROR)`);
    }
  };
  try {
    await makeData(owner, ids);
    for (const c of [L375, L1024, D1024])
      await run(`forum ${c.tag}`, async () => {
        figs.forum[c.tag] = await forumScenario(browser, owner, ids, c);
      });
    for (const c of [L1024, L1280])
      await run(`title ${c.tag}`, () =>
        titleScenario(browser, owner, ids, c, figs.title, [true, false]),
      );
    await run('title 375', () => titleScenario(browser, owner, ids, L375, figs.title, [false]));
    await run('other headers', () => otherHeaders(browser, owner, figs.title));
    for (const c of [L375, D375, L1024])
      await run(`sheet ${c.tag}`, async () => {
        figs.sheet[c.tag] = await sheetScenario(browser, owner, ids, c);
      });
  } finally {
    await removeData(owner, ids);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  for (const [k, f] of Object.entries(figs.forum)) {
    if (f.scroll[0]! > f.scroll[1]!) problems.push(`forum ${k}: page scrolls sideways ${f.scroll}`);
    if (f.hit.notReached) problems.push(`forum ${k}: ${f.hit.notReached} hit rows not reached`);
  }
  for (const [k, f] of Object.entries(figs.sheet)) {
    if (f.scroll[0]! > f.scroll[1]!) problems.push(`sheet ${k}: page scrolls sideways ${f.scroll}`);
    if (f.hit.notReached) problems.push(`sheet ${k}: ${f.hit.notReached} hit rows not reached`);
  }
  for (const [k, f] of Object.entries(figs.title))
    if (f.scroll[0]! > f.scroll[1]!) problems.push(`title ${k}: page scrolls sideways ${f.scroll}`);

  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as typeof figs;
    compareForum(base.forum, figs.forum, problems);
    compareTitle(base.title, figs.title, problems);
    compareSheet(base.sheet, figs.sheet, problems);
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  for (const [k, f] of Object.entries(figs.forum)) say(`forum ${k} ${JSON.stringify(f)}`);
  for (const [k, f] of Object.entries(figs.title)) say(`title ${k} ${JSON.stringify(f)}`);
  for (const [k, f] of Object.entries(figs.sheet)) say(`sheet ${k} ${JSON.stringify(f)}`);
  await owner.close();
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
});

// ------------------------------------------------------------------------------------------------ the limits
function compareForum(
  base: Record<string, ForumFig>,
  now: Record<string, ForumFig>,
  problems: string[],
) {
  for (const [k, a] of Object.entries(now)) {
    const b = base[k];
    if (!b) continue;
    const t = `forum ${k}`;
    if (k.startsWith('375')) {
      if (a.search && a.search.h < 43.5) problems.push(`${t}: search ${a.search.h} high, want 44`);
      a.views.forEach((v, i) => {
        if (v.w < 43.5 || v.h < 43.5) problems.push(`${t}: view button ${i} is ${v.w}x${v.h}`);
      });
      if (a.hit.fail > 0)
        problems.push(`${t}: hit test ${a.hit.fail} fails after: ${JSON.stringify(a.hitFails)}`);
      if (a.sort && b.sort && !(near(a.sort.w, b.sort.w) && near(a.sort.h, b.sort.h)))
        problems.push(`${t}: Sort ${bx(a.sort)} against base ${bx(b.sort)}`);
      if (
        a.newPost &&
        b.newPost &&
        !(near(a.newPost.w, b.newPost.w) && near(a.newPost.h, b.newPost.h))
      )
        problems.push(`${t}: New Post ${bx(a.newPost)} against base ${bx(b.newPost)}`);
      if (a.search && b.search && a.search.fontSize !== b.search.fontSize)
        problems.push(`${t}: search font ${a.search.fontSize} against ${b.search.fontSize}`);
      if (a.sortFont !== b.sortFont)
        problems.push(`${t}: Sort font ${a.sortFont} against ${b.sortFont}`);
    } else {
      const pairs: [string, Box | null, Box | null][] = [
        ['tools', a.tools, b.tools],
        ['New Post', a.newPost, b.newPost],
        ['search', a.search, b.search],
        ['Sort', a.sort, b.sort],
        ['first tag', a.firstTag, b.firstTag],
        ['first post', a.firstPost, b.firstPost],
      ];
      for (const [n, x, y] of pairs)
        if (!sameBox(x, y)) problems.push(`${t}: ${n} ${bx(x)} against base ${bx(y)}`);
      a.views.forEach((v, i) => {
        if (!sameBox(v, b.views[i] ?? null))
          problems.push(`${t}: view ${i} ${bx(v)} against base ${bx(b.views[i] ?? null)}`);
      });
      if (a.rows !== b.rows) problems.push(`${t}: ${a.rows} toolbar rows, base ${b.rows}`);
      if (a.hit.fail > b.hit.fail)
        problems.push(`${t}: hit fails ${a.hit.fail}, base ${b.hit.fail}`);
    }
  }
}

function compareTitle(
  base: Record<string, HeaderFig>,
  now: Record<string, HeaderFig>,
  problems: string[],
) {
  for (const [k, a] of Object.entries(now)) {
    const b = base[k];
    if (!b) continue;
    const t = `title ${k}`;
    const sameHeight = !!a.header && !!b.header && near(a.header.h, b.header.h);
    if (!sameHeight) problems.push(`${t}: header ${bx(a.header)} against base ${bx(b.header)}`);
    const sameTools =
      a.toolBoxes.length === b.toolBoxes.length &&
      a.toolBoxes.every((x, i) => sameBox(x.box, b.toolBoxes[i]!.box, false));
    const isChannelPage = k.startsWith('title ');
    if (isChannelPage && !k.endsWith('375')) {
      // tools: size always equal; position equal (they never shrink)
      if (!sameTools) problems.push(`${t}: a tool box moved or changed`);
      const cutBase = !!b.title && b.title.sw > b.title.cw + 0.5;
      // room the topic can give: the title's own box, the topic's box, and the separator and two gaps (about 17)
      const room = (b.title?.cw ?? 0) + (b.topic?.w ?? 0) + (b.topic ? 17 : 0);
      const topicRoom = !!b.title && room >= b.title.sw;
      if (cutBase && topicRoom && a.title && a.title.sw > a.title.cw + 0.5)
        problems.push(
          `${t}: title still cut (${a.title.sw} in ${a.title.cw}) although the base topic had ${b.topic?.w} px`,
        );
      if (cutBase && !topicRoom && a.title && a.title.sw > a.title.cw + 0.5)
        problems.push(
          `${t}: title cut (${a.title.sw} in ${a.title.cw}) and the base topic was already ${b.topic?.w ?? 'absent'} wide: the cause is the tools (${bx(a.tools)})`,
        );
      if (a.title && b.title && a.title.w < b.title.w - 0.5)
        problems.push(`${t}: title narrower, ${a.title.w} against base ${b.title.w}`);
      if (a.topic && b.topic && a.topic.w > b.topic.w + 0.5)
        problems.push(`${t}: topic wider, ${a.topic.w} against base ${b.topic.w}`);
      const bothFit =
        !!b.title && b.title.sw <= b.title.cw + 0.5 && !!b.topic && b.topic.sw <= b.topic.cw + 0.5;
      if (bothFit && !(sameBox(a.title, b.title) && sameBox(a.topic, b.topic)))
        problems.push(
          `${t}: both fitted in the base but moved: title ${bx(a.title)} topic ${bx(a.topic)}`,
        );
    } else {
      // 375, DM, Friends, Message Requests: nothing may move
      if (!sameBox(a.title, b.title))
        problems.push(`${t}: title ${bx(a.title)} against base ${bx(b.title)}`);
      if (!sameBox(a.topic, b.topic))
        problems.push(`${t}: topic ${bx(a.topic)} against base ${bx(b.topic)}`);
      if (!sameBox(a.header, b.header)) problems.push(`${t}: header box moved`);
      if (!sameTools) problems.push(`${t}: a tool box moved`);
    }
  }
}

function compareSheet(
  base: Record<string, SheetFig>,
  now: Record<string, SheetFig>,
  problems: string[],
) {
  for (const [k, a] of Object.entries(now)) {
    const b = base[k];
    if (!b) continue;
    const t = `sheet ${k}`;
    if (k.startsWith('375')) {
      if (!a.chips.length) problems.push(`${t}: no chips found`);
      for (const c of a.scrolled?.chips ?? [])
        if (c.visible < 0.99)
          problems.push(
            `${t}: scrolled to the end, chip "${c.name}" ${c.visible * 100}% inside its ancestors (panel top ${a.scrolled?.scrollTop} of ${a.scrolled?.scrollHeight})`,
          );
      for (const c of a.chips) {
        if (c.visible < 0.99)
          problems.push(`${t}: chip "${c.name}" ${c.visible * 100}% inside its ancestors`);
        if (c.box.h < 43.5) problems.push(`${t}: chip "${c.name}" is ${c.box.h} high`);
      }
      if (!sameBox(a.head, b.head))
        problems.push(`${t}: head ${bx(a.head)} against base ${bx(b.head)}`);
      if (!sameBox(a.searchBox, b.searchBox))
        problems.push(`${t}: search box ${bx(a.searchBox)} against base ${bx(b.searchBox)}`);
      for (const [n, x, y] of [
        ['count row', a.count, b.count],
        ['Sort', a.sort, b.sort],
      ] as const)
        if (x && y && !(near(x.w, y.w) && near(x.h, y.h)))
          problems.push(`${t}: ${n} ${bx(x)} against base ${bx(y)}`);
      if (a.hit.fail > b.hit.fail)
        problems.push(`${t}: hit fails ${a.hit.fail}, base ${b.hit.fail}`);
      const chipFail = a.hitFails.filter((f) => /chip/.test(f.control));
      if (chipFail.length) problems.push(`${t}: chip hit fails ${JSON.stringify(chipFail)}`);
    } else {
      const pairs: [string, Box | null, Box | null][] = [
        ['search box', a.searchBox, b.searchBox],
        ['chips row', a.chipsRow, b.chipsRow],
        ['count', a.count, b.count],
        ['Sort', a.sort, b.sort],
        ['list', a.list, b.list],
      ];
      for (const [n, x, y] of pairs)
        if (!sameBox(x, y)) problems.push(`${t}: ${n} ${bx(x)} against base ${bx(y)}`);
      a.chips.forEach((c, i) => {
        if (!sameBox(c.box, b.chips[i]?.box ?? null))
          problems.push(
            `${t}: chip ${c.name} ${bx(c.box)} against base ${bx(b.chips[i]?.box ?? null)}`,
          );
      });
    }
  }
}
