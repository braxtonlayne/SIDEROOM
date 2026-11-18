import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-958 (wave 101, mod): the 15 raw selects of Server settings > Moderation (the rule dialog), Community (Guide,
 * Insights, Directory), Integrations > Command permissions (the dialog), the Discord bridge (map form) and its
 * whole-server plan (link by hand) move to the kit's `NativeSelect`. A record of every select's box (and of every
 * control's box in its section or dialog) with the exact hit test, base then after, plus the assertions the comparison
 * needs. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world";
 * never edited). It makes no data on the server (D-942) and saves nothing: the bridge and the installed bot are
 * answered by `page.route` fakes in the browser (as `p61-bridge-shots.spec.ts` does), under names starting `p101-mod-`;
 * the keyboard cases change a draft or a view and put it back before the page is left.
 *
 * Phases: `SR_P101_MOD_PHASE=base` records the unedited tree (shots get a `base-` prefix, figures go to
 * `figures-base-<view>.json`); the default `after` records the same and, when the base figures are in the folder,
 * compares (block size not smaller than the base, at least 44 px at 375 touch, inline size not smaller, the controls
 * below moved by no more than the summed height difference above them, every block that holds a select no taller
 * than the base plus that difference, nothing overlapping, hit test equal row by row). Configs: 1024 x 768 Lounge
 * Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light. The account theme is set by PATCH and localStorage
 * and read back, `data-theme` is read from the page before each shot.
 *
 * Local only (SR_P101_MOD_ON=1). SR_P101_MOD_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8785, Vite 5785, metrics 9485,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5785 SR_P101_MOD_ON=1 \
 *   SR_P101_MOD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-mod \
 *   npx playwright test tests/p101-mod-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * After the merge there is no base to compare with (set `SR_P101_MOD_PHASE=base` only on a tree where the five files
 * still have raw selects); delete the `figures-base-*.json` of another tree first.
 */
const ON = process.env.SR_P101_MOD_ON === '1';
const PHASE = process.env.SR_P101_MOD_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P101_MOD_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-mod';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const APP_ID = '1300000000000000001';
const BOT_NAME = 'p101-mod-bot';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-light' | 'lounge-dark';
}
const CONFIGS: Cfg[] = [
  { tag: '1024-lounge-light', width: 1024, height: 768, touch: false, theme: 'lounge-light' },
  { tag: '1024-lounge-dark', width: 1024, height: 768, touch: false, theme: 'lounge-dark' },
  { tag: '375-lounge-light', width: 375, height: 812, touch: true, theme: 'lounge-light' },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

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

// ---- fakes (browser only; nothing is written to the server) ----

async function fakeBridge(page: Page, guildId: string, channelId: string, withPlan: boolean) {
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/sr/features$`), async (route) => {
    const res = await route.fetch();
    const list = (await res.json()) as { id: string; enabled: boolean }[];
    await route.fulfill({
      response: res,
      json: list.map((m) => (m.id === 'bridge.enabled' ? { ...m, enabled: true } : m)),
    });
  });
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/status$`), (route) =>
    route.fulfill({
      json: {
        platform_on: true,
        module_on: true,
        connected: true,
        bridge: {
          discord_guild_id: '1100000000000000001',
          discord_guild_name: 'p101-mod-friends',
          bot_user_id: '1100000000000000002',
          bot_name: 'p101-mod-relay',
          mode: 'selective',
          status: 'connected',
          intents_granted: (1 << 15) | 1,
          created_by: '1',
          created_at: 0,
          invite_url: 'https://discord.com/oauth2/authorize?client_id=1&scope=bot',
        },
        maps: [
          {
            id: '1200000000000000001',
            channel_id: channelId,
            discord_channel_id: '1100000000000000010',
            discord_channel_name: 'p101-mod-game-night',
            direction: 'both',
            has_webhook: true,
            queued: 0,
            dead: 0,
            shared_on_instance: false,
            created_at: 0,
          },
        ],
        queued: 0,
        dead_letters: 0,
        leftovers: [],
        erasure_leftovers: [],
        is_owner: true,
      },
    }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/discord-channels$`), (route) =>
    route.fulfill({
      json: [
        { id: '1100000000000000010', name: 'p101-mod-game-night' },
        { id: '1100000000000000011', name: 'p101-mod-off-topic' },
        { id: '1100000000000000012', name: 'p101-mod-announcements' },
      ],
    }),
  );
  if (!withPlan) return;
  const item = (action: string, name: string) => ({
    kind: 'channel',
    action,
    sr_id: action === 'create_here' ? null : `9${name.length}00000000000000`,
    discord_id: action === 'create_on_discord' ? null : `8${name.length}00000000000000`,
    name,
    sr_permissions: '0',
    discord_permissions: '0',
    dangerous_bits: '0',
    import: false,
  });
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/plan$`), (route) =>
    route.fulfill({ json: { plan_id: 'p101-mod-plan' } }),
  );
  await page.route(new RegExp(`/api/v1/guilds/${guildId}/bridge/plan/p101-mod-plan$`), (route) =>
    route.fulfill({
      json: {
        id: 'p101-mod-plan',
        status: 'ready',
        seed_structure: 'sr',
        plan_hash: 'p101-mod-hash',
        expires_at: Date.now() + 3_600_000,
        blockers: [],
        items: [
          item('match', 'p101-mod-general'),
          item('create_on_discord', 'p101-mod-only-here'),
          item('create_on_discord', 'p101-mod-only-here-two'),
          item('create_here', 'p101-mod-only-there'),
          item('create_here', 'p101-mod-only-there-two'),
        ],
        not_mirrored: [],
        estimated_writes: 4,
      },
    }),
  );
}

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
            bot: { id: '1300000000000000002', username: BOT_NAME },
          },
          user: { id: '1300000000000000003', username: 'p101-mod-owner' },
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
          { id: '1300000000000000011', name: 'p101-mod-ping' },
          { id: '1300000000000000012', name: 'p101-mod-roll' },
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

