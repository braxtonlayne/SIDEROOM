import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { CONTROL_SELECTOR, hitTestAll, openCollapsed } from '../support/hit-test';
import { over, parseColor, ratio } from '../support/contrast';

/**
 * Wave 108, pod tools (D-1022 the 44 px target on a phone, D-1024 the field contract). SHOTS BUILDER: measures what the
 * code builder tools changed (the Drawer's switcher field; .ud-btn, the developer tools' checkboxes, .sr-calc-hist-btn,
 * .sr-calc-bit, .sr-tz-pick); changes no app file. Copied from wave 107's p107-tool-shots.spec.ts (the pages that
 * wave's cures left alone are dropped: Studio, tier list, poll).
 *
 * Pages: the Drawer just opened (the switcher field), calculator, programmer calculator, unit converter, percent,
 * time zones (part 1); JSON, Base64, countdown timer, Invite card (part 2). Per page, per phase:
 *  - 375 x 812 touch, Lounge Light: openCollapsed + hitTestAll (default reach) over the drawer, then up to three
 *    one-tap overlays; every row whole (box, status, taker); the boxes of the rows and lists; shot.
 *  - 1024 x 768 Lounge Light and Lounge Dark: the same rows with reach 'box'; boxes (after must equal base); every raw
 *    text field read at rest, on hover and on keyboard focus (border, outline, placeholder, ratios); shots.
 *  - The switcher field (the Drawer's search input) is read after Tab (rest), hover, and Shift+Tab (keyboard focus), at
 *    all three configurations; its box must equal the base at every width.
 * After: when figures-base*.json is in OUT, every broken limit is a "PROBLEM" line; the run fails on any problem.
 * Switches: SR_P108_TOOL_ON=1, SR_P108_TOOL_PHASE=base|after, SR_P108_TOOL_PART=1|2 (0 or unset: all),
 * SR_P108_TOOL_OUT=<folder>. Files: figures-<phase>[-p<part>].json, report-<phase>[-p<part>].json, shots (base- prefix).
 * It makes no data, never seeds, never imports /src modules.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_TOOL_ON=1 --env SR_P108_TOOL_PHASE=after --env SR_P108_TOOL_PART=1 --env SR_P108_TOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-toolshots -- tests/p108-tool-shots.spec.ts
 * (part 2 the same with SR_P108_TOOL_PART=2)
 */
const ON = process.env.SR_P108_TOOL_ON === '1';
const PHASE = process.env.SR_P108_TOOL_PHASE === 'base' ? 'base' : 'after';
const PART = Number(process.env.SR_P108_TOOL_PART ?? '0');
const OUT =
  process.env.SR_P108_TOOL_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-toolshots';
const SUFFIX = PART ? `-p${PART}` : '';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const BUDGET_MS = 420_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const r1 = (n: number) => Math.round(n * 10) / 10;
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 200);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

type Theme = 'lounge-light' | 'lounge-dark';
interface Cfg {
  tag: string;
  w: number;
  h: number;
  touch: boolean;
  theme: Theme;
}
const CFGS: Cfg[] = [
  { tag: '375', w: 375, h: 812, touch: true, theme: 'lounge-light' },
  { tag: '1024-light', w: 1024, h: 768, touch: false, theme: 'lounge-light' },
  { tag: '1024-dark', w: 1024, h: 768, touch: false, theme: 'lounge-dark' },
];

// ---- the pages ------------------------------------------------------------------------------------------------------

const drawerOf = (page: Page) => page.getByRole('dialog', { name: 'Drawer', exact: true });

interface ToolDef {
  id: string;
  part: number;
  query: string;
  pick: RegExp;
  /** The Drawer just opened, no tool picked: the switcher field is the page. */
  switcher?: boolean;
}
const TOOLS: ToolDef[] = [
  { id: 'switcher', part: 1, query: '', pick: /./, switcher: true },
  { id: 'calculator', part: 1, query: 'calculator', pick: /^calculator/i },
  { id: 'programmer', part: 1, query: 'programmer', pick: /programmer/i },
  { id: 'units', part: 1, query: 'unit', pick: /unit converter/i },
  { id: 'percent', part: 1, query: 'percent', pick: /percentage calculator/i },
  { id: 'timezone', part: 1, query: 'time zones', pick: /time zones/i },
  { id: 'json', part: 2, query: 'json', pick: /^json/i },
  { id: 'base64', part: 2, query: 'base64', pick: /base64/i },
  { id: 'countdown', part: 2, query: 'countdown', pick: /countdown/i },
  { id: 'invite', part: 2, query: 'invite card', pick: /invite card/i },
];

