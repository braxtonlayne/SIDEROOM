import { mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { over, parseColor, ratio, type Rgba } from '../support/contrast';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 107, pod staff (D-1022 the 44 px target on a phone, D-1024 the field contract; MEASURES the cures of nav, kitbits
 * and server). Per page, both phases: at 375 x 812 touch every control is hit tested at the default reach and written
 * whole; at 1024 x 768 the same pages are measured again (reach 'box') and every box must equal the base. Fields under
 * cure B (the settings search, the Members `.ss-select` inputs) are read at rest, hovered and keyboard focused at 1024
 * in Lounge Light and Lounge Dark, with ratios from `contrast.ts`.
 *
 * Parts (`SR_P107_STAFF_PART`): 1 User settings (sections drawer, search), the Friends notices, Library Browse and My
 * library, the Study, Cinema Browse (chips, dropdown, the filters drawer's close); 2 Server settings (sections drawer,
 * Roles, Channels > Permissions, Watch Parties & Library, Members); 3 Admin (sections drawer, Bans, Live TV > New
 * channel, Library catalog > New series). It runs on the seeded sweep world and seeds nothing. Part 1 switches the
 * Study's storefront on and back off in a `finally`.
 *
 * Output: `figures-<phase>-part<N>.json` and the shots (base shots `base-` prefixed) in `OUT`. In the after phase it
 * compares with `figures-base-part<N>.json` when it is in `OUT` and lists every broken limit as a PROBLEM line.
 *
 * Local only (SR_P107_STAFF_ON=1). The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_STAFF_ON=1 --env SR_P107_STAFF_PHASE=after --env SR_P107_STAFF_PART=1 --env SR_P107_STAFF_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-staffshots -- tests/p107-staff-shots.spec.ts
 */
const ON = process.env.SR_P107_STAFF_ON === '1';
const PHASE = process.env.SR_P107_STAFF_PHASE === 'after' ? 'after' : 'base';
const PART = Number(process.env.SR_P107_STAFF_PART ?? '1');
const OUT =
  process.env.SR_P107_STAFF_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-staffshots';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const BUDGET_MS = 430_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const pre = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

type Theme = 'lounge-light' | 'lounge-dark';
type Width = 375 | 1024;
interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}
interface RowFig {
  w: Width;
  theme: string;
  page: string;
  state: string;
  key: string;
  name: string;
  tag: string;
  role: string | null;
  path: string;
  status: string;
  box: Box | null;
  taker: { tag: string; role: string | null; name: string; path: string; at: string } | null;
}
interface FieldFig {
  page: string;
  field: string;
  theme: string;
  box: Box | null;
  boxHeight375: number | null;
  restBorder: string;
  restBorderWidth: string;
  restBorderStyle: string;
  hoverBorder: string;
  focusOutlineStyle: string;
  focusOutlineWidth: string;
  focusOutlineColor: string;
  focusOutlineOffset: string;
  focusOn: string;
  placeholder: string | null;
  placeholderOpacity: string | null;
  faint: string;
  ring: string;
  outsideGround: string;
  fieldGround: string;
  ratioBorderOutside: number;
  ratioBorderField: number;
  ratioRing: number;
  ratioPlaceholder: number | null;
}
const rowsOut: RowFig[] = [];
const fieldsOut: FieldFig[] = [];
const notes: string[] = [];
const notCovered: string[] = [];
const sideways: Record<string, number[]> = {};

// ---- helpers --------------------------------------------------------------------------------------------------------

async function newCtx(
  browser: Browser,
  owner: BrowserContext,
  w: Width,
  theme: Theme,
  sec: string,
) {
  const phone = w === 375;
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width: 375, height: 812 } : { width: 1024, height: 768 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme, zoom: 100, sectionThemes: { library: sec, cinema: sec } },
  );
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function setPrefs(owner: BrowserContext, theme: Theme, sec: string) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100, sectionThemes: { library: sec, cinema: sec } } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function settle(page: Page, ms = 5000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page
    .waitForFunction(() => document.querySelectorAll('[aria-busy="true"]').length === 0, null, {
      timeout: 6000,
    })
    .catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}

const themeOf = (page: Page) =>
  page.evaluate(() => {
    const m = document.querySelector('main') ?? document.body;
    return (
      m.closest('[data-theme]')?.getAttribute('data-theme') ??
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null
    );
  });

