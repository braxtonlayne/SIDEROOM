import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 103, pod lounge (SEC-AUD-462, 464, 465; D-971, D-972, D-976): five raw `<select>` elements of two code builders
 * move to the kit's `NativeSelect fit="content"`, and `developers.css` stops styling selects by element name.
 *   forum  the forum's Sort (ForumView.tsx), the emoji picker's skin tone (EmojiPicker.tsx), the scrapbook's shelf
 *          picker (ShelfPanel.tsx)
 *   pages  the API reference's Area (Reference.tsx), the Discord bridge's Conflict log Kind filter (ConflictLog.tsx)
 * Dependants (their files do not change; their figures must equal the base): the scrapbook's filter selects (the same
 * `.lng-scr-actions` family of rows), the developer portal's Commands (Type, Option type), OAuth2 (Redirect URI) and
 * Rich Presence (asset Type) selects, and the public authorize page's "Add to server" select (it must not change at all).
 *
 * A "stop" is one page or dialog with one or more selects. Each select is found by role, label or id, or by tag inside a
 * root found that way, never by a class. For every select of a stop, at every size and theme, the spec writes
 * `figures-<phase>.json`: its box and its wrapper's box, whether it is the kit's, the width of the selected option's
 * text and the longest option's, the chevron's place, the computed font-size, the label's lines and whether the label
 * text is stacked above the select, the boxes of every control of the stop's scope (relative to the scope's corner),
 * the document's scrollWidth and clientWidth and the exact hit test (`e2e/support/hit-test.ts`) over the scope with
 * the takers of every fail. A select the spec cannot reach is written "not reached" with the reason, never guessed.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited) for the Owner and the guild. Data it makes goes through the API under `p103l-<digits>` names
 * (D-942) and is removed in a `finally`: a forum channel, two developer applications (the second with two redirect
 * URIs). The Scrapbook is operator-gated and per server: it is switched on for the run and off in a `finally`. The
 * shelf picker's mounts and the bridge (status, conflicts, features) are answered by `page.route` fakes in the browser,
 * the same in both phases. Keyboard cases change a draft or a view and put it back; nothing is saved.
 *
 * Phases: `SR_P103_LOUNGE_PHASE=base` records the unmerged tree (shots get a `base-` prefix); the default `after`
 * records the same and, when `figures-base.json` is in the folder, compares, writes `compare.json` and a side-by-side
 * picture `cmp-<shot>.png` (base left, after right), and lists every select or dependant that breaks the rule:
 *   a moved select: not the kit's; block size under 40 (44 at 375 touch); the selected option's whole text not clear of
 *   the chevron; wider than the base by more than 36 px; the label text newly stacked above it or newly wrapped;
 *   an unmoved select: its inline or block size differs from the base;
 *   every control: moved down by more than the summed height difference of the moved selects above it (or, in a stop
 *   with no moved select, at all), moved up, count changed (a sideways move is a note, except in a stop with no moved
 *   select); a hit-test row worse than the base; the document scrolls sideways; and three stop rules: the shelf's
 *   "Use this shelf" shares a line with the picker, the emoji search field is at least 160 px wide.
 * Configs: 1024 x 768 Lounge Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light (the Lounge, Server settings
 * and the developer portal all follow the account's main theme; `data-theme` is read back before each shot).
 *
 * Local only (SR_P103_LOUNGE_ON=1). SR_P103_LOUNGE_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (`SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL`
 * and `SR_CLI_BIN` set to the stack's database and sr-cli), for both phases (the base on a tree where the pod's files
 * still have raw selects, then the after on the merged tree):
 *   SR_E2E_BASE_URL=http://localhost:<vite> SR_P103_LOUNGE_ON=1 SR_P103_LOUNGE_PHASE=base \
 *   SR_P103_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-lounge \
 *   npx playwright test tests/p103-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * and the same without `SR_P103_LOUNGE_PHASE=base` for the after phase. `SR_P103_LOUNGE_STOPS=name,name` runs only
 * those stops (the figures of the others in the file are kept).
 */
const ON = process.env.SR_P103_LOUNGE_ON === '1';
const PHASE = process.env.SR_P103_LOUNGE_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const ONLY = process.env.SR_P103_LOUNGE_STOPS?.split(',') ?? null;
const WORLD_FILE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
  /** Drive the keyboard cases at this config. */
  keys: boolean;
  /** Run the hit test over each scope. */
  hit: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    keys: true,
    hit: true,
  },
  {
    tag: '1024-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    keys: false,
    hit: false,
  },
  {
    tag: '375-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    keys: false,
    hit: true,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(1_500_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
): Promise<T> {
  const headers: Record<string, string> = { Origin: ORIGIN };
  if (method !== 'GET') {
    const csrf = await request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
    headers['X-SR-CSRF'] = ((await csrf.json()) as { token: string }).token;
  }
  const res = await request.fetch(`${API}${path}`, {
    method,
    headers,
    ...(data !== undefined ? { data } : {}),
  });
  if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

// ------------------------------------------------------------------------------------------------ the stops

interface Ids {
  guildId: string;
  channelId: string;
  forumCh: string;
  appId: string;
  appId2: string;
}
interface Env {
  page: Page;
  ids: Ids;
}
interface Stop {
  name: string;
  area: 'lounge' | 'settings' | 'dev';
  /** Who owns the file the stop is about; "dependant" if its file does not change this wave. */
  builder: 'forum' | 'pages' | 'dependant';
  file: string;
  /** Keys (prefixes) of the selects the after phase expects moved to the kit's NativeSelect fit="content". */
  moved: string[];
  /** The select this stop is about (the keyboard drives it; the stop waits for it). */
  select: (page: Page) => Locator;
  open: (env: Env) => Promise<void>;
  /** What the measures, the hit test and the shot cover. */
  scope: (page: Page) => Locator;
  /** What the shot covers (default: the scope). */
  shotScope?: (page: Page) => Locator;
  keyboard?: boolean;
  /** A public page: its data-theme is recorded, not required. */
  publicPage?: boolean;
}

const settings = (ids: Ids, id: string) => `/settings/server/${ids.guildId}/${id}`;
const goto = (page: Page, url: string) =>
  page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
};
const up = (loc: Locator, xpath: string) => loc.locator(`xpath=${xpath}`);

async function fakeBridge(page: Page, g: string) {
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
          discord_guild_name: 'p103l-friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'p103l-relay',
          mode: 'whole_guild',
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
  const conflict = (id: string, kind: string, name: string) => ({
    id,
    kind,
    object_kind: 'channel',
    sr_object_id: '1400000000000000001',
    discord_object_id: '1400000000000000002',
    field: 'name',
    discord_value: 'p103l-new',
    sr_value: 'p103l-old',
    discord_actor_name: 'p103l-actor',
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
          conflict('1500000000000000001', 'cap', 'p103l-general'),
          conflict('1500000000000000002', 'cap', 'p103l-off-topic'),
        ],
        open: 2,
      },
    }),
  );
}