/** Rows and lists whose box is recorded (every match), by selector; the limits below name them. */
const CONTAINERS = [
  '.sr-tl-tabs',
  '.sr-tl-view',
  '.sr-tl-view-head',
  '.sr-calc',
  '.sr-calc-keys',
  '.sr-calc-seg',
  '.sr-calc-row',
  '.sr-calc-opts',
  '.sr-calc-views',
  '.sr-calc-bits',
  '.sr-calc-bitrow',
  '.sr-calc-examples',
  '.sr-calc-history',
  '.sr-tz-scroll',
  '.sr-tz-grid',
  '.sr-card',
  '.sr-card-row',
  '.ud-root',
  '.ud-timer',
  '.ud-row',
];

// ---- figures --------------------------------------------------------------------------------------------------------

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface RowFig {
  key: string;
  state: string;
  tag: string;
  cls: string;
  type: string;
  name: string;
  status: string;
  box: Box | null;
  taker: { tag: string; role: string | null; name: string; path: string; at: string } | null;
}
interface FieldFig {
  key: string;
  kit: boolean;
  tag: string;
  cls: string;
  name: string;
  box: Box;
  restBorder: string;
  hoverBorder: string | null;
  focusBorder: string;
  borderWidth: string;
  outlineStyle: string;
  outlineWidth: string;
  outlineColor: string;
  outlineOffset: string;
  boxShadow: string;
  placeholder: string | null;
  placeholderOpacity: string | null;
  faint: string;
  focusToken: string;
  borderToken: string;
  outside: string;
  ground: string;
  ratios: {
    borderOutside: number;
    borderGround: number;
    ring: number;
    placeholder: number | null;
  };
}
interface PageFig {
  theme: (string | null)[];
  scroll: number[];
  drawer: Box | null;
  rows: RowFig[];
  containers: Record<string, Box[]>;
  rowGap: string[];
  fields: FieldFig[];
  notes: string[];
}
type Figures = Record<string, Record<string, PageFig>>;

const figs: Figures = {};
const problems: string[] = [];
const notCovered: string[] = [];

const shotName = (id: string, cfg: Cfg, extra = '') => {
  const t = cfg.tag === '375' ? '375' : cfg.tag;
  return `${PHASE === 'base' ? 'base-' : ''}${id}${extra}-${t}.png`;
};

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.mouse.move(2, 2).catch(() => undefined);
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, name), timeout: 15_000 });
}

async function setMain(owner: BrowserContext, theme: Theme) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await owner.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.w, height: c.h },
    storageState: await owner.storageState(),
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

async function settle(page: Page, ms = 4000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(500);
}

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawerOf(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click();
  await page.waitForTimeout(900);
}

