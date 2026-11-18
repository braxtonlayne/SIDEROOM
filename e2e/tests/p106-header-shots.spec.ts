import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiDelete } from '../support/world';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 106, builder header (D-1014, SEC-AUD-495): when a channel's header is short of room the search field folds to its
 * icon; focusing it opens it to full width over the title.
 *  - headers: a text channel and a forum channel of my own, each with a long topic, Lounge Light, at 1280, 1024 and 900
 *    with the member list open and closed; the DM header at 1024; the text channel at 375 (touch). Per header: its box,
 *    the title's box and scrollWidth, the topic's box, the tools', every tool's and the search form's and input's, the
 *    focusable controls of the header in DOM (Tab) order.
 *  - focus: at 1024 with the list open: the field focused (focus(), as Tab arrives), its form box, the search panel's box,
 *    the shot; a real click on the field; typing a word (the panel opens); Escape (the form's box and the focused element).
 *  - hit test over the header at 1024 (box reach) and 375 (touch reach).
 * Own data (D-942), named `p106h ...`, removed in a `finally`. NEEDS THE SWEEP'S SEEDED WORLD, never seeded here. It
 * imports no `/src` module. No shortcut that focuses the header search exists in the app today: focus is tested by
 * focus(), a click and typing.
 *
 * Switches: SR_P106_HEADER_ON=1, SR_P106_HEADER_PHASE=base|after, SR_P106_HEADER_OUT=<shots folder>. Base shots have the
 * `base-` prefix; figures go to `figures-<phase>.json`; after compares when `figures-base.json` is in OUT.
 * The commander's command:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_HEADER_ON=1 --env SR_P106_HEADER_PHASE=after --env SR_P106_HEADER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-header -- tests/p106-header-shots.spec.ts
 */
const ON = process.env.SR_P106_HEADER_ON === '1';
const PHASE = process.env.SR_P106_HEADER_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P106_HEADER_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-header';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = Date.now().toString(36);
const LONG_TOPIC =
  'p106h topic: a long line of text that goes on and on so that the header has more topic than room, ' +
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
}
const C1280: Cfg = { tag: '1280', width: 1280, height: 800, touch: false };
const C1024: Cfg = { tag: '1024', width: 1024, height: 768, touch: false };
const C900: Cfg = { tag: '900', width: 900, height: 768, touch: false };
const C375: Cfg = { tag: '375', width: 375, height: 812, touch: true };

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
const near = (a: number, b: number, tol = 0.6) => Math.abs(a - b) <= tol;
const sameBox = (a: Box | null | undefined, b: Box | null | undefined) =>
  a === b || (!!a && !!b && near(a.x, b.x) && near(a.y, b.y) && near(a.w, b.w) && near(a.h, b.h));
const bx = (b: Box | null | undefined) => (b ? `${b.x},${b.y} ${b.w}x${b.h}` : 'none');

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
    settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe('lounge-light');
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  return ctx;
}

const goto = (page: Page, url: string) =>
  page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

async function hit(page: Page, root: Locator, reach: 'box' | undefined) {
  await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root, reach ? { reach } : {});
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  return {
    pass: n('pass'),
    fail: n('fail'),
    notReached: n('not-reached'),
    fails: rows
      .filter((r) => r.status === 'fail')
      .map((r) => ({
        control: `${r.name || r.tag} (${r.path})`,
        takers: (r.result?.points ?? []).flatMap((p) => (p.taker ? [JSON.stringify(p.taker)] : [])),
      })),
  };
}

interface Ids {
  guildId: string;
  textChan: string;
  forumChan: string;
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
    await apiPatch(o, `/channels/${c.id}`, { topic: LONG_TOPIC });
    return c.id;
  };
  ids.textChan = await chan(`p106h-text-${STAMP}`, 0);
  ids.forumChan = await chan(`p106h-forum-${STAMP}`, 15);
}

