import { mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTest, hitTestAll, openCollapsed, type HitOptions } from '../support/hit-test';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';

/**
 * Wave 108, loungeshots (pod lounge: lounge + shell; D-1022 the 44 px target, D-1024 the field contract). Two phases
 * (SR_P108_LOUNGE_PHASE=base|after), both in built mode on the seeded sweep world.
 *
 * Per page, at 375 x 812 touch (Lounge Light): `openCollapsed`, `hitTestAll` over the page's region (default reach), every
 * row whole; the NAMED controls (reaction pills, the inline edit's cancel and save, "Jump to original", the request link,
 * the Threads popover's tabs and search, the New Post textarea, the Friends header's buttons) each by `hitTest`; the
 * accepted ones (names, mentions, spoilers, links in message text, embed titles) counted only; the boxes of the rows and
 * lists around them. At 1024 x 768 (Lounge Light): the same boxes and a `reach: 'box'` hit test; after must equal base.
 * At 1024 in Lounge Light and Lounge Dark: the fields under cure B (composer, inline edit placeholder, Threads search,
 * New Post textarea): border at rest and on hover, outline with keyboard focus, placeholder, ratios from contrast.ts.
 *
 * Data: the seeded world only, plus one text channel `p108lounge-room-<stamp>` with one message (part 1) and one forum
 * `p108lounge-forum-<stamp>` with one post (part 2), made through the API and removed in a `finally`. The world's DM is a
 * message request and is NEVER accepted here, so the Home list keeps "No conversations yet." and its request link.
 *
 * Pages entered both ways (375): the Home list by URL (/channels/@me) and from the tab bar (/channels/@me, "Friends", then
 * the tab bar's "Home", which shows the Home list again); the Friends page by URL (/channels/@me, "Friends") and from the
 * tab bar (that same round trip, then "Friends" again). The app has no friends URL. From a channel the tab bar's Home
 * shows the server's channel list, not the Home list: not used.
 * Not measured (the seed has no such row): a wrapped reactions line (two lines of pills).
 *
 * Parts (SR_P108_LOUNGE_PART): 1 = the text channel (page, inline edit with the textarea emptied, cancel and save, the
 * thread) at 375 and at 1024 (Light, and Dark for the fields); 2 = Home list both ways, Threads popover, forum New Post,
 * Friends both ways, at 375 and 1024 (Dark for the fields). Without PART both run. Figures: figures-<phase>-part<N>.json
 * in OUT; in the after phase the spec compares with the base file of the same name when it is there and lists "problem"
 * lines (it never fails on them).
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_LOUNGE_ON=1 --env SR_P108_LOUNGE_PHASE=after --env SR_P108_LOUNGE_PART=1 --env SR_P108_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-loungeshots -- tests/p108-lounge-shots.spec.ts
 * (and the same with SR_P108_LOUNGE_PART=2)
 */
const ON = process.env.SR_P108_LOUNGE_ON === '1';
const PHASE = process.env.SR_P108_LOUNGE_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P108_LOUNGE_PART ?? '';
const OUT =
  process.env.SR_P108_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-loungeshots';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);
const FIG_NAME = `figures-${PHASE}${PART ? `-part${PART}` : ''}.json`;
const BASE_NAME = `figures-base${PART ? `-part${PART}` : ''}.json`;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);