async function closeTool(page: Page) {
  for (
    let i = 0;
    i < 3 &&
    (await drawerOf(page)
      .isVisible()
      .catch(() => false));
    i++
  ) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

async function testRoot(
  page: Page,
  root: Locator,
  state: string,
  cfg: Cfg,
  out: RowFig[],
  also?: string,
) {
  await openCollapsed(page, root);
  const sel = also ? `${CONTROL_SELECTOR},${also}` : CONTROL_SELECTOR;
  const rows = await hitTestAll(page, root, {
    ...(cfg.touch ? {} : { reach: 'box' as const }),
    ...(also ? { also } : {}),
  });
  const meta = await root.locator(sel).evaluateAll((els) =>
    els.map((e) => ({
      cls: (e.getAttribute('class') ?? '').trim().slice(0, 80),
      type: e.getAttribute('type') ?? '',
    })),
  );
  for (const r of rows) {
    const bad = r.result?.points.find((p) => !p.ok);
    const b = r.result?.box ?? null;
    out.push({
      key: `${state}|${r.index}:${r.tag}:${r.name}`,
      state,
      tag: r.tag,
      cls: meta[r.index]?.cls ?? '',
      type: meta[r.index]?.type ?? r.type ?? '',
      name: r.name,
      status: r.status,
      box: b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null,
      taker: bad?.taker
        ? {
            tag: bad.taker.tag,
            role: bad.taker.role,
            name: bad.taker.name,
            path: bad.taker.path,
            at: bad.at,
          }
        : null,
    });
  }
  const bad = rows.filter((r) => r.status === 'not-reached');
  if (bad.length)
    problems.push(
      `${state}: not-reached (a spec error): ${bad.map((r) => `${r.name}: ${r.reason}`).join('; ')}`,
    );
}

/** Up to three one-tap overlays (menu or kit select triggers), tested as the census did. */
async function overlays(page: Page, drawer: Locator, id: string, cfg: Cfg, out: RowFig[]) {
  const cands = drawer.locator(
    'button[aria-haspopup]:not([aria-haspopup="false"]),[role=combobox]',
  );
  const n = Math.min(await cands.count(), 20);
  let done = 0;
  const seen = new Set<string>();
  for (let i = 0; i < n && done < 3; i++) {
    const loc = cands.nth(i);
    const info = await loc
      .evaluate((el) => {
        const r = el.getBoundingClientRect();
        return {
          name: (el.getAttribute('aria-label') ?? el.textContent ?? '').trim().slice(0, 40),
          shown: r.width > 0 && r.height > 0,
          expanded: el.getAttribute('aria-expanded'),
          search: el.classList.contains('sr-tl-input'),
        };
      })
      .catch(() => null);
    if (!info || !info.shown || info.expanded === 'true' || info.search || seen.has(info.name))
      continue;
    seen.add(info.name);
    try {
      await loc.scrollIntoViewIfNeeded({ timeout: 4000 });
      const before = await page
        .locator('[role=menu],[role=listbox]')
        .filter({ visible: true })
        .count();
      await loc.evaluate((el) => (el as HTMLElement).click());
      await page.waitForTimeout(500);
      const ov = page.locator('[role=menu],[role=listbox]').filter({ visible: true });
      if ((await ov.count()) > before) {
        await testRoot(
          page,
          ov.last(),
          `${id} overlay: ${info.name}`,
          cfg,
          out,
          '[role=menuitem],[role=option]',
        );
        done++;
      }
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
    } catch (e) {
      notCovered.push(`${id} overlay "${info.name}": ${msg(e)}`);
    }
  }
}

/** Every raw text field of the drawer: rest, hover, keyboard focus; the colours and ratios. */
async function readFields(page: Page, drawer: Locator, switcher = false): Promise<FieldFig[]> {
  const sel = switcher
    ? 'input.sr-tl-input'
    : 'input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]):not([type=range]):not([type=color]):not([type=submit]):not([type=button]), textarea';
  const all = drawer.locator(sel);
  const n = await all.count();
  const out: FieldFig[] = [];
  const read = (loc: Locator) =>
    loc.evaluate((f) => {
      const cv = document.createElement('canvas');
      cv.width = cv.height = 1;
      const g = cv.getContext('2d', { willReadFrequently: true })!;
      const norm = (css: string): string => {
        g.clearRect(0, 0, 1, 1);
        g.fillStyle = '#000';
        g.fillStyle = css;
        g.fillRect(0, 0, 1, 1);
        const d = g.getImageData(0, 0, 1, 1).data;
        return `rgb(${d[0]}, ${d[1]}, ${d[2]})`;
      };
      const cs = getComputedStyle(f);
      const alphaOf = (c: string) => {
        const m = /rgba?\([^)]*?,\s*([\d.]+)\)$/.exec(c) ?? /\/\s*([\d.]+)\)$/.exec(c);
        return m ? Number(m[1]) : 1;
      };
      const flat = (css: string, ground: string): string => {
        // a colour with alpha laid over its ground, as rgb()
        const a = alphaOf(css);
        if (a >= 1) return norm(css);
        const t = norm(css);
        const gr = norm(ground);
        const p = (s: string) => /(\d+), (\d+), (\d+)/.exec(s)!.slice(1).map(Number);
        const tc = p(t);
        const gc = p(gr);
        return `rgb(${tc.map((v, i) => Math.round(v * a + gc[i]! * (1 - a))).join(', ')})`;
      };
      // ground behind the field: the first opaque ancestor background, laid up from there
      const stack: string[] = [];
      for (let a: Element | null = f.parentElement; a; a = a.parentElement) {
        const c = getComputedStyle(a).backgroundColor;
        if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') {
          stack.push(c);
          if (alphaOf(c) >= 1) break;
        }
      }
      let outside = 'rgb(255, 255, 255)';
      for (const c of stack.reverse()) outside = flat(c, outside);
      const ground = flat(cs.backgroundColor, outside);
      const probe = document.createElement('span');
      document.body.appendChild(probe);
      const tok = (k: string) => {
        probe.style.color = '';
        probe.style.color = cs.getPropertyValue(k).trim() || 'transparent';
        return norm(getComputedStyle(probe).color);
      };
      const faint = tok('--ui-text-faint');
      const focusToken = tok('--ui-focus');
      const borderToken = tok('--ui-border');
      probe.remove();
      const side =
        ['Top', 'Right', 'Bottom', 'Left'].find(
          (s) =>
            (cs as unknown as Record<string, string>)[`border${s}Style`] !== 'none' &&
            parseFloat((cs as unknown as Record<string, string>)[`border${s}Width`] as string) > 0,
        ) ?? 'Top';
      const bc = (cs as unknown as Record<string, string>)[`border${side}Color`] as string;
      const bw = (cs as unknown as Record<string, string>)[`border${side}Width`] as string;
      const r = f.getBoundingClientRect();
      const ph = f.hasAttribute('placeholder') ? getComputedStyle(f, '::placeholder') : null;
      const cls = (typeof f.className === 'string' ? f.className : '').trim();
      const labels = (f as HTMLInputElement).labels;
      const name =
        f.getAttribute('aria-label') ||
        (labels && labels[0]?.textContent) ||
        f.getAttribute('placeholder') ||
        f.id ||
        '';
      return {
        kit: /(^|\s)([A-Za-z0-9]*_?control_|control)/.test(cls) && !/(^|\s)[a-z]{1,5}-/.test(cls),
        tag: f.tagName.toLowerCase(),
        cls: cls.slice(0, 80),
        name: name.trim().replace(/\s+/g, ' ').slice(0, 40),
        box: {
          x: Math.round(r.left * 10) / 10,
          y: Math.round(r.top * 10) / 10,
          w: Math.round(r.width * 10) / 10,
          h: Math.round(r.height * 10) / 10,
        },
        border: flat(bc, outside),
        bw,
        outlineStyle: cs.outlineStyle,
        outlineWidth: cs.outlineWidth,
        outlineColor: norm(cs.outlineColor),
        outlineOffset: cs.outlineOffset,
        boxShadow: cs.boxShadow,
        placeholder: ph ? norm(ph.color) : null,
        placeholderOpacity: ph ? ph.opacity : null,
        faint,
        focusToken,
        borderToken,
        outside,
        ground,
      };
    });
  for (let i = 0; i < n; i++) {
    const f = all.nth(i);
    if (!(await f.isVisible().catch(() => false))) continue;
    if (!switcher && (await f.evaluate((e) => e.classList.contains('sr-tl-input')))) continue;
    if (await f.isDisabled().catch(() => false)) continue;
    await f.scrollIntoViewIfNeeded().catch(() => undefined);
    await page.mouse.move(2, 2);
    if (switcher) {
      // the switcher opens focused: move focus away with Tab, so the rest read is real
      await page.keyboard.press('Tab');
    }
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.waitForTimeout(80);
    const rest = await read(f);
    if (rest.box.w < 40 || rest.box.h < 16) continue;
    let hover: string | null = null;
    try {
      await f.hover({ timeout: 3000 });
      await page.waitForTimeout(120);
      hover = (await read(f)).border;
    } catch {
      /* covered */
    }
    await page.mouse.move(2, 2);
    if (switcher) {
      // focus by keyboard: Tab to the next control, then Shift+Tab back onto the field
      await page.keyboard.press('Tab');
      await page.keyboard.press('Shift+Tab');
    } else await f.focus();
    await page.waitForTimeout(150);
    const foc = await read(f);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    const C = (s: string) => parseColor(s);
    const outside = C(rest.outside);
    const ground = C(rest.ground);
    const bor = over(C(rest.border), ground);
    const ring = over(C(foc.outlineColor), outside);
    const ph = rest.placeholder ? over(C(rest.placeholder), ground) : null;
    out.push({
      key: `${i}:${rest.tag}:${rest.name}`,
      kit: rest.kit,
      tag: rest.tag,
      cls: rest.cls,
      name: rest.name,
      box: { x: rest.box.x, y: rest.box.y, w: rest.box.w, h: rest.box.h },
      restBorder: rest.border,
      hoverBorder: hover,
      focusBorder: foc.border,
      borderWidth: rest.bw,
      outlineStyle: foc.outlineStyle,
      outlineWidth: foc.outlineWidth,
      outlineColor: foc.outlineColor,
      outlineOffset: foc.outlineOffset,
      boxShadow: foc.boxShadow,
      placeholder: rest.placeholder,
      placeholderOpacity: rest.placeholderOpacity,
      faint: rest.faint,
      focusToken: rest.focusToken,
      borderToken: rest.borderToken,
      outside: rest.outside,
      ground: rest.ground,
      ratios: {
        borderOutside: Math.round(ratio(bor, outside) * 100) / 100,
        borderGround: Math.round(ratio(bor, ground) * 100) / 100,
        ring: Math.round(ratio(ring, outside) * 100) / 100,
        placeholder: ph ? Math.round(ratio(ph, ground) * 100) / 100 : null,
      },
    });
  }
  return out;
}