/** Two storage mounts for the shelf picker (the world has none), the same in both phases. */
async function fakeMounts(page: Page, g: string) {
  await page.route(new RegExp(`/api/v1/guilds/${g}/storage/mounts$`), (route) =>
    route.fulfill({
      json: [
        { id: 'p103l-mount-one', label: 'p103l shelf one' },
        { id: 'p103l-mount-two', label: 'p103l shelf two' },
      ],
    }),
  );
}

const STOPS: Stop[] = [
  // ---- forum: the Lounge
  {
    name: 'forum-sort',
    area: 'lounge',
    builder: 'forum',
    file: 'lounge/forum/ForumView.tsx',
    moved: [],
    select: (page) => page.getByRole('combobox', { name: 'Sort by' }),
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/channels/${ids.guildId}/${ids.forumCh}`);
      await page.getByRole('combobox', { name: 'Sort by' }).waitFor({ timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => up(page.getByRole('combobox', { name: 'Sort by' }), 'ancestor::label/..'),
  },
  {
    name: 'emoji-tone',
    area: 'lounge',
    builder: 'forum',
    file: 'lounge/components/EmojiPicker.tsx',
    moved: ['Skin tone'],
    select: (page) => page.getByRole('combobox', { name: 'Skin tone' }),
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/channels/${ids.guildId}/${ids.channelId}`);
      await page
        .getByRole('button', { name: 'Open emoji picker' })
        .first()
        .click({ timeout: 40_000 });
      await page
        .getByRole('dialog', { name: 'Emoji picker' })
        .last()
        .getByRole('combobox', { name: 'Skin tone' })
        .waitFor({ timeout: 25_000 });
      await settled(page);
    },
    scope: (page) => up(page.getByRole('combobox', { name: 'Skin tone' }), 'ancestor::div[1]'),
    shotScope: (page) => page.getByRole('dialog', { name: 'Emoji picker' }).last(),
  },
  {
    name: 'scrapbook-shelf',
    area: 'lounge',
    builder: 'forum',
    file: 'lounge/scrapbook/ShelfPanel.tsx (and the Gallery filters beside it)',
    moved: ['Choose a shelf'],
    select: (page) => page.getByRole('combobox', { name: 'Choose a shelf' }),
    keyboard: true,
    open: async ({ page, ids }) => {
      await fakeMounts(page, ids.guildId);
      await goto(page, `/channels/${ids.guildId}/scrapbook`);
      await page.getByRole('combobox', { name: 'Choose a shelf' }).waitFor({ timeout: 40_000 });
      await settled(page);
      await page.waitForTimeout(500);
    },
    scope: (page) => page.locator('main').first(),
    shotScope: (page) =>
      up(page.getByRole('combobox', { name: 'Choose a shelf' }), 'ancestor::section[1]'),
  },
  // ---- pages: Server settings and the developer portal
  {
    name: 'bridge-conflicts',
    area: 'settings',
    builder: 'pages',
    file: 'settings/server/bridge/ConflictLog.tsx',
    moved: [],
    select: (page) => page.getByTestId('bridge-conflicts').getByRole('combobox'),
    keyboard: true,
    open: async ({ page, ids }) => {
      await fakeBridge(page, ids.guildId);
      await goto(page, settings(ids, 'bridge'));
      await page.getByTestId('bridge-conflicts').getByRole('combobox').waitFor({ timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => page.getByTestId('bridge-conflicts'),
  },
  {
    name: 'api-reference-area',
    area: 'dev',
    builder: 'pages',
    file: 'developers/Reference.tsx',
    moved: ['Area'],
    select: (page) => page.getByRole('combobox', { name: 'Area' }),
    keyboard: true,
    open: async ({ page }) => {
      await goto(page, '/developers/docs/api-reference');
      await page.getByRole('combobox', { name: 'Area' }).waitFor({ timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => up(page.getByRole('combobox', { name: 'Area' }), 'ancestor::div[1]'),
  },
  // ---- dependants of developers.css
  {
    name: 'commands-type',
    area: 'dev',
    builder: 'dependant',
    file: 'developers/tabs/Commands.tsx (Type, Option type)',
    moved: [],
    select: (page) => page.locator('#cmd-type'),
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId}?tab=commands`);
      await page.waitForSelector('#cmd-type', { timeout: 40_000 });
      await page.getByRole('button', { name: 'Add option' }).click();
      await page.waitForSelector('#opt-type-0');
      await settled(page);
    },
    scope: (page) => up(page.locator('#cmd-type'), 'ancestor::section[1]'),
  },
  {
    name: 'oauth2-redirect',
    area: 'dev',
    builder: 'dependant',
    file: 'developers/tabs/OAuth2.tsx',
    moved: [],
    select: (page) => page.locator('section select').first(),
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId2}?tab=oauth2`);
      await page.waitForSelector('section select', { timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => up(page.locator('section select').first(), 'ancestor::section[1]'),
  },
  {
    name: 'richpresence-type',
    area: 'dev',
    builder: 'dependant',
    file: 'developers/tabs/Media.tsx',
    moved: [],
    select: (page) => page.locator('form select').first(),
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId}?tab=richpresence`);
      await page.waitForSelector('main form select', { timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => up(page.locator('form select').first(), 'ancestor::form[1]'),
  },
  {
    name: 'authorize-guild',
    area: 'dev',
    builder: 'dependant',
    file: 'developers/Authorize.tsx (a public page: must not change at all)',
    moved: [],
    publicPage: true,
    select: (page) => page.locator('#dev-auth-guild'),
    open: async ({ page, ids }) => {
      await goto(
        page,
        `/oauth2/authorize?client_id=${ids.appId}&scope=${encodeURIComponent('bot applications.commands')}`,
      );
      await page.waitForSelector('#dev-auth-guild', { timeout: 40_000 });
      await settled(page);
    },
    scope: (page) => page.locator('main').first(),
  },
];

// ------------------------------------------------------------------------------------------------ measuring

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface SelectFig {
  box: Box;
  wrap: Box | null;
  kit: boolean;
  options: number;
  labelLines: number;
  labelText: string;
  stacked: boolean;
  labelGap: number | null;
  fontSize: string;
  selectedText: string;
  textW: number;
  maxOptW: number;
  textRight: number;
  chevronX: number | null;
  textClear: boolean;
}
interface ControlFig {
  key: string;
  tag: string;
  selKey: string | null;
  box: Box;
}
interface Measured {
  scroll: number[];
  selects: Record<string, SelectFig>;
  controls: ControlFig[];
  overlaps: string[];
}
interface HitCount {
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
}
interface HitRow {
  key: string;
  status: string;
}
interface View extends Measured {
  hit: HitCount;
  hitRows: HitRow[];
  hitFails: unknown[];
  theme: string | null;
}
type StopFig = View | { missing: string };
type Figures = Record<string, StopFig>;

/** Boxes of every select and every control under `scope`, relative to the scope's top-left corner. */
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
    const selEls = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).filter(visible);
    const selects: Record<string, unknown> = {};
    const keyOf = new Map<Element, string>();
    const canvas = document.createElement('canvas').getContext('2d')!;
    selEls.forEach((s, n) => {
      const label =
        s.closest('label') ?? (s.id ? document.querySelector(`label[for="${s.id}"]`) : null);
      let labelText = '';
      const tops = new Set<number>();
      let textBottom = -1;
      let textRightMax = -1;
      if (label) {
        const w = document.createTreeWalker(label, NodeFilter.SHOW_TEXT);
        for (let t = w.nextNode(); t; t = w.nextNode()) {
          if (s.contains(t)) continue;
          if (!(t.textContent ?? '').trim()) continue;
          labelText += (t.textContent ?? '').trim() + ' ';
          const range = document.createRange();
          range.selectNodeContents(t);
          for (const r of Array.from(range.getClientRects()))
            if (r.width > 0 && r.height > 0) {
              tops.add(Math.round(r.top));
              textBottom = Math.max(textBottom, r.bottom);
              textRightMax = Math.max(textRightMax, r.right);
            }
        }
      }
      const sr = s.getBoundingClientRect();
      const cs = getComputedStyle(s);
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const selectedText = s.options[s.selectedIndex]?.text ?? '';
      const textW = canvas.measureText(selectedText).width;
      let maxOptW = 0;
      for (const o of Array.from(s.options))
        maxOptW = Math.max(maxOptW, canvas.measureText(o.text).width);
      const padL = parseFloat(cs.paddingLeft);
      const bl = parseFloat(cs.borderLeftWidth);
      const textRight = sr.left + bl + padL + textW;
      const wrapEl = s.parentElement;
      const kit = !!wrapEl && wrapEl.tagName === 'SPAN' && wrapEl.children.length === 1;
      // The kit draws the chevron at the wrapper's inline end: 14 px in, 6 px wide (a rotated square: about 8 px).
      const chevronX = kit ? wrapEl!.getBoundingClientRect().right - 14 - 6 - 1 : null;
      const key = `${s.id.replace(/^_r_[a-z0-9]+_-?/, '') || s.getAttribute('aria-label') || labelText.trim() || 'select'}#${n}`;
      keyOf.set(s, key);
      selects[key] = {
        box: box(s),
        wrap: kit ? box(wrapEl!) : null,
        kit,
        options: s.options.length,
        labelLines: tops.size,
        labelText: labelText.trim(),
        labelGap: label && textRightMax >= 0 ? rnd(sr.left - textRightMax) : null,
        stacked: label ? textBottom >= 0 && textBottom <= sr.top + 1 : false,
        fontSize: cs.fontSize,
        selectedText,
        textW: rnd(textW),
        maxOptW: rnd(maxOptW),
        textRight: rnd(textRight - rr.left),
        chevronX: chevronX === null ? null : rnd(chevronX - rr.left),
        textClear: kit ? textRight <= chevronX! + 0.5 : true,
      };
    });
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter(visible);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24)}`,
      tag: e.tagName.toLowerCase(),
      selKey: keyOf.get(e) ?? null,
      box: box(e),
    }));
    const overlaps: string[] = [];
    for (const s of selEls) {
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `${keyOf.get(s)} x ${o.tagName.toLowerCase()} (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
    }
    return {
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      overlaps,
    } as never;
  });
}