interface World {
  guildId: string;
  channelId: string;
  threadId: string;
  dmId: string | null;
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

type Theme = 'lounge-light' | 'lounge-dark';
interface Cfg {
  w: number;
  h: number;
  touch: boolean;
  theme: Theme;
}
const PHONE: Cfg = { w: 375, h: 812, touch: true, theme: 'lounge-light' };
const DESK_L: Cfg = { w: 1024, h: 768, touch: false, theme: 'lounge-light' };
const DESK_D: Cfg = { w: 1024, h: 768, touch: false, theme: 'lounge-dark' };

// ---- the rows ------------------------------------------------------------------------------------------------------

interface Taker {
  at: string;
  tag: string;
  role: string | null;
  name: string;
}
/** A control: one row per control (hitTestAll or a named probe). */
interface HitRow {
  kind: 'hit' | 'named';
  page: string;
  bp: number;
  key: string;
  name: string;
  tag: string;
  status: string;
  box: { w: number; h: number } | null;
  takers: Taker[];
  takesCentre: boolean;
}
/** The size of a row or list around the cured controls. */
interface BoxRow {
  kind: 'box';
  page: string;
  bp: number;
  key: string;
  w: number;
  h: number;
  scrollH: number | null;
  clientH: number | null;
}
interface CountRow {
  kind: 'count';
  page: string;
  bp: number;
  key: string;
  count: number;
  failing: number;
}
interface FieldRow {
  kind: 'field';
  page: string;
  theme: string;
  key: string;
  data: Record<string, unknown>;
  problems: string[];
}
type Row = HitRow | BoxRow | CountRow | FieldRow;
const rows: Row[] = [];
const notes: string[] = [];
const problems: string[] = [];
const covered: string[] = [];

// ---- what is named -------------------------------------------------------------------------------------------------

interface Named {
  id: string;
  sel: string;
  /** a selector of the control's own label or wrapper (the input is tested alone, its label counts as it) */
  wrapper?: string;
  max?: number;
}
/** The named controls, tested one by one by `hitTest`. Which builder: lounge, shell. */
const NAMED: Named[] = [
  { id: 'reaction', sel: '.lng-reactions button' },
  { id: 'inline-btn', sel: '.lng-inline-hint .lng-link-btn' },
  { id: 'request-link', sel: '.lng-dm-empty .lng-link-btn' },
  { id: 'link-btn-standalone', sel: '.lng-link-btn:not(.sr-md .lng-link-btn)', max: 8 },
  { id: 'threads-tab', sel: '.lng-threads-tabs button' },
  { id: 'threads-search', sel: '.lng-threads-search' },
  { id: 'threads-item', sel: '.lng-threads-list button', max: 4 },
  { id: 'post-textarea', sel: '.lng-post-textarea' },
  { id: 'friends-button', sel: '.lng-friends .lng-header button' },
  { id: 'thread-chip', sel: '.lng-thread-chip', max: 6 },
  { id: 'composer-button', sel: '.lng-composer .lng-icon-btn' },
  { id: 'avatar', sel: 'button.lng-avatar-btn', max: 12 },
];
/** The named controls the task asks to cure: a failing one at 375 after is a problem line. The others are watched only. */
const CURED = new Set([
  'reaction',
  'inline-btn',
  'request-link',
  'link-btn-standalone',
  'threads-tab',
  'threads-search',
  'post-textarea',
  'friends-button',
]);
/** Written "accepted" (A.4): inside a line of running text. Counted, never listed as problems. */
const ACCEPTED: Named[] = [
  { id: 'name-btn', sel: '.lng-name-btn' },
  { id: 'mention', sel: '.sr-md-mention' },
  { id: 'spoiler', sel: '.sr-md-spoiler' },
  { id: 'md-link', sel: '.sr-md a' },
  { id: 'embed-title-link', sel: '.lng-embed-title a' },
];
/** The rows and lists the cures may move (or must not), measured by size. */
const BOXES: Named[] = [
  { id: 'composer', sel: '.lng-composer', max: 1 },
  { id: 'composer-field', sel: '.lng-composer-field', max: 1 },
  { id: 'composer-textarea', sel: '.lng-composer .lng-textarea', max: 1 },
  { id: 'composer-button', sel: '.lng-composer .lng-icon-btn' },
  { id: 'message', sel: 'article.lng-msg', max: 14 },
  { id: 'message-rx', sel: 'article.lng-msg:has(.lng-reactions)', max: 6 },
  { id: 'message-plain', sel: 'article.lng-msg:not(:has(.lng-reactions))', max: 14 },
  { id: 'message-name', sel: '.lng-name-btn', max: 14 },
  { id: 'avatar', sel: 'button.lng-avatar-btn', max: 12 },
  { id: 'reactions-row', sel: '.lng-reactions', max: 8 },
  { id: 'reaction', sel: '.lng-reactions button', max: 12 },
  { id: 'thread-chip', sel: '.lng-thread-chip', max: 6 },
  { id: 'inline-edit', sel: '.lng-inline-edit', max: 1 },
  { id: 'inline-textarea', sel: '.lng-inline-textarea', max: 1 },
  { id: 'inline-hint', sel: '.lng-inline-hint', max: 1 },
  { id: 'inline-btn', sel: '.lng-inline-hint .lng-link-btn' },
  { id: 'thread-head', sel: '.lng-thread-head', max: 1 },
  { id: 'sys-thread', sel: '.lng-sys-thread', max: 4 },
  { id: 'dm-empty', sel: '.lng-dm-empty', max: 1 },
  { id: 'request-link', sel: '.lng-dm-empty .lng-link-btn' },
  { id: 'nav-item', sel: '.lng-nav-item', max: 4 },
  { id: 'find', sel: '.lng-find', max: 1 },
  { id: 'threads-pop', sel: '.lng-threads-pop', max: 1 },
  { id: 'threads-tabs', sel: '.lng-threads-tabs', max: 1 },
  { id: 'threads-tab', sel: '.lng-threads-tabs button', max: 4 },
  { id: 'threads-search', sel: '.lng-threads-search', max: 1 },
  { id: 'threads-list', sel: '.lng-threads-list', max: 1 },
  { id: 'threads-item', sel: '.lng-threads-list button', max: 4 },
  { id: 'post-textarea', sel: '.lng-post-textarea', max: 1 },
  { id: 'friends-header', sel: '.lng-friends .lng-header', max: 1 },
  { id: 'friends-button', sel: '.lng-friends .lng-header button' },
  { id: 'friends-tabs', sel: '.lng-friends-tabs', max: 1 },
  { id: 'friends-body', sel: '.lng-friends-body', max: 1 },
];

// ---- helpers -------------------------------------------------------------------------------------------------------

async function newPage(browser: Browser, owner: BrowserContext, cfg: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: cfg.w, height: cfg.h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(cfg.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  }, cfg.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function setTheme(owner: BrowserContext, theme: Theme) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
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
      .catch(() => notes.push(`${url}: ready locator not seen`));
  await settle(page);
}
const tap = async (loc: Locator) => {
  await loc
    .first()
    .scrollIntoViewIfNeeded({ timeout: 5000 })
    .catch(() => undefined);
  await loc.first().click({ timeout: 6000 });
};
const esc = async (page: Page) => {
  await page.keyboard.press('Escape').catch(() => undefined);
  await page.waitForTimeout(400);
};
const main = (page: Page) => page.locator('#lounge-main');

async function shot(page: Page, name: string, suffix: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}-${suffix}: scrollWidth/clientWidth ${sw}/${cw}`);
  if (sw > cw) problems.push(`${name}-${suffix}: the page scrolls sideways (${sw} > ${cw})`);
  await page.mouse.move(1, 1).catch(() => undefined);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}-${suffix}.png`) });
}

