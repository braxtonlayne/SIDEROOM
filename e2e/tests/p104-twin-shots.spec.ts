import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, ElementHandle, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * D-988 (wave 104, twin): shoot three pages in dev mode and in built mode on one tree and one database, and record the
 * boxes and the computed layout of their control rows, so the two can be compared and any difference explained.
 *
 * Pages (each at 1280 x 800 and 1024 x 768, and at 375 x 812 touch, Lounge Light):
 *   1. Server settings > Members: the filter row (search, role, the two dates, the checkbox, sort, Prune).
 *   2. The Library's Browse toolbar (`sectionThemes.library` set to `sideroom`, `data-theme` read back).
 *   3. The Lounge's main view: the channel header's controls and the composer's row.
 * Per container: computed display, flex-wrap, gap, column-gap; per control: its box, its cell's box (the container's child
 * that holds it), the row (cells with the same top share a row) and computed flex, min-inline-size, inline-size, font-size,
 * padding-inline. Per page: the document's scrollWidth / clientWidth and the stylesheets in the order the page loaded them.
 *
 * READ-ONLY on the world: it creates nothing and deletes nothing (it sets the account's main theme, the Library's section
 * theme and zoom, the same in both modes), so both modes see the same rows. Elements are found by role, label or id; the
 * rows' containers are found as the lowest common ancestor of two controls. Nothing is masked.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited).
 *
 * Local only (SR_P104_TWIN_ON=1). SR_P104_TWIN_MODE=dev|built; SR_P104_TWIN_OUT is the folder of the shots and of
 * `figures-<mode>.json`. Dev: from e2e/, on a Vite + sr-server stack (API 8805, Vite 5805, metrics 9505):
 *   SR_E2E_BASE_URL=http://localhost:5805 SR_P104_TWIN_ON=1 SR_P104_TWIN_MODE=dev \
 *   SR_P104_TWIN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-twin \
 *   npx playwright test tests/p104-twin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Built: the same database served by a cell over `apps/web/dist` (cli-agents/cell.sh), and
 *   SR_E2E_WEB=server SR_E2E_SERVER_PORT=8805 SR_E2E_BASE_URL=http://localhost:8805 SR_P104_TWIN_MODE=built (rest as above).
 */
