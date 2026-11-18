import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot, runCli } from '../support/cli';
import { bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * D-958 (wave 102, pod pages): the 16 raw `<select>` elements of the pod's three code builders move to the kit's
 * `NativeSelect`.
 *   srv    Server settings: the prune dialog, the webhook dialog (Integrations), the Community wizard, Community
 *          (the rules/updates/safety lists), the Discord bridge's conflict log.
 *   reader Library: the series page's Edition, Rankings' Kind, My library's Sort, a list's visibility (new and edit),
 *          the reader's Chapter.
 *   devs   Developer portal: the command builder's Type and Option type, the OAuth2 URL generator's Redirect URI,
 *          Rich Presence's asset Type, the API reference's Area, the authorize page's Add to server.
 * A "stop" is one page or dialog with one or more selects. For every select of a stop, at every size and theme, the
 * spec writes `measure-<phase>.json`: file, builder, name, box, the boxes of the controls around it (relative to the
 * stop's scope), the document's scrollWidth and clientWidth, the label's line count, the exact hit test
 * (`e2e/support/hit-test.ts`) over the scope with the takers of every fail. A select the spec cannot reach is written
 * "not reached" with the reason, never guessed.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited) for the Owner and the guild. Data it makes goes through the API under `p102-pages-<digits>`
 * names (D-942) and is removed in a `finally`: a library series (two editions, two chapters of three pages, the
 * pictures answered by `page.route`), a reading list, two developer applications. Server settings views change no
 * server data: the Community wizard and the bridge conflict log are answered by `page.route` fakes in the browser (as
 * `p101-mod-shots.spec.ts` does); the keyboard cases change a draft or a view and put it back.
 *
 * Phases: `SR_P102_PAGES_PHASE=base` records the unedited tree (shots get a `base-` prefix, `measure-base.json`); the
 * default `after` records the same and, when `measure-base.json` is in the folder, compares and writes
 * `compare.json`, a side-by-side picture `cmp-<shot>.png` (base left, after right) for every shot, and lists every
 * select that breaks the rule (block size under 40, or 44 at 375 touch, for a moved select; inline size smaller than
 * the base; a neighbour moved by more than the height differences above it; a hit-test row worse than the base; the
 * label wraps into more lines than in the base; the document scrolls sideways).
 * Configs: Server settings 1024 x 768 Lounge Light and Dark and 375 x 812 touch Lounge Light; Library 1024 and 375
 * touch in `sectionThemes.library = 'sideroom'` and 1024 in `'lounge-light'`; Developers 1024 and 375 touch in
 * the main theme `lounge-dark` and 1024 in `lounge-light` (the portal has no section theme of its own). The account theme is set by PATCH and
 * localStorage and `data-theme` is read back from the select's nearest themed ancestor before each shot.
 *
 * Local only (SR_P102_PAGES_ON=1). SR_P102_PAGES_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8798, Vite 5798, metrics 9498,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5798 SR_P102_PAGES_ON=1 SR_P102_PAGES_PHASE=base \
 *   SR_P102_PAGES_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-pages \
 *   npx playwright test tests/p102-pages-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * and the same without `SR_P102_PAGES_PHASE=base` for the after phase. After the merge there is no base (set the phase
 * to base only on a tree where the pod's files still have raw selects); delete `measure-base.json` of another tree.
 */
const ON = process.env.SR_P102_PAGES_ON === '1';
const PHASE = process.env.SR_P102_PAGES_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_PAGES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-pages';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
/** `SR_P102_PAGES_AREAS=settings,library,dev` (default: all three) lets a round be split. */
/** `SR_P102_PAGES_STOPS=name,name` runs only those stops. */
const ONLY = process.env.SR_P102_PAGES_STOPS?.split(',') ?? null;
const AREAS = process.env.SR_P102_PAGES_AREAS?.split(',') ?? null;
const WORLD_FILE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

type Area = 'settings' | 'library' | 'dev';
interface Cfg {
  tag: string;
  area: Area;
  width: number;
  height: number;
  touch: boolean;
  /** Settings and developers (which follows the main theme): the account's main theme. Library: its section theme. */
  theme: 'lounge-light' | 'lounge-dark' | 'sideroom';
  /** Take pictures at this config. */
  shots: boolean;
  /** Drive the keyboard cases at this config. */
  keys: boolean;
  /** Run the hit test over each scope (the phone configs, and each area's first desktop config). */
  hit: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: 'set-1024-light',
    area: 'settings',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: true,
    keys: true,
    hit: true,
  },
  {
    tag: 'set-1024-dark',
    area: 'settings',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: true,
    keys: false,
    hit: false,
  },
  {
    tag: 'set-375-light',
    area: 'settings',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: true,
    keys: false,

    hit: true,
  },
  {
    tag: 'lib-1024-sideroom',
    area: 'library',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'sideroom',
    shots: true,
    keys: true,
    hit: true,
  },
  {
    tag: 'lib-375-sideroom',
    area: 'library',
    width: 375,
    height: 812,
    touch: true,
    theme: 'sideroom',
    shots: true,
    keys: false,

    hit: true,
  },
  {
    tag: 'lib-1024-light',
    area: 'library',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: true,
    keys: false,
    hit: false,
  },
  {
    tag: 'dev-1024-dark',
    area: 'dev',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: true,
    keys: true,
    hit: true,
  },
  {
    tag: 'dev-375-dark',
    area: 'dev',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-dark',
    shots: true,
    keys: false,

    hit: true,
  },
  {
    tag: 'dev-1024-light',
    area: 'dev',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: true,
    keys: false,
    hit: false,
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
  seriesId: string;
  chapterId: string;
  chapter2Id: string;
  listId: string;
  appId: string;
  appId2: string;
}
interface Env {
  page: Page;
  ids: Ids;
}
interface Stop {
  name: string;
  area: Area;
  builder: 'srv' | 'reader' | 'devs';
  file: string;
  /** The select this stop is about (the keyboard drives it; the stop waits for it). */
  select: string;
  open: (env: Env) => Promise<void>;
  /** What the measures, the hit test and the shot cover. */
  scope: (page: Page) => Locator;
  /** The keyboard case runs at the configs with `keys`. */
  keyboard?: boolean;
}

const settings = (ids: Ids, id: string) => `/settings/server/${ids.guildId}/${id}`;
const goto = (page: Page, url: string) =>
  page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
/** The nearest ancestor with this tag and a descendant that matches `sel` (the innermost). */
const holding = (page: Page, css: string, sel: string) =>
  page
    .locator(css)
    .filter({ has: page.locator(sel) })
    .last();

async function fakeCommunity(page: Page, g: string) {
  await page.route(new RegExp(`/api/v1/guilds/${g}/sr/community$`), async (route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const res = await route.fetch();
    const body = (await res.json()) as Record<string, unknown>;
    return route.fulfill({ response: res, json: { ...body, community_enabled: false } });
  });
  await page.route(new RegExp(`/api/v1/guilds/${g}([?].*)?$`), async (route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const res = await route.fetch();
    const body = (await res.json()) as Record<string, unknown>;
    return route.fulfill({
      response: res,
      json: { ...body, verification_level: 1, explicit_content_filter: 2 },
    });
  });
}

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
          discord_guild_name: 'p102-pages-friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'p102-pages-relay',
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
    discord_value: 'p102-pages-new',
    sr_value: 'p102-pages-old',
    discord_actor_name: 'p102-pages-actor',
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
          conflict('1500000000000000001', 'cap', 'p102-pages-general'),
          conflict('1500000000000000002', 'cap', 'p102-pages-off-topic'),
        ],
        open: 2,
      },
    }),
  );
}

