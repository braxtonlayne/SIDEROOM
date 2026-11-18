import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 104, pod lounge (D-992, D-995, D-996, D-997): four scenarios, one spec.
 *   keys    D-995  Alt+ArrowDown / Alt+ArrowUp on a focused select leave the key to the select (the search results'
 *                  Sort and the forum toolbar's Sort); on the message list they still go to the next / previous channel.
 *   event   D-992  the Create Event dialog is the kit's medium width (about 560 px at 1024 and 1280), Starts and Ends
 *                  on one line there; 375 equals the base within 1 px.
 *   forum   D-996  the forum toolbar's Sort is the kit's select (fit="content") at 14 px.
 *   bridge  D-997  the Conflict log's Kind is the kit's select at 14 px regular, gap kept; every other labelled
 *                  control of the page must not move.
 * Elements are found by role, label or id, or by tag inside a root found that way, never by class. Hit tests use the
 * shared tool (reach 'box' at 1024 and 1280, the default at 375 touch). Data it makes (D-942), named `p104l-<stamp>`:
 * three text channels, six messages with one word, a forum channel with two posts (and a tag if the API takes one);
 * removed in a `finally`. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; never edited). The bridge (status, conflicts, features) is answered by `page.route` fakes, the same in
 * both phases. Keyboard cases put every value back; nothing is saved.
 *
 * Phases: `SR_P104_LOUNGE_PHASE=base` records the unmerged tree (shots get a `base-` prefix, `figures-base.json`);
 * the default `after` records the same and, when `figures-base.json` is in the folder, compares: every element that
 * breaks a "must not change" or leaves a "may change" limit of the four task files is a problem line with its
 * figures. `MOVED` says per scenario whether the after phase expects the change (true) or the base (false).
 *
 * Local only (SR_P104_LOUNGE_ON=1). SR_P104_LOUNGE_OUT is the folder of the shots and the figures. From e2e/:
 *   SR_E2E_BASE_URL=http://localhost:5801 SR_P104_LOUNGE_ON=1 SR_P104_LOUNGE_PHASE=base \
 *   SR_P104_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-lounge \
 *   npx playwright test tests/p104-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * and the same without `SR_P104_LOUNGE_PHASE=base` for the after phase. `SR_P104_LOUNGE_ONLY=keys,event` runs only
 * those scenarios (the figures of the others in the file are kept).
 */
const ON = process.env.SR_P104_LOUNGE_ON === '1';
const PHASE = process.env.SR_P104_LOUNGE_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P104_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const ONLY = process.env.SR_P104_LOUNGE_ONLY?.split(',') ?? null;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);
const WORD = `p104lq${Date.now().toString(36)}`;

/** Per scenario: does the after phase expect the change (true) or the base (false)? Put a change back: set it false. */
const MOVED = { keys: true, event: true, forum: true, bridge: true };

test.skip(!ON, 'local visual check only');
test.setTimeout(1_500_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
}
const L1024: Cfg = {
  tag: '1024-lounge-light',
  width: 1024,
  height: 768,
  touch: false,
  theme: 'lounge-light',
};
const D1024: Cfg = { ...L1024, tag: '1024-lounge-dark', theme: 'lounge-dark' };
const L1280: Cfg = { ...L1024, tag: '1280-lounge-light', width: 1280, height: 800 };
const L375: Cfg = {
  tag: '375-lounge-light',
  width: 375,
  height: 812,
  touch: true,
  theme: 'lounge-light',
};

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
const near = (a: number, b: number, tol = 0.5) => Math.abs(a - b) <= tol;
const bx = (b: Box | null) => (b ? `${b.x},${b.y} ${b.w}x${b.h}` : 'none');

// ------------------------------------------------------------------------------------------------ plumbing

/** A create call that answers 429: one retry after a short wait (the brief's rule); said in the report. */
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

const goto = (page: Page, url: string) =>
  page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};
const up = (loc: Locator, xpath: string) => loc.locator(`xpath=${xpath}`);

async function shot(page: Page, name: string, target?: Locator) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  const file = resolve(OUT, `${shotName(name)}.png`);
  if (target) {
    try {
      await target.scrollIntoViewIfNeeded({ timeout: 3000 });
      const b = await target.boundingBox();
      const vp = page.viewportSize()!;
      if (b && b.height > 0) {
        const x = Math.max(0, b.x - 8);
        const y = Math.max(0, b.y - 8);
        const clip = {
          x,
          y,
          width: Math.min(vp.width - x, b.width + 16),
          height: Math.min(vp.height - y, b.height + 16),
        };
        if (clip.width > 20 && clip.height > 20) {
          await page.screenshot({ path: file, clip, timeout: 10_000 });
          return;
        }
      }
    } catch {
      /* the viewport */
    }
  }
  await page.screenshot({ path: file, timeout: 10_000 });
}

