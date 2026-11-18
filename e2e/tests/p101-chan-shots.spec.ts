import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTest, hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-958 (wave 101, chan): the kit's `NativeSelect` in five Server settings sections (14 selects): Channels (the editor's
 * Slowmode `#ss-slowmode` and Category `#ss-parent`), Members (`#ss-member-role`, `#ss-member-sort`, and the Ban and
 * Timeout dialogs' `#ss-ban-delete`, `#ss-timeout-len`), Channel tabs (`#ss-thread-archive`, `#ss-thread-slow`, a forum's
 * `#ss-forum-sort`, `#ss-forum-layout`), Member bulk (`#ss-bulk-role`, `#ss-bulk-delete`), Audit log (`#ss-audit-user`,
 * `#ss-audit-action`). For each select: its box (relative to the section or dialog it sits in), the boxes of every
 * control around it, the exact hit test, the document's sideways scroll, and one select per file driven by keyboard.
 * Base then after, in one round on one database: `SR_P101_CHAN_PHASE=base` records the tree with the five files put back
 * to the base's text (shots get a `base-` prefix, figures go to `figures-base.json`); the default `after` records the same
 * and, when `figures-base.json` is in the folder, compares (block size not smaller than the base, at least 44 px at 375
 * touch, inline size not smaller, controls below moved by no more than the summed height difference above them, nothing
 * overlapping, hit test equal in count and status). D-951 (the heading in view and focused after a pick at 375, "Back to
 * channels" passing the hit test) is checked in both phases.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never
 * edited). Data of its own (D-942): channels `p101-chan-text`, `p101-chan-fill-<n>` (up to 24 channels, the size where
 * D-951's fault showed, D-961), a category `p101-chan-cat` and a forum `p101-chan-forum`, all removed in a `finally`. A
 * select it cannot reach is reported as "not reached" and left to the handoff. It saves nothing: the keyboard cases change
 * a draft or a filter and put it back.
 *
 * Configs: 1024 x 768 Lounge Light, 1024 x 768 Lounge Dark, 375 x 812 touch Lounge Light. The account theme is set by
 * PATCH and localStorage and read back; `data-theme` is read from the page.
 *
 * Local only (SR_P101_CHAN_ON=1). SR_P101_CHAN_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8784, Vite 5784, metrics 9484,
 * `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5784 SR_P101_CHAN_ON=1 \
 *   SR_P101_CHAN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-chan \
 *   npx playwright test tests/p101-chan-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * After the merge there is no base (the spec then only records and asserts the `after` rules); delete `figures-base.json`
 * first if the folder holds a base from another tree.
 */
const ON = process.env.SR_P101_CHAN_ON === '1';
const PHASE = process.env.SR_P101_CHAN_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P101_CHAN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-chan';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const ROW = '.ss-tree-row button.ss-link';
const BACK = '.ss-editor-back';
const UNSAVED = 'Careful, you have unsaved changes!';

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
test.setTimeout(560_000);

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
  page.setDefaultTimeout(12_000);
  return { ctx, page };
}

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  theme: { html: string | null; first: string | null };
  scroll: number[];
  selects: Record<string, Box | null>;
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: { status: string; control: string; points: unknown[] }[];
  opened: unknown[];
  extra?: Record<string, unknown>;
}

/** Boxes relative to the scope's own top-left corner, so a scrolled page and a centred dialog compare with the base. */
async function measure(page: Page, scopeSel: string, ids: string[]): Promise<Measured> {
  return page.evaluate(
    ({ scopeSel, ids }) => {
      const all = Array.from(document.querySelectorAll(scopeSel));
      const scope = all[all.length - 1]!;
      const o = scope.getBoundingClientRect();
      const box = (e: Element) => {
        const r = e.getBoundingClientRect();
        return {
          x: Math.round((r.left - o.left) * 10) / 10,
          y: Math.round((r.top - o.top) * 10) / 10,
          w: Math.round(r.width * 10) / 10,
          h: Math.round(r.height * 10) / 10,
        };
      };
      const selects: Record<string, ReturnType<typeof box> | null> = {};
      for (const id of ids) {
        const e = document.getElementById(id);
        selects[id] = e ? box(e) : null;
      }
      const els = Array.from(
        scope.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
      ).filter((e) => e.getClientRects().length > 0);
      const controls = els.map((e, i) => ({
        key: `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 30)}`,
        box: box(e),
      }));
      const overlaps: string[] = [];
      for (const id of ids) {
        const s = document.getElementById(id);
        if (!s) continue;
        const a = s.getBoundingClientRect();
        for (const el of els) {
          if (el === s || s.contains(el) || el.contains(s)) continue;
          const b = el.getBoundingClientRect();
          const ix = Math.min(a.right, b.right) - Math.max(a.left, b.left);
          const iy = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top);
          if (ix > 1 && iy > 1)
            overlaps.push(
              `${id} x ${el.tagName.toLowerCase()}#${el.id} (${Math.round(ix)}x${Math.round(iy)})`,
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
        overlaps,
      };
    },
    { scopeSel, ids },
  );
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