const count = (rows: ControlRow[]): HitCount => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function view(
  page: Page,
  scope: Locator,
  tag: string,
  problems: string[],
  withHit: boolean,
): Promise<View> {
  const m = await measure(scope);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);
  const theme = await scope
    .locator('select')
    .first()
    .evaluate((e) => e.closest('[data-theme]')?.getAttribute('data-theme') ?? null)
    .catch(() => null);
  let rows: ControlRow[] = [];
  if (withHit) {
    await openCollapsed(page, scope);
    rows = await hitTestAll(page, scope);
  }
  const hit = count(rows);
  const hitRows = rows.map((r) => ({ key: `${r.tag} "${r.name}"`, status: r.status }));
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      reason: r.reason,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: selects ${JSON.stringify(m.selects)} hit ${JSON.stringify(hit)}`);
  // Not-reached rows are probe errors of this spec, never page faults: reported, never put on a builder.
  if (hit.notReached) say(`${tag}: ${hit.notReached} hit-test rows not reached (probe)`);
  return { ...m, hit, hitRows, hitFails, theme };
}

// ------------------------------------------------------------------------------------------------ the run

async function setMain(ctx: BrowserContext, theme: string) {
  const csrf = await ctx.request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
  const token = ((await csrf.json()) as { token: string }).token;
  const res = await ctx.request.fetch(`${API}/users/@me/settings`, {
    method: 'PATCH',
    headers: { Origin: ORIGIN, 'X-SR-CSRF': token },
    data: { settings: { user_prefs: { theme, zoom: 100 } } },
  });
  expect(res.ok()).toBeTruthy();
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function newContext(browser: Browser, owner: BrowserContext, c: Cfg) {
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

async function shot(page: Page, stop: Stop, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  const file = resolve(OUT, `${shotName(name)}.png`);
  const target = (stop.shotScope ?? stop.scope)(page);
  try {
    await stop.select(page).first().scrollIntoViewIfNeeded({ timeout: 3000 });
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
    /* fall through to the viewport */
  }
  await page.screenshot({ path: file, timeout: 10_000 });
}

interface KeyFig {
  before: string;
  changed: string;
  back: string;
  focusVisible: boolean;
  reacted: boolean;
  enabledOptions: number;
}

/** Focus, ArrowDown changes the value, the page reacts, ArrowUp puts it back. */
async function keyboard(
  page: Page,
  stop: Stop,
  tag: string,
  problems: string[],
): Promise<KeyFig | null> {
  const sel = stop.select(page).first();
  const enabledOptions = await sel.evaluate(
    (e) => Array.from((e as HTMLSelectElement).options).filter((o) => !o.disabled).length,
  );
  if (enabledOptions < 2) {
    say(`${tag}: keyboard not driven (one enabled option)`);
    return null;
  }
  await sel.scrollIntoViewIfNeeded();
  await page.keyboard.press('Shift');
  await sel.focus();
  const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
  const text0 = await page.locator('body').innerText();
  const before = await sel.inputValue();
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(500);
  const changed = await stop.select(page).first().inputValue();
  const text1 = await page.locator('body').innerText();
  await stop.select(page).first().focus();
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(500);
  const back = await stop.select(page).first().inputValue();
  const k: KeyFig = {
    before,
    changed,
    back,
    focusVisible,
    reacted: text0 !== text1 || changed !== before,
    enabledOptions,
  };
  say(`${tag}: keyboard ${JSON.stringify(k)}`);
  if (changed === before) problems.push(`${tag}: ArrowDown did not change the value`);
  if (back !== before) problems.push(`${tag}: ArrowUp did not put the value back`);
  if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
  return k;
}

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  ids: Ids,
  problems: string[],
  notes: string[],
  keys: Record<string, KeyFig | null>,
): Promise<Figures> {
  await setMain(owner, c.theme);
  const fig: Figures = {};
  const ctx = await newContext(browser, owner, c);
  try {
    for (const stop of STOPS) {
      if (ONLY && !ONLY.includes(stop.name)) continue;
      const tag = `${c.tag} ${stop.name}`;
      const page = await ctx.newPage();
      page.setDefaultTimeout(20_000);
      try {
        await stop.open({ page, ids });
        await page.waitForTimeout(600);
        const scope = stop.scope(page);
        await scope.waitFor({ state: 'visible', timeout: 10_000 });
        const v = await view(page, scope, tag, problems, c.hit);
        fig[stop.name] = v;
        if (v.theme !== c.theme) {
          const msg = `${tag}: data-theme is ${v.theme}, wanted ${c.theme}`;
          say(msg);
          if (stop.publicPage) notes.push(msg);
          else problems.push(msg);
        }
        await shot(page, stop, `${stop.name}-${c.tag}`);
        if (c.keys && stop.keyboard) keys[tag] = await keyboard(page, stop, tag, problems);
      } catch (e) {
        const reason = String(e).replace(/\s+/g, ' ').slice(0, 400);
        fig[stop.name] = { missing: reason };
        say(`${tag}: not reached: ${reason}`);
      } finally {
        await page.close().catch(() => undefined);
      }
    }
  } finally {
    await ctx.close();
  }
  return fig;
}

// ------------------------------------------------------------------------------------------------ comparing

interface Verdict {
  tag: string;
  stop: string;
  builder: string;
  file: string;
  select: string;
  moved: boolean;
  base: Box | null;
  after: Box | null;
  kit: boolean;
  reasons: string[];
}

const near = (a: number, b: number, tol = 0.5) => Math.abs(a - b) <= tol;

function compare(c: Cfg, base: Figures, after: Figures, verdicts: Verdict[], notes: string[]) {
  for (const stop of STOPS) {
    const a = after[stop.name];
    const b = base[stop.name];
    const label = `${c.tag} ${stop.name}`;
    if (!a || !b) continue;
    if ('missing' in a || 'missing' in b) {
      if ('missing' in a !== 'missing' in b)
        notes.push(
          `${label}: reached in only one phase (${'missing' in a ? a.missing : 'missing' in b ? b.missing : ''})`,
        );
      else notes.push(`${label}: not reached in either phase`);
      continue;
    }
    const sum = (h: HitCount) => h.pass + h.fail + h.notReached + h.skipped;
    const isMoved = (key: string) => stop.moved.some((p) => key.startsWith(p));
    const stopVerdicts: Verdict[] = [];
    // The summed height gained by the moved selects, for the controls beside and below them.
    const movedKeys = Object.keys(a.selects).filter((k) => isMoved(k) && b.selects[k]);
    const stopReasons: string[] = [];
    for (const key of Object.keys(a.selects)) {
      const as = a.selects[key]!;
      const bs = b.selects[key];
      const reasons: string[] = [];
      if (!bs) {
        notes.push(`${label} "${key}": not in the base`);
        continue;
      }
      const moved = isMoved(key);
      if (moved) {
        if (!as.kit) reasons.push(`still a raw select (expected the kit's NativeSelect)`);
        if (as.box.h < 40 - 0.5) reasons.push(`block size ${as.box.h} under 40`);
        if (c.touch && as.box.h < 44 - 0.5) reasons.push(`block size ${as.box.h} under 44 at 375`);
        if (!as.textClear)
          reasons.push(
            `the selected text "${as.selectedText}" runs to ${as.textRight}, past the chevron at ${as.chevronX}`,
          );
        if (as.box.w > bs.box.w + 36.5)
          reasons.push(`inline size ${as.box.w} is more than 36 px over the base's ${bs.box.w}`);
        if (as.stacked && !bs.stacked)
          reasons.push('the label text is newly stacked above the select');
        if (as.labelGap !== null && bs.labelGap !== null && as.labelGap < 4 && bs.labelGap >= 4)
          reasons.push(
            `the label text touches the select: gap ${as.labelGap} px against the base's ${bs.labelGap}`,
          );
        if (as.labelLines > bs.labelLines)
          reasons.push(`label wraps: ${as.labelLines} lines against ${bs.labelLines}`);
        if (as.fontSize !== bs.fontSize)
          notes.push(`${label} "${key}": font-size ${bs.fontSize} -> ${as.fontSize}`);
      } else {
        if (!near(as.box.w, bs.box.w) || !near(as.box.h, bs.box.h))
          reasons.push(
            `dependant changed: ${bs.box.w}x${bs.box.h} -> ${as.box.w}x${as.box.h} (its file and rule were not to change it)`,
          );
        if (as.fontSize !== bs.fontSize)
          reasons.push(`dependant font-size ${bs.fontSize} -> ${as.fontSize}`);
        if (as.kit !== bs.kit) reasons.push(`dependant kit ${bs.kit} -> ${as.kit}`);
      }
      stopVerdicts.push({
        tag: c.tag,
        stop: stop.name,
        builder: stop.builder,
        file: stop.file,
        select: key,
        moved,
        base: bs.box,
        after: as.box,
        kit: as.kit,
        reasons,
      });
    }
    // Controls: each may move down by at most the height the moved selects wholly above it gained, nothing else moves.
    if (a.controls.length !== b.controls.length) {
      stopReasons.push(`control count ${b.controls.length} -> ${a.controls.length}`);
    } else {
      b.controls.forEach((bc, i) => {
        const ac = a.controls[i]!;
        if (ac.selKey && isMoved(ac.selKey)) return;
        let allowed = 0;
        for (const k2 of movedKeys) {
          const s2a = a.selects[k2]!;
          const s2b = b.selects[k2]!;
          const diff = s2a.box.h - s2b.box.h;
          if (s2b.box.y + s2b.box.h <= bc.box.y + 0.5) allowed += diff;
          else if (s2b.box.y < bc.box.y + bc.box.h) allowed += Math.abs(diff);
        }
        const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
        const mvx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
        if (mv > allowed + 1)
          stopReasons.push(
            `${bc.key} moved ${mv} px down, allowed ${Math.round(allowed * 10) / 10}`,
          );
        if (mv < -(Math.abs(allowed) + 1)) stopReasons.push(`${bc.key} moved ${mv} px up`);
        if (Math.abs(mvx) > 1) {
          if (movedKeys.length === 0)
            stopReasons.push(`${bc.key} (a dependant) moved ${mvx} px sideways`);
          else notes.push(`${label}: ${bc.key} moved ${mvx} px sideways beside a moved select`);
        }
        if (movedKeys.length === 0 && (!near(ac.box.w, bc.box.w) || !near(ac.box.h, bc.box.h)))
          stopReasons.push(
            `${bc.key} (a dependant) is ${bc.box.w}x${bc.box.h} -> ${ac.box.w}x${ac.box.h}`,
          );
      });
    }
    // The stop's own rules.
    if (stop.name === 'scrapbook-shelf') {
      const sel = a.selects[movedKeys[0] ?? ''];
      const btn = a.controls.find((x) => x.tag === 'button' && /Use this shelf/i.test(x.key));
      if (!sel || !btn)
        notes.push(`${label}: the "Use this shelf" button or the picker was not found`);
      else {
        const cy = btn.box.y + btn.box.h / 2;
        if (!(cy >= sel.box.y && cy <= sel.box.y + sel.box.h))
          stopReasons.push(
            `"Use this shelf" is not on the picker's line (button y ${btn.box.y}+${btn.box.h}, picker y ${sel.box.y}+${sel.box.h})`,
          );
      }
    }
    if (stop.name === 'emoji-tone') {
      const search = a.controls.find((x) => x.tag === 'input');
      const bsearch = b.controls.find((x) => x.tag === 'input');
      if (!search) notes.push(`${label}: the emoji search field was not found`);
      else {
        notes.push(`${label}: emoji search field ${bsearch?.box.w} -> ${search.box.w} px`);
        if (search.box.w < 160)
          stopReasons.push(`the emoji search field is ${search.box.w} px (under 160)`);
      }
    }
    // Hit test: the same rows, none worse.
    const br = new Map(b.hitRows.map((r) => [r.key, r.status] as const));
    const rank: Record<string, number> = { pass: 3, skipped: 2, fail: 1, 'not-reached': 0 };
    if (sum(a.hit) !== sum(b.hit)) stopReasons.push(`hit test rows ${sum(b.hit)} -> ${sum(a.hit)}`);
    for (const r of a.hitRows) {
      const was = br.get(r.key);
      if (was !== undefined && rank[r.status]! < rank[was]!)
        stopReasons.push(`hit test ${r.key}: ${was} -> ${r.status}`);
    }
    if (a.hit.fail > b.hit.fail)
      stopReasons.push(
        `hit test fails ${b.hit.fail} -> ${a.hit.fail}: ${JSON.stringify(a.hitFails)}`,
      );
    if (stopReasons.length) {
      const target = stopVerdicts.find((v) => v.moved) ?? stopVerdicts[0];
      if (target) target.reasons.push(...stopReasons);
    }
    verdicts.push(...stopVerdicts);
  }
}