interface HitCount {
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
}
const count = (rows: ControlRow[]): HitCount => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function hit(page: Page, scope: Locator, c: Cfg, tag: string, problems: string[]) {
  const opened = await openCollapsed(page, scope);
  const rows = await hitTestAll(page, scope, c.touch ? {} : { reach: 'box' });
  const h = count(rows);
  const fails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: hit ${JSON.stringify(h)} ${h.fail ? JSON.stringify(fails) : ''}`);
  if (h.notReached)
    problems.push(`${tag}: ${h.notReached} hit-test rows not reached (probe error)`);
  return { hit: h, hitFails: fails, opened: opened.length };
}

/** Boxes of every select and every control under `scope` (relative to the scope's corner), plus label figures. */
interface SelectFig {
  box: Box;
  wrap: Box | null;
  kit: boolean;
  labelLines: number;
  labelText: string;
  stacked: boolean;
  labelGap: number | null;
  fontSize: string;
  fontWeight: string;
  textTransform: string;
  letterSpacing: string;
  labelStyle: {
    fontSize: string;
    fontWeight: string;
    textTransform: string;
    letterSpacing: string;
    color: string;
  } | null;
  selectedText: string;
  textW: number;
  textRight: number;
  chevronX: number | null;
  textClear: boolean;
  field: Box | null;
}
interface ControlFig {
  key: string;
  tag: string;
  box: Box;
}
interface Measured {
  scroll: number[];
  selects: Record<string, SelectFig>;
  controls: ControlFig[];
  /** Every label of the scope that holds a control: the label's box, its control's, the label text's style. */
  labelled: Record<string, { label: Box; control: Box; gap: number | null; style: unknown }>;
  theme: string | null;
}

async function measure(scope: Locator): Promise<Measured> {
  return scope.evaluate((root) => {
    const rr = root.getBoundingClientRect();
    const rnd = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return {
        x: rnd(r.left - rr.left),
        y: rnd(r.top - rr.top),
        w: rnd(r.width),
        h: rnd(r.height),
      };
    };
    const visible = (e: Element) => e.getClientRects().length > 0;
    const canvas = document.createElement('canvas').getContext('2d')!;
    const textInfo = (label: Element, skip: Element) => {
      let text = '';
      const tops = new Set<number>();
      let bottom = -1;
      let right = -1;
      let style: Element | null = null;
      const w = document.createTreeWalker(label, NodeFilter.SHOW_TEXT);
      for (let t = w.nextNode(); t; t = w.nextNode()) {
        if (skip.contains(t)) continue;
        if (!(t.textContent ?? '').trim()) continue;
        text += (t.textContent ?? '').trim() + ' ';
        style = style ?? t.parentElement;
        const range = document.createRange();
        range.selectNodeContents(t);
        for (const r of Array.from(range.getClientRects()))
          if (r.width > 0 && r.height > 0) {
            tops.add(Math.round(r.top));
            bottom = Math.max(bottom, r.bottom);
            right = Math.max(right, r.right);
          }
      }
      const cs = style ? getComputedStyle(style) : null;
      return {
        text: text.trim(),
        lines: tops.size,
        bottom,
        right,
        style: cs
          ? {
              fontSize: cs.fontSize,
              fontWeight: cs.fontWeight,
              textTransform: cs.textTransform,
              letterSpacing: cs.letterSpacing,
              color: cs.color,
            }
          : null,
      };
    };
    const selEls = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).filter(visible);
    const selects: Record<string, unknown> = {};
    selEls.forEach((s, n) => {
      const label =
        s.closest('label') ?? (s.id ? document.querySelector(`label[for="${s.id}"]`) : null);
      const ti = label ? textInfo(label, s) : null;
      const sr = s.getBoundingClientRect();
      const cs = getComputedStyle(s);
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const selectedText = s.options[s.selectedIndex]?.text ?? '';
      const textW = canvas.measureText(selectedText).width;
      const textRight =
        sr.left + parseFloat(cs.borderLeftWidth) + parseFloat(cs.paddingLeft) + textW;
      const wrapEl = s.parentElement;
      const kit = !!wrapEl && wrapEl.tagName === 'SPAN' && wrapEl.children.length === 1;
      const chevronX = kit ? wrapEl!.getBoundingClientRect().right - 14 - 6 - 1 : null;
      const key = `${s.id.replace(/^_r_[a-z0-9]+_-?/, '') || s.getAttribute('aria-label') || ti?.text || 'select'}#${n}`;
      selects[key] = {
        box: box(s),
        wrap: kit ? box(wrapEl!) : null,
        kit,
        labelLines: ti?.lines ?? 0,
        labelText: ti?.text ?? '',
        labelGap: ti && ti.right >= 0 ? rnd(sr.left - ti.right) : null,
        stacked: ti ? ti.bottom >= 0 && ti.bottom <= sr.top + 1 : false,
        fontSize: cs.fontSize,
        fontWeight: cs.fontWeight,
        textTransform: cs.textTransform,
        letterSpacing: cs.letterSpacing,
        labelStyle: ti?.style ?? null,
        selectedText,
        textW: rnd(textW),
        textRight: rnd(textRight - rr.left),
        chevronX: chevronX === null ? null : rnd(chevronX - rr.left),
        textClear: kit ? textRight <= chevronX! + 0.5 : true,
        field: null,
      };
    });
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter(visible);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24)}`,
      tag: e.tagName.toLowerCase(),
      box: box(e),
    }));
    // every label that holds a control
    const labelled: Record<string, unknown> = {};
    Array.from(root.querySelectorAll<HTMLLabelElement>('label'))
      .filter(visible)
      .forEach((l, n) => {
        const ctl = l.querySelector<HTMLElement>('select, input, textarea');
        if (!ctl || !visible(ctl)) return;
        const ti = textInfo(l, ctl);
        const cr = ctl.getBoundingClientRect();
        const cs = getComputedStyle(ctl);
        labelled[`${ti.text.slice(0, 30)}#${n}`] = {
          label: box(l),
          control: box(ctl),
          gap: ti.right >= 0 ? rnd(cr.left - ti.right) : null,
          style: {
            label: ti.style,
            control: {
              fontSize: cs.fontSize,
              fontWeight: cs.fontWeight,
              textTransform: cs.textTransform,
              letterSpacing: cs.letterSpacing,
            },
          },
        };
      });
    return {
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      labelled,
      theme:
        root.closest('[data-theme]')?.getAttribute('data-theme') ??
        document.documentElement.getAttribute('data-theme'),
    } as never;
  });
}