interface Target {
  key: string;
  ids: string[];
  scopeSel: string;
  /** Opens the page / dialog; throws when it cannot be reached. */
  open: (page: Page, c: Cfg) => Promise<void>;
  /** The select driven by keyboard (1024 Light only), the arrow, and a reader of what the section shows. */
  kbd?: { id: string; read?: (page: Page) => Promise<unknown> };
  /** Shots: [name, config tag, the select to bring to view]. */
  shots?: { name: string; tags: string[]; id: string; block?: 'start' | 'center' }[];
  /** After the boxes and the hit test: more figures. */
  after?: (page: Page, c: Cfg, fig: Figures, problems: string[]) => Promise<void>;
}

interface Ids {
  text: string;
  forum: string | null;
}

const SECTION = (section: string) => `/settings/server/${world().guildId}/${section}`;

function targets(ids: Ids): Target[] {
  const goto = async (page: Page, url: string, wait: string) => {
    await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 20_000 });
    await page.waitForSelector(wait, { timeout: 20_000 });
    await idle(page, 4000);
    await page.evaluate(() => document.fonts?.ready);
  };
  /** A member row whose "..." menu has an enabled item named like `item`; opens that dialog. */
  const memberDialog = async (page: Page, item: RegExp, selectId: string) => {
    const triggers = page.locator('button[aria-label^="More actions for "]');
    const n = await triggers.count();
    for (let i = 0; i < n; i++) {
      await triggers.nth(i).scrollIntoViewIfNeeded();
      await triggers.nth(i).click();
      const mi = page.getByRole('menuitem', { name: item }).first();
      if (await mi.isVisible().catch(() => false)) {
        const disabled = await mi.getAttribute('aria-disabled');
        if (disabled !== 'true' && (await mi.isEnabled())) {
          await mi.click();
          await page.waitForSelector(selectId, { timeout: 6000 });
          return;
        }
      }
      await page.keyboard.press('Escape');
      await page.waitForTimeout(200);
    }
    throw new Error(`no member row with an enabled ${String(item)} (${n} rows)`);
  };
  const bulk = async (page: Page, button: string, selectId: string) => {
    await goto(page, SECTION('members'), '#ss-member-role');
    const box = page.getByRole('checkbox', { name: /^Select (?!all)/ }).first();
    await box.scrollIntoViewIfNeeded();
    await box.click();
    const bar = page.getByRole('toolbar', { name: 'Bulk actions' });
    await bar.getByRole('button', { name: button, exact: true }).click();
    await page.waitForSelector(selectId, { timeout: 6000 });
  };
  const pickText = async (page: Page, c: Cfg) => {
    await goto(page, SECTION('channels'), ROW);
    const row = page.locator(ROW).filter({ hasText: 'p101-chan-text' }).first();
    await row.scrollIntoViewIfNeeded();
    if (c.touch) await row.tap();
    else await row.click();
    await page.waitForSelector('#ss-slowmode', { timeout: 8000 });
    await page.waitForTimeout(c.touch ? 1700 : 600); // D-951's smooth scroll
  };
  return [
    {
      key: 'channels-editor',
      ids: ['ss-slowmode', 'ss-parent'],
      scopeSel: '.ss-split',
      open: pickText,
      kbd: { id: 'ss-slowmode' },
      shots: [
        { name: 'channels-editor-375-lounge-light', tags: ['375-lounge-light'], id: 'ss-parent' },
        { name: 'channels-editor-1024-lounge-light', tags: ['1024-lounge-light'], id: 'ss-parent' },
        { name: 'channels-editor-1024-lounge-dark', tags: ['1024-lounge-dark'], id: 'ss-parent' },
      ],
      after: async (page, c, fig, problems) => {
        // D-951: the heading in view and focused after a pick (phone), Back passes the hit test; not rendered above 720.
        const snap = await page.evaluate((backSel) => {
          const h = document.querySelector<HTMLElement>('.ss-split h2.ss-title');
          const top = h ? Math.round(h.getBoundingClientRect().top) : null;
          const back = document.querySelector<HTMLElement>(backSel);
          return {
            headingTop: top,
            inView: top !== null && top >= 0 && top < window.innerHeight,
            focused: !!h && document.activeElement === h,
            backShown: !!back && getComputedStyle(back).display !== 'none' && back.offsetHeight > 0,
          };
        }, BACK);
        fig.extra = { ...fig.extra, d951: snap };
        say(`${c.tag}: D-951 ${JSON.stringify(snap)}`);
        if (c.touch) {
          if (!snap.inView) problems.push(`${c.tag}: D-951 heading not in view after a pick`);
          if (!snap.focused) problems.push(`${c.tag}: D-951 heading not focused after a pick`);
          if (!snap.backShown) problems.push(`${c.tag}: D-951 Back to channels not shown`);
          else {
            const back = page.locator(BACK);
            await back.scrollIntoViewIfNeeded();
            const hit = await hitTest(page, back, { sides: true });
            fig.extra = { ...fig.extra, backHit: hit.pass };
            if (!hit.pass) problems.push(`${c.tag}: D-951 Back to channels fails the hit test`);
          }
        } else if (snap.backShown) problems.push(`${c.tag}: Back to channels is shown at 1024`);
      },
    },
    {
      key: 'members',
      ids: ['ss-member-role', 'ss-member-sort'],
      scopeSel: 'main',
      open: (page) => goto(page, SECTION('members'), '#ss-member-role'),
      kbd: {
        id: 'ss-member-sort',
        read: (page) =>
          page.evaluate(() =>
            document.querySelector('main table tbody tr')?.textContent?.slice(0, 40),
          ),
      },
      shots: [
        {
          name: 'members-375-lounge-light',
          tags: ['375-lounge-light'],
          id: 'ss-member-role',
          block: 'start',
        },
        {
          name: 'members-1024-lounge-light',
          tags: ['1024-lounge-light'],
          id: 'ss-member-role',
          block: 'start',
        },
      ],
    },
    {
      key: 'members-ban',
      ids: ['ss-ban-delete'],
      scopeSel: '[role=dialog]:has(#ss-ban-delete)',
      open: async (page) => {
        await goto(page, SECTION('members'), '#ss-member-role');
        await memberDialog(page, /^Ban /, '#ss-ban-delete');
      },
    },
    {
      key: 'members-timeout',
      ids: ['ss-timeout-len'],
      scopeSel: '[role=dialog]:has(#ss-timeout-len)',
      open: async (page) => {
        await goto(page, SECTION('members'), '#ss-member-role');
        await memberDialog(page, /^Timeout /, '#ss-timeout-len');
      },
    },
    {
      key: 'member-bulk-role',
      ids: ['ss-bulk-role'],
      scopeSel: '[role=dialog]:has(#ss-bulk-role)',
      open: (page) => bulk(page, 'Add role', '#ss-bulk-role'),
    },
    {
      key: 'member-bulk-ban',
      ids: ['ss-bulk-delete'],
      scopeSel: '[role=dialog]:has(#ss-bulk-delete)',
      open: (page) => bulk(page, 'Ban', '#ss-bulk-delete'),
      kbd: { id: 'ss-bulk-delete' },
      shots: [
        {
          name: 'member-bulk-375-lounge-light',
          tags: ['375-lounge-light'],
          id: 'ss-bulk-delete',
          block: 'center',
        },
      ],
    },
    {
      key: 'channel-tabs',
      ids: ['ss-thread-archive', 'ss-thread-slow'],
      scopeSel: '.ss-split',
      open: (page) =>
        goto(page, `${SECTION('channels')}?channel=${ids.text}&tab=features`, '#ss-thread-archive'),
      shots: [
        {
          name: 'channel-tabs-375-lounge-light',
          tags: ['375-lounge-light'],
          id: 'ss-thread-archive',
        },
      ],
    },
    {
      key: 'channel-tabs-forum',
      ids: ['ss-forum-sort', 'ss-forum-layout'],
      scopeSel: '.ss-split',
      open: async (page) => {
        if (!ids.forum) throw new Error('no forum channel could be made');
        await goto(
          page,
          `${SECTION('channels')}?channel=${ids.forum}&tab=features`,
          '#ss-forum-sort',
        );
      },
      kbd: { id: 'ss-forum-sort' },
    },
    {
      key: 'audit-log',
      ids: ['ss-audit-user', 'ss-audit-action'],
      scopeSel: 'main',
      open: (page) => goto(page, SECTION('audit-log'), '#ss-audit-user'),
      kbd: {
        id: 'ss-audit-action',
        read: (page) =>
          page.evaluate(() => (document.querySelector('main')?.textContent ?? '').length),
      },
      shots: [
        {
          name: 'audit-log-1024-lounge-light',
          tags: ['1024-lounge-light'],
          id: 'ss-audit-user',
          block: 'start',
        },
      ],
    },
  ];
}