async function boxesOf(drawer: Locator, page: Page) {
  const dbox = await drawer.boundingBox();
  const containers = await drawer.evaluate((root, sels) => {
    const o: Record<string, { x: number; y: number; w: number; h: number }[]> = {};
    for (const s of sels) {
      const list = Array.from(root.querySelectorAll(s))
        .filter((e) => e.getClientRects().length > 0)
        .map((e) => {
          const b = e.getBoundingClientRect();
          const rd = (n: number) => Math.round(n * 10) / 10;
          return { x: rd(b.x), y: rd(b.y), w: rd(b.width), h: rd(b.height) };
        });
      if (list.length) o[s] = list;
    }
    return o;
  }, CONTAINERS);
  const rowGap = await drawer.evaluate((root) =>
    Array.from(root.querySelectorAll('.sr-st-bar, .sr-st-row, .sr-st-panel')).map((e) => {
      const cs = getComputedStyle(e);
      return `${(e as HTMLElement).className}: row-gap ${cs.rowGap}`;
    }),
  );
  const scroll = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  return {
    drawer: dbox ? { x: r1(dbox.x), y: r1(dbox.y), w: r1(dbox.width), h: r1(dbox.height) } : null,
    containers,
    rowGap,
    scroll,
  };
}

// ---- one page in one configuration ------------------------------------------------------------------------------------