/** Every named control, accepted ones, and every box, on the page as it is now. */
async function measure(page: Page, pageName: string, bp: number, touch: boolean, root: Locator) {
  const reach: HitOptions = touch ? {} : { reach: 'box' };
  for (const n of BOXES) {
    const got = await root
      .locator(n.sel)
      .evaluateAll(
        (els, max) =>
          els.slice(0, max).map((e) => {
            const r = e.getBoundingClientRect();
            const hid = getComputedStyle(e).display === 'none' || r.width === 0;
            return {
              hid,
              w: Math.round(r.width * 10) / 10,
              h: Math.round(r.height * 10) / 10,
              s: e.scrollHeight,
              c: e.clientHeight,
            };
          }),
        n.max ?? 40,
      )
      .catch(() => []);
    got
      .filter((g) => !g.hid)
      .forEach((g, i) =>
        rows.push({
          kind: 'box',
          page: pageName,
          bp,
          key: `${n.id}#${i}`,
          w: g.w,
          h: g.h,
          scrollH: g.s,
          clientH: g.c,
        }),
      );
  }
  for (const n of ACCEPTED) {
    const loc = root.locator(n.sel);
    const count = await loc.count();
    let failing = 0;
    for (let i = 0; i < Math.min(count, 12); i++) {
      if (
        !(await loc
          .nth(i)
          .isVisible()
          .catch(() => false))
      )
        continue;
      try {
        const r = await hitTest(page, loc.nth(i), reach);
        if (!r.pass) failing++;
      } catch {
        /* not reachable: not counted */
      }
    }
    rows.push({ kind: 'count', page: pageName, bp, key: `accepted:${n.id}`, count, failing });
  }
  for (const n of NAMED) {
    const loc = root.locator(n.sel);
    const count = Math.min(await loc.count(), n.max ?? 20);
    for (let i = 0; i < count; i++) {
      const one = loc.nth(i);
      if (!(await one.isVisible().catch(() => false))) continue;
      const name = (
        (await one.getAttribute('aria-label').catch(() => null)) ??
        (await one.innerText().catch(() => '')) ??
        ''
      )
        .trim()
        .replace(/\s+/g, ' ')
        .slice(0, 40);
      const row: HitRow = {
        kind: 'named',
        page: pageName,
        bp,
        key: `${n.id}#${i}`,
        name,
        tag: n.id,
        status: 'pass',
        box: null,
        takers: [],
        takesCentre: false,
      };
      try {
        const r = await hitTest(page, one, {
          ...reach,
          ...(n.wrapper ? { wrapper: n.wrapper } : {}),
        });
        row.box = { w: Math.round(r.box.width), h: Math.round(r.box.height) };
        row.status = r.pass ? 'pass' : 'fail';
        const bad = r.points.filter((p) => !p.ok);
        row.takers = bad.map((p) => ({
          at: p.at,
          tag: p.taker?.tag ?? '',
          role: p.taker?.role ?? null,
          name: p.taker?.name ?? '',
        }));
        row.takesCentre = bad.some((p) => p.at === 'centre');
      } catch (e) {
        row.status = 'not-reached';
        notes.push(`${pageName} ${bp}: ${n.id}#${i} not reached (${msg(e)})`);
      }
      rows.push(row);
    }
  }
  // Every control of the region.
  if ((await root.count()) === 0) {
    notes.push(`${pageName} ${bp}: root not found`);
    return;
  }
  await openCollapsed(page, root.first()).catch(() => undefined);
  const all = await hitTestAll(page, root.first(), reach);
  const seen = new Map<string, number>();
  for (const r of all) {
    const base = `${r.tag}|${r.role ?? ''}|${r.name}`;
    const n = (seen.get(base) ?? 0) + 1;
    seen.set(base, n);
    const bad = (r.result?.points ?? []).filter((p) => !p.ok);
    rows.push({
      kind: 'hit',
      page: pageName,
      bp,
      key: `${base}#${n}`,
      name: r.name,
      tag: r.tag,
      status: r.status,
      box: r.result
        ? { w: Math.round(r.result.box.width), h: Math.round(r.result.box.height) }
        : null,
      takers: bad.map((p) => ({
        at: p.at,
        tag: p.taker?.tag ?? '',
        role: p.taker?.role ?? null,
        name: p.taker?.name ?? '',
      })),
      takesCentre: bad.some((p) => p.at === 'centre'),
    });
  }
  const f = all.filter((r) => r.status === 'fail').length;
  say(
    `${pageName} @${bp}: ${all.length} rows, ${f} fail, ${all.filter((r) => r.status === 'not-reached').length} not-reached`,
  );
  for (const r of all.filter((x) => x.status === 'not-reached'))
    notes.push(`${pageName} ${bp}: not-reached ${r.tag} "${r.name}" ${r.reason ?? ''}`);
}

// ---- the fields (cure B) -------------------------------------------------------------------------------------------

interface FieldSpec {
  id: string;
  /** the field */
  field: () => Locator;
  /** the element that draws the box and the ring (the composer's wrapper), when it is not the field */
  wrap?: string;
  /** 'placeholder': only the placeholder is checked (the inline edit draws no border) */
  kind: 'composer' | 'full' | 'placeholder';
  /** the field has no placeholder attribute (the New Post textarea): the placeholder is read, never a problem */
  noPh?: boolean;
}