/** Focus, ArrowDown changes the value, the page reacts, ArrowUp puts it back. */
interface KeyFig {
  before: string;
  changed: string;
  back: string;
  focusVisible: boolean;
  reacted: boolean;
  order?: { before: string[]; changed: string[]; back: string[] };
}
async function keyboard(
  page: Page,
  sel: () => Locator,
  tag: string,
  problems: string[],
  order?: () => Promise<string[]>,
): Promise<KeyFig | null> {
  const s0 = sel().first();
  const enabled = await s0.evaluate(
    (e) => Array.from((e as HTMLSelectElement).options).filter((o) => !o.disabled).length,
  );
  if (enabled < 2) {
    say(`${tag}: keyboard not driven (one enabled option)`);
    return null;
  }
  await s0.scrollIntoViewIfNeeded();
  await page.keyboard.press('Shift');
  await s0.focus();
  const focusVisible = await s0.evaluate((e) => e.matches(':focus-visible'));
  const text0 = await page.locator('body').innerText();
  const o0 = order ? await order() : [];
  const before = await s0.inputValue();
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(600);
  const changed = await sel().first().inputValue();
  const text1 = await page.locator('body').innerText();
  const o1 = order ? await order() : [];
  await sel().first().focus();
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(600);
  const back = await sel().first().inputValue();
  const o2 = order ? await order() : [];
  const k: KeyFig = {
    before,
    changed,
    back,
    focusVisible,
    reacted: text0 !== text1 || changed !== before,
    ...(order ? { order: { before: o0, changed: o1, back: o2 } } : {}),
  };
  say(`${tag}: keyboard ${JSON.stringify(k)}`);
  if (changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
  if (back !== before) problems.push(`${tag}: ArrowUp did not put the value back`);
  if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
  if (order && JSON.stringify(o1) === JSON.stringify(o0))
    problems.push(`${tag}: the post order did not follow the Sort (${o0.join(' | ')})`);
  return k;
}

// ------------------------------------------------------------------------------------------------ data

interface Ids {
  guildId: string;
  chans: string[];
  forum: string;
  forumTitles: string[];
  tag: boolean;
}

async function makeData(octx: BrowserContext, ids: Ids) {
  const o = octx.request;
  const w = world();
  ids.guildId = w.guildId;
  for (const n of ['a', 'b', 'c']) {
    const c = await mk(
      () =>
        apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
          name: `p104l-${n}-${STAMP}`,
          type: 0,
        }),
      'channel create',
    );
    ids.chans.push(c.id);
  }
  for (let i = 1; i <= 3; i++) {
    await mk(
      () => apiPost(o, `/channels/${ids.chans[0]}/messages`, { content: `${WORD} line m${i}` }),
      'message',
    );
    await sleep(700);
  }
  const f = await mk(
    () =>
      apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
        name: `p104l-forum-${STAMP}`,
        type: 15,
      }),
    'forum create',
  );
  ids.forum = f.id;
  try {
    await apiPatch(o, `/channels/${f.id}`, { available_tags: [{ name: `p104l-tag` }] });
    ids.tag = true;
  } catch (e) {
    say(`no forum tag made: ${String(e).slice(0, 160)}`);
  }
  // Two posts; then a reply in the older one, so "latest activity" and "newest" order them differently.
  const mkPost = async (name: string) =>
    (
      await mk(
        () =>
          apiPost<{ id: string }>(o, `/channels/${f.id}/threads`, {
            name,
            message: { content: `${name} body` },
          }),
        'forum post',
      )
    ).id;
  const p1 = await mkPost(`p104l-post-one-${STAMP}`);
  await sleep(1200);
  await mkPost(`p104l-post-two-${STAMP}`);
  await sleep(1200);
  await mk(() => apiPost(o, `/channels/${p1}/messages`, { content: 'a later reply' }), 'reply');
  ids.forumTitles = [`p104l-post-one-${STAMP}`, `p104l-post-two-${STAMP}`];
}

async function removeData(octx: BrowserContext, ids: Ids) {
  for (const id of [...ids.chans, ids.forum].filter(Boolean))
    await apiDelete(octx.request, `/channels/${id}`).catch((e) =>
      say(`channel ${id} (p104l-*-${STAMP}) not deleted: ${String(e).slice(0, 160)}`),
    );
}

const forumSort = (page: Page) => page.getByRole('combobox', { name: 'Sort by' });
async function openForum(page: Page, ids: Ids) {
  await goto(page, `/channels/${ids.guildId}/${ids.forum}`);
  await forumSort(page).waitFor({ timeout: 40_000 });
  await page.getByText(ids.forumTitles[0]!).first().waitFor({ timeout: 20_000 });
  await settled(page);
}
const postTitles = (page: Page, ids: Ids) => async () =>
  page.evaluate(
    (names) =>
      Array.from(document.querySelectorAll('main li, [role="main"] li'))
        .map((l) => names.find((n) => (l.textContent ?? '').includes(n)) ?? '')
        .filter(Boolean),
    ids.forumTitles,
  );

// ------------------------------------------------------------------------------------------------ keys

interface KeyStep {
  step: string;
  path: string;
  panel: boolean;
  active: string;
  value: string | null;
}
const activeDesc = (page: Page) =>
  page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return 'none';
    const lab =
      a.getAttribute('aria-label') ??
      (a as HTMLInputElement).labels?.[0]?.textContent?.trim() ??
      a.id ??
      (a.textContent ?? '').trim().slice(0, 24);
    return `${a.tagName.toLowerCase()} "${lab}"`;
  });

async function keysScenario(browser: Browser, owner: BrowserContext, ids: Ids, problems: string[]) {
  const c = L1024;
  const steps: KeyStep[] = [];
  const ctx = await newCtx(browser, owner, c);
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  const panel = page.getByRole('region', { name: 'Search results' });
  const sortSel = () => panel.getByRole('combobox', { name: 'Sort by' });
  const rec = async (step: string): Promise<KeyStep> => {
    const s: KeyStep = {
      step,
      path: new URL(page.url()).pathname,
      panel: await panel.isVisible().catch(() => false),
      active: await activeDesc(page),
      value: await page
        .evaluate(
          () =>
            document.querySelector<HTMLSelectElement>('[aria-label="Search results"] select')
              ?.value ?? null,
        )
        .catch(() => null),
    };
    steps.push(s);
    say(`keys ${JSON.stringify(s)}`);
    return s;
  };
  const home = `/channels/${ids.guildId}/${ids.chans[0]}`;
  const search = async () => {
    await goto(page, home);
    const sb = page.getByRole('searchbox').first();
    await sb.waitFor({ timeout: 40_000 });
    for (let i = 0; i < 12; i++) {
      await sb.fill(WORD);
      await sb.press('Enter');
      const ok = await sortSel()
        .waitFor({ timeout: 8000 })
        .then(() => true)
        .catch(() => false);
      if (
        ok &&
        (await panel
          .getByText(WORD)
          .first()
          .isVisible()
          .catch(() => false))
      )
        break;
      await page.waitForTimeout(1000);
    }
    await sortSel().waitFor({ timeout: 10_000 });
    await page.waitForTimeout(800);
  };
  try {
    // 1. the search results' Sort: Alt+ArrowDown
    await search();
    const s0 = await rec('search: start');
    await sortSel().focus();
    await rec('search: Sort focused');
    await page.keyboard.press('Alt+ArrowDown');
    await page.waitForTimeout(900);
    const s1 = await rec('search: Alt+ArrowDown on the Sort');
    await shot(page, 'keys-sort-altdown-1024-lounge-light');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(500);
    const s2 = await rec('search: Escape after it');
    if (MOVED.keys) {
      if (s1.path !== s0.path)
        problems.push(
          `keys (keys) LoungeShell.tsx the search Sort: Alt+ArrowDown changed the path ${s0.path} -> ${s1.path}`,
        );
      if (!s1.panel)
        problems.push('keys (keys) the search Sort: Alt+ArrowDown closed the results panel');
      if (!/^select/.test(s1.active))
        problems.push(`keys (keys) the search Sort: focus left the select (${s1.active})`);
    } else if (s1.path === s0.path)
      problems.push(
        `keys: the base expects Alt+ArrowDown to change the path, it stayed ${s1.path}`,
      );
    // plain ArrowDown on the Sort (D-980)
    await search();
    const k1 = await keyboard(page, sortSel, 'keys search Sort', problems);
    steps.push({
      step: `search: ArrowDown ${JSON.stringify(k1)}`,
      path: '',
      panel: true,
      active: '',
      value: null,
    });
    void s2;

    // 2. the forum toolbar's Sort
    await openForum(page, ids);
    const f0 = await rec('forum: start');
    await forumSort(page).focus();
    await page.keyboard.press('Alt+ArrowDown');
    await page.waitForTimeout(900);
    const f1 = await rec('forum: Alt+ArrowDown on the Sort');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
    await rec('forum: Escape after it');
    if (MOVED.keys) {
      if (f1.path !== f0.path)
        problems.push(
          `keys (keys) the forum Sort: Alt+ArrowDown changed the path ${f0.path} -> ${f1.path}`,
        );
      if (!/^select/.test(f1.active))
        problems.push(`keys (keys) the forum Sort: focus left the select (${f1.active})`);
    }
    await openForum(page, ids);
    await keyboard(page, () => forumSort(page), 'keys forum Sort', problems);

    // 3. the other side, both phases: focus on the message list, Alt+ArrowDown / Alt+ArrowUp change channel
    await goto(page, home);
    const list = page.getByRole('log').first();
    await list.waitFor({ timeout: 40_000 });
    await settled(page);
    await list.click({ position: { x: 5, y: 5 }, force: true }).catch(() => undefined);
    const m0 = await rec('list: focus on the message list');
    await page.keyboard.press('Alt+ArrowDown');
    await page.waitForTimeout(1200);
    const m1 = await rec('list: Alt+ArrowDown');
    if (m1.path === m0.path)
      problems.push(
        `keys: Alt+ArrowDown with focus on the message list did not go to the next channel (${m0.path})`,
      );
    await page.keyboard.press('Alt+ArrowUp');
    await page.waitForTimeout(1200);
    const m2 = await rec('list: Alt+ArrowUp');
    if (m2.path !== m0.path)
      problems.push(`keys: Alt+ArrowUp did not come back (${m0.path} -> ${m1.path} -> ${m2.path})`);
    // a button and the body keep the shortcut too
    for (const target of ['button', 'body'] as const) {
      await goto(page, home);
      await list.waitFor({ timeout: 40_000 });
      await settled(page);
      if (target === 'button') await page.getByRole('button').first().focus();
      else await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
      const t0 = await rec(`${target}: focus`);
      await page.keyboard.press('Alt+ArrowDown');
      await page.waitForTimeout(1200);
      const t1 = await rec(`${target}: Alt+ArrowDown`);
      if (t1.path === t0.path)
        problems.push(
          `keys: Alt+ArrowDown with focus on a ${target} did not go to the next channel`,
        );
    }
  } finally {
    await ctx.close();
  }
  return steps;
}