async function doPage(page: Page, t: ToolDef, cfg: Cfg, url: string): Promise<PageFig> {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
  await settle(page);
  if (t.switcher) {
    await expect(async () => {
      await page.keyboard.press('ControlOrMeta+k');
      await expect(drawerOf(page)).toBeVisible({ timeout: 1000 });
    }).toPass({ timeout: 20_000 });
    await page.waitForTimeout(900);
  } else await openTool(page, t.query, t.pick);
  const drawer = drawerOf(page);
  await expect(drawer).toBeVisible();
  await settle(page, 3000);
  const fig: PageFig = {
    theme: await page.evaluate(() => [
      document.documentElement.getAttribute('data-theme'),
      document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    ]),
    scroll: [],
    drawer: null,
    rows: [],
    containers: {},
    rowGap: [],
    fields: [],
    notes: [],
  };
  if (!fig.theme.includes(cfg.theme))
    problems.push(`${t.id} ${cfg.tag}: data-theme is ${JSON.stringify(fig.theme)}`);
  const b = await boxesOf(drawer, page);
  Object.assign(fig, b);
  if (b.scroll[0]! > b.scroll[1]!)
    problems.push(`${t.id} ${cfg.tag}: the page scrolls sideways (${b.scroll.join(' > ')})`);
  await testRoot(page, drawer, `${t.id}`, cfg, fig.rows);
  if (t.switcher) {
    fig.fields = await readFields(page, drawer, true);
    await page.keyboard.press('Tab');
    await page.keyboard.press('Shift+Tab');
    await page.waitForTimeout(250);
    await page.screenshot({ path: resolve(OUT, shotName(t.id, cfg)), timeout: 15_000 });
  } else if (cfg.touch) {
    await shot(page, shotName(t.id, cfg));
    await overlays(page, drawer, t.id, cfg, fig.rows);
  } else {
    fig.fields = await readFields(page, drawer);
    const first = drawer
      .locator(
        'input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=file]):not(.sr-tl-input), textarea',
      )
      .first();
    if (await first.count()) {
      await first.scrollIntoViewIfNeeded().catch(() => undefined);
      await first.focus();
      await page.keyboard.press('Shift+Tab');
      await page.keyboard.press('Tab');
    }
    await page.waitForTimeout(250);
    await page.screenshot({ path: resolve(OUT, shotName(t.id, cfg)), timeout: 15_000 });
  }
  await closeTool(page);
  return fig;
}