async function shot(page: Page, name: string, expectTheme?: string) {
  mkdirSync(OUT, { recursive: true });
  const s = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  sideways[name] = s;
  if (s[0]! > s[1]!) notes.push(`${name}: the page scrolls sideways (${s[0]} > ${s[1]})`);
  if (expectTheme) {
    const t = await themeOf(page);
    if (t !== expectTheme) notes.push(`${name}: data-theme is ${t}, wanted ${expectTheme}`);
  }
  await page.screenshot({ path: resolve(OUT, `${pre(name)}.png`), timeout: 15_000 });
}

const CONTROLISH = new Set(['button', 'a', 'input', 'select', 'textarea', 'summary', 'label']);
const CONTROL_ROLES = new Set(['button', 'link', 'combobox', 'checkbox', 'switch', 'radio']);

function toRow(
  w: Width,
  theme: string,
  pageName: string,
  state: string,
  r: ControlRow,
  nth: Map<string, number>,
): RowFig {
  const bad = (r.result?.points ?? []).find((p) => !p.ok);
  const base = `${w}|${pageName}|${state}|${r.tag}|${r.role ?? ''}|${r.name}`;
  const n = (nth.get(base) ?? 0) + 1;
  nth.set(base, n);
  return {
    w,
    theme,
    page: pageName,
    state,
    key: `${base}|${n}`,
    name: r.name,
    tag: r.tag,
    role: r.role,
    path: r.path,
    status: r.status,
    box: r.result?.box ?? null,
    taker: bad?.taker
      ? {
          tag: bad.taker.tag,
          role: bad.taker.role,
          name: bad.taker.name,
          path: bad.taker.path,
          at: bad.at,
        }
      : null,
  };
}

const nths = new Map<string, number>();
async function record(
  page: Page,
  root: Locator,
  w: Width,
  theme: string,
  pageName: string,
  state: string,
  also?: string,
) {
  await openCollapsed(page, root);
  const rows = await hitTestAll(page, root, {
    ...(w === 1024 ? { reach: 'box' as const } : {}),
    ...(also ? { also } : {}),
  });
  for (const r of rows) rowsOut.push(toRow(w, theme, pageName, state, r, nths));
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  say(
    `${w} ${pageName} [${state}]: ${rows.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}`,
  );
  for (const r of rows)
    if (r.status === 'not-reached')
      notes.push(`${w} ${pageName} [${state}] not-reached ${r.name}: ${r.reason}`);
}

// ---- the pages ------------------------------------------------------------------------------------------------------

interface Extra {
  key: string;
  /** Sets up one more state of the page and returns its root (or null). */
  run: (page: Page, root: Locator) => Promise<Locator | null>;
  phoneOnly?: boolean;
}
interface PageDef {
  name: string;
  part: number;
  /** 'main' follows the account's main theme; 'section' the library/cinema section theme. */
  kind: 'main' | 'section';
  open: (page: Page, w: World) => Promise<Locator>;
  /** The settings shell: the sections list and the close are tested too. */
  chrome?: boolean;
  extras?: Extra[];
  /** Fields under cure B, read at 1024 in both themes. */
  fields?: { key: string; find: (page: Page) => Locator }[];
  /** Also measured in Lounge Dark at 1024 (settings pages). */
  dark?: boolean;
}

const mainOf = (page: Page) => page.getByRole('main').first();
const go =
  (path: (w: World) => string, ready?: (p: Page) => Promise<unknown>) =>
  async (page: Page, w: World) => {
    await page.goto(path(w), { waitUntil: 'domcontentloaded', timeout: 45_000 });
    await mainOf(page)
      .waitFor({ timeout: 30_000 })
      .catch(() => notes.push(`${path(w)}: no <main>`));
    if (ready) await ready(page);
    await settle(page);
    return mainOf(page);
  };
const dialogRoot = async (page: Page, ms = 4000) => {
  const d = page
    .locator('[role="dialog"], [role="menu"], [role="listbox"]')
    .filter({ hasNot: page.locator('main') })
    .last();
  await d.waitFor({ state: 'visible', timeout: ms }).catch(() => undefined);
  return (await d.isVisible().catch(() => false)) ? d : null;
};
const tapButton = (name: RegExp, phoneOnly = false): Extra => ({
  key: `tap ${name.source}`,
  phoneOnly,
  run: async (page, root) => {
    const b = root.getByRole('button', { name }).or(page.getByRole('button', { name })).first();
    if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
    await b.scrollIntoViewIfNeeded();
    await b.click();
    await page.waitForTimeout(800);
    return (await dialogRoot(page, 2500)) ?? root;
  },
});