// ------------------------------------------------------------------------------------------------ event

interface EventFig {
  theme: string | null;
  dialog: Box | null;
  body: { sw: number; cw: number; box: Box } | null;
  doc: number[];
  fields: { label: string; box: Box; sw: number; cw: number; value: string }[];
  datesShareTop: boolean | null;
  radios: Box[];
  past: string[];
  controls: { key: string; box: Box }[];
  hit: HitCount;
  hitFails: unknown[];
}

async function openEvent(page: Page, ids: Ids) {
  await goto(page, `/channels/${ids.guildId}/${ids.chans[1]}`);
  await page
    .getByTestId('channel-sidebar')
    .first()
    .waitFor({ timeout: 20_000 })
    .catch(() => undefined);
  await page.waitForTimeout(1200);
  await page.evaluate(
    async (d) => {
      const m = (await import(/* @vite-ignore */ String('/src/lounge/store.ts'))) as {
        useLoungeUi: { getState: () => { openDialog: (x: unknown) => void } };
      };
      m.useLoungeUi.getState().openDialog(d);
    },
    { kind: 'createEvent', guildId: ids.guildId },
  );
  const dlg = page.getByRole('dialog', { name: /Create Event/ });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('input[type="datetime-local"]').first().waitFor({ timeout: 15_000 });
  await settled(page);
  return dlg;
}

async function measureEvent(page: Page): Promise<Omit<EventFig, 'hit' | 'hitFails'>> {
  return page.evaluate(() => {
    const dlg = document.querySelector<HTMLElement>('[role="dialog"]')!;
    const round = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return { x: round(r.left), y: round(r.top), w: round(r.width), h: round(r.height) };
    };
    const form = dlg.querySelector('form') ?? dlg;
    let bodyEl: HTMLElement = dlg;
    for (let p: HTMLElement | null = form.parentElement; p; p = p.parentElement) {
      const o = getComputedStyle(p);
      if (/(auto|scroll)/.test(o.overflowX + o.overflowY)) {
        bodyEl = p;
        break;
      }
      if (p === dlg) break;
    }
    const inputs = Array.from(
      dlg.querySelectorAll<HTMLInputElement>('input[type="datetime-local"]'),
    );
    const cs = getComputedStyle(bodyEl);
    const bb = bodyEl.getBoundingClientRect();
    const contentRight = bb.right - parseFloat(cs.paddingRight) - parseFloat(cs.borderRightWidth);
    const past: string[] = [];
    for (const e of Array.from(form.querySelectorAll<HTMLElement>('*'))) {
      if (e.getClientRects().length === 0) continue;
      const r = e.getBoundingClientRect();
      if (r.width > 0 && r.right > contentRight + 1)
        past.push(
          `${e.tagName.toLowerCase()} right=${round(r.right)} (limit ${round(contentRight)})`,
        );
    }
    const els = Array.from(
      dlg.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    return {
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      dialog: box(dlg),
      body: { sw: bodyEl.scrollWidth, cw: bodyEl.clientWidth, box: box(bodyEl) },
      doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      fields: inputs.map((i) => ({
        label: (i.labels?.[0]?.textContent ?? i.getAttribute('aria-label') ?? i.id)
          .trim()
          .slice(0, 30),
        box: box(i),
        sw: i.scrollWidth,
        cw: i.clientWidth,
        value: i.value,
      })),
      datesShareTop:
        inputs.length >= 2
          ? Math.abs(
              inputs[0]!.getBoundingClientRect().top - inputs[1]!.getBoundingClientRect().top,
            ) < 1
          : null,
      radios: Array.from(dlg.querySelectorAll<HTMLInputElement>('input[type="radio"]')).map((r) =>
        box(r.closest('label') ?? r),
      ),
      past: past.slice(0, 12),
      controls: els.map((e, i) => ({
        key: `${i}:${e.tagName.toLowerCase()}:${(
          e.getAttribute('aria-label') ??
          e.closest('label')?.firstElementChild?.textContent ??
          e.textContent ??
          ''
        )
          .trim()
          .slice(0, 24)}`,
        box: box(e),
      })),
    };
  });
}