async function runTarget(
  page: Page,
  t: Target,
  c: Cfg,
  problems: string[],
  focusShot: boolean,
): Promise<Figures | null> {
  const tag = `${c.tag}/${t.key}`;
  try {
    await t.open(page, c);
  } catch (e) {
    say(`${tag}: NOT REACHED ${String(e).slice(0, 200)}`);
    problems.push(`${tag}: not reached (${String(e).slice(0, 160)})`);
    return null;
  }
  const m = await measure(page, t.scopeSel, t.ids);
  if (m.theme.html !== c.theme && m.theme.first !== c.theme)
    problems.push(`${tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
  if (m.scroll[0]! > m.scroll[1]!)
    problems.push(`${tag}: the document scrolls sideways ${m.scroll.join('/')}`);
  say(`${tag}: select boxes ${JSON.stringify(m.selects)}`);
  for (const id of t.ids) if (!m.selects[id]) problems.push(`${tag}: ${id} not in the page`);
  if (m.overlaps.length) problems.push(`${tag}: overlaps ${m.overlaps.join('; ')}`);

  const fig: Figures = {
    ...m,
    hit: { pass: 0, fail: 0, notReached: 0, skipped: 0 },
    hitFails: [],
    opened: [],
  };
  const scope: Locator = page.locator(t.scopeSel).last();
  if (t.after) await t.after(page, c, fig, problems);
  fig.opened = await openCollapsed(page, scope);
  const rows = await hitTestAll(page, scope);
  fig.hit = count(rows);
  fig.hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: hit ${JSON.stringify(fig.hit)}`);
  if (fig.hit.notReached)
    problems.push(`${tag}: ${fig.hit.notReached} controls not reached (probe error)`);

  mkdirSync(OUT, { recursive: true });
  for (const s of t.shots ?? []) {
    if (!s.tags.includes(c.tag)) continue;
    await page.evaluate(
      ({ id, block }) =>
        document
          .getElementById(id)
          ?.scrollIntoView({ block: (block ?? 'center') as ScrollLogicalPosition }),
      { id: s.id, block: s.block },
    );
    await page.waitForTimeout(500);
    await page.screenshot({ path: resolve(OUT, `${shotName(s.name)}.png`), timeout: 10_000 });
  }

  // The ring by keyboard, and the select driven by keyboard (one per file, 1024 Light).
  if (t.kbd) {
    const sel = page.locator(`#${t.kbd.id}`);
    await sel.scrollIntoViewIfNeeded();
    await page.keyboard.press('Shift');
    await sel.focus();
    const focusVisible = await sel.evaluate((el) => el.matches(':focus-visible'));
    const ring = await sel.evaluate((el) => {
      const cs = getComputedStyle(el);
      return {
        outline: `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor}`,
        offset: cs.outlineOffset,
      };
    });
    fig.extra = { ...fig.extra, focusVisible, ring };
    if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
    if (focusShot) {
      await page.waitForTimeout(300);
      await page.screenshot({
        path: resolve(OUT, `${shotName('channels-editor-focus-lounge-dark')}.png`),
        timeout: 10_000,
      });
    }
    if (c.tag === '1024-lounge-light') {
      const readBefore = t.kbd.read ? await t.kbd.read(page) : null;
      const before = await sel.inputValue();
      await page.keyboard.press('ArrowDown');
      await idle(page, 2500);
      const changed = await sel.inputValue();
      const readChanged = t.kbd.read ? await t.kbd.read(page) : null;
      const bar = await page
        .getByText(UNSAVED)
        .isVisible()
        .catch(() => false);
      await page.keyboard.press('ArrowUp');
      await idle(page, 2500);
      const back = await sel.inputValue();
      fig.extra = {
        ...fig.extra,
        kbd: { id: t.kbd.id, before, changed, back, readBefore, readChanged, bar },
      };
      say(`${tag}: keyboard ${JSON.stringify(fig.extra['kbd'])}`);
      if (changed === before) problems.push(`${tag}: ArrowDown did not change ${t.kbd.id}`);
      if (back !== before) problems.push(`${tag}: ArrowUp did not restore ${t.kbd.id}`);
      if (t.key === 'channels-editor' && !bar)
        problems.push(`${tag}: the unsaved-changes bar did not show after ArrowDown`);
    }
  }
  return fig;
}