const sv = (s: string) => (w: World) => `/settings/server/${w.guildId}/${s}`;
const ADMIN = (s: string) => () => `/admin/${s}`;

async function openFriends(page: Page) {
  await page.goto('/channels/@me', { waitUntil: 'domcontentloaded', timeout: 45_000 });
  await page.locator('.lng-left').first().waitFor({ timeout: 30_000 });
  await settle(page, 3000);
  const f = page
    .getByRole('button', { name: /^Friends$/ })
    .or(page.getByRole('link', { name: /^Friends$/ }))
    .first();
  await f.click();
  await settle(page, 2500);
  const sec = page.locator('section.lng-friends').first();
  await sec.waitFor({ timeout: 10_000 });
  return sec;
}

const json = (r: import('@playwright/test').Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
const FILES = [
  { key: 'Films/heat.mp4', size: 1000 },
  { key: 'Music/blue.mp3', size: 2000 },
  { key: 'Covers/one.png', size: 3000 },
];
async function fakeStudy(page: Page) {
  await page.route('**/api/v1/shelves/visible', (r) =>
    json(r, { shelves: [{ shelf_id: '9501', label: 'p107 Home', via: 'own', object_count: 3 }] }),
  );
  await page.route('**/api/v1/storage/mounts/*/objects*', (r) =>
    json(r, {
      folders: [],
      objects: FILES.map((f) => ({ ...f, last_modified: '2026-10-01T00:00:00Z' })),
      next_cursor: null,
      can_upload: false,
      can_manage: true,
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) =>
    json(r, {
      objects: FILES.map((f) => ({
        object_hash: 'a'.repeat(64),
        path: f.key,
        size: f.size,
        class: 'other',
        title: '',
        cover: '',
        note: '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: true,
    }),
  );
  await page.route('https://bucket.invalid/**', (r) => r.fulfill({ status: 404 }));
}

const filtersDrawer = tapButton(/^Filters/, true);

const membersBulk: Extra = {
  key: 'bulk bar',
  run: async (page, root) => {
    const boxes = root.locator('table tbody tr input[type=checkbox]');
    const c = await boxes.count();
    if (c === 0) return null;
    for (let i = 0; i < Math.min(2, c); i++)
      await boxes.nth(i).evaluate((el) => (el as HTMLElement).click());
    await page.waitForTimeout(700);
    return root;
  },
};

const DEFS: PageDef[] = [
  {
    name: 'user-account',
    part: 1,
    kind: 'main',
    dark: true,
    chrome: true,
    open: go(() => '/settings/user/account'),
    fields: [
      { key: 'settings-search', find: (p) => p.getByRole('search').locator('input').first() },
    ],
  },
  { name: 'friends', part: 1, kind: 'main', open: (p) => openFriends(p) },
  {
    name: 'lib-browse',
    part: 1,
    kind: 'section',
    open: go(
      () => '/library/browse',
      (p) => p.waitForTimeout(1500),
    ),
    extras: [filtersDrawer],
  },
  {
    name: 'lib-me',
    part: 1,
    kind: 'section',
    open: go(
      () => '/library/me',
      (p) => p.waitForTimeout(1500),
    ),
  },
  {
    name: 'study',
    part: 1,
    kind: 'main',
    open: go(
      () => '/study',
      (p) =>
        p
          .getByRole('button', { name: /^Edit details of / })
          .first()
          .waitFor({ timeout: 40_000 }),
    ),
  },
  {
    name: 'cin-browse',
    part: 1,
    kind: 'section',
    open: go(
      () => '/cinema/movies',
      (p) => p.waitForTimeout(1500),
    ),
    extras: [filtersDrawer],
  },
  {
    name: 'server-overview',
    part: 2,
    kind: 'main',
    dark: true,
    chrome: true,
    open: go(sv('overview')),
  },
  {
    name: 'server-roles',
    part: 2,
    kind: 'main',
    open: go(sv('roles')),
  },
  {
    name: 'server-channel-permissions',
    part: 2,
    kind: 'main',
    open: go(
      (w) => `/settings/server/${w.guildId}/channels?channel=${w.channelId}&tab=permissions`,
    ),
  },
  { name: 'server-watch-library', part: 2, kind: 'main', open: go(sv('watch-library')) },
  {
    name: 'server-members',
    part: 2,
    kind: 'main',
    dark: true,
    open: go(sv('members')),
    extras: [membersBulk],
    fields: [
      { key: 'joined-after', find: (p) => p.locator('#ss-member-after') },
      { key: 'joined-before', find: (p) => p.locator('#ss-member-before') },
    ],
  },
  { name: 'admin-dashboard', part: 3, kind: 'main', chrome: true, open: go(ADMIN('dashboard')) },
  { name: 'admin-bans', part: 3, kind: 'main', open: go(ADMIN('bans')) },
  {
    name: 'admin-cinema-live-tv',
    part: 3,
    kind: 'main',
    open: go(ADMIN('cinema-live-tv')),
    extras: [
      {
        key: 'new channel form',
        run: async (page, root) => {
          const b = root.getByRole('button', { name: /^New channel/ }).first();
          if (!(await b.isVisible({ timeout: 6000 }).catch(() => false))) return null;
          await b.scrollIntoViewIfNeeded();
          await b.click();
          await page.waitForTimeout(900);
          return root;
        },
      },
    ],
  },
  {
    name: 'admin-library-catalog',
    part: 3,
    kind: 'main',
    open: go(ADMIN('library-catalog')),
    extras: [tapButton(/^New series/)],
  },
];

// ---- one page, one width --------------------------------------------------------------------------------------------

async function measure(
  browser: Browser,
  owner: BrowserContext,
  def: PageDef,
  w: World,
  width: Width,
  theme: Theme,
  sec: string,
) {
  const tag = `${def.name}-${width}${width === 1024 ? (theme === 'lounge-dark' ? '-dark' : '-light') : ''}`;
  const { ctx, page } = await newCtx(browser, owner, width, theme, sec);
  try {
    if (def.name === 'study') await fakeStudy(page);
    const root = await def.open(page, w);
    const root0 = (await root.count()) ? root : page.locator('body');
    const expectTheme =
      def.kind === 'section' ? sec : theme === 'lounge-dark' ? 'lounge-dark' : 'lounge-light';
    await shot(page, tag, expectTheme);
    if (def.chrome) {
      const dlg = page.locator('[role=dialog][aria-label]').first();
      const sidebar = dlg.locator('> div').first();
      const topline = dlg.locator('> div').nth(1).locator('> div').first();
      await record(page, sidebar, width, theme, def.name, 'shell: sections list');
      if (width === 375)
        await shot(page, `${def.name}-375-sections`, expectTheme).catch(() => undefined);
      await record(page, topline, width, theme, def.name, 'shell: close');
    }
    await record(page, root0, width, theme, def.name, 'page');
    for (const ex of def.extras ?? []) {
      if (ex.phoneOnly && width !== 375) continue;
      try {
        const r = await ex.run(page, root0);
        if (!r) {
          notCovered.push(`${width} ${def.name}: ${ex.key} not found`);
          continue;
        }
        await page.waitForTimeout(400);
        await shot(page, `${tag}-${ex.key.replace(/[^a-z0-9]+/gi, '-').slice(0, 24)}`);
        await record(page, r, width, theme, def.name, ex.key);
        if (ex.key.startsWith('tap')) {
          await page.keyboard.press('Escape');
          await page.waitForTimeout(300);
        }
      } catch (e) {
        notCovered.push(`${width} ${def.name}: ${ex.key}: ${String(e).slice(0, 200)}`);
      }
    }
    if (def.fields)
      for (const f of def.fields) {
        const loc = f.find(page);
        if (!(await loc.count())) {
          notCovered.push(`${width} ${def.name}: field ${f.key} not found`);
          continue;
        }
        if (width === 375) {
          await openCollapsed(page, page.locator('body')).catch(() => undefined);
          await loc.scrollIntoViewIfNeeded().catch(() => undefined);
          const b = await loc.boundingBox();
          fieldsOut.push(emptyField(def.name, f.key, 'phone', b));
        } else {
          await readField(page, def.name, f.key, theme, loc, tag);
        }
      }
  } catch (e) {
    notCovered.push(`${width} ${def.name}: ${String(e).slice(0, 300)}`);
    say(`${width} ${def.name}: ERROR ${String(e).slice(0, 300)}`);
  } finally {
    await ctx.close();
  }
}

// ---- field reads ----------------------------------------------------------------------------------------------------

function emptyField(page: string, field: string, theme: string, box: Box | null): FieldFig {
  return {
    page,
    field,
    theme,
    box,
    boxHeight375: box ? box.height : null,
    restBorder: '',
    restBorderWidth: '',
    restBorderStyle: '',
    hoverBorder: '',
    focusOutlineStyle: '',
    focusOutlineWidth: '',
    focusOutlineColor: '',
    focusOutlineOffset: '',
    focusOn: '',
    placeholder: null,
    placeholderOpacity: null,
    faint: '',
    ring: '',
    outsideGround: '',
    fieldGround: '',
    ratioBorderOutside: 0,
    ratioBorderField: 0,
    ratioRing: 0,
    ratioPlaceholder: null,
  };
}

/** The colour `css` resolves to on this element's page: a throwaway span next to the field. */
const resolveVar = (loc: Locator, v: string) =>
  loc.evaluate((el, name) => {
    const s = document.createElement('span');
    s.style.color = `var(${name})`;
    el.parentElement?.appendChild(s);
    const c = getComputedStyle(s).color;
    s.remove();
    return c;
  }, v);

async function groundOf(loc: Locator, skipSelf: boolean): Promise<Rgba> {
  const chain = await loc.evaluate((el, skip) => {
    const out: string[] = [];
    for (let x: Element | null = skip ? el.parentElement : el; x; x = x.parentElement)
      out.push(getComputedStyle(x).backgroundColor);
    return out;
  }, skipSelf);
  let ground: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const c of chain.reverse()) {
    const p = parseColor(c);
    if (p.a > 0) ground = over(p, ground);
  }
  return ground;
}
const css = (c: Rgba) => `rgba(${Math.round(c.r)}, ${Math.round(c.g)}, ${Math.round(c.b)}, ${c.a})`;

async function readField(
  page: Page,
  pageName: string,
  key: string,
  theme: string,
  loc: Locator,
  tag: string,
) {
  await loc.scrollIntoViewIfNeeded();
  const rest = await loc.evaluate((el) => {
    const s = getComputedStyle(el);
    const ph = getComputedStyle(el, '::placeholder');
    const isText = el instanceof HTMLInputElement && ['text', 'search', ''].includes(el.type);
    return {
      color: s.borderTopColor,
      width: s.borderTopWidth,
      style: s.borderTopStyle,
      ph: isText ? ph.color : null,
      phOpacity: isText ? ph.opacity : null,
    };
  });
  const box = await loc.boundingBox();
  const outside = await groundOf(loc, true);
  const fieldGround = over(await groundOf(loc, false), outside);
  const faint = await resolveVar(loc, '--ui-text-faint');
  const ringCss = await resolveVar(loc, '--ui-focus');
  await loc.hover();
  await page.waitForTimeout(250);
  const hover = await loc.evaluate((el) => getComputedStyle(el).borderTopColor);
  await page.mouse.move(2, 2);
  // Keyboard focus: a Tab first (keyboard modality), then focus the field.
  await page.keyboard.press('Tab');
  await loc.focus();
  await page.waitForTimeout(250);
  const read = (el: Element) => {
    const s = getComputedStyle(el);
    return { st: s.outlineStyle, w: s.outlineWidth, c: s.outlineColor, o: s.outlineOffset };
  };
  const onField = await loc.evaluate(read);
  let focusOn = 'field';
  let ring = onField;
  if (onField.st === 'none' || onField.w === '0px') {
    const wrap = await loc.evaluate((el) => {
      for (let x: Element | null = el.parentElement, k = 0; x && k < 2; x = x.parentElement, k++) {
        const s = getComputedStyle(x);
        if (s.outlineStyle !== 'none' && s.outlineWidth !== '0px')
          return { st: s.outlineStyle, w: s.outlineWidth, c: s.outlineColor, o: s.outlineOffset };
      }
      return null;
    });
    if (wrap) {
      ring = wrap;
      focusOn = 'wrapper';
    } else focusOn = 'none';
  }
  await shot(page, `${tag}-focus-${key}`).catch(() => undefined);
  const borderC = parseColor(rest.color);
  const borderOut = over(borderC, fieldGround);
  const ringC = over(parseColor(ring.c), outside);
  const ph = rest.ph ? over(parseColor(rest.ph), fieldGround) : null;
  const f = emptyField(pageName, key, theme, box);
  Object.assign(f, {
    restBorder: rest.color,
    restBorderWidth: rest.width,
    restBorderStyle: rest.style,
    hoverBorder: hover,
    focusOutlineStyle: ring.st,
    focusOutlineWidth: ring.w,
    focusOutlineColor: ring.c,
    focusOutlineOffset: ring.o,
    focusOn,
    placeholder: rest.ph,
    placeholderOpacity: rest.phOpacity,
    faint,
    ring: ringCss,
    outsideGround: css(outside),
    fieldGround: css(fieldGround),
    ratioBorderOutside: ratio(borderOut, outside),
    ratioBorderField: ratio(borderOut, fieldGround),
    ratioRing: ratio(ringC, outside),
    ratioPlaceholder: ph ? ratio(ph, fieldGround) : null,
  });
  fieldsOut.push(f);
  await loc.evaluate((el) => (el as HTMLElement).blur());
}

// ---- the comparison (after phase) -----------------------------------------------------------------------------------

const isControlTaker = (t: NonNullable<RowFig['taker']>) =>
  CONTROLISH.has(t.tag) || (t.role !== null && CONTROL_ROLES.has(t.role));

/** The controls the builders' tasks name; a failure of one of these after is a problem. */
const NAMED: Record<string, (r: RowFig) => boolean> = {
  'user-account': (r) => r.state === 'shell: sections list' || /search/i.test(r.name),
  'server-overview': (r) => r.state === 'shell: sections list',
  'admin-dashboard': (r) => r.state === 'shell: sections list',
  friends: (r) => /dismiss/i.test(r.name),
  'lib-browse': (r) =>
    /_chip_|dropdown|_select_/i.test(r.path) ||
    /_close_/.test(r.path) ||
    /^room selection/i.test(r.name),
  'lib-me': (r) =>
    /_chip_|dropdown|_select_/i.test(r.path) ||
    /_close_/.test(r.path) ||
    /^room selection/i.test(r.name),
  study: (r) =>
    /_chip_|dropdown|_select_/i.test(r.path) ||
    /_close_/.test(r.path) ||
    /^room selection/i.test(r.name),
  'cin-browse': (r) =>
    /_chip_|dropdown|_select_/i.test(r.path) ||
    /_close_/.test(r.path) ||
    /^room selection/i.test(r.name),
  'server-roles': (r) => /default permissions/i.test(r.name),
  'server-channel-permissions': (r) => /chip/i.test(r.path),
  'server-watch-library': (r) => r.tag === 'a',
  'server-members': (r) => /^(add|remove) role/i.test(r.name),
  'admin-bans': (r) => /days/i.test(r.name),
  'admin-cinema-live-tv': (r) => /age-restricted|remove daypart/i.test(r.name),
  'admin-library-catalog': (r) => r.state.startsWith('tap') && /^close$/i.test(r.name),
};

/** Where a cure may move a drawn box on a phone (D-990): the whole state is listed as info, not as a problem. */
const MAY_MOVE_375 = (r: { page: string; state: string }) =>
  r.state === 'shell: sections list' ||
  (r.page === 'user-account' && r.state === 'page') ||
  (r.page === 'server-members' && r.state === 'bulk bar') ||
  (r.page === 'admin-bans' && r.state === 'page') ||
  (r.page === 'admin-cinema-live-tv' && r.state === 'new channel form') ||
  r.page === 'server-channel-permissions' ||
  r.page === 'server-watch-library';

const same = (a: Box, b: Box) =>
  Math.abs(a.x - b.x) < 0.6 &&
  Math.abs(a.y - b.y) < 0.6 &&
  Math.abs(a.width - b.width) < 0.6 &&
  Math.abs(a.height - b.height) < 0.6;
const bx = (b: Box | null) =>
  b
    ? `${Math.round(b.width * 10) / 10}x${Math.round(b.height * 10) / 10}@${Math.round(b.x)},${Math.round(b.y)}`
    : '-';

interface Saved {
  rows: RowFig[];
  fields: FieldFig[];
}

function compare(base: Saved, after: Saved): { problems: string[]; info: string[] } {
  const problems: string[] = [];
  const info: string[] = [];
  const byKey = new Map(base.rows.map((r) => [r.key, r]));
  const afterKeys = new Set(after.rows.map((r) => r.key));
  let fails = 0;
  let baseFails = 0;
  for (const r of base.rows) if (r.w === 375 && r.status === 'fail') baseFails++;
  const moved375: Record<string, { n: number; maxDy: number; maxDh: number }> = {};
  for (const r of after.rows) {
    const b = byKey.get(r.key);
    if (r.w === 375 && r.status === 'fail') {
      fails++;
      const named = NAMED[r.page]?.(r) ?? false;
      const was = b?.status;
      const t = r.taker
        ? `${r.taker.tag}${r.taker.role ? `[${r.taker.role}]` : ''} "${r.taker.name}" ${r.taker.path}`
        : '?';
      if (was === 'pass' || (!b && !named))
        problems.push(
          `NEW FAIL at 375: ${r.page} [${r.state}] ${r.tag} "${r.name}" ${bx(r.box)}, taker ${r.taker?.at ?? ''}: ${t}${r.taker && isControlTaker(r.taker) ? ' (a control: a tap area takes its point?)' : ''}`,
        );
      else if (named)
        problems.push(
          `STILL FAILS at 375 (named): ${r.page} [${r.state}] ${r.tag} "${r.name}" ${bx(r.box)}, taker ${r.taker?.at ?? ''}: ${t}${was === 'fail' ? ' (failed in base too)' : ''}`,
        );
      else info.push(`still fails (not named): ${r.page} [${r.state}] "${r.name}" taker ${t}`);
    }
    if (!b) {
      problems.push(
        `NEW ROW (not in base): ${r.w} ${r.page} [${r.state}] ${r.tag} "${r.name}" ${bx(r.box)}`,
      );
      continue;
    }
    if (r.box && b.box && !same(r.box, b.box)) {
      if (r.w === 1024)
        problems.push(
          `MOVED at 1024: ${r.page} [${r.state}] ${r.tag} "${r.name}" base ${bx(b.box)} after ${bx(r.box)}`,
        );
      else if (MAY_MOVE_375(r)) {
        const m = (moved375[`${r.page} [${r.state}]`] ??= { n: 0, maxDy: 0, maxDh: 0 });
        m.n++;
        m.maxDy = Math.max(m.maxDy, Math.abs(r.box.y - b.box.y));
        m.maxDh = Math.max(m.maxDh, Math.abs(r.box.height - b.box.height));
      } else
        problems.push(
          `MOVED at 375 (not allowed): ${r.page} [${r.state}] ${r.tag} "${r.name}" base ${bx(b.box)} after ${bx(r.box)}`,
        );
    }
    if (r.status !== b.status && r.w === 1024)
      problems.push(
        `STATUS at 1024: ${r.page} [${r.state}] "${r.name}" ${b.status} -> ${r.status}`,
      );
    if (r.w === 375 && b.status === 'fail' && r.status === 'pass')
      info.push(`FIXED: ${r.page} [${r.state}] "${r.name}"`);
  }
  for (const b of base.rows)
    if (!afterKeys.has(b.key)) problems.push(`ROW GONE: ${b.w} ${b.page} [${b.state}] "${b.name}"`);
  for (const [k, m] of Object.entries(moved375))
    info.push(
      `allowed move at 375: ${k}: ${m.n} boxes, largest shift ${m.maxDy} px, largest height change ${m.maxDh} px`,
    );
  info.push(`375 fails: base ${baseFails}, after ${fails}`);
  // Fields: cure B.
  for (const f of after.fields) {
    if (f.theme === 'phone') {
      if ((f.boxHeight375 ?? 0) < 44)
        problems.push(`FIELD ${f.page}/${f.field}: ${f.boxHeight375} px tall at 375, wanted 44`);
      continue;
    }
    const who = `FIELD ${f.page}/${f.field} (${f.theme})`;
    if (f.ratioBorderOutside < 3)
      problems.push(
        `${who}: rest border vs outside ground ${f.ratioBorderOutside.toFixed(2)} < 3.0`,
      );
    if (f.ratioBorderField < 3)
      problems.push(`${who}: rest border vs field ground ${f.ratioBorderField.toFixed(2)} < 3.0`);
    if (f.hoverBorder !== f.faint)
      problems.push(`${who}: hover border ${f.hoverBorder}, wanted --ui-text-faint ${f.faint}`);
    if (f.hoverBorder === f.restBorder) problems.push(`${who}: hover border equals rest`);
    if (
      f.focusOutlineStyle !== 'solid' ||
      f.focusOutlineWidth !== '2px' ||
      f.focusOutlineColor !== f.ring ||
      f.focusOutlineOffset !== '1px'
    )
      problems.push(
        `${who}: ring ${f.focusOutlineStyle} ${f.focusOutlineWidth} ${f.focusOutlineColor} offset ${f.focusOutlineOffset} on ${f.focusOn}; wanted solid 2px ${f.ring} offset 1px`,
      );
    if (f.ratioRing < 3) problems.push(`${who}: ring vs ground ${f.ratioRing.toFixed(2)} < 3.0`);
    if (f.placeholder !== null) {
      if (f.placeholder !== f.faint || f.placeholderOpacity !== '1')
        problems.push(
          `${who}: placeholder ${f.placeholder} opacity ${f.placeholderOpacity}, wanted ${f.faint} opacity 1`,
        );
      if ((f.ratioPlaceholder ?? 0) < 4.5)
        problems.push(`${who}: placeholder ratio ${(f.ratioPlaceholder ?? 0).toFixed(2)} < 4.5`);
    }
    const bf = base.fields.find(
      (x) => x.page === f.page && x.field === f.field && x.theme === f.theme,
    );
    if (
      bf &&
      f.box &&
      bf.box &&
      (Math.abs(f.box.width - bf.box.width) > 0.6 ||
        Math.abs(f.box.height - bf.box.height) > 0.6 ||
        Math.abs(f.box.x - bf.box.x) > 0.6)
    )
      problems.push(`${who}: box at 1024 base ${bx(bf.box)} after ${bx(f.box)}`);
  }
  return { problems, info };
}

// ---- the test -------------------------------------------------------------------------------------------------------

test(`p107 staff (${PHASE}), part ${PART}: 44 px targets and fields`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const started = Date.now();
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const defs = DEFS.filter((d) => d.part === PART);
  if (PART === 1)
    await setStudySurfacesWith(owner.request, true).catch((e) =>
      say(`storefront switches: ${String(e).slice(0, 120)}`),
    );
  try {
    // Phone (Lounge Light, sections in their shipped theme), then 1024 light, then 1024 dark for the settings pages.
    await setPrefs(owner, 'lounge-light', 'sideroom');
    for (const d of defs) {
      if (Date.now() - started > BUDGET_MS) {
        notCovered.push(`${d.name}: time budget used up`);
        continue;
      }
      await measure(browser, owner, d, w, 375, 'lounge-light', 'sideroom');
      await measure(browser, owner, d, w, 1024, 'lounge-light', 'sideroom');
    }
    const darkDefs = defs.filter((d) => d.dark);
    if (darkDefs.length) {
      await setPrefs(owner, 'lounge-dark', 'sideroom');
      for (const d of darkDefs)
        await measure(browser, owner, d, w, 1024, 'lounge-dark', 'sideroom');
    }
  } finally {
    if (PART === 1) await setStudySurfacesWith(owner.request, false).catch(() => undefined);
    await setPrefs(owner, 'lounge-light', 'sideroom').catch(() => undefined);
    writeFileSync(
      resolve(OUT, `figures-${PHASE}-part${PART}.json`),
      JSON.stringify(
        { phase: PHASE, part: PART, rows: rowsOut, fields: fieldsOut, notes, notCovered, sideways },
        null,
        1,
      ),
    );
    await owner.close();
  }
  for (const n of notes) say(`note: ${n}`);
  for (const n of notCovered) say(`not covered: ${n}`);
  say(
    `part ${PART} ${PHASE}: ${rowsOut.length} rows, 375 fail ${rowsOut.filter((r) => r.w === 375 && r.status === 'fail').length}, ${Math.round((Date.now() - started) / 1000)} s`,
  );
  const nr = rowsOut.filter((r) => r.status === 'not-reached');
  const problems: string[] = [];
  if (PHASE === 'after') {
    const bf = resolve(OUT, `figures-base-part${PART}.json`);
    if (existsSync(bf)) {
      const base = JSON.parse(readFileSync(bf, 'utf8')) as Saved;
      const c = compare(base, { rows: rowsOut, fields: fieldsOut });
      problems.push(...c.problems);
      for (const i of c.info) say(`info: ${i}`);
    } else say('no figures-base for this part: nothing compared');
  }
  for (const p of problems) say(`PROBLEM ${p}`);
  say(`problems: ${problems.length}`);
  expect(nr.map((r) => `${r.page} [${r.state}] ${r.name}`)).toEqual([]);
});