/** A side-by-side picture of the base and the after shot (base left), rendered in a page of its own. */
async function composites(browser: Browser) {
  const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const page = await ctx.newPage();
  const made: string[] = [];
  for (const c of CONFIGS) {
    for (const s of STOPS) {
      const name = `${s.name}-${c.tag}`;
      const bf = resolve(OUT, `base-${name}.png`);
      const af = resolve(OUT, `${name}.png`);
      if (!existsSync(bf) || !existsSync(af)) continue;
      const img = (f: string) => `data:image/png;base64,${readFileSync(f).toString('base64')}`;
      await page.setContent(
        `<body style="margin:0;background:#888;font:12px sans-serif;color:#fff"><div style="display:flex;gap:12px;padding:8px;align-items:flex-start"><div>base<br><img src="${img(bf)}"></div><div>after<br><img src="${img(af)}"></div></div></body>`,
      );
      await page.screenshot({ path: resolve(OUT, `cmp-${name}.png`), fullPage: true });
      made.push(`cmp-${name}.png`);
    }
  }
  await ctx.close();
  return made;
}

/** The Scrapbook is operator-gated and per server: switch it on for the run and off again in a finally. */
async function scrapbookSwitch(owner: BrowserContext, guildId: string, on: boolean) {
  await apiPatch(owner.request, '/admin/features/lounge.scrapbook', {
    enabled: on,
    reason: 'p103 lounge shots',
  });
  await apiPatch(owner.request, `/guilds/${guildId}/sr/features/lounge.scrapbook`, {
    enabled: on,
  });
}