async function eventScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
  problems: string[],
): Promise<EventFig> {
  const tag = `event ${c.tag}`;
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(15_000);
    const dlg = await openEvent(page, ids);
    const m = await measureEvent(page);
    if (m.theme !== c.theme) problems.push(`${tag}: data-theme is ${m.theme}, wanted ${c.theme}`);
    say(
      `${tag}: dialog ${bx(m.dialog as never)} ${JSON.stringify(m.dialog)} body ${JSON.stringify(m.body)}`,
    );
    say(`${tag}: fields ${JSON.stringify(m.fields)} sharesTop ${m.datesShareTop}`);
    if (m.doc[0]! > m.doc[1]!)
      problems.push(`${tag}: the document scrolls sideways ${m.doc.join('/')}`);
    if (m.body && m.body.sw > m.body.cw)
      problems.push(`${tag}: the dialog body scrolls sideways ${m.body.sw}/${m.body.cw}`);
    for (const f of m.fields)
      if (f.sw > f.cw)
        problems.push(`${tag}: date field "${f.label}" clips its value ${f.sw}/${f.cw}`);
    if (m.past.length)
      problems.push(`${tag}: elements past the body's content edge: ${m.past.join('; ')}`);
    const h = await hit(page, dlg, c, tag, problems);
    await dlg.evaluate((e) => {
      for (const el of Array.from(e.querySelectorAll('*'))) {
        (el as HTMLElement).scrollTop = 0;
        (el as HTMLElement).scrollLeft = 0;
      }
    });
    if (c.tag !== '375-lounge-light' || true) await shot(page, `event-${c.tag}`);
    return { ...m, hit: h.hit, hitFails: h.hitFails };
  } finally {
    await ctx.close();
  }
}

function compareEvent(c: Cfg, b: EventFig, a: EventFig, problems: string[]) {
  const tag = `event (event) CreateEventDialog.tsx / events.css ${c.tag}`;
  const key = (k: string) => k.replace(/^\d+:/, '');
  if (
    JSON.stringify(b.controls.map((x) => key(x.key))) !==
    JSON.stringify(a.controls.map((x) => key(x.key)))
  )
    problems.push(
      `${tag}: the controls or their order changed (${b.controls.length} -> ${a.controls.length})`,
    );
  if (a.hit.fail > b.hit.fail || a.hit.pass + a.hit.fail !== b.hit.pass + b.hit.fail)
    problems.push(`${tag}: hit test ${JSON.stringify(b.hit)} -> ${JSON.stringify(a.hit)}`);
  if (c.width === 375 || !MOVED.event) {
    if (b.controls.length === a.controls.length)
      b.controls.forEach((x, i) => {
        const y = a.controls[i]!;
        for (const k of ['x', 'y', 'w', 'h'] as const)
          if (Math.abs(y.box[k] - x.box[k]) > 1)
            problems.push(`${tag}: ${x.key} ${k} ${x.box[k]} -> ${y.box[k]} (must equal the base)`);
      });
    return;
  }
  // 1024 / 1280: about 560 wide, the dates on one line, no field's height changed, radios whole
  if (!a.dialog || a.dialog.w < 550 || a.dialog.w > 570)
    problems.push(
      `${tag}: the dialog is ${a.dialog?.w} px wide, expected about 560 (base ${b.dialog?.w})`,
    );
  if (a.datesShareTop !== true)
    problems.push(
      `${tag}: Starts and Ends do not share a line (fields ${JSON.stringify(a.fields.map((f) => f.box))}); a fault unless event's handoff says they cannot fit`,
    );
  if (b.controls.length === a.controls.length)
    b.controls.forEach((x, i) => {
      const y = a.controls[i]!;
      if (/^\d+:(input|select|textarea)/.test(x.key) && Math.abs(y.box.h - x.box.h) > 1)
        problems.push(`${tag}: field ${x.key} height ${x.box.h} -> ${y.box.h}`);
      if (y.box.h > x.box.h + 1)
        problems.push(`${tag}: ${x.key} grew taller ${x.box.h} -> ${y.box.h}`);
    });
  b.radios.forEach((r, i) => {
    const q = a.radios[i];
    if (q && q.h > r.h + 1) problems.push(`${tag}: radio ${i} wraps/grows ${r.h} -> ${q.h}`);
  });
}

// ------------------------------------------------------------------------------------------------ forum

interface ForumFig extends Measured {
  tools: Box | null;
  search: { box: Box; fontSize: string } | null;
  views: Box[];
  firstTag: Box | null;
  firstPost: Box | null;
  rows: number;
  hit: HitCount;
  hitFails: unknown[];
}

async function forumScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
  problems: string[],
  keys: Record<string, KeyFig | null>,
): Promise<ForumFig> {
  const tag = `forum ${c.tag}`;
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await openForum(page, ids);
    const tools = up(forumSort(page), 'ancestor::label/..');
    const m = await measure(tools);
    if (m.theme !== c.theme) problems.push(`${tag}: data-theme is ${m.theme}, wanted ${c.theme}`);
    if (m.scroll[0]! > m.scroll[1]!)
      problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
    const extra = await tools.evaluate((root) => {
      const rr = root.getBoundingClientRect();
      const rnd = (n: number) => Math.round(n * 10) / 10;
      const box = (e: Element) => {
        const r = e.getBoundingClientRect();
        return {
          x: rnd(r.left - rr.left),
          y: rnd(r.top - rr.top),
          w: rnd(r.width),
          h: rnd(r.height),
        };
      };
      const s = root.querySelector<HTMLInputElement>('input[type="search"]');
      const views = Array.from(root.querySelectorAll<HTMLElement>('button[aria-pressed]')).map(box);
      const scrollerTop = root.getBoundingClientRect().top;
      const group = root.nextElementSibling;
      const chip = group?.querySelector('button');
      const post = document.querySelector('main li');
      const tops = new Set<number>();
      root.querySelectorAll('button, input, select').forEach((e) => {
        if (e.getClientRects().length)
          tops.add(
            Math.round((e.getBoundingClientRect().top + e.getBoundingClientRect().bottom) / 2 / 12),
          );
      });
      return {
        tools: { x: 0, y: 0, w: rnd(rr.width), h: rnd(rr.height) },
        search: s ? { box: box(s), fontSize: getComputedStyle(s).fontSize } : null,
        views,
        firstTag:
          chip && group?.getAttribute('role') === 'group' && group !== root
            ? ({ ...box(chip), y: rnd(chip.getBoundingClientRect().top - scrollerTop) } as Box)
            : null,
        firstPost: post
          ? ({ ...box(post), y: rnd(post.getBoundingClientRect().top - scrollerTop) } as Box)
          : null,
        rows: tops.size,
      };
    });
    const h = await hit(page, tools, c, tag, problems);
    say(`${tag}: selects ${JSON.stringify(m.selects)} extra ${JSON.stringify(extra)}`);
    await shot(page, `forum-${c.tag}`, tools);
    if (c.tag === L1024.tag)
      keys[tag] = await keyboard(page, () => forumSort(page), tag, problems, postTitles(page, ids));
    return { ...m, ...extra, hit: h.hit, hitFails: h.hitFails };
  } finally {
    await ctx.close();
  }
}