// ---- the views ----

interface View {
  name: string;
  /** What the measures and the hit test cover ('dialog': the innermost dialog, the settings layer is one too). */
  scope: string;
  /** A second shot with the last select of the scope in view. */
  lastShot?: boolean;
  /** A select the keyboard drives, and what it must do. */
  keyboard?: (
    page: Page,
    say: (s: string) => void,
    problems: string[],
    tag: string,
  ) => Promise<void>;
  open: (page: Page, g: string, channelId: string) => Promise<void>;
}

const section = (g: string, id: string) => `/settings/server/${g}/${id}`;

async function arrow(page: Page, sel: string, key: 'ArrowDown' | 'ArrowUp') {
  await page.locator(sel).first().focus();
  await page.keyboard.press(key);
  await page.waitForTimeout(250);
}

const VIEWS: View[] = [
  {
    name: 'moderation',
    scope: 'dialog',
    lastShot: true,
    open: async (page, g) => {
      await page.goto(section(g, 'moderation'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByRole('button', { name: 'Create rule' }).first().click();
      await page.waitForSelector('#ss-rule-trigger');
      await page.getByLabel('Send alert to a channel').check({ force: true });
      await page.getByLabel('Timeout member').check({ force: true });
      await page.waitForSelector('#ss-rule-alert');
      await page.waitForSelector('#ss-rule-timeout');
    },
    keyboard: async (page, sayIt, problems, tag) => {
      const dlg = page.locator('[role=dialog]').last();
      const before = await page.locator('#ss-rule-trigger').inputValue();
      const textBefore = await dlg.innerText();
      await arrow(page, '#ss-rule-trigger', 'ArrowDown');
      const changed = await page.locator('#ss-rule-trigger').inputValue();
      const textAfter = await dlg.innerText();
      await arrow(page, '#ss-rule-trigger', 'ArrowUp');
      const back = await page.locator('#ss-rule-trigger').inputValue();
      sayIt(
        `${tag}: moderation keyboard ${JSON.stringify({ before, changed, back, formChanged: textBefore !== textAfter })}`,
      );
      if (changed === before)
        problems.push(`${tag}: moderation trigger: ArrowDown did not change the value`);
      if (textBefore === textAfter)
        problems.push(`${tag}: moderation trigger: the form did not react`);
      if (back !== before)
        problems.push(`${tag}: moderation trigger: ArrowUp did not restore the value`);
    },
  },
  {
    name: 'community-guide',
    scope: 'main',
    open: async (page, g) => {
      await page.goto(section(g, 'community'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByRole('tab', { name: 'Server Guide' }).click();
      await page.getByRole('button', { name: 'Add a to-do' }).click();
      await page.waitForSelector('#ss-guide-ch-0');
    },
  },
  {
    name: 'community-insights',
    scope: 'main',
    open: async (page, g) => {
      await page.goto(section(g, 'community'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByRole('tab', { name: 'Insights' }).click();
      await page.waitForSelector('#ss-insights-days');
    },
    keyboard: async (page, sayIt, problems, tag) => {
      const urls: string[] = [];
      page.on('request', (r) => {
        if (/insights/.test(r.url())) urls.push(r.url());
      });
      const before = await page.locator('#ss-insights-days').inputValue();
      await arrow(page, '#ss-insights-days', 'ArrowDown');
      const changed = await page.locator('#ss-insights-days').inputValue();
      await page.waitForTimeout(500);
      await arrow(page, '#ss-insights-days', 'ArrowUp');
      const back = await page.locator('#ss-insights-days').inputValue();
      sayIt(
        `${tag}: insights keyboard ${JSON.stringify({ before, changed, back, requests: urls.map((u) => new URL(u).search) })}`,
      );
      if (changed === before)
        problems.push(`${tag}: insights range: ArrowDown did not change the value`);
      if (!urls.some((u) => new RegExp(`days=${changed}\\b`).test(u)))
        problems.push(`${tag}: insights range: no request for days=${changed}`);
      if (back !== before)
        problems.push(`${tag}: insights range: ArrowUp did not restore the value`);
    },
  },
  {
    name: 'community-directory',
    scope: 'main',
    open: async (page, g) => {
      await page.goto(section(g, 'community'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByRole('tab', { name: 'Directory' }).click();
      await page.waitForSelector('#ss-listing-category');
    },
  },
  {
    name: 'bot-permissions',
    scope: 'dialog',
    open: async (page, g, channelId) => {
      await fakeBot(page, g, channelId);
      await page.goto(section(g, 'integrations'), {
        waitUntil: 'domcontentloaded',
        timeout: 20_000,
      });
      await page.getByRole('button', { name: `Command permissions of ${BOT_NAME}` }).click();
      await page.waitForSelector('#cmdperm-target');
      await page.waitForSelector('[role=dialog] select[aria-label^="Rule for"]');
    },
    keyboard: async (page, sayIt, problems, tag) => {
      const before = await page.locator('#cmdperm-target').inputValue();
      await arrow(page, '#cmdperm-target', 'ArrowDown');
      const changed = await page.locator('#cmdperm-target').inputValue();
      await arrow(page, '#cmdperm-target', 'ArrowUp');
      const back = await page.locator('#cmdperm-target').inputValue();
      sayIt(`${tag}: command permissions keyboard ${JSON.stringify({ before, changed, back })}`);
      if (changed === before)
        problems.push(`${tag}: command target: ArrowDown did not change the value`);
      if (back !== before)
        problems.push(`${tag}: command target: ArrowUp did not restore the value`);
    },
  },
  {
    name: 'bridge',
    scope: 'main',
    open: async (page, g, channelId) => {
      await fakeBridge(page, g, channelId, false);
      await page.goto(section(g, 'bridge'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.waitForSelector('.ss-bridge-add select');
    },
    keyboard: async (page, sayIt, problems, tag) => {
      const dir = '.ss-bridge-add select >> nth=2';
      const before = await page.locator(dir).inputValue();
      await arrow(page, dir, 'ArrowDown');
      const changed = await page.locator(dir).inputValue();
      await arrow(page, dir, 'ArrowUp');
      const back = await page.locator(dir).inputValue();
      sayIt(`${tag}: bridge direction keyboard ${JSON.stringify({ before, changed, back })}`);
      if (changed === before)
        problems.push(`${tag}: bridge direction: ArrowDown did not change the value`);
      if (back !== before)
        problems.push(`${tag}: bridge direction: ArrowUp did not restore the value`);
    },
  },
  {
    name: 'bridge-plan',
    scope: 'main',
    open: async (page, g, channelId) => {
      await fakeBridge(page, g, channelId, true);
      await page.goto(section(g, 'bridge'), { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByRole('button', { name: 'Prepare the whole-server plan' }).click();
      await page.getByRole('button', { name: 'Build the plan' }).click();
      await page.getByRole('tab', { name: /^Channels \(/ }).click();
      await page.waitForSelector('[data-testid=bridge-plan] .ss-bridge-add select');
    },
    keyboard: async (page, sayIt, problems, tag) => {
      const sel = '[data-testid=bridge-plan] .ss-bridge-add select';
      const link = page.locator('[data-testid=bridge-plan] .ss-bridge-add').getByRole('button', {
        name: 'Link',
        exact: true,
      });
      const disabledBefore = await link.isDisabled();
      await arrow(page, `${sel} >> nth=0`, 'ArrowDown');
      await arrow(page, `${sel} >> nth=1`, 'ArrowDown');
      const hereVal = await page.locator(`${sel} >> nth=0`).inputValue();
      const thereVal = await page.locator(`${sel} >> nth=1`).inputValue();
      const enabled = !(await link.isDisabled());
      await arrow(page, `${sel} >> nth=0`, 'ArrowUp');
      await arrow(page, `${sel} >> nth=1`, 'ArrowUp');
      const disabledAfter = await link.isDisabled();
      sayIt(
        `${tag}: plan link keyboard ${JSON.stringify({ disabledBefore, hereVal, thereVal, enabled, disabledAfter })}`,
      );
      if (!disabledBefore) problems.push(`${tag}: plan link: Link was enabled before a choice`);
      if (!enabled) problems.push(`${tag}: plan link: Link stayed disabled after two choices`);
      if (!disabledAfter) problems.push(`${tag}: plan link: ArrowUp did not put the choices back`);
    },
  },
];

// ---- measures ----

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  theme: { html: string | null; first: string | null };
  scroll: number[];
  selects: { key: string; box: Box }[];
  controls: { key: string; box: Box }[];
  blocks: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  rows: { key: string; status: string }[];
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: unknown[];
  opened: unknown[];
}

/** Boxes in scope coordinates: every scrolled ancestor's offset added, so a scrolled dialog compares with a top one. */
async function measure(page: Page, scope: string): Promise<Measured> {
  return page.evaluate((scopeSel) => {
    const root =
      scopeSel === 'dialog'
        ? Array.from(document.querySelectorAll('[role=dialog]')).pop()!
        : document.querySelector(scopeSel)!;
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      let sx = 0;
      let sy = 0;
      for (let p: Element | null = e.parentElement; p; p = p.parentElement) {
        sx += p.scrollLeft;
        sy += p.scrollTop;
      }
      return {
        x: Math.round((r.left + sx) * 10) / 10,
        y: Math.round((r.top + sy) * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const name = (e: Element) =>
      (
        e.id ||
        e.getAttribute('aria-label') ||
        e.closest('label')?.firstChild?.textContent ||
        e.textContent ||
        ''
      )
        .trim()
        .slice(0, 30);
    const sels = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).filter(
      (e) => e.getClientRects().length > 0,
    );
    const selects = sels.map((e, i) => ({ key: `${i}:select#${name(e)}`, box: box(e) }));
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}#${name(e)}`,
      box: box(e),
    }));
    const holders = new Set<Element>();
    for (const s of sels) {
      const b = s.closest('.ss-row, .ss-bridge-add, .ss-card, .ss-form');
      if (b) holders.add(b);
    }
    const blocks = Array.from(holders).map((e, i) => ({
      key: `${i}:${e.className.toString().split(' ')[0]}`,
      box: box(e),
    }));
    const overlaps: string[] = [];
    for (const s of sels) {
      const a = s.getBoundingClientRect();
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
        const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `${name(s)} x ${o.tagName.toLowerCase()}#${name(o)} (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
    }
    return {
      theme: {
        html: document.documentElement.getAttribute('data-theme'),
        first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      },
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      selects,
      controls,
      blocks,
      overlaps,
    };
  }, scope);
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  v: View,
  c: Cfg,
  g: string,
  channelId: string,
  problems: string[],
) {
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  const tag = `${v.name} ${c.tag}`;
  await v.open(page, g, channelId);
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready);
  await page.waitForTimeout(1200);

  const scope =
    v.scope === 'dialog' ? page.locator('[role=dialog]').last() : page.locator(v.scope).first();
  const m = await measure(page, v.scope);
  say(`${tag}: data-theme html=${m.theme.html} first=${m.theme.first}`);
  if (m.theme.html !== c.theme && m.theme.first !== c.theme)
    problems.push(`${tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  if (m.selects.length === 0) problems.push(`${tag}: no select in the scope (not reached)`);
  say(`${tag}: ${m.selects.length} selects ${JSON.stringify(m.selects)}`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);

  // Shot first, with the first select in view (a hit test scrolls).
  mkdirSync(OUT, { recursive: true });
  await scope.locator('select').first().scrollIntoViewIfNeeded();
  await page.waitForTimeout(400);
  await page.screenshot({
    path: resolve(OUT, `${shotName(`${v.name}-${c.tag}`)}.png`),
    timeout: 10_000,
  });
  if (v.lastShot) {
    // The selects further down the scrolled dialog (the alert channel and the timeout), in view for a second shot.
    await scope.locator('select').last().scrollIntoViewIfNeeded();
    await page.waitForTimeout(400);
    await page.screenshot({
      path: resolve(OUT, `${shotName(`${v.name}-actions-${c.tag}`)}.png`),
      timeout: 10_000,
    });
  }

  const opened = await openCollapsed(page, scope);
  const rows = await hitTestAll(page, scope);
  const hit = count(rows);
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: hit ${JSON.stringify(hit)}`);
  if (hit.notReached) problems.push(`${tag}: ${hit.notReached} controls not reached (probe error)`);
  const fig: Figures = {
    ...m,
    rows: rows.map((r) => ({ key: `${r.tag} "${r.name}"`, status: r.status })),
    hit,
    hitFails,
    opened,
  };

  if (c.tag === '1024-lounge-light' && v.keyboard) {
    await page.evaluate(() => window.scrollTo(0, 0));
    await v.keyboard(page, say, problems, tag);
  }
  await ctx.close();
  return fig;
}

function compare(c: Cfg, v: string, base: Figures, after: Figures, problems: string[]) {
  const tag = `${v} ${c.tag}`;
  const diffs: Record<string, unknown> = {};
  if (base.selects.length !== after.selects.length) {
    problems.push(`${tag}: select count ${base.selects.length} -> ${after.selects.length}`);
    return diffs;
  }
  const sel: { key: string; base: Box; after: Box }[] = [];
  base.selects.forEach((b, i) => {
    const a = after.selects[i]!;
    sel.push({ key: b.key, base: b.box, after: a.box });
    if (a.box.h < b.box.h - 0.5)
      problems.push(`${tag}: ${b.key} shorter than base (${a.box.h} < ${b.box.h})`);
    if (c.touch && a.box.h < 44)
      problems.push(`${tag}: ${b.key} is ${a.box.h} px tall at 375 touch`);
    if (a.box.w < b.box.w - 0.5)
      problems.push(`${tag}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
  });
  diffs['selects'] = sel;
  const grow = (belowY: number) =>
    sel.reduce((s, x) => (x.base.y + x.base.h <= belowY + 0.5 ? s + (x.after.h - x.base.h) : s), 0);
  if (base.controls.length !== after.controls.length) {
    problems.push(`${tag}: control count ${base.controls.length} -> ${after.controls.length}`);
    return diffs;
  }
  const moved: { key: string; moved: number; allowed: number }[] = [];
  base.controls.forEach((bc, i) => {
    const ac = after.controls[i]!;
    const allowed = Math.round(grow(bc.box.y) * 10) / 10;
    const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
    moved.push({ key: bc.key, moved: mv, allowed });
    if (mv > allowed + 1) problems.push(`${tag}: ${bc.key} moved ${mv} px, allowed ${allowed}`);
    if (mv < -1) problems.push(`${tag}: ${bc.key} moved up ${mv} px`);
  });
  diffs['moved'] = moved.filter((x) => x.moved !== 0 || x.allowed !== 0);
  const blocks: unknown[] = [];
  base.blocks.forEach((bb, i) => {
    const ab = after.blocks[i];
    if (!ab) return;
    const own = sel
      .filter((x) => x.base.y >= bb.box.y - 0.5 && x.base.y + x.base.h <= bb.box.y + bb.box.h + 0.5)
      .reduce((s, x) => s + (x.after.h - x.base.h), 0);
    blocks.push({ key: bb.key, base: bb.box, after: ab.box, selectGrowth: own });
    if (ab.box.h > bb.box.h + own + 1)
      problems.push(`${tag}: block ${bb.key} grew ${ab.box.h - bb.box.h} px (selects grew ${own})`);
    if (ab.box.w < bb.box.w - 1) problems.push(`${tag}: block ${bb.key} narrower than base`);
  });
  diffs['blocks'] = blocks;
  const bs = JSON.stringify(base.rows);
  const as = JSON.stringify(after.rows);
  if (bs !== as) {
    const diff = base.rows
      .map((r, i) => ({ r, a: after.rows[i] }))
      .filter((x) => !x.a || x.a.key !== x.r.key || x.a.status !== x.r.status)
      .map((x) => `${x.r.key}: ${x.r.status} -> ${x.a?.status}`);
    problems.push(`${tag}: hit test rows differ: ${diff.join('; ')}`);
  }
  return diffs;
}

for (const v of VIEWS) {
  test(`p101 mod: ${v.name}, ${PHASE}`, async ({ browser }) => {
    mkdirSync(OUT, { recursive: true });
    const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
    const g = world().guildId;
    const chRes = await owner.request.get(`/api/v1/guilds/${g}/channels`);
    const chans = (await chRes.json()) as { id: string; type: number }[];
    const channelId = chans.find((c) => c.type === 0)!.id;
    const problems: string[] = [];
    const figs: Record<string, Figures> = {};
    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, owner, v, c, g, channelId, problems);
      } catch (e) {
        problems.push(`${v.name} ${c.tag}: ${String(e).slice(0, 500)}`);
      }
    }
    await owner.close();
    writeFileSync(resolve(OUT, `figures-${PHASE}-${v.name}.json`), JSON.stringify(figs, null, 1));
    const baseFile = resolve(OUT, `figures-base-${v.name}.json`);
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
      const cmp: Record<string, unknown> = {};
      for (const c of CONFIGS)
        if (base[c.tag] && figs[c.tag])
          cmp[c.tag] = compare(c, v.name, base[c.tag]!, figs[c.tag]!, problems);
      writeFileSync(resolve(OUT, `compare-${v.name}.json`), JSON.stringify(cmp, null, 1));
    } else if (PHASE === 'after') say(`${v.name}: no base figures: nothing to compare against`);
    say(`${v.name} problems: ${JSON.stringify(problems)}`);
    expect(problems).toEqual([]);
  });
}