// ---- compare --------------------------------------------------------------------------------------------------------

const CONTROL_TAKER = /^(button|a|input|textarea|select)$/;
const eq = (a: Box, b: Box) =>
  Math.abs(a.x - b.x) < 0.6 &&
  Math.abs(a.y - b.y) < 0.6 &&
  Math.abs(a.w - b.w) < 0.6 &&
  Math.abs(a.h - b.h) < 0.6;
/** What a container may grow by at 375 (px of height); named in the builders' tasks. Others: reported, not limited. */
const GROW_LIMIT: Record<string, number> = {
  '.sr-calc-keys': 40,
  '.sr-calc-history': 0,
  '.sr-card': 130,
  '.sr-tl-view-head': 60,
};

function compare(id: string, cfg: Cfg, b: PageFig, a: PageFig) {
  const tag = `${id} ${cfg.tag}`;
  // the drawer's width never moves
  if (b.drawer && a.drawer && Math.abs(b.drawer.w - a.drawer.w) > 0.6)
    problems.push(`${tag}: drawer width ${b.drawer.w} -> ${a.drawer.w}`);
  const bm = new Map(b.rows.map((r) => [r.key, r]));
  for (const r of a.rows) {
    const o = bm.get(r.key);
    if (cfg.touch) {
      if (r.status === 'fail') {
        const t = r.taker;
        problems.push(
          `${tag}: STILL FAILS ${r.key} [${r.cls || r.tag}] ${r.box ? `${r.box.w}x${r.box.h}` : '-'} base ${o?.status ?? 'absent'} taker ${t ? `${t.at}: ${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}" ${t.path}` : '-'}${
            t && (CONTROL_TAKER.test(t.tag) || /button|link|checkbox|combobox/.test(t.role ?? ''))
              ? ' (A TAP AREA TAKES ANOTHER CONTROL)'
              : ''
          }`,
        );
      }
      if (o && o.status === 'pass' && r.status !== 'pass' && r.status !== 'skipped')
        problems.push(`${tag}: REGRESSION ${r.key} passed in the base, now ${r.status}`);
      // a one-line field is 44 tall on a phone
      if (
        r.tag === 'input' &&
        !/checkbox|radio|file/.test(r.type) &&
        r.box &&
        r.status !== 'skipped' &&
        !/sr-tl-input/.test(r.cls) &&
        r.box.h < 43.5
      )
        problems.push(`${tag}: field ${r.key} is ${r.box.h} tall at 375, wanted 44`);
    } else if (o?.box && r.box && !eq(o.box, r.box)) {
      problems.push(
        `${tag}: MOVED at 1024 ${r.key}: ${JSON.stringify(o.box)} -> ${JSON.stringify(r.box)}`,
      );
    }
  }
  // the switcher field changes colour and ring offset only: its box never moves, at any width
  for (const f of a.fields) {
    if (!/sr-tl-input/.test(f.cls)) continue;
    const o = b.fields.find((x) => x.key === f.key);
    if (!o) problems.push(`${tag}: switcher field ${f.key} has no base figures`);
    else if (!eq(o.box, f.box))
      problems.push(
        `${tag}: switcher field MOVED ${JSON.stringify(o.box)} -> ${JSON.stringify(f.box)}`,
      );
    if (cfg.touch) {
      const miss: string[] = [];
      if (f.restBorder === f.borderToken) miss.push('rest border is still plain --ui-border');
      if (f.hoverBorder !== f.faint)
        miss.push(`hover border ${f.hoverBorder} is not --ui-text-faint`);
      if (f.outlineWidth !== '2px' || f.outlineOffset !== '1px')
        miss.push(`focus outline ${f.outlineWidth} offset ${f.outlineOffset} (2px offset 1px)`);
      if (miss.length) problems.push(`${tag}: SWITCHER ${miss.join('; ')}`);
    }
  }
  if (cfg.touch) {
    // the bit grid: 44 px rows may not take it past twice its height (then a tap area with a row gap is asked for)
    const bh = (b.containers['.sr-calc-bits'] ?? []).reduce((x, y) => x + y.h, 0);
    const ah = (a.containers['.sr-calc-bits'] ?? []).reduce((x, y) => x + y.h, 0);
    if (bh) say(`note: ${tag}: bit grid ${r1(bh)} -> ${r1(ah)} px (x${r1(ah / bh)})`);
    if (bh && ah > 2 * bh)
      problems.push(
        `${tag}: the bit grid is ${r1(ah)} px tall, base ${r1(bh)}: more than twice (use a tap area with a row gap)`,
      );
    for (const sel of [
      '.sr-calc-examples',
      '.sr-tz-grid',
      '.sr-tz-scroll',
      '.ud-row',
      '.ud-timer',
    ]) {
      const x = (b.containers[sel] ?? []).reduce((q, w) => q + w.h, 0);
      const y = (a.containers[sel] ?? []).reduce((q, w) => q + w.h, 0);
      if (x || y) say(`note: ${tag}: ${sel} ${r1(x)} -> ${r1(y)} px`);
    }
  }
  if (!cfg.touch) {
    for (const [sel, list] of Object.entries(a.containers)) {
      const ol = b.containers[sel] ?? [];
      list.forEach((box, i) => {
        if (ol[i] && !eq(ol[i]!, box))
          problems.push(
            `${tag}: container ${sel}#${i} moved at 1024 ${JSON.stringify(ol[i])} -> ${JSON.stringify(box)}`,
          );
      });
      if (ol.length !== list.length)
        problems.push(`${tag}: container ${sel} count ${ol.length} -> ${list.length}`);
    }
    for (const f of a.fields) {
      if (f.kit) continue;
      const miss: string[] = [];
      const o = b.fields.find((x) => x.key === f.key);
      if (o && Math.abs(o.box.h - f.box.h) > 0.6)
        problems.push(`${tag}: field ${f.key} height ${o.box.h} -> ${f.box.h} at 1024`);
      if (o && Math.abs(o.box.w - f.box.w) > 0.6)
        problems.push(`${tag}: field ${f.key} width ${o.box.w} -> ${f.box.w} at 1024`);
      if (f.ratios.borderOutside < 3 && !/sr-tier-name/.test(f.cls))
        miss.push(`rest border ${f.ratios.borderOutside} on the outside (3.0)`);
      if (f.ratios.borderGround < 3)
        miss.push(`rest border ${f.ratios.borderGround} on the field (3.0)`);
      if (f.restBorder === f.borderToken) miss.push('rest border is still plain --ui-border');
      if (f.hoverBorder !== f.faint)
        miss.push(`hover border ${f.hoverBorder} is not --ui-text-faint ${f.faint}`);
      if (f.outlineStyle !== 'solid' || f.outlineWidth !== '2px' || f.outlineOffset !== '1px')
        miss.push(
          `focus outline ${f.outlineStyle} ${f.outlineWidth} offset ${f.outlineOffset} (solid 2px offset 1px)`,
        );
      if (f.outlineColor !== f.focusToken)
        miss.push(`ring colour ${f.outlineColor} is not --ui-focus ${f.focusToken}`);
      const line = /rgb\(\d+, \d+, \d+\)/.exec(f.boxShadow)?.[0];
      if (f.ratios.ring < 3) {
        if (!line) miss.push(`ring ${f.ratios.ring} on its ground (3.0) and no inner line`);
        else {
          const lr = Math.round(ratio(parseColor(f.outlineColor), parseColor(line)) * 100) / 100;
          fig_note(
            `${tag}: ${f.key} ring ${f.ratios.ring} on head ${f.outside}; against its --ui-bg line ${line}: ${lr}`,
          );
          if (lr < 3) miss.push(`ring ${f.ratios.ring} on the head and ${lr} on its line (3.0)`);
        }
      }
      if (f.placeholder !== null) {
        if (f.placeholder !== f.faint)
          miss.push(`placeholder ${f.placeholder} is not --ui-text-faint ${f.faint}`);
        if (f.placeholderOpacity !== '1') miss.push(`placeholder opacity ${f.placeholderOpacity}`);
        if ((f.ratios.placeholder ?? 0) < 4.5)
          miss.push(`placeholder ${f.ratios.placeholder} on the field (4.5)`);
      }
      if (miss.length) problems.push(`${tag}: FIELD ${f.key} [${f.cls}] ${miss.join('; ')}`);
    }
  } else {
    for (const [sel, lim] of Object.entries(GROW_LIMIT)) {
      const bh = (b.containers[sel] ?? []).reduce((s, x) => s + x.h, 0);
      const ah = (a.containers[sel] ?? []).reduce((s, x) => s + x.h, 0);
      if (ah - bh > lim + 0.6)
        problems.push(`${tag}: ${sel} grew ${r1(ah - bh)} px (limit ${lim})`);
    }
  }
}
function fig_note(s: string) {
  say(`note: ${s}`);
}