const ON = process.env.SR_P104_TWIN_ON === '1';
const MODE = process.env.SR_P104_TWIN_MODE === 'built' ? 'built' : 'dev';
const OUT =
  process.env.SR_P104_TWIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-twin';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
}
const CONFIGS: Cfg[] = [
  { tag: '1280', width: 1280, height: 800, touch: false },
  { tag: '1024', width: 1024, height: 768, touch: false },
  { tag: '375', width: 375, height: 812, touch: true },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface ControlFig {
  name: string;
  tag: string;
  box: Box;
  cell: Box;
  row: number;
  css: Record<string, string>;
  cellCss: Record<string, string>;
}
interface ContainerFig {
  box: Box;
  css: Record<string, string>;
  /** The number of rows (distinct cell tops). */
  rows: number;
  controls: ControlFig[];
}
interface PageFig {
  theme: string | null;
  page: [number, number];
  sheets: { href: string | null; first: string | null; rules: number; devId: string | null }[];
  containers: Record<string, ContainerFig>;
}

/** Everything is measured inside the page: the container's computed layout and every visible control inside it. */
function measure(page: Page, container: ElementHandle): Promise<ContainerFig> {
  return page.evaluate((rootNode) => {
    const root = rootNode as unknown as Element;
    const rd = (n: number) => Math.round(n * 100) / 100;
    const bx = (e: Element) => {
      const b = e.getBoundingClientRect();
      return { x: rd(b.x), y: rd(b.y), w: rd(b.width), h: rd(b.height) };
    };
    const pick = (e: Element, props: string[]) => {
      const cs = getComputedStyle(e);
      const out: Record<string, string> = {};
      for (const p of props) out[p] = cs.getPropertyValue(p);
      return out;
    };
    const CONTAINER_PROPS = [
      'display',
      'flex-wrap',
      'gap',
      'column-gap',
      'row-gap',
      'flex-direction',
      'align-items',
    ];
    const CONTROL_PROPS = [
      'display',
      'flex',
      'min-inline-size',
      'inline-size',
      'font-size',
      'padding-inline-start',
      'padding-inline-end',
      'margin-inline-start',
      'max-inline-size',
    ];
    const INTERACTIVE =
      'button, input, select, textarea, a[href], [role=checkbox], [role=radio], [role=switch], [role=tab], [role=combobox]';
    const nameOf = (e: Element) => {
      const el = e as HTMLInputElement;
      const aria = e.getAttribute('aria-label');
      if (aria) return aria.trim();
      const labels = el.labels
        ? Array.from(el.labels).map((l) => (l.textContent ?? '').trim())
        : [];
      if (labels.join(' ').trim()) return labels.join(' ').trim();
      const text = (e.textContent ?? '').trim();
      if (text) return text.slice(0, 40);
      return el.placeholder || el.id || e.tagName.toLowerCase();
    };
    const cells = Array.from(root.children);
    const tops = Array.from(new Set(cells.map((c) => rd(c.getBoundingClientRect().top)))).sort(
      (a, b) => a - b,
    );
    const controls: unknown[] = [];
    for (const e of Array.from(root.querySelectorAll(INTERACTIVE))) {
      const b = e.getBoundingClientRect();
      if (b.width === 0 && b.height === 0) continue;
      const cell = cells.find((c) => c.contains(e)) ?? (e as Element);
      const cb = cell.getBoundingClientRect();
      controls.push({
        name: nameOf(e),
        tag:
          e.tagName.toLowerCase() +
          ((e as HTMLInputElement).type ? `[${(e as HTMLInputElement).type}]` : ''),
        box: bx(e),
        cell: bx(cell),
        row: tops.indexOf(rd(cb.top)),
        css: pick(e, CONTROL_PROPS),
        cellCss: pick(cell, CONTROL_PROPS),
      });
    }
    return {
      box: bx(root),
      css: pick(root, CONTAINER_PROPS),
      rows: tops.length,
      controls,
    };
  }, container) as Promise<ContainerFig>;
}

/** The lowest common ancestor of two elements. */
async function lca(page: Page, a: Locator, b: Locator): Promise<ElementHandle> {
  const ha = await a.elementHandle();
  const hb = await b.elementHandle();
  if (!ha || !hb) throw new Error('lca: an element was not found');
  const h = await page.evaluateHandle(
    ([x, y]) => {
      for (let e: Element | null = x as Element; e; e = e.parentElement)
        if (e.contains(y as Element)) return e;
      return null;
    },
    [ha, hb] as const,
  );
  const el = h.asElement();
  if (!el) throw new Error('lca: no common ancestor');
  return el;
}

async function parentOf(page: Page, h: ElementHandle): Promise<ElementHandle> {
  const p = await page.evaluateHandle((e) => (e as Element).parentElement, h);
  const el = p.asElement();
  if (!el) throw new Error('no parent');
  return el;
}

function pageFacts(page: Page): Promise<Pick<PageFig, 'theme' | 'page' | 'sheets'>> {
  return page.evaluate(() => {
    const sheets = Array.from(document.styleSheets).map((s) => {
      let first: string | null = null;
      let rules = 0;
      try {
        rules = s.cssRules.length;
        const r = s.cssRules[0] as CSSStyleRule | undefined;
        first = r ? (r.selectorText ?? r.cssText.slice(0, 60)) : null;
      } catch {
        first = 'cross-origin';
      }
      const node = s.ownerNode as Element | null;
      return {
        href: s.href ? new URL(s.href).pathname : null,
        first,
        rules,
        devId: node?.getAttribute('data-vite-dev-id')?.split(/[\\/]/).slice(-3).join('/') ?? null,
      };
    });
    return {
      theme:
        document.documentElement.getAttribute('data-theme') ??
        document.querySelector('[data-theme]')?.getAttribute('data-theme') ??
        null,
      page: [document.documentElement.scrollWidth, document.documentElement.clientWidth] as [
        number,
        number,
      ],
      sheets,
    };
  });
}

async function setPrefs(ctx: BrowserContext) {
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: {
      user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: { library: 'sideroom' } },
    },
  });
  const back = (await (await ctx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string; sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.theme).toBe('lounge-light');
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe('sideroom');
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
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
        JSON.stringify({
          state: {
            prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: { library: 'sideroom' } },
          },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

type Finder = (page: Page) => Promise<Record<string, ElementHandle>>;
interface PageDef {
  key: string;
  open: (page: Page) => Promise<void>;
  /** Named containers, found by the lowest common ancestor of controls found by role, label or id. */
  containers: Finder;
}

const PAGES: PageDef[] = [
  {
    key: 'members',
    open: async (page) => {
      const w = world();
      await page.goto(`/settings/server/${w.guildId}/members`, {
        waitUntil: 'domcontentloaded',
        timeout: 20_000,
      });
      await page.waitForSelector('#ss-member-role', { timeout: 20_000 });
      await page.waitForSelector('main table tbody tr', { timeout: 20_000 });
      await idle(page, 4000);
      await page.evaluate(() => document.fonts?.ready);
    },
    containers: async (page) => ({
      filterRow: await lca(
        page,
        page.getByRole('searchbox').first(),
        page.getByRole('button', { name: 'Prune', exact: false }).first(),
      ),
    }),
  },
  {
    key: 'library',
    open: async (page) => {
      await page.goto('/library/browse', { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.getByLabel('Sort by').and(page.locator('select')).first().waitFor();
      await idle(page, 5000);
      await page.evaluate(() => document.fonts?.ready);
    },
    containers: async (page) => {
      const tools = await lca(
        page,
        page.getByLabel('Sort by').and(page.locator('select')).first(),
        page.getByRole('button', { name: /^Filters/ }).first(),
      );
      return { tools, bar: await parentOf(page, tools) };
    },
  },
  {
    key: 'lounge',
    open: async (page) => {
      const w = world();
      await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
        waitUntil: 'domcontentloaded',
        timeout: 20_000,
      });
      await page.getByRole('heading', { level: 1 }).first().waitFor({ timeout: 15_000 });
      await page
        .getByRole('textbox', { name: /^Message/ })
        .first()
        .waitFor({ timeout: 15_000 });
      await idle(page, 5000);
      await page.evaluate(() => document.fonts?.ready);
    },
    containers: async (page) => {
      const title = await page.getByRole('heading', { level: 1 }).first().elementHandle();
      const header = title
        ? (
            await page.evaluateHandle(
              (e) => (e as Element).closest('header') ?? (e as Element).parentElement,
              title,
            )
          ).asElement()
        : null;
      if (!header) throw new Error('no channel header');
      const send = page.getByRole('button', { name: /^Send/ }).first();
      const field = page.getByRole('textbox', { name: /^Message/ }).first();
      return { header, composerRow: await lca(page, field, send) };
    },
  },
];

test(`p104 twin: Members filter row, Library toolbar and Lounge view, ${MODE}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const figs: Record<string, PageFig> = {};
  const problems: string[] = [];
  try {
    await setPrefs(owner);
    for (const c of CONFIGS) {
      for (const def of PAGES) {
        const tag = `${def.key}-${c.tag}`;
        const { ctx, page } = await newPage(browser, owner, c);
        try {
          await def.open(page);
          await page.evaluate(() => window.scrollTo(0, 0));
          const facts = await pageFacts(page);
          const containers: Record<string, ContainerFig> = {};
          const found = await def.containers(page);
          for (const [name, h] of Object.entries(found)) containers[name] = await measure(page, h);
          figs[tag] = { ...facts, containers };
          const rows = Object.entries(containers)
            .map(([n, f]) => `${n}: ${f.controls.length} controls, ${f.rows} rows`)
            .join('; ');
          say(
            `${MODE} ${tag}: theme ${facts.theme}, page ${facts.page.join('/')}, ${facts.sheets.length} sheets; ${rows}`,
          );
          if (facts.theme === null) problems.push(`${tag}: no data-theme`);
          await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}.png`), timeout: 10_000 });
        } catch (e) {
          problems.push(`${tag}: ${String(e).slice(0, 400)}`);
        } finally {
          await ctx.close();
        }
      }
    }
    writeFileSync(resolve(OUT, `figures-${MODE}.json`), JSON.stringify(figs, null, 1));
  } finally {
    await owner.close();
  }
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