const firstSel = (f: Measured) => Object.values(f.selects)[0] ?? null;

function compareForum(c: Cfg, b: ForumFig, a: ForumFig, problems: string[]) {
  const tag = `forum (forum) ForumView.tsx / forum.css ${c.tag}`;
  const bs = firstSel(b);
  const as = firstSel(a);
  if (!bs || !as) return problems.push(`${tag}: no select measured`);
  if (!MOVED.forum) {
    if (!near(as.box.w, bs.box.w) || !near(as.box.h, bs.box.h) || as.kit)
      problems.push(
        `${tag}: expected the base: select ${bx(bs.box)} -> ${bx(as.box)} kit ${as.kit}`,
      );
    return;
  }
  const minH = c.touch ? 44 : 40;
  const grow = as.box.h - bs.box.h;
  const growW = as.box.w - bs.box.w;
  if (!as.kit) problems.push(`${tag}: the Sort is not the kit's select`);
  if (as.box.h < minH - 0.5) problems.push(`${tag}: Sort height ${as.box.h}, wanted ${minH}`);
  if (as.fontSize !== '14px') problems.push(`${tag}: Sort font-size ${as.fontSize}, wanted 14px`);
  if (!as.textClear)
    problems.push(
      `${tag}: the selected text runs into the chevron (text right ${as.textRight}, chevron ${as.chevronX})`,
    );
  if (growW > 36 + Math.max(0, as.textW - bs.textW) + 0.5)
    problems.push(
      `${tag}: Sort width ${bs.box.w} -> ${as.box.w} (limit +36 plus the type: text ${bs.textW} -> ${as.textW})`,
    );
  if (as.labelGap !== null && bs.labelGap !== null && as.labelGap < bs.labelGap - 0.5)
    problems.push(`${tag}: label gap ${bs.labelGap} -> ${as.labelGap} (under the base's)`);
  if (as.stacked && !bs.stacked)
    problems.push(`${tag}: the label text is newly stacked above the Sort`);
  if (a.rows > b.rows)
    problems.push(`${tag}: the toolbar wraps onto more rows (${b.rows} -> ${a.rows})`);
  const dh = (a.tools?.h ?? 0) - (b.tools?.h ?? 0);
  if (dh > grow + 1)
    problems.push(
      `${tag}: toolbar height ${b.tools?.h} -> ${a.tools?.h}, more than the Sort's growth ${grow}`,
    );
  if (a.search && b.search) {
    if (!near(a.search.box.h, b.search.box.h, 1) || a.search.fontSize !== b.search.fontSize)
      problems.push(
        `${tag}: search field ${bx(b.search.box)} ${b.search.fontSize} -> ${bx(a.search.box)} ${a.search.fontSize}`,
      );
    if (a.search.box.w > b.search.box.w + 1)
      problems.push(`${tag}: search field wider ${b.search.box.w} -> ${a.search.box.w}`);
  }
  b.views.forEach((v, i) => {
    const q = a.views[i];
    if (!q) return;
    if (!near(q.w, v.w, 1) || !near(q.h, v.h, 1))
      problems.push(`${tag}: view button ${i} size ${bx(v)} -> ${bx(q)}`);
    if (a.rows === b.rows && q.x - v.x > growW + 1)
      problems.push(
        `${tag}: view button ${i} x ${v.x} -> ${q.x}, more than the Sort's growth ${growW}`,
      );
  });
  for (const [nm, bb, aa] of [
    ['first tag chip', b.firstTag, a.firstTag],
    ['first post', b.firstPost, a.firstPost],
  ] as const)
    if (bb && aa && (aa.y - bb.y > dh + 1 || aa.y - bb.y < -1))
      problems.push(`${tag}: ${nm} top ${bb.y} -> ${aa.y} (toolbar grew ${dh})`);
  if (c.touch && a.scroll[0]! > a.scroll[1]!) problems.push(`${tag}: sideways scroll at 375`);
  if (a.hit.fail > b.hit.fail || a.hit.pass + a.hit.fail !== b.hit.pass + b.hit.fail)
    problems.push(`${tag}: hit test ${JSON.stringify(b.hit)} -> ${JSON.stringify(a.hit)}`);
}

// ------------------------------------------------------------------------------------------------ bridge

async function fakeBridge(
  page: Page,
  g: string,
  mode: 'whole_guild' | 'selective' = 'whole_guild',
) {
  await page.route(new RegExp(`/api/v1/guilds/${g}/bridge/discord-channels$`), (route) =>
    route.fulfill({
      json: [
        { id: '1600000000000000001', name: 'p104l-disc-one' },
        { id: '1600000000000000002', name: 'p104l-disc-two' },
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
          discord_guild_name: 'p104l-friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'p104l-relay',
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
    discord_value: 'p104l-new',
    sr_value: 'p104l-old',
    discord_actor_name: 'p104l-actor',
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
          conflict('1500000000000000001', 'p104l-general'),
          conflict('1500000000000000002', 'p104l-off-topic'),
        ],
        open: 2,
      },
    }),
  );
}

interface BridgeFig extends Measured {
  switchBox: Box | null;
  card: Box | null;
  hit: HitCount;
  hitFails: unknown[];
}