// ---- the test -------------------------------------------------------------------------------------------------------

test(`tools pod ${PHASE}${SUFFIX}: 44 px targets and the field contract`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const started = Date.now();
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const tools = TOOLS.filter((t) => !PART || t.part === PART);
  const baseFile = resolve(OUT, `figures-base${SUFFIX}.json`);
  const base =
    PHASE === 'after' && existsSync(baseFile)
      ? (JSON.parse(readFileSync(baseFile, 'utf8')) as { figures: Figures }).figures
      : null;
  if (PHASE === 'after' && !base) say(`no ${baseFile}: nothing compared`);
  const flush = () => {
    writeFileSync(
      resolve(OUT, `figures-${PHASE}${SUFFIX}.json`),
      JSON.stringify({ figures: figs, notCovered }, null, 1),
    );
    writeFileSync(
      resolve(OUT, `report-${PHASE}${SUFFIX}.json`),
      JSON.stringify({ problems, notCovered }, null, 1),
    );
  };
  try {
    for (const cfg of CFGS) {
      await setMain(owner, cfg.theme);
      const { ctx, page } = await newPage(browser, owner, cfg);
      page.on('pageerror', (e) => say(`pageerror: ${msg(e)}`));
      try {
        for (const t of tools) {
          if (Date.now() - started > BUDGET_MS) {
            notCovered.push(`${t.id} ${cfg.tag}: the time budget was used up`);
            continue;
          }
          try {
            const url = `/channels/${w.guildId}/${w.channelId}`;
            const fig = await doPage(page, t, cfg, url);
            (figs[t.id] ??= {})[cfg.tag] = fig;
            const fails = fig.rows.filter((r) => r.status === 'fail').length;
            say(
              `${t.id} ${cfg.tag}: rows ${fig.rows.length}, fail ${fails}, fields ${fig.fields.length}, scroll ${fig.scroll.join('/')}, ${Math.round((Date.now() - started) / 1000)} s`,
            );
            if (fig.notes.length) say(`${t.id} ${cfg.tag}: ${fig.notes.join('; ')}`);
          } catch (e) {
            notCovered.push(`${t.id} ${cfg.tag}: ${msg(e)}`);
            say(`NOT COVERED ${t.id} ${cfg.tag}: ${msg(e)}`);
            await closeTool(page).catch(() => undefined);
          }
          flush();
        }
      } finally {
        await ctx.close();
      }
    }
    if (base) {
      for (const t of tools)
        for (const cfg of CFGS) {
          const a = figs[t.id]?.[cfg.tag];
          const b = base[t.id]?.[cfg.tag];
          if (a && b) compare(t.id, cfg, b, a);
          else if (a) problems.push(`${t.id} ${cfg.tag}: no base figures to compare`);
        }
    }
    // the tier name's ring against each head colour (after only; the figures name every head)
    for (const [id, byCfg] of Object.entries(figs))
      for (const [tag, f] of Object.entries(byCfg))
        for (const fd of f.fields)
          if (/sr-tier-name/.test(fd.cls))
            say(
              `RING ${id} ${tag} tier name ${fd.key}: head ${fd.outside}, ring ${fd.outlineColor} ${fd.outlineWidth} offset ${fd.outlineOffset}, ratio ${fd.ratios.ring}, box-shadow ${fd.boxShadow}`,
            );
  } finally {
    flush();
    await owner.close();
  }
  for (const p of problems) say(`PROBLEM ${p}`);
  for (const n of notCovered) say(`not covered: ${n}`);
  say(`${PHASE}${SUFFIX}: problems ${problems.length}, not covered ${notCovered.length}`);
  if (PHASE === 'after' && base) expect(problems, 'no problems').toEqual([]);
  expect(notCovered, 'every page covered').toEqual([]);
});