function compare(
  key: string,
  t: Target,
  c: Cfg,
  base: Figures,
  after: Figures,
  problems: string[],
) {
  const tag = `${c.tag}/${key}`;
  const diffs: Record<string, unknown> = {};
  for (const id of t.ids) {
    const b = base.selects[id];
    const a = after.selects[id];
    if (!b || !a) {
      if (!!b !== !!a) problems.push(`${tag}: ${id} present in only one phase`);
      continue;
    }
    diffs[id] = { base: b, after: a };
    if (a.h < b.h - 0.5) problems.push(`${tag}: ${id} shorter than base (${a.h} < ${b.h})`);
    if (c.touch && a.h < 44) problems.push(`${tag}: ${id} is ${a.h} px tall at 375 touch`);
    if (a.w < b.w - 0.5) problems.push(`${tag}: ${id} narrower than base (${a.w} < ${b.w})`);
  }
  if (base.controls.length !== after.controls.length) {
    problems.push(`${tag}: control count ${base.controls.length} -> ${after.controls.length}`);
    return diffs;
  }
  const moved: { key: string; moved: number; allowed: number }[] = [];
  base.controls.forEach((bc, i) => {
    const ac = after.controls[i]!;
    let allowed = 0;
    for (const id of t.ids) {
      const b = base.selects[id];
      const a = after.selects[id];
      if (b && a && b.y + b.h <= bc.box.y + 0.5) allowed += a.h - b.h;
    }
    const mv = Math.round((ac.box.y - bc.box.y) * 10) / 10;
    moved.push({ key: bc.key, moved: mv, allowed: Math.round(allowed * 10) / 10 });
    if (mv > allowed + 1) problems.push(`${tag}: ${bc.key} moved ${mv} px, allowed ${allowed}`);
    if (mv < -1) problems.push(`${tag}: ${bc.key} moved up ${mv} px`);
    if (Math.abs(ac.box.x - bc.box.x) > 1) problems.push(`${tag}: ${bc.key} moved sideways`);
  });
  diffs['moved'] = moved.filter((x) => x.moved !== 0 || x.allowed !== 0);
  if (JSON.stringify(base.hit) !== JSON.stringify(after.hit))
    problems.push(`${tag}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}`);
  return diffs;
}