const readField = (loc: Locator, wrapSel: string | null) =>
  loc.first().evaluate((f, ws) => {
    const probe = document.createElement('span');
    document.body.appendChild(probe);
    const rgb = (v: string) => {
      probe.style.color = '';
      probe.style.color = v.trim();
      return getComputedStyle(probe).color;
    };
    const w = ws ? (f.closest(ws) as HTMLElement | null) : null;
    const pick = (e: Element) => {
      const c = getComputedStyle(e);
      return {
        borderColor: c.borderTopColor,
        borderWidth: c.borderTopWidth,
        borderStyle: c.borderTopStyle,
        outlineStyle: c.outlineStyle,
        outlineWidth: c.outlineWidth,
        outlineColor: c.outlineColor,
        outlineOffset: c.outlineOffset,
        bg: c.backgroundColor,
      };
    };
    // The ground: backgrounds up the tree until one is opaque (list, innermost first).
    const ground = (e: Element) => {
      const st: string[] = [];
      for (let a = e.parentElement; a; a = a.parentElement) {
        const bg = getComputedStyle(a).backgroundColor;
        if (bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {
          st.push(bg);
          if (/^rgb\(/.test(bg) || /, 1\)$/.test(bg)) break;
        }
      }
      return st;
    };
    const cs = getComputedStyle(f);
    const ph = getComputedStyle(f, '::placeholder');
    const out = {
      field: pick(f),
      wrap: w ? pick(w) : null,
      placeholder: ph.color,
      placeholderOpacity: ph.opacity,
      fieldGround: ground(f),
      wrapGround: w ? ground(w) : null,
      tokens: {
        border: rgb(cs.getPropertyValue('--ui-border') || 'transparent'),
        faint: rgb(cs.getPropertyValue('--ui-text-faint') || 'transparent'),
        focus: rgb(cs.getPropertyValue('--ui-focus') || 'transparent'),
      },
    };
    probe.remove();
    return out;
  }, wrapSel);

type FieldRead = Awaited<ReturnType<typeof readField>>;

const flat = (stack: string[]): Rgba => {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const c of [...stack].reverse()) g = over(parseColor(c), g);
  return g;
};
const r2 = (n: number) => Math.round(n * 100) / 100;
const hexOf = (c: Rgba) =>
  `#${[c.r, c.g, c.b].map((v) => Math.round(v).toString(16).padStart(2, '0')).join('')}`;

async function fieldStates(page: Page, spec: FieldSpec, theme: string, pageName: string) {
  const f = spec.field();
  if ((await f.count()) === 0) {
    notes.push(`${pageName} / ${spec.id}: field not found (${theme})`);
    return;
  }
  const wrapSel = spec.wrap ?? null;
  const box = await f.first().boundingBox();
  await page.mouse.move(1, 1);
  await f.first().evaluate((e) => (e as HTMLElement).blur());
  await page.waitForTimeout(250);
  const rest = await readField(f, wrapSel);
  if (box) await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.waitForTimeout(250);
  const hover = await readField(f, wrapSel);
  await page.mouse.move(1, 1);
  // Keyboard focus: the field is focused by script after a key press, which makes :focus-visible match as it does for Tab.
  await page.keyboard.press('Shift').catch(() => undefined);
  await f.first().focus();
  await page.waitForTimeout(300);
  const focus = await readField(f, wrapSel);

  const drawn = (s: FieldRead) => s.wrap ?? s.field; // the element that draws the box
  const ground = flat(rest.wrapGround ?? rest.fieldGround);
  const fieldGround = over(parseColor(drawn(rest).bg), ground);
  const probs: string[] = [];
  const data: Record<string, unknown> = {
    rest: drawn(rest),
    hover: drawn(hover),
    focus: { field: focus.field, wrap: focus.wrap },
    placeholder: rest.placeholder,
    placeholderOpacity: rest.placeholderOpacity,
    tokens: rest.tokens,
    ground: hexOf(ground),
    fieldGround: hexOf(fieldGround),
  };
  const bc = (s: FieldRead) => over(parseColor(drawn(s).borderColor), fieldGround);
  const hasBorder = drawn(rest).borderStyle !== 'none' && parseFloat(drawn(rest).borderWidth) > 0;
  if (hasBorder) {
    data.borderRatioOutside = r2(ratio(bc(rest), ground));
    data.borderRatioField = r2(ratio(bc(rest), fieldGround));
  }
  const ringEl = spec.kind === 'composer' ? (focus.wrap ?? focus.field) : focus.field;
  const ringOutside = ringEl === focus.field ? flat(rest.fieldGround) : ground;
  const ringOk = ringEl.outlineStyle !== 'none' && parseFloat(ringEl.outlineWidth) > 0;
  if (ringOk)
    data.ringRatioOutside = r2(
      ratio(over(parseColor(ringEl.outlineColor), ringOutside), ringOutside),
    );
  const phC = over(parseColor(rest.placeholder), fieldGround);
  data.placeholderRatio = r2(ratio(phC, fieldGround));
  const sameColour = (a: string, b: string) => {
    const x = parseColor(a);
    const y = parseColor(b);
    return Math.abs(x.r - y.r) < 1.5 && Math.abs(x.g - y.g) < 1.5 && Math.abs(x.b - y.b) < 1.5;
  };
  if (PHASE === 'after') {
    if (spec.kind === 'placeholder') {
      /* only the placeholder below */
    } else if (!ringOk) probs.push('no focus ring drawn (outline none or 0 px)');
    else {
      const wantW = spec.kind === 'composer' ? 1 : 2;
      const wantO = spec.kind === 'composer' ? -1 : 1;
      if (Math.round(parseFloat(ringEl.outlineWidth)) !== wantW)
        probs.push(`ring width ${ringEl.outlineWidth}, wanted ${wantW}px`);
      if (Math.round(parseFloat(ringEl.outlineOffset)) !== wantO)
        probs.push(`ring offset ${ringEl.outlineOffset}, wanted ${wantO}px`);
      if (ringEl.outlineStyle !== 'solid') probs.push(`ring style ${ringEl.outlineStyle}`);
      if (!sameColour(ringEl.outlineColor, rest.tokens.focus))
        probs.push(`ring colour ${ringEl.outlineColor} is not --ui-focus ${rest.tokens.focus}`);
      if ((data.ringRatioOutside as number) < 3)
        probs.push(`ring ratio ${data.ringRatioOutside} < 3.0`);
    }
    if (spec.kind === 'full') {
      if (!hasBorder) probs.push('no border at rest');
      else {
        if ((data.borderRatioOutside as number) < 3)
          probs.push(`rest border ratio against outside ${data.borderRatioOutside} < 3.0`);
        if ((data.borderRatioField as number) < 3)
          probs.push(`rest border ratio against field ${data.borderRatioField} < 3.0`);
        if (!sameColour(drawn(hover).borderColor, rest.tokens.faint))
          probs.push(
            `hover border ${drawn(hover).borderColor} is not --ui-text-faint ${rest.tokens.faint}`,
          );
      }
    }
    if (spec.noPh) {
      /* no placeholder attribute: the read stays in the figures */
    } else if (!sameColour(rest.placeholder, rest.tokens.faint))
      probs.push(`placeholder ${rest.placeholder} is not --ui-text-faint ${rest.tokens.faint}`);
    if (!spec.noPh && rest.placeholderOpacity !== '1')
      probs.push(`placeholder opacity ${rest.placeholderOpacity}`);
    if (!spec.noPh && phC && (data.placeholderRatio as number) < 4.5)
      probs.push(`placeholder ratio ${data.placeholderRatio} < 4.5`);
  }
  for (const p of probs) problems.push(`${pageName} / ${spec.id} (${theme}): ${p}`);
  rows.push({ kind: 'field', page: pageName, theme, key: spec.id, data, problems: probs });
  say(`${pageName} / ${spec.id} (${theme}): ${JSON.stringify(data).slice(0, 400)}`);
}