async function removeData(octx: BrowserContext, ids: Ids) {
  for (const id of ids.all)
    await apiDelete(octx.request, `/channels/${id}`).catch((e) =>
      say(`channel ${id} (p106h-*-${STAMP}) not deleted: ${String(e).slice(0, 160)}`),
    );
}

interface HeaderFig {
  scroll: number[];
  header: Box | null;
  title: (Box & { sw: number; cw: number; text: string }) | null;
  topic: (Box & { sw: number; cw: number }) | null;
  tools: Box | null;
  toolBoxes: { name: string; box: Box }[];
  search: (Box & { opacity: string }) | null;
  input: Box | null;
  tabOrder: string[];
  members: boolean | null;
}

async function headerFig(page: Page, heading: Locator): Promise<HeaderFig> {
  const scroll = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  const m = await heading.first().evaluate((h) => {
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: rnd(r.left), y: rnd(r.top), w: rnd(r.width), h: rnd(r.height) };
    };
    const header = h.closest('header') ?? h.parentElement!;
    const hh = h as HTMLElement;
    const topic = Array.from(header.querySelectorAll<HTMLElement>(':scope > button')).find(
      (b) => b.title && b.title.startsWith('p106h topic'),
    );
    const tools = Array.from(header.children).find(
      (e) => e.tagName === 'DIV' && e.querySelector('button'),
    );
    const name = (e: Element) =>
      e.getAttribute('aria-label') ??
      (e as HTMLInputElement).placeholder ??
      (e.textContent ?? '').trim().slice(0, 30);
    const toolBoxes = tools
      ? Array.from(tools.querySelectorAll('button')).flatMap((e) =>
          e.getClientRects().length ? [{ name: name(e), box: box(e) }] : [],
        )
      : [];
    const form = header.querySelector<HTMLElement>('[role="search"]');
    const input = header.querySelector<HTMLInputElement>('input[type="search"]');
    const tabOrder = Array.from(
      header.querySelectorAll<HTMLElement>('a[href], button, input, select, [tabindex]'),
    )
      .filter((e) => e.getClientRects().length && !e.hasAttribute('disabled'))
      .map((e) => `${e.tagName.toLowerCase()}:${name(e)}`);
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
      tools: tools ? box(tools) : null,
      toolBoxes,
      search: form ? { ...box(form), opacity: getComputedStyle(form).opacity } : null,
      input: input ? box(input) : null,
      tabOrder,
      members: mem ? mem.getAttribute('aria-pressed') === 'true' : null,
    };
  });
  return { scroll, ...m };
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

interface Focus {
  form: Box | null;
  panel: Box | null;
  title: Box | null;
  clickForm: Box | null;
  clickFocused: boolean;
  typedPanel: boolean;
  afterEscapeForm: Box | null;
  afterEscapeActive: string;
  scroll: number[];
}

const box = (loc: Locator) => loc.boundingBox().then((b) => (b ? rb(b) : null));
const rb = (b: { x: number; y: number; width: number; height: number }): Box => ({
  x: Math.round(b.x * 10) / 10,
  y: Math.round(b.y * 10) / 10,
  w: Math.round(b.width * 10) / 10,
  h: Math.round(b.height * 10) / 10,
});