test(`p103 lounge: forum, emoji, shelf, reference and conflict-log selects, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);

  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
  const chans = (await (
    await octx.request.get(`${API}/guilds/${world.guildId}/channels`)
  ).json()) as { id: string; type: number }[];
  const stamp = String(Date.now()).slice(-6); // digits only: the same width every run
  const problems: string[] = [];
  const notes: string[] = [];
  const figs: Figures[] & Record<string, Figures> = {} as never;
  const keys: Record<string, KeyFig | null> = {};
  const ids: Ids = {
    guildId: world.guildId,
    channelId: chans.find((x) => x.type === 0)!.id,
    forumCh: '',
    appId: '',
    appId2: '',
  };
  let scrapbookOn = false;
  try {
    await scrapbookSwitch(octx, ids.guildId, true);
    scrapbookOn = true;
    const forum = await call<{ id: string }>(
      octx.request,
      'POST',
      `/guilds/${ids.guildId}/channels`,
      {
        name: `p103l-forum-${stamp}`,
        type: 15,
      },
    );
    ids.forumCh = forum.id;
    const mkApp = async (name: string) =>
      (await call<{ application: { id: string } }>(octx.request, 'POST', '/applications', { name }))
        .application.id;
    ids.appId = await mkApp(`p103l-app-${stamp}`);
    ids.appId2 = await mkApp(`p103l-app2-${stamp}`);
    await call(octx.request, 'PATCH', `/applications/${ids.appId2}`, {
      redirect_uris: [
        `https://p103l-${stamp}.example.com/one`,
        `https://p103l-${stamp}.example.com/two`,
      ],
    });
    say(`ids ${JSON.stringify(ids)}`);

    for (const c of CONFIGS) {
      try {
        (figs as Record<string, Figures>)[c.tag] = await runConfig(
          browser,
          octx,
          c,
          ids,
          problems,
          notes,
          keys,
        );
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
    if (ids.forumCh)
      await call(octx.request, 'DELETE', `/channels/${ids.forumCh}`).catch((e) =>
        say(
          `forum channel ${ids.forumCh} (p103l-forum-${stamp}) not deleted: ${String(e).slice(0, 160)}`,
        ),
      );
    for (const id of [ids.appId, ids.appId2].filter(Boolean))
      await call(octx.request, 'DELETE', `/applications/${id}`, { password: owner.password }).catch(
        (e) =>
          say(`application ${id} not deleted (p103l-app-${stamp}): ${String(e).slice(0, 160)}`),
      );
    if (scrapbookOn) await scrapbookSwitch(octx, ids.guildId, false).catch(() => undefined);
    await octx.close();
  }
  const ff = resolve(OUT, `figures-${PHASE}.json`);
  const all = figs as unknown as Record<string, Figures>;
  if (ONLY && existsSync(ff)) {
    const prev = JSON.parse(readFileSync(ff, 'utf8')) as {
      figs: Record<string, Figures>;
      keys: typeof keys;
    };
    for (const [tag, f] of Object.entries(all))
      prev.figs[tag] = { ...(prev.figs[tag] ?? {}), ...f };
    Object.assign(prev.keys, keys);
    writeFileSync(ff, JSON.stringify(prev, null, 1));
    for (const tag of Object.keys(prev.figs)) all[tag] = prev.figs[tag]!;
  } else writeFileSync(ff, JSON.stringify({ figs: all, keys }, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  const verdicts: Verdict[] = [];
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as {
      figs: Record<string, Figures>;
    };
    for (const c of CONFIGS)
      if (base.figs[c.tag] && all[c.tag])
        compare(c, base.figs[c.tag]!, all[c.tag]!, verdicts, notes);
    const made = await composites(browser);
    const broken = verdicts.filter((v) => v.reasons.length);
    writeFileSync(
      resolve(OUT, 'compare.json'),
      JSON.stringify({ verdicts, broken, notes, composites: made }, null, 1),
    );
    for (const v of broken)
      problems.push(`${v.tag} ${v.builder} ${v.file} "${v.select}": ${v.reasons.join('; ')}`);
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  for (const n of notes) say(`note: ${n}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