const STOPS: Stop[] = [
  // ---- srv: Server settings
  {
    name: 'prune',
    area: 'settings',
    builder: 'srv',
    file: 'settings/server/sections/PruneDialog.tsx',
    select: '#ss-prune-days',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, settings(ids, 'members'));
      await page.getByRole('button', { name: 'Prune', exact: true }).click();
      await page.waitForSelector('#ss-prune-days');
    },
    scope: (page) => holding(page, '[role="dialog"]', '#ss-prune-days'),
  },
  {
    name: 'webhook',
    area: 'settings',
    builder: 'srv',
    file: 'settings/server/sections/Integrations.tsx',
    select: '#ss-webhook-channel',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, settings(ids, 'integrations'));
      await page.getByRole('tab', { name: 'Webhooks' }).click();
      await page.getByRole('button', { name: 'Create webhook' }).first().click();
      await page.waitForSelector('#ss-webhook-channel');
    },
    scope: (page) => holding(page, '[role="dialog"]', '#ss-webhook-channel'),
  },
  {
    name: 'community-wizard',
    area: 'settings',
    builder: 'srv',
    file: 'settings/server/sections/CommunityWizard.tsx',
    select: '#ss-wiz-rules',
    keyboard: true,
    open: async ({ page, ids }) => {
      await fakeCommunity(page, ids.guildId);
      await goto(page, settings(ids, 'community'));
      await page.getByLabel('Enable Community').first().click({ force: true });
      const dlg = page.getByRole('dialog', { name: 'Enable Community' });
      await dlg.waitFor();
      await expect(dlg.getByRole('button', { name: 'Next' })).toBeEnabled({ timeout: 15_000 });
      await dlg.getByRole('button', { name: 'Next' }).click();
      await page.waitForSelector('#ss-wiz-rules');
    },
    scope: (page) => holding(page, '[role="dialog"]', '#ss-wiz-rules'),
  },
  {
    name: 'community',
    area: 'settings',
    builder: 'srv',
    file: 'settings/server/sections/Community.tsx',
    select: '#ss-rules-channel',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, settings(ids, 'community'));
      await page.waitForSelector('#ss-rules-channel');
    },
    scope: (page) => holding(page, '.ss-form', '#ss-rules-channel'),
  },
  {
    name: 'bridge-conflicts',
    area: 'settings',
    builder: 'srv',
    file: 'settings/server/bridge/ConflictLog.tsx',
    select: '[data-testid=bridge-conflicts] select',
    keyboard: true,
    open: async ({ page, ids }) => {
      await fakeBridge(page, ids.guildId);
      await goto(page, settings(ids, 'bridge'));
      await page.waitForSelector('[data-testid=bridge-conflicts] select');
    },
    scope: (page) => page.locator('[data-testid=bridge-conflicts]'),
  },
  // ---- reader: the Library
  {
    name: 'series-edition',
    area: 'library',
    builder: 'reader',
    file: 'library/SeriesPage.tsx',
    select: '.lib-edition select',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/library/s/${ids.seriesId}`);
      await page.waitForSelector('.lib-edition select');
    },
    scope: (page) => page.locator('.lib-edition').first().locator('xpath=..'),
  },
  {
    name: 'rankings-kind',
    area: 'library',
    builder: 'reader',
    file: 'library/Rankings.tsx',
    select: '.lib-select select',
    keyboard: true,
    open: async ({ page }) => {
      await goto(page, '/library/rankings');
      await page.waitForSelector('.lib-select select');
    },
    scope: (page) => page.locator('.lib-select').first().locator('xpath=..'),
  },
  {
    name: 'mylibrary-sort',
    area: 'library',
    builder: 'reader',
    file: 'library/Pages.tsx',
    select: '.lib-page-head select',
    keyboard: true,
    open: async ({ page }) => {
      await goto(page, '/library/me');
      await page.waitForSelector('.lib-page-head select');
    },
    scope: (page) => page.locator('.lib-page-head').first(),
  },
  {
    name: 'lists-new',
    area: 'library',
    builder: 'reader',
    file: 'library/Lists.tsx (new list form)',
    select: 'form.lib-review-form select',
    keyboard: true,
    open: async ({ page }) => {
      await goto(page, '/library/lists');
      await page.getByRole('button', { name: 'New list' }).first().click();
      await page.waitForSelector('form.lib-review-form select');
    },
    scope: (page) => page.locator('form.lib-review-form').first(),
  },
  {
    name: 'lists-edit',
    area: 'library',
    builder: 'reader',
    file: 'library/Lists.tsx (edit list form)',
    select: 'form.lib-review-form select',
    open: async ({ page, ids }) => {
      await goto(page, `/library/lists/${ids.listId}`);
      await page.getByRole('button', { name: 'Edit list' }).first().click();
      await page.waitForSelector('form.lib-review-form select');
    },
    scope: (page) => page.locator('form.lib-review-form').first(),
  },
  {
    name: 'reader-chapter',
    area: 'library',
    builder: 'reader',
    file: 'library/reader/Reader.tsx',
    select: '.lib-chapter-select select',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/library/r/${ids.chapterId}/1`);
      for (let i = 0; i < 6; i++) {
        if (
          await page
            .locator('.lib-chapter-select select')
            .first()
            .isVisible()
            .catch(() => false)
        )
          return;
        // The reader's bars come and go: a key press or a click in the middle brings them back.
        await page.keyboard.press('ArrowRight').catch(() => undefined);
        await page.keyboard.press('ArrowLeft').catch(() => undefined);
        await page.waitForTimeout(700);
        await page.mouse.click(page.viewportSize()!.width / 2, page.viewportSize()!.height / 2);
        await page.waitForTimeout(700);
      }
      await page.waitForSelector('.lib-chapter-select select', { state: 'visible', timeout: 5000 });
    },
    scope: (page) => page.locator('.lib-chapter-select').first().locator('xpath=..'),
  },
  // ---- devs: the developer portal
  {
    name: 'commands-type',
    area: 'dev',
    builder: 'devs',
    file: 'developers/tabs/Commands.tsx (Type, Option type)',
    select: '#cmd-type',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId}?tab=commands`);
      await page.waitForSelector('#cmd-type');
      // The option's type select needs a slash command with an option.
      await page.getByRole('button', { name: 'Add option' }).click();
      await page.waitForSelector('#opt-type-0');
    },
    scope: (page) => holding(page, 'section.dev-card', '#cmd-type'),
  },
  {
    name: 'oauth2-redirect',
    area: 'dev',
    builder: 'devs',
    file: 'developers/tabs/OAuth2.tsx',
    select: 'section.dev-card select.dev-select, section.dev-card select',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId2}?tab=oauth2`);
      await page.waitForSelector('section.dev-card select');
    },
    scope: (page) => holding(page, 'section.dev-card', 'select'),
  },
  {
    name: 'richpresence-type',
    area: 'dev',
    builder: 'devs',
    file: 'developers/tabs/Media.tsx',
    select: 'form select',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(page, `/developers/applications/${ids.appId}?tab=richpresence`);
      await page.waitForSelector('.dev-main form select');
    },
    scope: (page) => holding(page, '.dev-main form', 'select'),
  },
  {
    name: 'api-reference-area',
    area: 'dev',
    builder: 'devs',
    file: 'developers/Reference.tsx',
    select: '.pkg-toolbar select',
    keyboard: true,
    open: async ({ page }) => {
      await goto(page, '/developers/docs/api-reference');
      await page.waitForSelector('.pkg-toolbar select', { timeout: 30_000 });
    },
    scope: (page) => page.locator('.pkg-toolbar').first(),
  },
  {
    name: 'authorize-guild',
    area: 'dev',
    builder: 'devs',
    file: 'developers/Authorize.tsx',
    select: '#dev-auth-guild',
    keyboard: true,
    open: async ({ page, ids }) => {
      await goto(
        page,
        `/oauth2/authorize?client_id=${ids.appId}&scope=${encodeURIComponent('bot applications.commands')}`,
      );
      await page.waitForSelector('#dev-auth-guild', { timeout: 30_000 });
    },
    scope: (page) => page.locator('.dev-auth-card').first(),
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
  kit: boolean;
  options: number;
  labelLines: number;
  labelText: string;
}
interface Measured {
  scroll: number[];
  selects: Record<string, SelectFig>;
  controls: { key: string; box: Box }[];
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
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return {
        x: Math.round((r.left - rr.left) * 10) / 10,
        y: Math.round((r.top - rr.top) * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const visible = (e: Element) => e.getClientRects().length > 0;
    const selEls = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).filter(visible);
    const selects: Record<string, SelectFig> = {};
    selEls.forEach((s, n) => {
      const label =
        s.closest('label') ?? (s.id ? document.querySelector(`label[for="${s.id}"]`) : null);
      let labelText = '';
      let labelLines = 0;
      if (label) {
        const w = document.createTreeWalker(label, NodeFilter.SHOW_TEXT);
        const tops = new Set<number>();
        for (let t = w.nextNode(); t; t = w.nextNode()) {
          if (s.contains(t)) continue;
          if (!(t.textContent ?? '').trim()) continue;
          labelText += (t.textContent ?? '').trim() + ' ';
          const range = document.createRange();
          range.selectNodeContents(t);
          for (const r of Array.from(range.getClientRects()))
            if (r.width > 0 && r.height > 0) tops.add(Math.round(r.top));
        }
        labelLines = tops.size;
      }
      const key = `${s.id.replace(/^_r_[a-z0-9]+_-?/, '') || s.getAttribute('aria-label') || labelText.trim() || 'select'}#${n}`;
      selects[key] = {
        box: box(s),
        kit: s.parentElement?.tagName === 'SPAN',
        options: s.options.length,
        labelLines,
        labelText: labelText.trim(),
      };
    });
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter(visible);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24)}`,
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
            `${s.id || s.className} x ${o.tagName.toLowerCase()} (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
    }
    return {
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      overlaps,
    };
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