async function focusScenario(page: Page, h1: Locator, shotTag: string): Promise<Focus> {
  const header = page.locator('header.lng-header').first();
  const form = header.locator('[role="search"]');
  const input = header.getByRole('searchbox', { name: /^Search/ });
  const panel = page.getByRole('region', { name: 'Search results' });
  // Tab arrival: focus().
  await input.focus();
  await page.waitForTimeout(500);
  const f1 = await box(form);
  const p1 = await box(panel);
  const t1 = await box(h1);
  await shot(page, `${shotTag}-search-focused`);
  const scroll = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  // Escape.
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);
  const afterEscapeForm = await box(form);
  const afterEscapeActive = await page.evaluate(
    () => document.activeElement?.tagName.toLowerCase() ?? '',
  );
  // A real click on the field (folded or not).
  await page.mouse.click(0, 0);
  await page.waitForTimeout(300);
  const ib = await box(input);
  let clickFocused = false;
  let clickForm: Box | null = null;
  if (ib) {
    await page.mouse.click(ib.x + ib.w / 2, ib.y + ib.h / 2);
    await page.waitForTimeout(500);
    clickFocused = await input.evaluate((e) => e === document.activeElement);
    clickForm = await box(form);
  }
  // Typing.
  await input.fill('p106h');
  await page.waitForTimeout(400);
  const typedPanel = (await panel.count()) > 0;
  await page.keyboard.press('Escape');
  await input.evaluate((e) => (e as HTMLInputElement).blur());
  await page.mouse.click(0, 0);
  return {
    form: f1,
    panel: p1,
    title: t1,
    clickForm,
    clickFocused,
    typedPanel,
    afterEscapeForm,
    afterEscapeActive,
    scroll,
  };
}

interface Figs {
  headers: Record<string, HeaderFig>;
  focus: Record<string, Focus>;
  hits: Record<string, Awaited<ReturnType<typeof hit>>>;
}

async function channelScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  figs: Figs,
  kind: 'text' | 'forum',
  c: Cfg,
  members: boolean[],
  opts: { focus?: boolean; hit?: boolean; shots?: Record<string, string> } = {},
) {
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    const id = kind === 'text' ? ids.textChan : ids.forumChan;
    await goto(page, `/channels/${ids.guildId}/${id}`);
    const h1 = page.getByRole('heading', { level: 1, name: `p106h-${kind}-${STAMP}` });
    await h1.waitFor({ timeout: 40_000 }).catch(async (e) => {
      await shot(page, `probe-${kind}-${c.tag}`);
      throw e;
    });
    for (const open of members) {
      if (!c.touch) await setMembers(page, open);
      else await settled(page);
      const key = c.touch ? `${kind} ${c.tag}` : `${kind} ${c.tag} ${open ? 'open' : 'closed'}`;
      figs.headers[key] = await headerFig(page, h1);
      const sn = opts.shots?.[key];
      if (sn) await shot(page, sn);
      if (opts.hit && open === members[0]) {
        figs.hits[key] = await hit(
          page,
          page.locator('header.lng-header').first(),
          c.touch ? undefined : 'box',
        );
      }
      if (opts.focus && !c.touch && open === members[0])
        figs.focus[key] = await focusScenario(
          page,
          h1,
          `header-${c.tag}-members-${open ? 'open' : 'closed'}`,
        );
    }
  } finally {
    await ctx.close();
  }
}

async function dmScenario(browser: Browser, owner: BrowserContext, figs: Figs) {
  const w = world();
  const ctx = await newCtx(browser, owner, C1024);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await goto(page, `/channels/@me/${w.dmId}`);
    const h = page.getByRole('heading', { level: 1 }).first();
    await h.waitFor({ timeout: 30_000 });
    await settled(page);
    figs.headers['dm 1024'] = await headerFig(page, h);
  } finally {
    await ctx.close();
  }
}