test(`p101 chan: Server settings selects, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = owner.request;
  const gid = world().guildId;
  const problems: string[] = [];
  const made: string[] = [];
  const figs: Record<string, Figures> = {};
  try {
    // Data of our own: a text channel, fillers up to 24 channels (D-961), a category, a forum.
    const list = (await (await o.get(`/api/v1/guilds/${gid}/channels`)).json()) as { id: string }[];
    const mk = async (name: string, type: number) => {
      const ch = await apiPost<{ id: string }>(o, `/guilds/${gid}/channels`, { name, type });
      made.push(ch.id);
      return ch.id;
    };
    const text = await mk('p101-chan-text', 0);
    await mk('p101-chan-cat', 4);
    let forum: string | null = null;
    try {
      forum = await mk('p101-chan-forum', 15);
    } catch (e) {
      say(`forum channel not made: ${String(e).slice(0, 200)}`);
    }
    for (let n = list.length + made.length; n < 24; n++) await mk(`p101-chan-fill-${n}`, 0);
    say(`channels: ${list.length} before, ${made.length} made`);

    const ts = targets({ text, forum });
    for (const c of CONFIGS) {
      await setMain(owner, c.theme);
      const { ctx, page } = await newPage(browser, owner, c);
      for (const t of ts) {
        try {
          const f = await runTarget(
            page,
            t,
            c,
            problems,
            c.tag === '1024-lounge-dark' && t.key === 'channels-editor',
          );
          if (f) figs[`${c.tag}/${t.key}`] = f;
        } catch (e) {
          problems.push(`${c.tag}/${t.key}: ${String(e).slice(0, 400)}`);
        }
      }
      await ctx.close();
    }
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
      const cmp: Record<string, unknown> = {};
      for (const c of CONFIGS)
        for (const t of ts) {
          const k = `${c.tag}/${t.key}`;
          if (base[k] && figs[k]) cmp[k] = compare(t.key, t, c, base[k]!, figs[k]!, problems);
          else if (!!base[k] !== !!figs[k]) problems.push(`${k}: reached in only one phase`);
        }
      writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
    } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  } finally {
    for (const id of made) await apiDelete(o, `channels/${id}`).catch(() => undefined);
    await owner.close();
  }
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
