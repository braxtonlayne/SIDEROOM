import { mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTest, hitTestAll, openCollapsed, type HitOptions } from '../support/hit-test';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';

/**
 * Wave 107, loungeshots (pod lounge A: compose and msgs; D-1022 the 44 px target, D-1023 the composer shows focus,
 * D-1024 the field contract). Two phases (SR_P107_LOUNGE_PHASE=base|after), both in built mode on the seeded sweep world.
 *
 * Per page, at 375 x 812 touch (Lounge Light): `openCollapsed`, `hitTestAll` over the page's region (default reach), every
 * row whole; the NAMED controls (the five composer buttons, the emoji cells, categories and search, the avatar, the
 * standalone link buttons, the reaction pills, the tab bars' tabs, the inbox filter checkboxes) each by `hitTest`; the
 * accepted ones (author name, mentions, spoilers, links in message text, embed titles) counted only; the boxes of the
 * rows and lists around them. At 1024 x 768 (Lounge Light): the same boxes and a `reach: 'box'` hit test; after must
 * equal base. At 1024 in Lounge Light and Lounge Dark: the fields under cure B (composer wrapper ring, inline edit,
 * emoji search): border at rest and on hover, outline with keyboard focus, placeholder, ratios from contrast.ts.
 *
 * Data: the seeded world only, plus (part 2) one forum channel `p107lounge-forum-<stamp>` with one post, made through the
 * API and removed in a `finally`. The DM of the world is a message request: the spec accepts it (the world is fresh).
 *
 * Parts (SR_P107_LOUNGE_PART): 1 = the text channel (page, emoji picker and its categories, GIF/Sticker tabs, inline
 * edit) and a thread, at 375 and at 1024 (Light, and Dark for the fields); 2 = forum post, DM, inbox, Home list. Without
 * PART both run. Figures: figures-<phase>-part<N>.json (figures-<phase>.json without PART) in OUT; in the after phase the
 * spec compares with the base file of the same name when it is there and lists "problem" lines (it never fails on them).
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_LOUNGE_ON=1 --env SR_P107_LOUNGE_PHASE=after --env SR_P107_LOUNGE_PART=1 --env SR_P107_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-loungeshots -- tests/p107-lounge-shots.spec.ts
 * (and the same with SR_P107_LOUNGE_PART=2)
 */
const ON = process.env.SR_P107_LOUNGE_ON === '1';
const PHASE = process.env.SR_P107_LOUNGE_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P107_LOUNGE_PART ?? '';
const OUT =
  process.env.SR_P107_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-loungeshots';
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
/** The named controls, tested one by one by `hitTest`. Which builder: compose, msgs. */
const NAMED: Named[] = [
  { id: 'composer-button', sel: '.lng-composer .lng-icon-btn' },
  { id: 'emoji-cell', sel: '.lng-emoji-btn', max: 60 },
  { id: 'emoji-cat', sel: '.lng-emoji-cat' },
  { id: 'emoji-search', sel: '.lng-emoji-search' },
  { id: 'avatar', sel: 'button.lng-avatar-btn', max: 12 },
  { id: 'reaction', sel: '.lng-reactions button' },
  { id: 'tab', sel: '.lng-expr-tabs button' },
  { id: 'inbox-filter', sel: '.lng-inbox-filters input', wrapper: 'label' },
  { id: 'link-btn-standalone', sel: '.lng-link-btn:not(.sr-md .lng-link-btn)', max: 8 },
];
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
  { id: 'emoji-picker', sel: '[role=dialog][aria-label*="moji" i]', max: 1 },
  { id: 'emoji-grid', sel: '.lng-emoji-grid', max: 6 },
  { id: 'emoji-cats', sel: '.lng-emoji-cats', max: 1 },
  { id: 'emoji-cell', sel: '.lng-emoji-btn', max: 3 },
  { id: 'emoji-search', sel: '.lng-emoji-search', max: 1 },
  { id: 'message', sel: 'article.lng-msg', max: 14 },
  { id: 'message-name', sel: '.lng-name-btn', max: 14 },
  { id: 'avatar', sel: 'button.lng-avatar-btn', max: 12 },
  { id: 'reactions-row', sel: '.lng-reactions', max: 8 },
  { id: 'reaction', sel: '.lng-reactions button', max: 12 },
  { id: 'thread-chip', sel: '.lng-thread-chip', max: 6 },
  { id: 'tabs-bar', sel: '.lng-expr-tabs', max: 3 },
  { id: 'tab', sel: '.lng-expr-tabs button', max: 6 },
  { id: 'inbox-filters', sel: '.lng-inbox-filters', max: 1 },
  { id: 'inbox-filter-row', sel: '.lng-inbox-filters label', max: 4 },
  { id: 'inline-edit', sel: '.lng-inline-textarea', max: 1 },
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
const slug = (s: string) => s.replace(/[^a-z0-9]+/gi, '-').toLowerCase();