test(`p106 header: search folds when the header is short of room, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const ids: Ids = { guildId: '', textChan: '', forumChan: '', all: [] };
  const figs: Figs = { headers: {}, focus: {}, hits: {} };
  const run = async (what: string, f: () => Promise<void>) => {
    try {
      await f();
    } catch (e) {
      problems.push(`${what}: ${String(e).split('\n')[0]!.slice(0, 400)} (SPEC/PROBE ERROR)`);
    }
  };
  try {
    await makeData(owner, ids);
    await run('text 1280', () =>
      channelScenario(browser, owner, ids, figs, 'text', C1280, [true, false], {
        shots: { 'text 1280 open': 'header-1280-members-open' },
      }),
    );
    await run('text 1024', () =>
      channelScenario(browser, owner, ids, figs, 'text', C1024, [true, false], {
        focus: true,
        hit: true,
        shots: {
          'text 1024 open': 'header-1024-members-open',
          'text 1024 closed': 'header-1024-members-closed',
        },
      }),
    );
    await run('text 900', () =>
      channelScenario(browser, owner, ids, figs, 'text', C900, [true, false]),
    );
    await run('forum 1024', () =>
      channelScenario(browser, owner, ids, figs, 'forum', C1024, [true, false], {
        shots: { 'forum 1024 open': 'forum-header-1024-members-open' },
      }),
    );
    await run('forum 1280', () =>
      channelScenario(browser, owner, ids, figs, 'forum', C1280, [true, false]),
    );
    await run('forum 900', () =>
      channelScenario(browser, owner, ids, figs, 'forum', C900, [true, false]),
    );
    await run('text 375', () =>
      channelScenario(browser, owner, ids, figs, 'text', C375, [false], {
        hit: true,
        shots: { 'text 375': 'header-375' },
      }),
    );
    await run('dm 1024', () => dmScenario(browser, owner, figs));
  } finally {
    await removeData(owner, ids);
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  for (const [k, f] of Object.entries(figs.headers)) {
    if (f.scroll[0]! > f.scroll[1]!) problems.push(`${k}: page scrolls sideways ${f.scroll}`);
    if (f.header && f.header.h !== 48) say(`${k}: header height ${f.header.h}`);
  }
  for (const [k, f] of Object.entries(figs.hits))
    if (f.notReached) problems.push(`hit ${k}: ${f.notReached} rows not reached (probe error)`);

  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    compare(JSON.parse(readFileSync(baseFile, 'utf8')) as Figs, figs, problems);
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  for (const [k, f] of Object.entries(figs.headers)) {
    const cut =
      f.title && f.title.sw > f.title.cw + 0.5 ? `CUT ${f.title.sw}>${f.title.cw}` : 'whole';
    say(
      `${k}: header ${bx(f.header)} title ${bx(f.title)} ${cut} topic ${bx(f.topic)} tools ${bx(f.tools)} search ${bx(f.search)} input ${bx(f.input)} tools ${f.toolBoxes.map((t) => `${t.name}@${t.box.x}/${t.box.w}`).join('; ')}`,
    );
  }
  for (const [k, f] of Object.entries(figs.focus)) say(`focus ${k}: ${JSON.stringify(f)}`);
  for (const [k, f] of Object.entries(figs.hits)) say(`hit ${k}: ${JSON.stringify(f)}`);
  await owner.close();
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'after') expect(problems).toEqual([]);
});

function compare(base: Figs, after: Figs, problems: string[]) {
  for (const [k, b] of Object.entries(base.headers)) {
    const a = after.headers[k];
    if (!a) {
      problems.push(`${k}: not measured in after`);
      continue;
    }
    if (!sameBox(a.header, b.header))
      problems.push(`${k}: header ${bx(a.header)} against base ${bx(b.header)}`);
    if (JSON.stringify(a.tabOrder) !== JSON.stringify(b.tabOrder))
      problems.push(
        `${k}: focus order changed: ${a.tabOrder.join(' > ')} against ${b.tabOrder.join(' > ')}`,
      );
    const folded = !!a.search && !!b.search && a.search.w < b.search.w - 1;
    if (!folded) {
      if (!sameBox(a.title, b.title))
        problems.push(`${k}: title ${bx(a.title)} against base ${bx(b.title)} (not folded)`);
      if (!sameBox(a.topic, b.topic))
        problems.push(`${k}: topic ${bx(a.topic)} against base ${bx(b.topic)} (not folded)`);
      if (!sameBox(a.tools, b.tools))
        problems.push(`${k}: tools ${bx(a.tools)} against base ${bx(b.tools)} (not folded)`);
      if (!sameBox(a.search, b.search))
        problems.push(`${k}: search ${bx(a.search)} against base ${bx(b.search)} (not folded)`);
      b.toolBoxes.forEach((t, i) => {
        if (!sameBox(a.toolBoxes[i]?.box, t.box))
          problems.push(
            `${k}: tool "${t.name}" ${bx(a.toolBoxes[i]?.box)} against base ${bx(t.box)} (not folded)`,
          );
      });
      continue;
    }
    say(`${k}: FOLDED search ${bx(b.search)} -> ${bx(a.search)}`);
    const delta = b.search!.w - a.search!.w;
    if (!near(a.search!.w, 28, 1))
      problems.push(`${k}: folded search is ${bx(a.search)}, expected about 28 wide`);
    if (a.input && a.input.w < 1) problems.push(`${k}: folded input is ${a.input.w} wide`);
    if (a.title && b.title && a.title.w < b.title.w - 0.5)
      problems.push(`${k}: title narrower ${a.title.w} against base ${b.title.w}`);
    if (a.tools && b.tools && !near(a.tools.w, b.tools.w - delta, 1.5))
      problems.push(`${k}: tools ${a.tools.w} wide, expected ${b.tools.w - delta}`);
    if (a.tools && b.tools && !near(a.tools.x + a.tools.w, b.tools.x + b.tools.w))
      problems.push(`${k}: tools right edge moved`);
    // The tools before the search shift by delta; the ones after it keep x; none changes size.
    const iSearch = b.toolBoxes.length; // the search is not a button: tools after it are the last two buttons (inbox, help)
    b.toolBoxes.forEach((t, i) => {
      const at = a.toolBoxes[i]?.box;
      if (!at) return problems.push(`${k}: tool "${t.name}" missing in after`);
      if (!near(at.w, t.box.w) || !near(at.h, t.box.h) || !near(at.y, t.box.y))
        problems.push(`${k}: tool "${t.name}" size/y ${bx(at)} against base ${bx(t.box)}`);
      const afterSearch = i >= iSearch - 2;
      const wantX = afterSearch ? t.box.x : t.box.x + delta;
      if (!near(at.x, wantX, 1.5))
        problems.push(`${k}: tool "${t.name}" x ${at.x}, expected ${wantX}`);
    });
    if (a.title && a.title.sw > a.title.cw + 0.5)
      say(
        `${k}: title still cut after the fold (${a.title.sw} in ${a.title.cw}); base ${b.title ? `${b.title.sw} in ${b.title.cw}` : 'none'}`,
      );
  }
  for (const [k, b] of Object.entries(base.focus)) {
    const a = after.focus[k];
    if (!a) continue;
    if (!sameBox(a.panel, b.panel))
      problems.push(`focus ${k}: search panel ${bx(a.panel)} against base ${bx(b.panel)}`);
    if (
      !a.form ||
      !b.form ||
      !near(a.form.w, b.form.w) ||
      !near(a.form.x + a.form.w, b.form.x + b.form.w)
    )
      problems.push(`focus ${k}: focused field ${bx(a.form)} against base ${bx(b.form)}`);
    if (!a.clickFocused) problems.push(`focus ${k}: a click on the field did not focus it`);
    if (!a.typedPanel) problems.push(`focus ${k}: typing did not open the panel`);
    if (a.scroll[0]! > a.scroll[1]!)
      problems.push(`focus ${k}: page scrolls sideways when focused`);
    const folded =
      after.headers[k]?.search &&
      base.headers[k]?.search &&
      after.headers[k]!.search!.w < base.headers[k]!.search!.w - 1;
    if (folded && a.afterEscapeForm && !near(a.afterEscapeForm.w, after.headers[k]!.search!.w, 1))
      problems.push(`focus ${k}: after Escape the field is ${bx(a.afterEscapeForm)}, not folded`);
  }
  for (const [k, b] of Object.entries(base.hits)) {
    const a = after.hits[k];
    if (a && a.fail > b.fail)
      problems.push(`hit ${k}: ${a.fail} fails against base ${b.fail}: ${JSON.stringify(a.fails)}`);
  }
}