async function setPrefs(ctx: BrowserContext, c: Cfg) {
  const main = c.area === 'library' ? 'lounge-dark' : c.theme;
  const sectionThemes = {
    library: c.area === 'library' ? c.theme : 'sideroom',
  };
  const csrf = await ctx.request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
  const token = ((await csrf.json()) as { token: string }).token;
  const res = await ctx.request.fetch(`${API}/users/@me/settings`, {
    method: 'PATCH',
    headers: { Origin: ORIGIN, 'X-SR-CSRF': token },
    data: { settings: { user_prefs: { theme: main, zoom: 100, sectionThemes } } },
  });
  expect(res.ok()).toBeTruthy();
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(main);
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(sectionThemes.library);
  return { main, sectionThemes };
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg, prefs: unknown) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((p) => {
    try {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    } catch {
      /* no storage */
    }
  }, prefs);
  await ctx.route('**/cdn/p102-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p102-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function shot(page: Page, scope: Locator, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  const file = resolve(OUT, `${shotName(name)}.png`);
  try {
    await scope.locator('select').first().scrollIntoViewIfNeeded({ timeout: 3000 });
    const b = await scope.boundingBox();
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

/** Focus, ArrowDown changes the value, the scope reacts, ArrowUp puts it back. */
async function keyboard(
  page: Page,
  stop: Stop,
  tag: string,
  problems: string[],
): Promise<KeyFig | null> {
  const sel = page.locator(stop.select).first();
  const scope = stop.scope(page);
  const enabledOptions = await sel.evaluate(
    (e) => Array.from((e as HTMLSelectElement).options).filter((o) => !o.disabled).length,
  );
  if (enabledOptions < 2) {
    say(`${tag} ${stop.name}: keyboard not driven (one enabled option)`);
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
  const sel2 = page.locator(stop.select).first();
  const changed = await sel2.inputValue();
  const text1 = await page.locator('body').innerText();
  await sel2.focus();
  await page.keyboard.press('ArrowUp');
  await page.waitForTimeout(500);
  const back = await page.locator(stop.select).first().inputValue();
  void scope;
  const k: KeyFig = {
    before,
    changed,
    back,
    focusVisible,
    reacted: text0 !== text1 || changed !== before,
    enabledOptions,
  };
  say(`${tag} ${stop.name}: keyboard ${JSON.stringify(k)}`);
  if (changed === before) problems.push(`${tag} ${stop.name}: ArrowDown did not change the value`);
  if (back !== before) problems.push(`${tag} ${stop.name}: ArrowUp did not put the value back`);
  if (!focusVisible) problems.push(`${tag} ${stop.name}: :focus-visible did not match after a key`);
  return k;
}

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  ids: Ids,
  problems: string[],
  keys: Record<string, KeyFig | null>,
): Promise<Figures> {
  const prefs = await setPrefs(owner, c);
  const fig: Figures = {};
  const { ctx } = await newPage(browser, owner, c, {
    theme: prefs.main,
    zoom: 100,
    sectionThemes: prefs.sectionThemes,
  });
  try {
    for (const stop of STOPS.filter((s) => s.area === c.area)) {
      if (AREAS && !AREAS.includes(c.area)) continue;
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
          problems.push(msg);
        }
        if (c.shots) await shot(page, scope, `${stop.name}-${c.tag}`);
        if (c.keys && stop.keyboard)
          keys[`${c.tag} ${stop.name}`] = await keyboard(page, stop, tag, problems);
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
  base: Box | null;
  after: Box | null;
  kit: boolean;
  reasons: string[];
}

function compare(c: Cfg, base: Figures, after: Figures, verdicts: Verdict[], notes: string[]) {
  for (const stop of STOPS.filter((s) => s.area === c.area)) {
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
    for (const key of Object.keys(a.selects)) {
      const as = a.selects[key]!;
      const bs = b.selects[key];
      const reasons: string[] = [];
      if (!bs) {
        notes.push(`${label} "${key}": not in the base`);
        continue;
      }
      if (as.kit) {
        if (as.box.h < 40 - 0.5) reasons.push(`block size ${as.box.h} under 40`);
        if (c.touch && as.box.h < 44 - 0.5) reasons.push(`block size ${as.box.h} under 44 at 375`);
      }
      if (as.box.w < bs.box.w - 0.5)
        reasons.push(`inline size ${as.box.w} under the base's ${bs.box.w}`);
      if (as.labelLines > bs.labelLines)
        reasons.push(`label wraps: ${as.labelLines} lines against ${bs.labelLines}`);
      // Neighbours: each control may move down by at most the height the selects wholly above it gained.
      if (a.controls.length !== b.controls.length) {
        reasons.push(`control count ${b.controls.length} -> ${a.controls.length}`);
      } else {
        b.controls.forEach((bc, i) => {
          const ac = a.controls[i]!;
          let allowed = 0;
          for (const k2 of Object.keys(a.selects)) {
            const s2a = a.selects[k2];
            const s2b = b.selects[k2];
            if (!s2a || !s2b) continue;
            const diff = s2a.box.h - s2b.box.h;
            if (s2b.box.y + s2b.box.h <= bc.box.y + 0.5) allowed += diff;
            else if (s2b.box.y < bc.box.y + bc.box.h) allowed += Math.abs(diff);
          }
          const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
          const mvx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
          if (mv > allowed + 1 && !bc.key.includes(':select#'))
            reasons.push(`${bc.key} moved ${mv} px down, allowed ${Math.round(allowed * 10) / 10}`);
          if (mv < -(Math.abs(allowed) + 1) && !bc.key.includes(':select#'))
            reasons.push(`${bc.key} moved ${mv} px up`);
          if (Math.abs(mvx) > 1 && !bc.key.includes(':select#'))
            notes.push(
              `${label} "${key}": ${bc.key} moved ${mvx} px sideways (the select is ${as.box.w - bs.box.w} px wider)`,
            );
        });
      }
      verdicts.push({
        tag: c.tag,
        stop: stop.name,
        builder: stop.builder,
        file: stop.file,
        select: key,
        base: bs.box,
        after: as.box,
        kit: as.kit,
        reasons,
      });
    }
    // Hit test: the same rows, none worse.
    const br = new Map(b.hitRows.map((r) => [r.key, r.status] as const));
    const rank: Record<string, number> = { pass: 3, skipped: 2, fail: 1, 'not-reached': 0 };
    const hitReasons: string[] = [];
    if (sum(a.hit) !== sum(b.hit)) hitReasons.push(`hit test rows ${sum(b.hit)} -> ${sum(a.hit)}`);
    for (const r of a.hitRows) {
      const was = br.get(r.key);
      if (was !== undefined && rank[r.status]! < rank[was]!)
        hitReasons.push(`hit test ${r.key}: ${was} -> ${r.status}`);
    }
    if (a.hit.fail > b.hit.fail)
      hitReasons.push(
        `hit test fails ${b.hit.fail} -> ${a.hit.fail}: ${JSON.stringify(a.hitFails)}`,
      );
    if (hitReasons.length) {
      for (const v of verdicts.filter((x) => x.tag === c.tag && x.stop === stop.name))
        v.reasons.push(...hitReasons);
    }
  }
}

/** A side-by-side picture of the base and the after shot (base left), rendered in a page of its own. */
async function composites(browser: Browser) {
  const ctx = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const page = await ctx.newPage();
  const made: string[] = [];
  for (const c of CONFIGS) {
    for (const s of STOPS.filter((x) => x.area === c.area)) {
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

test(`p102 pages: Server settings, Library and Developer portal selects, ${PHASE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  try {
    runCli([
      'staff',
      'appoint',
      owner.username,
      '--role',
      'Senior Staff',
      '--reason',
      'e2e p102 pages shots',
    ]);
  } catch (e) {
    say(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);

  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
  const chans = (await (
    await octx.request.get(`${API}/guilds/${world.guildId}/channels`)
  ).json()) as { id: string; type: number }[];
  const stamp = String(Date.now()).slice(-6); // digits only: the same width every run
  const problems: string[] = [];
  const notes: string[] = [];
  const figs: Record<string, Figures> = {};
  const keys: Record<string, KeyFig | null> = {};
  const ids: Ids = {
    guildId: world.guildId,
    channelId: chans.find((x) => x.type === 0)!.id,
    seriesId: '',
    chapterId: '',
    chapter2Id: '',
    listId: '',
    appId: '',
    appId2: '',
  };
  try {
    // ---- the Library's data: one series, two editions, two chapters of the first, one of the second
    const series = await call<{ id: string }>(octx.request, 'POST', '/admin/library/series', {
      kind: 'manga',
      title: `p102-pages-${stamp}`,
      synopsis: 'A series made by the wave 102 pages spec.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    });
    ids.seriesId = series.id;
    const edition = await call<{ id: string }>(
      octx.request,
      'POST',
      `/admin/library/series/${ids.seriesId}/editions`,
      { language: 'en', label: 'Official', is_default: true },
    );
    const edition2 = await call<{ id: string }>(
      octx.request,
      'POST',
      `/admin/library/series/${ids.seriesId}/editions`,
      { language: 'fr', label: 'Fan edition', is_default: false },
    );
    const mkChapter = async (editionId: string, number: number) => {
      const ch = await call<{ id: string }>(
        octx.request,
        'POST',
        `/admin/library/editions/${editionId}/chapters`,
        { number, title: `p102-pages chapter ${number}`, format: 'images' },
      );
      await call(
        octx.request,
        'PUT',
        `/admin/library/chapters/${ch.id}/pages`,
        [0, 1, 2].map((i) => ({
          key: `library/${ids.seriesId}/${ch.id}/00${i + 1}.webp`,
          width: 200,
          height: 300,
          variants: {},
        })),
      );
      await call(octx.request, 'POST', `/admin/library/chapters/${ch.id}/publish`);
      return ch.id;
    };
    ids.chapterId = await mkChapter(edition.id, 1);
    ids.chapter2Id = await mkChapter(edition.id, 2);
    await mkChapter(edition2.id, 1);
    await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/publish`);
    const list = await call<{ id: string }>(octx.request, 'POST', '/library/lists', {
      name: `p102-pages-list-${stamp}`,
      visibility: 'private',
    });
    ids.listId = list.id;
    // ---- the developer portal's data: two applications; the second has two redirect URIs
    const mkApp = async (name: string) =>
      (await call<{ application: { id: string } }>(octx.request, 'POST', '/applications', { name }))
        .application.id;
    ids.appId = await mkApp(`p102-pages-app-${stamp}`);
    ids.appId2 = await mkApp(`p102-pages-app2-${stamp}`);
    await call(octx.request, 'PATCH', `/applications/${ids.appId2}`, {
      redirect_uris: [
        `https://p102-pages-${stamp}.example.com/one`,
        `https://p102-pages-${stamp}.example.com/two`,
      ],
    });
    say(`ids ${JSON.stringify(ids)}`);

    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, octx, c, ids, problems, keys);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
    if (ids.seriesId) {
      await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/unpublish`).catch(
        () => undefined,
      );
      await call(octx.request, 'DELETE', `/admin/library/series/${ids.seriesId}`).catch((e) =>
        say(`series delete: ${String(e).slice(0, 160)}`),
      );
    }
    if (ids.listId)
      await call(octx.request, 'DELETE', `/library/lists/${ids.listId}`).catch((e) =>
        say(`list delete: ${String(e).slice(0, 160)}`),
      );
    for (const id of [ids.appId, ids.appId2].filter(Boolean))
      await call(octx.request, 'DELETE', `/applications/${id}`, { password: owner.password }).catch(
        (e) =>
          say(
            `application ${id} not deleted (p102-pages-app-${stamp}): ${String(e).slice(0, 160)}`,
          ),
      );
    await octx.close();
  }
  const mf = resolve(OUT, `measure-${PHASE}.json`);
  if ((AREAS || ONLY) && existsSync(mf)) {
    const prev = JSON.parse(readFileSync(mf, 'utf8')) as {
      figs: Record<string, Figures>;
      keys: typeof keys;
    };
    for (const [tag, f] of Object.entries(figs))
      prev.figs[tag] = { ...(prev.figs[tag] ?? {}), ...f };
    Object.assign(prev.keys, keys);
    writeFileSync(mf, JSON.stringify(prev, null, 1));
    for (const tag of Object.keys(prev.figs)) figs[tag] = prev.figs[tag]!;
  } else writeFileSync(mf, JSON.stringify({ figs, keys }, null, 1));
  const baseFile = resolve(OUT, 'measure-base.json');
  const verdicts: Verdict[] = [];
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as {
      figs: Record<string, Figures>;
    };
    for (const c of CONFIGS)
      if (base.figs[c.tag] && figs[c.tag])
        compare(c, base.figs[c.tag]!, figs[c.tag]!, verdicts, notes);
    const made = await composites(browser);
    const broken = verdicts.filter((v) => v.reasons.length);
    writeFileSync(
      resolve(OUT, 'compare.json'),
      JSON.stringify({ verdicts, broken, notes, composites: made }, null, 1),
    );
    for (const v of broken)
      problems.push(`${v.tag} ${v.builder} ${v.file} "${v.select}": ${v.reasons.join('; ')}`);
  } else if (PHASE === 'after') say('no measure-base.json: nothing to compare against');
  for (const n of notes) say(`note: ${n}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