/** Every named control, accepted ones, and every box, on the page as it is now. */
async function measure(page: Page, pageName: string, bp: number, touch: boolean, root: Locator) {
  const reach: HitOptions = touch ? {} : { reach: 'box' };
  for (const n of BOXES) {
    const got = await page
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
    const loc = page.locator(n.sel);
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
    const loc = page.locator(n.sel);
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
  kind: 'composer' | 'full';
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
    if (!ringOk) probs.push('no focus ring drawn (outline none or 0 px)');
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
    if (!sameColour(rest.placeholder, rest.tokens.faint))
      probs.push(`placeholder ${rest.placeholder} is not --ui-text-faint ${rest.tokens.faint}`);
    if (rest.placeholderOpacity !== '1')
      probs.push(`placeholder opacity ${rest.placeholderOpacity}`);
    if (phC && (data.placeholderRatio as number) < 4.5)
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

async function openEmoji(page: Page) {
  await tap(page.getByRole('button', { name: /emoji picker/i }));
  await page
    .getByRole('dialog', { name: /emoji picker/i })
    .last()
    .waitFor({ timeout: 8000 });
  await settle(page, 2500);
}

/** The inline edit: ArrowUp in the empty composer edits the member's last message. */
async function openInlineEdit(page: Page): Promise<boolean> {
  const ta = page.locator('.lng-composer .lng-textarea').first();
  await ta.click({ timeout: 6000 }).catch(() => undefined);
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(800);
  return (await page.locator('.lng-inline-textarea').count()) > 0;
}

/** One page of the text channel at one width: the page, its overlays, the inline edit, and (desktop) the fields. */
async function channelAt(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg, tag: string) {
  const { ctx, page } = await newPage(browser, owner, cfg);
  const touch = cfg.touch;
  const bp = cfg.w;
  const suffix = touch ? '375' : `${cfg.w}-${cfg.theme === 'lounge-dark' ? 'dark' : 'light'}`;
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    const dt = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
    say(`${tag}: data-theme ${dt}`);
    if (!touch) {
      await fieldStates(page, composerField(page), cfg.theme, `channel@${bp}`);
    }
    if (touch || cfg.theme === 'lounge-light') {
      await measure(page, 'channel', bp, touch, main(page));
      await shot(page, 'channel', suffix);
      covered.push(`channel @${bp}`);
    }
    // The emoji picker from the composer.
    try {
      await openEmoji(page);
      const dlg = page.getByRole('dialog', { name: /emoji picker/i }).last();
      if (!touch)
        await fieldStates(
          page,
          {
            id: 'emoji-search',
            field: () => dlg.locator('.lng-emoji-search'),
            kind: 'full',
          },
          cfg.theme,
          `emoji-picker@${bp}`,
        );
      if (touch || cfg.theme === 'lounge-light') {
        await measure(page, 'emoji-picker', bp, touch, dlg);
        await shot(page, 'emoji-picker', suffix);
        covered.push(`emoji-picker @${bp}`);
      }
    } catch (e) {
      notes.push(`emoji-picker @${bp}: not opened (${msg(e)})`);
    }
    await esc(page);
    // The GIF picker and its tabs (GIF, Sticker, Emoji).
    if (touch || cfg.theme === 'lounge-light') {
      try {
        await go(page, url, page.getByRole('log'));
        await tap(page.getByRole('button', { name: /GIF picker/i }));
        const dlg = page.getByRole('dialog', { name: /GIF/i }).last();
        await dlg.waitFor({ timeout: 8000 });
        await settle(page, 2500);
        await measure(page, 'gif-picker', bp, touch, dlg);
        await shot(page, 'gif-picker', suffix);
        covered.push(`gif-picker @${bp}`);
        for (const tab of [/sticker/i, /emoji/i]) {
          const t = dlg.getByRole('tab', { name: tab });
          if ((await t.count()) === 0) {
            notes.push(`gif-picker @${bp}: tab ${tab} not found`);
            continue;
          }
          await t.first().click({ timeout: 5000 });
          await settle(page, 2000);
          const nm = `picker-tab-${slug(String(tab))}`;
          await measure(page, nm, bp, touch, dlg);
          await shot(page, nm, suffix);
          covered.push(`${nm} @${bp}`);
        }
      } catch (e) {
        notes.push(`gif-picker @${bp}: not opened (${msg(e)})`);
      }
      await esc(page);
    }
    // The inline edit.
    try {
      await go(page, url, page.getByRole('log'));
      if (await openInlineEdit(page)) {
        await fieldStates(
          page,
          { id: 'inline-edit', field: () => page.locator('.lng-inline-textarea'), kind: 'full' },
          cfg.theme,
          `inline-edit@${bp}`,
        );
        if (touch || cfg.theme === 'lounge-light') {
          await measure(
            page,
            'inline-edit',
            bp,
            touch,
            page.locator('.lng-inline-textarea').locator('xpath=..'),
          );
          await shot(page, 'inline-edit', suffix);
          covered.push(`inline-edit @${bp}`);
        }
      } else notes.push(`inline-edit @${bp}: did not open (no own last message?)`);
    } catch (e) {
      notes.push(`inline-edit @${bp}: ${msg(e)}`);
    }
    await esc(page);
    // A thread (once per width, Light).
    if (touch || cfg.theme === 'lounge-light') {
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
  await setTheme(owner, 'lounge-light');
  await channelAt(browser, owner, w, PHONE, 'channel-375');
  await channelAt(browser, owner, w, DESK_L, 'channel-1024-light');
  await setTheme(owner, 'lounge-dark');
  await channelAt(browser, owner, w, DESK_D, 'channel-1024-dark');
  await setTheme(owner, 'lounge-light');
}

async function part2(browser: Browser, owner: BrowserContext, w: World) {
  const o = owner.request;
  let forumId: string | null = null;
  await setTheme(owner, 'lounge-light');
  try {
    try {
      const f = await apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
        name: `p107lounge-forum-${STAMP}`,
        type: 15,
      });
      forumId = f.id;
      await sleep(1200);
      await apiPost(o, `/channels/${f.id}/threads`, {
        name: `p107lounge-post-${STAMP}`,
        message: { content: 'A post for the lounge shots.' },
      });
    } catch (e) {
      notes.push(`forum not made: ${msg(e)}`);
    }
    for (const cfg of [PHONE, DESK_L]) {
      const touch = cfg.touch;
      const bp = cfg.w;
      const suffix = touch ? '375' : `${cfg.w}-light`;
      const { ctx, page } = await newPage(browser, owner, cfg);
      try {
        if (forumId) {
          try {
            await go(
              page,
              `/channels/${w.guildId}/${forumId}`,
              page.getByRole('combobox', { name: 'Sort by' }),
            );
            await tap(page.locator('.lng-post-btn'));
            await settle(page, 3000);
            await measure(page, 'forum-post', bp, touch, main(page));
            await shot(page, 'forum-post', suffix);
            covered.push(`forum-post @${bp}`);
          } catch (e) {
            notes.push(`forum-post @${bp}: ${msg(e)}`);
          }
        }
        if (w.dmId) {
          try {
            await go(page, `/channels/@me/${w.dmId}`);
            const accept = main(page).getByRole('button', { name: /^Accept/ });
            if ((await accept.count()) > 0) await tap(accept);
            await page.getByRole('log').first().waitFor({ timeout: 20_000 });
            await settle(page);
            await measure(page, 'dm', bp, touch, main(page));
            await shot(page, 'dm', suffix);
            covered.push(`dm @${bp}`);
          } catch (e) {
            notes.push(`dm @${bp}: ${msg(e)}`);
          }
        } else notes.push('dm: the world has no DM');
        // The inbox: the header's Inbox button (a phone: inside "More actions").
        try {
          await go(page, `/channels/${w.guildId}/${w.channelId}`, page.getByRole('log'));
          const more = page.getByRole('button', { name: 'More actions' });
          if (
            (await more.count()) > 0 &&
            (await more.first().getAttribute('aria-expanded')) === 'false'
          )
            await more.first().click({ timeout: 5000 });
          await tap(page.getByRole('button', { name: /^Inbox/ }));
          const root = page
            .getByRole('dialog')
            .last()
            .or(page.getByRole('region', { name: /Inbox/ }));
          await root.first().waitFor({ timeout: 8000 });
          await settle(page, 2500);
          await measure(page, 'inbox', bp, touch, root);
          await shot(page, 'inbox', suffix);
          covered.push(`inbox @${bp}`);
          for (const tab of ['Mentions', 'Unreads']) {
            const t = root.first().getByRole('tab', { name: tab });
            if ((await t.count()) === 0) continue;
            await t.first().click({ timeout: 5000 });
            await settle(page, 1500);
            await measure(page, `inbox-${tab.toLowerCase()}`, bp, touch, root);
            await shot(page, `inbox-${tab.toLowerCase()}`, suffix);
            covered.push(`inbox-${tab.toLowerCase()} @${bp}`);
          }
        } catch (e) {
          notes.push(`inbox @${bp}: ${msg(e)}`);
        }
        // The Home list ("1 message request is waiting"); a phone shows the list pane alone.
        try {
          await go(page, '/channels/@me', page.locator('.lng-left'));
          await measure(page, 'home-list', bp, touch, page.locator('.lng-left'));
          await shot(page, 'home-list', suffix);
          covered.push(`home-list @${bp}`);
        } catch (e) {
          notes.push(`home-list @${bp}: ${msg(e)}`);
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    if (forumId)
      await apiDelete(o, `/channels/${forumId}`).catch((e) =>
        notes.push(`forum ${forumId} (p107lounge-forum-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
}

// ---- the comparison (after phase) ----------------------------------------------------------------------------------

/** Growth in height a box may have at 375 (D-990 lists of the two builders); everything else: none. */
const MAY_GROW_375: Record<string, number> = {
  'emoji-cell': 12,
  'emoji-cat': 12,
  'emoji-search': 12,
  'emoji-grid': 1e9,
  'emoji-cats': 1e9,
  'reactions-row': 12,
  reaction: 0,
  message: 12,
  'tabs-bar': 12.5,
  tab: 12.5,
  'inbox-filters': 100,
  'inbox-filter-row': 32,
};
const ONLY_NAMED_FAIL_EXCEPT = new Set<string>();

function compare(base: Row[], after: Row[]) {
  const key = (r: Row) => `${r.kind}|${r.page}|${'bp' in r ? r.bp : ''}|${r.key}`;
  const bm = new Map(base.map((r) => [key(r), r]));
  const out: string[] = [];
  for (const a of after) {
    const b = bm.get(key(a));
    if (a.kind === 'named' && a.bp === 375 && a.status !== 'pass') {
      const t = a.takers[0];
      out.push(
        `375 named control still ${a.status}: ${a.page} / ${a.tag} "${a.name}" ${a.box ? `${a.box.w}x${a.box.h}` : ''} taker ${t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}" at ${t.at}` : '-'}${b && b.kind === 'named' ? ` (base: ${b.status})` : ''}`,
      );
    }
    if (a.kind === 'named' && a.bp !== 375 && a.status === 'fail')
      out.push(`${a.bp} named control fails the box hit test: ${a.page} / ${a.tag} "${a.name}"`);
    if (a.kind === 'hit' && b && b.kind === 'hit' && b.status === 'pass' && a.status === 'fail') {
      const t = a.takers[0];
      out.push(
        `${a.bp}: passed in base, fails after: ${a.page} / ${a.tag} "${a.name}" taker ${t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}" at ${t.at}` : '-'}${a.takesCentre ? ' (its CENTRE line is taken: a tap area covers it)' : ''}`,
      );
    }
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
        if (Math.abs(dw) > 0.5 && id !== 'emoji-cell' && id !== 'emoji-cat')
          out.push(`375: width changed: ${a.page} / ${a.key} ${b.w} -> ${a.w}`);
        const allow = MAY_GROW_375[id] ?? 0;
        if (dh > allow + 0.5)
          out.push(
            `375: grew more than allowed (${allow}): ${a.page} / ${a.key} height ${b.h} -> ${a.h}`,
          );
        if (dh < -0.5 && id !== 'message' && id !== 'emoji-grid')
          out.push(`375: shrank: ${a.page} / ${a.key} height ${b.h} -> ${a.h}`);
        if (id === 'emoji-picker' && (Math.abs(dw) > 0.5 || Math.abs(dh) > 0.5))
          out.push(
            `375: the picker's outer size changed: ${a.page} ${b.w}x${b.h} -> ${a.w}x${a.h}`,
          );
      }
    }
    if (a.kind === 'box' && !b) say(`info: box without base twin ${a.page} @${a.bp} ${a.key}`);
    if (a.kind === 'count' && b && b.kind === 'count' && a.count !== b.count)
      out.push(
        `count of ${a.key} on ${a.page} @${a.bp}: ${b.count} -> ${a.count} (the world differs?)`,
      );
  }
  void ONLY_NAMED_FAIL_EXCEPT;
  return out;
}

// ---- the test ------------------------------------------------------------------------------------------------------

test(`wave 107 lounge pod A: compose and msgs, ${PHASE}, part ${PART || 'all'}`, async ({
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
    for (const c of rows.filter((r): r is CountRow => r.kind === 'count' && r.count > 0))
      say(`accepted ${c.key} ${c.page} @${c.bp}: ${c.count}, failing ${c.failing}`);
    for (const p of problems) say(`PROBLEM ${p}`);
    await ownerCtx.close();
  }
});