// ---- pages ---------------------------------------------------------------------------------------------------------

const composerField = (page: Page): FieldSpec => ({
  id: 'composer',
  field: () => page.locator('.lng-composer .lng-textarea'),
  wrap: '.lng-composer',
  kind: 'composer',
});

/** The inline edit: ArrowUp in the empty composer edits the member's last message. */
async function openInlineEdit(page: Page): Promise<boolean> {
  const ta = page.locator('.lng-composer .lng-textarea').first();
  await ta.click({ timeout: 6000 }).catch(() => undefined);
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(800);
  return (await page.locator('.lng-inline-textarea').count()) > 0;
}

const openMore = async (page: Page) => {
  const more = page.getByRole('button', { name: 'More actions' });
  if ((await more.count()) > 0 && (await more.first().getAttribute('aria-expanded')) === 'false')
    await more.first().click({ timeout: 5000 });
};

/** The Home tab of the phone's tab bar. */
const tabHome = (page: Page) =>
  page.locator('nav.lng-tabbar').getByRole('button', { name: /^Home$/ });
const friendsLink = (page: Page) =>
  page.getByRole('button', { name: /^Friends$/ }).or(page.getByRole('link', { name: /^Friends$/ }));

/** One page of the text channel at one width: the page, the inline edit (emptied, cancel, save), and the thread. */
async function channelAt(
  browser: Browser,
  owner: BrowserContext,
  w: World,
  cfg: Cfg,
  tag: string,
  roomId: string | null,
) {
  const { ctx, page } = await newPage(browser, owner, cfg);
  const touch = cfg.touch;
  const bp = cfg.w;
  const suffix = touch ? '375' : `${cfg.w}-${cfg.theme === 'lounge-dark' ? 'dark' : 'light'}`;
  const light = touch || cfg.theme === 'lounge-light';
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    const dt = await page.evaluate(
      () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    );
    say(`${tag}: data-theme ${dt}`);
    if (!touch) await fieldStates(page, composerField(page), cfg.theme, `channel@${bp}`);
    if (light) {
      await measure(page, 'channel', bp, touch, main(page));
      await shot(page, 'channel', suffix);
      covered.push(`channel @${bp}`);
    }
    // The inline edit, its textarea emptied so the placeholder would show; then cancel and save by a tap.
    try {
      const room = `/channels/${w.guildId}/${roomId ?? w.channelId}`;
      await go(page, room, page.getByRole('log'));
      if (await openInlineEdit(page)) {
        await page
          .locator('.lng-inline-textarea')
          .fill('')
          .catch(() => undefined);
        await fieldStates(
          page,
          {
            id: 'inline-edit',
            field: () => page.locator('.lng-inline-textarea'),
            kind: 'placeholder',
            noPh: false,
          },
          cfg.theme,
          `inline-edit@${bp}`,
        );
        if (light) {
          await measure(page, 'inline-edit', bp, touch, main(page));
          await shot(page, 'inline-edit-empty', suffix);
          covered.push(`inline-edit @${bp}`);
        }
        if (touch) {
          const hint = page.locator('.lng-inline-hint');
          await hint
            .getByRole('button', { name: /^cancel$/i })
            .click({ timeout: 5000 })
            .catch((e) => notes.push(`inline edit: cancel not tapped (${msg(e)})`));
          await page.waitForTimeout(500);
          const closed1 = (await page.locator('.lng-inline-textarea').count()) === 0;
          say(`inline edit @${bp}: cancel closes it: ${closed1}`);
          if (!closed1) notes.push('inline edit: the cancel button did not close the edit');
          if (await openInlineEdit(page)) {
            await hint
              .getByRole('button', { name: /^save$/i })
              .click({ timeout: 5000 })
              .catch((e) => notes.push(`inline edit: save not tapped (${msg(e)})`));
            await page.waitForTimeout(900);
            const closed2 = (await page.locator('.lng-inline-textarea').count()) === 0;
            say(`inline edit @${bp}: save closes it: ${closed2}`);
            if (!closed2) notes.push('inline edit: the save button did not close the edit');
          } else notes.push('inline edit: did not reopen after cancel');
        }
      } else notes.push(`inline-edit @${bp}: did not open (no own last message?)`);
    } catch (e) {
      notes.push(`inline-edit @${bp}: ${msg(e)}`);
    }
    await esc(page);
    // A thread (once per width, Light): "Jump to original" and the quoted head.
    if (light) {
      await go(page, `${url}/threads/${w.threadId}`, page.getByRole('log'));
      await measure(page, 'thread', bp, touch, page.locator('aside.lng-thread-pane'));
      await shot(page, 'thread', suffix);
      covered.push(`thread @${bp}`);
    }
  } finally {
    await ctx.close();
  }
}