async function bridgeScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
  problems: string[],
  keys: Record<string, KeyFig | null>,
): Promise<BridgeFig> {
  const tag = `bridge ${c.tag}`;
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await fakeBridge(page, ids.guildId);
    await goto(page, `/settings/server/${ids.guildId}/bridge`);
    const card = page.getByTestId('bridge-conflicts');
    const kind = () => card.getByRole('combobox');
    await kind().waitFor({ timeout: 40_000 });
    await settled(page);
    await card.scrollIntoViewIfNeeded();
    // every labelled control of the page (the Conflict log's own and the others), measured from the main area
    const main = page.locator('main').first();
    const pageM = await measure(main);
    const m = await measure(card);
    if (m.theme !== c.theme) problems.push(`${tag}: data-theme is ${m.theme}, wanted ${c.theme}`);
    if (m.scroll[0]! > m.scroll[1]!)
      problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
    const sw = card.getByRole('switch').first();
    const switchBox = (await sw.count())
      ? await sw.evaluate((e) => {
          const r = e.getBoundingClientRect();
          const cr = e.closest('[data-testid="bridge-conflicts"]')!.getBoundingClientRect();
          return { x: r.left - cr.left, y: r.top - cr.top, w: r.width, h: r.height };
        })
      : null;
    const cardBox = await card.boundingBox();
    const h = await hit(page, card, c, tag, problems);
    say(
      `${tag}: kind ${JSON.stringify(m.selects)} switch ${JSON.stringify(switchBox)} labelled ${JSON.stringify(pageM.labelled)}`,
    );
    await shot(page, `bridge-${c.tag}`, card);
    if (c.tag === L1024.tag) keys[tag] = await keyboard(page, kind, tag, problems);
    return {
      ...m,
      labelled: pageM.labelled,
      switchBox,
      card: cardBox ? { x: cardBox.x, y: cardBox.y, w: cardBox.width, h: cardBox.height } : null,
      hit: h.hit,
      hitFails: h.hitFails,
    };
  } finally {
    await ctx.close();
  }
}

/** The map card's "Add" fieldset (selective mode): three labelled kit selects that this wave must not move. */
interface MapFig {
  labelled: Measured['labelled'];
  scroll: number[];
}
async function bridgeMapScenario(
  browser: Browser,
  owner: BrowserContext,
  ids: Ids,
  c: Cfg,
  problems: string[],
): Promise<MapFig> {
  const tag = `bridge map ${c.tag}`;
  const ctx = await newCtx(browser, owner, c);
  try {
    const page = await ctx.newPage();
    page.setDefaultTimeout(20_000);
    await fakeBridge(page, ids.guildId, 'selective');
    await goto(page, `/settings/server/${ids.guildId}/bridge`);
    const add = page.getByRole('group', { name: /Mirror another channel/ }).first();
    await add.waitFor({ timeout: 40_000 });
    await settled(page);
    await add.scrollIntoViewIfNeeded();
    const m = await measure(page.locator('main').first());
    if (m.theme !== c.theme) problems.push(`${tag}: data-theme is ${m.theme}, wanted ${c.theme}`);
    say(`${tag}: labelled ${JSON.stringify(m.labelled)}`);
    await shot(page, `bridge-map-${c.tag}`, add);
    return { labelled: m.labelled, scroll: m.scroll };
  } finally {
    await ctx.close();
  }
}

function compareBridgeMap(c: Cfg, b: MapFig, a: MapFig, problems: string[]) {
  const tag = `bridge (bridge) other labelled controls (map card) ${c.tag}`;
  const bk = Object.entries(b.labelled);
  const ak = Object.entries(a.labelled);
  if (bk.length !== ak.length) problems.push(`${tag}: count ${bk.length} -> ${ak.length}`);
  bk.forEach(([k, bv], i) => {
    const av = ak[i]?.[1];
    if (!av) return;
    if (JSON.stringify(bv.style) !== JSON.stringify(av.style))
      problems.push(
        `${tag}: "${k}" style ${JSON.stringify(bv.style)} -> ${JSON.stringify(av.style)}`,
      );
    for (const f of ['label', 'control'] as const)
      for (const d of ['x', 'y', 'w', 'h'] as const)
        if (Math.abs(av[f][d] - bv[f][d]) > 0.5)
          problems.push(`${tag}: "${k}" ${f} ${d} ${bv[f][d]} -> ${av[f][d]}`);
    if (bv.gap !== null && av.gap !== null && Math.abs(av.gap - bv.gap) > 0.5)
      problems.push(`${tag}: "${k}" gap ${bv.gap} -> ${av.gap}`);
  });
}