async function part1(browser: Browser, owner: BrowserContext, w: World) {
  let roomId: string | null = null;
  try {
    const room = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p108lounge-room-${STAMP}`,
      type: 0,
    });
    roomId = room.id;
    await sleep(800);
    await apiPost(owner.request, `/channels/${room.id}/messages`, {
      content: 'p108 lounge message to edit',
    });
  } catch (e) {
    notes.push(`edit room not made: ${msg(e)}`);
  }
  try {
    await setTheme(owner, 'lounge-light');
    await channelAt(browser, owner, w, PHONE, 'channel-375', roomId);
    await channelAt(browser, owner, w, DESK_L, 'channel-1024-light', roomId);
    await setTheme(owner, 'lounge-dark');
    await channelAt(browser, owner, w, DESK_D, 'channel-1024-dark', roomId);
    await setTheme(owner, 'lounge-light');
  } finally {
    if (roomId)
      await apiDelete(owner.request, `/channels/${roomId}`).catch((e) =>
        notes.push(`room ${roomId} (p108lounge-room-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
}

/** The Threads popover of the channel header (a phone: inside "More actions"). */
async function threadsPopover(page: Page, url: string) {
  await go(page, url, page.getByRole('log'));
  await openMore(page);
  await tap(page.getByRole('button', { name: 'Threads', exact: true }));
  await page.locator('.lng-threads-pop').first().waitFor({ timeout: 8000 });
  await settle(page, 2000);
}

/** The forum's New Post dialog. */
async function newPostDialog(page: Page, furl: string) {
  await go(page, furl, page.getByRole('combobox', { name: 'Sort by' }));
  await tap(page.getByRole('button', { name: /new post|create post/i }));
  await page.getByRole('dialog').last().waitFor({ timeout: 8000 });
  await settle(page, 3000);
  await page.waitForTimeout(1200); // the dialog scales open
}

async function part2(browser: Browser, owner: BrowserContext, w: World) {
  const o = owner.request;
  let forumId: string | null = null;
  await setTheme(owner, 'lounge-light');
  try {
    try {
      const f = await apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
        name: `p108lounge-forum-${STAMP}`,
        type: 15,
      });
      forumId = f.id;
      await sleep(1200);
      await apiPost(o, `/channels/${f.id}/threads`, {
        name: `p108lounge-post-${STAMP}`,
        message: { content: 'A post for the lounge shots.' },
      });
    } catch (e) {
      notes.push(`forum not made: ${msg(e)}`);
    }
    const curl = `/channels/${w.guildId}/${w.channelId}`;
    const left = (page: Page) => page.locator('.lng-left, nav.lng-tabbar');

    for (const cfg of [PHONE, DESK_L]) {
      const touch = cfg.touch;
      const bp = cfg.w;
      const suffix = touch ? '375' : `${cfg.w}-light`;
      const { ctx, page } = await newPage(browser, owner, cfg);
      try {
        // The Home list with the world's waiting message request: by URL, and (phone) from the tab bar.
        try {
          await go(page, '/channels/@me', page.locator('.lng-left'));
          await measure(page, 'home-list-url', bp, touch, left(page));
          await shot(page, 'home-list-url', suffix);
          covered.push(`home-list-url @${bp}`);
        } catch (e) {
          notes.push(`home-list-url @${bp}: ${msg(e)}`);
        }
        // The Friends page: by URL (/channels/@me, then "Friends"), and (phone) from the tab bar: Friends, the tab bar's
        // Home (back to the Home list: measured as home-list-tab), Friends again.
        const entries: ('url' | 'tab')[] = touch ? ['url', 'tab'] : ['url'];
        for (const how of entries) {
          try {
            await go(page, '/channels/@me', page.locator('.lng-left'));
            await tap(friendsLink(page));
            await settle(page, 2000);
            if (how === 'tab') {
              await tap(tabHome(page));
              await settle(page, 2500);
              await measure(page, 'home-list-tab', bp, touch, left(page));
              await shot(page, 'home-list-tab', suffix);
              covered.push(`home-list-tab @${bp}`);
              await tap(friendsLink(page));
              await settle(page, 2500);
            }
            await page.locator('section.lng-friends').first().waitFor({ timeout: 8000 });
            await measure(page, `friends-${how}`, bp, touch, page.locator('section.lng-friends'));
            await shot(page, `friends-${how}`, suffix);
            covered.push(`friends-${how} @${bp}`);
          } catch (e) {
            notes.push(`friends-${how} @${bp}: ${msg(e)}`);
          }
        }
        // The Threads popover.
        try {
          await threadsPopover(page, curl);
          await measure(page, 'threads-popover', bp, touch, page.locator('.lng-threads-pop'));
          await shot(page, 'threads-popover', suffix);
          covered.push(`threads-popover @${bp}`);
        } catch (e) {
          notes.push(`threads-popover @${bp}: ${msg(e)}`);
        }
        // The forum's New Post dialog.
        if (forumId) {
          try {
            await newPostDialog(page, `/channels/${w.guildId}/${forumId}`);
            await measure(page, 'new-post', bp, touch, page.getByRole('dialog').last());
            await shot(page, 'new-post', suffix);
            covered.push(`new-post @${bp}`);
          } catch (e) {
            notes.push(`new-post @${bp}: ${msg(e)}`);
          }
        }
        // The fields (desktop): the Threads search and the New Post textarea.
        if (!touch) {
          try {
            await threadsPopover(page, curl);
            const s = page.locator('.lng-threads-search');
            await fieldStates(
              page,
              { id: 'threads-search', field: () => s, kind: 'full' },
              cfg.theme,
              `threads@${bp}`,
            );
          } catch (e) {
            notes.push(`threads fields @${bp}: ${msg(e)}`);
          }
          if (forumId) {
            try {
              await newPostDialog(page, `/channels/${w.guildId}/${forumId}`);
              await fieldStates(
                page,
                {
                  id: 'post-textarea',
                  field: () => page.locator('.lng-post-textarea'),
                  kind: 'full',
                  noPh: true,
                },
                cfg.theme,
                `new-post@${bp}`,
              );
            } catch (e) {
              notes.push(`new-post field @${bp}: ${msg(e)}`);
            }
          }
        }
      } finally {
        await ctx.close();
      }
    }
    // Dark: the fields only.
    try {
      await setTheme(owner, 'lounge-dark');
      const { ctx, page } = await newPage(browser, owner, DESK_D);
      try {
        await threadsPopover(page, curl);
        await fieldStates(
          page,
          { id: 'threads-search', field: () => page.locator('.lng-threads-search'), kind: 'full' },
          DESK_D.theme,
          'threads@1024',
        );
        if (forumId) {
          await newPostDialog(page, `/channels/${w.guildId}/${forumId}`);
          await fieldStates(
            page,
            {
              id: 'post-textarea',
              field: () => page.locator('.lng-post-textarea'),
              kind: 'full',
              noPh: true,
            },
            DESK_D.theme,
            'new-post@1024',
          );
        }
      } finally {
        await ctx.close();
      }
    } catch (e) {
      notes.push(`dark fields: ${msg(e)}`);
    } finally {
      await setTheme(owner, 'lounge-light').catch(() => undefined);
    }
  } finally {
    if (forumId)
      await apiDelete(o, `/channels/${forumId}`).catch((e) =>
        notes.push(`forum ${forumId} (p108lounge-forum-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
}

// ---- the comparison (after phase) ----------------------------------------------------------------------------------

/** Growth in height a box may have at 375 (D-990 lists of the two builders); everything else: none. */
const MAY_GROW_375: Record<string, number> = {
  'message-rx': 12,
  message: 12,
  'inline-edit': 30,
  'inline-hint': 30,
  'inline-btn': 30,
  'thread-head': 30,
  'sys-thread': 30,
  'dm-empty': 30,
  'request-link': 30,
  'threads-pop': 1e9,
  'threads-list': 1e9,
  'threads-tabs': 12.5,
  'threads-tab': 12.5,
  'threads-search': 15.5,
  'post-textarea': 1e9,
  'friends-header': 4,
};
/** Pages where a message row holds a cured foot line (the inline edit's, the thread's "Jump to original"). */
const MESSAGE_ROWS_GROW: Record<string, number> = { 'inline-edit': 40, thread: 30 };

function compare(base: Row[], after: Row[]) {
  const key = (r: Row) => `${r.kind}|${r.page}|${'bp' in r ? r.bp : ''}|${r.key}`;
  const bm = new Map(base.map((r) => [key(r), r]));
  const out: string[] = [];
  for (const a of after) {
    const b = bm.get(key(a));
    const tk = (x: HitRow) => {
      const t = x.takers[0];
      return t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}" at ${t.at}` : '-';
    };
    if (a.kind === 'named' && a.bp === 375 && a.status !== 'pass') {
      const was = b && b.kind === 'named' ? b.status : 'no base';
      if (CURED.has(a.tag) || was === 'pass')
        out.push(
          `375 named control ${CURED.has(a.tag) ? 'still ' : 'now '}${a.status}: ${a.page} / ${a.tag} "${a.name}" ${a.box ? `${a.box.w}x${a.box.h}` : ''} taker ${tk(a)} (base: ${was})`,
        );
      else
        say(
          `info: 375 ${a.page} / ${a.tag} "${a.name}" ${a.status} (not in the task; base ${was})`,
        );
    }
    if (a.kind === 'named' && a.bp !== 375 && a.status === 'fail')
      out.push(`${a.bp} named control fails the box hit test: ${a.page} / ${a.tag} "${a.name}"`);
    if (a.kind === 'hit' && b && b.kind === 'hit' && b.status === 'pass' && a.status === 'fail')
      out.push(
        `${a.bp}: passed in base, fails after: ${a.page} / ${a.tag} "${a.name}" taker ${tk(a)}${a.takesCentre ? ' (its CENTRE line is taken: a tap area covers it)' : ''}`,
      );
    if (a.kind === 'hit' && b && b.kind === 'hit' && b.status === 'fail' && a.status === 'fail') {
      const bt = b.takers[0]?.name;
      const at = a.takers[0]?.name;
      if (a.takesCentre && !b.takesCentre)
        out.push(
          `${a.bp}: now its centre is taken: ${a.page} / ${a.tag} "${a.name}" taker "${at}"`,
        );
      else if (bt !== at)
        say(`info: still failing, taker changed ${a.page} / "${a.name}": "${bt}" -> "${at}"`);
    }
    if (a.kind === 'hit' && !b && a.status === 'fail' && a.bp === 375)
      out.push(`375: new failing row with no base twin: ${a.page} / ${a.tag} "${a.name}"`);
    if (a.kind === 'box' && b && b.kind === 'box') {
      const dw = a.w - b.w;
      const dh = a.h - b.h;
      const id = a.key.split('#')[0]!;
      if (a.bp !== 375) {
        if (Math.abs(dw) > 0.5 || Math.abs(dh) > 0.5)
          out.push(
            `${a.bp}: moved at a pointer width: ${a.page} / ${a.key} ${b.w}x${b.h} -> ${a.w}x${a.h}`,
          );
      } else {
        if (Math.abs(dh) > 0.5 || Math.abs(dw) > 0.5)
          say(`info: 375 ${a.page} / ${a.key}: ${b.w}x${b.h} -> ${a.w}x${a.h}`);
        if (Math.abs(dw) > 0.5)
          out.push(`375: width changed: ${a.page} / ${a.key} ${b.w} -> ${a.w}`);
        const isMsg = id === 'message' || id === 'message-rx' || id === 'message-plain';
        const allow =
          isMsg && MESSAGE_ROWS_GROW[a.page] !== undefined
            ? MESSAGE_ROWS_GROW[a.page]!
            : id === 'message-plain'
              ? 0
              : (MAY_GROW_375[id] ?? 0);
        if (dh > allow + 0.5)
          out.push(
            `375: grew more than allowed (${allow}): ${a.page} / ${a.key} height ${b.h} -> ${a.h}`,
          );
        if (dh < -0.5 && !isMsg && id !== 'dm-empty')
          out.push(`375: shrank: ${a.page} / ${a.key} height ${b.h} -> ${a.h}`);
      }
    }
    if (a.kind === 'box' && !b) say(`info: box without base twin ${a.page} @${a.bp} ${a.key}`);
    if (a.kind === 'count' && b && b.kind === 'count' && a.count !== b.count)
      out.push(
        `count of ${a.key} on ${a.page} @${a.bp}: ${b.count} -> ${a.count} (the world differs?)`,
      );
  }
  return out;
}

// ---- the test ------------------------------------------------------------------------------------------------------

test(`wave 108 lounge pod: lounge and shell, ${PHASE}, part ${PART || 'all'}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    if (PART === '' || PART === '1') await part1(browser, ownerCtx, w);
    if (PART === '' || PART === '2') await part2(browser, ownerCtx, w);
  } finally {
    const baseFile = resolve(OUT, BASE_NAME);
    if (PHASE === 'after') {
      if (existsSync(baseFile)) {
        const base = JSON.parse(readFileSync(baseFile, 'utf8')) as { rows: Row[] };
        problems.push(...compare(base.rows, rows));
      } else notes.push(`no ${BASE_NAME} in OUT: nothing compared`);
    }
    writeFileSync(
      resolve(OUT, FIG_NAME),
      JSON.stringify(
        { phase: PHASE, part: PART || 'all', rows, notes, problems, covered },
        null,
        1,
      ),
    );
    for (const n of notes) say(`note: ${n}`);
    const named = rows.filter((r): r is HitRow => r.kind === 'named');
    const fails = (bp: number) => named.filter((r) => r.bp === bp && r.status === 'fail').length;
    const hits = rows.filter((r): r is HitRow => r.kind === 'hit');
    say(
      `${PHASE}: named controls ${named.length} (375 fail ${fails(375)}, 1024 fail ${fails(1024)}); all rows at 375: ${hits.filter((r) => r.bp === 375).length}, fail ${hits.filter((r) => r.bp === 375 && r.status === 'fail').length}; at 1024 fail ${hits.filter((r) => r.bp === 1024 && r.status === 'fail').length}`,
    );
    for (const r of named.filter((x) => x.bp === 375 && x.status === 'fail'))
      say(
        `  375 FAIL ${r.page} / ${r.tag} "${r.name}" ${r.box ? `${r.box.w}x${r.box.h}` : ''} taker ${r.takers[0] ? `${r.takers[0].tag} "${r.takers[0].name}" at ${r.takers[0].at}` : '-'}`,
      );
    for (const c of rows.filter((r): r is CountRow => r.kind === 'count' && r.count > 0))
      say(`accepted ${c.key} ${c.page} @${c.bp}: ${c.count}, failing ${c.failing}`);
    for (const p of problems) say(`PROBLEM ${p}`);
    await ownerCtx.close();
  }
});