function compareBridge(c: Cfg, b: BridgeFig, a: BridgeFig, problems: string[]) {
  const tag = `bridge (bridge) ConflictLog.tsx / server-settings.css ${c.tag}`;
  const bs = firstSel(b);
  const as = firstSel(a);
  if (!bs || !as) return problems.push(`${tag}: no Kind select measured`);
  say(
    `${tag}: base Kind weight ${bs.fontWeight}, size ${bs.fontSize}, transform ${bs.textTransform}, spacing ${bs.letterSpacing}, gap ${bs.labelGap}`,
  );
  if (!MOVED.bridge) {
    if (!near(as.box.w, bs.box.w) || !near(as.box.h, bs.box.h) || as.kit)
      problems.push(`${tag}: expected the base: Kind ${bx(bs.box)} -> ${bx(as.box)} kit ${as.kit}`);
    return;
  }
  const minH = c.touch ? 44 : 40;
  const grow = as.box.h - bs.box.h;
  const growW = as.box.w - bs.box.w;
  if (!as.kit) problems.push(`${tag}: the Kind is not the kit's select`);
  if (as.box.h < minH - 0.5) problems.push(`${tag}: Kind height ${as.box.h}, wanted ${minH}`);
  if (
    as.fontSize !== '14px' ||
    as.fontWeight !== '400' ||
    as.textTransform !== 'none' ||
    !/^(normal|0px)$/.test(as.letterSpacing)
  )
    problems.push(
      `${tag}: Kind type ${as.fontSize}/${as.fontWeight}/${as.textTransform}/${as.letterSpacing}, wanted 14px/400/none/normal (base ${bs.fontSize}/${bs.fontWeight}/${bs.textTransform}/${bs.letterSpacing})`,
    );
  if (bs.fontWeight !== as.fontWeight)
    say(`${tag}: NOTE the base Kind weight is ${bs.fontWeight}, the after is ${as.fontWeight}`);
  if (!as.textClear)
    problems.push(
      `${tag}: the selected text runs into the chevron (${as.textRight} vs ${as.chevronX})`,
    );
  if (growW > 36.5) problems.push(`${tag}: Kind width ${bs.box.w} -> ${as.box.w} (limit +36)`);
  if (
    as.labelGap !== null &&
    bs.labelGap !== null &&
    (as.labelGap < bs.labelGap - 0.5 || as.labelGap > bs.labelGap + 2)
  )
    problems.push(`${tag}: label gap ${bs.labelGap} -> ${as.labelGap} (limit base to base+2)`);
  if (as.stacked !== bs.stacked || as.labelLines !== bs.labelLines)
    problems.push(
      `${tag}: label/field line sharing changed: stacked ${bs.stacked} -> ${as.stacked}, label lines ${bs.labelLines} -> ${as.labelLines}`,
    );
  if (JSON.stringify(as.labelStyle) !== JSON.stringify(bs.labelStyle))
    problems.push(
      `${tag}: the label's type or colour changed ${JSON.stringify(bs.labelStyle)} -> ${JSON.stringify(as.labelStyle)}`,
    );
  if (b.switchBox && a.switchBox && Math.abs(a.switchBox.x - b.switchBox.x - growW) > 1.5)
    problems.push(
      `${tag}: the Switch moved ${a.switchBox.x - b.switchBox.x} px, the Kind grew ${growW}`,
    );
  if (b.switchBox && a.switchBox && !near(a.switchBox.w, b.switchBox.w, 1))
    problems.push(
      `${tag}: the Switch changed size ${bx(b.switchBox as never)} -> ${bx(a.switchBox as never)}`,
    );
  // every other labelled control of the page must not move
  const bk = Object.entries(b.labelled);
  const ak = Object.entries(a.labelled);
  if (bk.length !== ak.length)
    problems.push(`${tag}: labelled controls on the page ${bk.length} -> ${ak.length}`);
  const kindLabel = bs.labelText;
  bk.forEach(([k, bv], i) => {
    const [ak2, av] = ak[i] ?? [];
    if (!av) return;
    if (
      bv.style &&
      JSON.stringify(bv.style) !== JSON.stringify(av.style) &&
      !k.startsWith(kindLabel)
    ) {
      problems.push(
        `${tag}: other labelled control "${k}" style changed ${JSON.stringify(bv.style)} -> ${JSON.stringify(av.style)}`,
      );
    }
    if (k.startsWith(kindLabel) || (ak2 ?? '').split('#')[0] !== k.split('#')[0]) return;
    for (const f of ['label', 'control'] as const)
      for (const d of ['w', 'h', 'x'] as const)
        if (Math.abs(av[f][d] - bv[f][d]) > 0.5)
          problems.push(
            `${tag}: other labelled control "${k}" ${f} ${d} ${bv[f][d]} -> ${av[f][d]}`,
          );
    if (bv.gap !== null && av.gap !== null && Math.abs(av.gap - bv.gap) > 0.5)
      problems.push(`${tag}: other labelled control "${k}" gap ${bv.gap} -> ${av.gap}`);
  });
  const dh = (a.card?.h ?? 0) - (b.card?.h ?? 0);
  if (dh > grow + 1)
    problems.push(`${tag}: the card grew ${dh}, more than the Kind's growth ${grow}`);
  if (a.hit.fail > b.hit.fail || a.hit.pass + a.hit.fail !== b.hit.pass + b.hit.fail)
    problems.push(`${tag}: hit test ${JSON.stringify(b.hit)} -> ${JSON.stringify(a.hit)}`);
}

// ------------------------------------------------------------------------------------------------ the run

test(`p104 lounge: keys, event, forum and bridge, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const keysKb: Record<string, KeyFig | null> = {};
  const want = (n: string) => !ONLY || ONLY.includes(n);
  const figs: Record<string, Record<string, unknown>> = {
    keys: {},
    event: {},
    forum: {},
    bridge: {},
  };
  const ids: Ids = { guildId: '', chans: [], forum: '', forumTitles: [], tag: false };
  try {
    await makeData(owner, ids);
    say(`ids ${JSON.stringify(ids)}`);
    const run = async (
      name: string,
      f: () => Promise<unknown>,
      into: Record<string, unknown>,
      key: string,
    ) => {
      try {
        into[key] = await f();
      } catch (e) {
        const msg = String(e).replace(/\s+/g, ' ').slice(0, 500);
        problems.push(`${name} ${key}: spec error / not reached: ${msg}`);
      }
    };
    if (want('keys'))
      await run('keys', () => keysScenario(browser, owner, ids, problems), figs.keys!, 'steps');
    if (want('event'))
      for (const c of [L1280, L1024, D1024, L375])
        await run(
          'event',
          () => eventScenario(browser, owner, ids, c, problems),
          figs.event!,
          c.tag,
        );
    if (want('forum'))
      for (const c of [L1024, D1024, L375])
        await run(
          'forum',
          () => forumScenario(browser, owner, ids, c, problems, keysKb),
          figs.forum!,
          c.tag,
        );
    if (want('bridge'))
      for (const c of [L1024, D1024, L375])
        await run(
          'bridge',
          () => bridgeScenario(browser, owner, ids, c, problems, keysKb),
          figs.bridge!,
          c.tag,
        );
    if (want('bridge'))
      for (const c of [L1024, L375])
        await run(
          'bridge map',
          () => bridgeMapScenario(browser, owner, ids, c, problems),
          figs.bridge!,
          `map ${c.tag}`,
        );
  } finally {
    await removeData(owner, ids);
    await setMain(owner, 'lounge-light').catch(() => undefined);
  }
  const ff = resolve(OUT, `figures-${PHASE}.json`);
  if (ONLY && existsSync(ff)) {
    const prev = JSON.parse(readFileSync(ff, 'utf8')) as { figs: typeof figs; keys: typeof keysKb };
    for (const n of Object.keys(figs)) if (want(n)) prev.figs[n] = figs[n]!;
    Object.assign(prev.keys, keysKb);
    writeFileSync(ff, JSON.stringify(prev, null, 1));
    for (const n of Object.keys(figs)) figs[n] = prev.figs[n]!;
  } else writeFileSync(ff, JSON.stringify({ figs, keys: keysKb }, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as { figs: typeof figs };
    for (const [scn, cmp, cfgs] of [
      ['event', compareEvent, [L1280, L1024, D1024, L375]],
      ['forum', compareForum, [L1024, D1024, L375]],
      ['bridge', compareBridge, [L1024, D1024, L375]],
      ['bridge', compareBridgeMap, [L1024, L375]],
    ] as const)
      if (want(scn))
        for (const c of cfgs) {
          const pre = cmp === compareBridgeMap ? 'map ' : '';
          const b = base.figs[scn]?.[pre + c.tag];
          const a = figs[scn]?.[pre + c.tag];
          if (b && a)
            (cmp as (c: Cfg, b: never, a: never, p: string[]) => void)(
              c,
              b as never,
              a as never,
              problems,
            );
        }
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems, null, 1)}`);
  if (PHASE === 'base')
    say('base phase: problems above are the base faults or spec errors, not a failure');
  else expect(problems).toEqual([]);
});
