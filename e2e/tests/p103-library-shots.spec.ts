import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import {
  CONTROL_SELECTOR,
  hitTest,
  hitTestAll,
  openCollapsed,
  type ControlRow,
} from '../support/hit-test';
import { over, parseColor, ratio } from '../support/contrast';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 103, pod library (SEC-AUD-465 item 1, 468, 469; D-972, D-974): `library.css` stops styling selects by element
 * name; the Library's last five raw selects (the series page's Edition, Rankings' Kind, My library's Sort, a list's
 * visibility, the review form's rating) and the Reader's chapter picker are the kit's `NativeSelect` with
 * `fit="content"`; the Library's 14 px comes from the label (D-974), so the Browse toolbar's sort reads 14 px too.
 * Dependants measured although their file does not change: the Browse toolbar sort and the Browse filter selects, the
 * series "Your rating", the reader end card's "Rate this chapter".
 * SEC-AUD-468: the Reader's top and bottom bars are held shown (a `mousemove` on the bar every 400 ms; the bars hide
 * 3 s after the last poke), the VIEWPORT is shot at 375 and at 1024, `hitTestAll` runs over each bar and no row may be
 * "not reached", and the chevron's colour is read against the bar's ground with `e2e/support/contrast.ts` (3:1).
 *
 * Finding a select: by its label (`getByLabel` narrowed to the `select` tag), never by class. Roots (for the neighbours
 * and the hit test) are xpath ancestors of the select (`ancestor::form[1]`, `ancestor::header[1]`, ...).
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never
 * edited). Data it makes through the API under `p103-lib-<digits>` names (D-942), all removed in a `finally`: one
 * published series with two editions (Official: three chapters, the second with a long title so the picker's widest
 * option passes its 220 px cap; Fan edition: one chapter, so Edition shows) and one private reading list. The reader's
 * pictures are answered by `page.route`.
 *
 * Phases: `SR_P103_LIBRARY_PHASE=base` records the tree as it is (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (every select that `moved` below must be the kit's after; the rest must not change) and writes `compare.json`.
 * Configs: 1024 x 768 and 375 x 812 touch in `sectionThemes.library = 'sideroom'` (the shipped theme) and 1024 in
 * `'lounge-light'`, set by PATCH and localStorage and read back from the room's root `data-theme`.
 *
 * Local only (SR_P103_LIBRARY_ON=1). SR_P103_LIBRARY_OUT is the folder of the shots and the figures.
 * The commander's command, from e2e/, on a stack with the sweep's world (`SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL`
 * and `SR_CLI_BIN` set to the stack's database and sr-cli), for each phase:
 *   SR_E2E_BASE_URL=http://localhost:5812 SR_P103_LIBRARY_ON=1 SR_P103_LIBRARY_PHASE=base
 *   SR_P103_LIBRARY_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-library
 *   npx playwright test tests/p103-library-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (the base on the tree before the pod's branches are merged; then the same without PHASE=base on the merged tree.)
 */
const ON = process.env.SR_P103_LIBRARY_ON === '1';
const PHASE = process.env.SR_P103_LIBRARY_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-library';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const API = '/api/v1';
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PIXEL =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==';

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  section: 'sideroom' | 'lounge-light';
  size: string;
  keyboard: boolean;
  hit: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-sideroom',
    width: 1024,
    height: 768,
    touch: false,
    section: 'sideroom',
    size: '1024',
    keyboard: true,
    hit: true,
  },
  {
    tag: '375-sideroom',
    width: 375,
    height: 812,
    touch: true,
    section: 'sideroom',
    size: '375',
    keyboard: false,
    hit: true,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    section: 'lounge-light',
    size: '1024',
    keyboard: false,
    hit: false,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

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

/* ------------------------------------------------------------------------------------------- the select list */

interface SelDef {
  /** The figure's key. */
  key: string;
  file: string;
  builder: 'shelves' | 'reader' | 'already the kit';
  /** The select's label text (`getByLabel`, substring) narrowed to the `select` tag. */
  label: string;
  /** Whether the after phase expects this select to have become the kit's `NativeSelect` (wrapped in a span). */
  moved: boolean;
  /** `content` (the base's raw select was as wide as its content) or `fill`. */
  fit: 'content' | 'fill';
  /** xpath from the select to the root of its neighbours and its hit test. */
  root: string;
  /** Hit-test this select's root (selects that share a root test it once). */
  hit: boolean;
  /** Driven by the keyboard (focus, ArrowDown changes the value, ArrowUp puts it back). */
  keyboard?: boolean;
  /** Found inside the open dialog (the filter drawer's page behind it holds a select whose options say "Rating"). */
  within?: 'dialog';
  /** An expected computed font size, after (D-974). */
  font14?: boolean;
}
interface Group {
  key: string;
  /** Shot name stem; the shot is `<stem>-<size>-<theme>`. */
  open: (page: Page, ids: Ids) => Promise<void>;
  selects: SelDef[];
}
interface Ids {
  seriesId: string;
  chapterId: string;
  chapter2Id: string;
  listId: string;
}

const GROUPS: Group[] = [
  {
    key: 'browse-sort',
    open: async (page) => {
      await page.goto('/library/browse', { waitUntil: 'domcontentloaded' });
      await page.getByLabel('Sort by').and(page.locator('select')).first().waitFor();
      await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
      await page.waitForTimeout(800);
    },
    selects: [
      {
        key: 'browse-sort',
        file: 'library/Browse.tsx (toolbar sort)',
        builder: 'already the kit',
        label: 'Sort by',
        moved: false,
        fit: 'fill',
        root: 'ancestor::label[1]/../..',
        hit: true,
        keyboard: true,
        font14: true,
      },
    ],
  },
  {
    key: 'browse-filters',
    open: async (page) => {
      await page.goto('/library/browse', { waitUntil: 'domcontentloaded' });
      await page
        .getByRole('button', { name: /^Filters/ })
        .first()
        .click();
      await selectOf(page, 'Status', 'dialog').waitFor();
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'filters-status',
        file: 'library/Browse.tsx (filter drawer)',
        builder: 'already the kit',
        label: 'Status',
        within: 'dialog',
        moved: false,
        fit: 'fill',
        root: "ancestor::*[@role='dialog'][1]",
        hit: true,
        font14: true,
      },
      {
        key: 'filters-rating',
        file: 'library/Browse.tsx (filter drawer)',
        builder: 'already the kit',
        label: 'Rating',
        within: 'dialog',
        moved: false,
        fit: 'fill',
        root: "ancestor::*[@role='dialog'][1]",
        hit: false,
        font14: true,
      },
      {
        key: 'filters-age',
        file: 'library/Browse.tsx (filter drawer)',
        builder: 'already the kit',
        label: 'Age-restricted titles',
        within: 'dialog',
        moved: false,
        fit: 'fill',
        root: "ancestor::*[@role='dialog'][1]",
        hit: false,
        font14: true,
      },
    ],
  },
  {
    key: 'series',
    open: async (page, ids) => {
      await page.goto(`/library/s/${ids.seriesId}`, { waitUntil: 'domcontentloaded' });
      await page.getByLabel('Your rating').and(page.locator('select')).first().waitFor();
      await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
      await page.waitForTimeout(800);
    },
    selects: [
      {
        key: 'series-edition',
        file: 'library/SeriesPage.tsx',
        builder: 'shelves',
        label: 'Edition',
        moved: true,
        fit: 'content',
        root: 'ancestor::header[1]',
        hit: true,
        keyboard: true,
        font14: true,
      },
      {
        key: 'series-your-rating',
        file: 'library/SeriesSocial.tsx (Your rating)',
        builder: 'already the kit',
        label: 'Your rating',
        moved: false,
        fit: 'fill',
        root: 'ancestor::section[1]',
        hit: true,
        font14: true,
      },
    ],
  },
  {
    key: 'review',
    open: async (page, ids) => {
      await page.goto(`/library/s/${ids.seriesId}`, { waitUntil: 'domcontentloaded' });
      await page.getByRole('tab', { name: 'Reviews' }).click();
      await page.getByRole('button', { name: /Write a review|Edit your review/ }).click();
      await page
        .getByLabel('Rating with this review')
        .and(page.locator('select'))
        .first()
        .waitFor();
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'review-rating',
        file: 'library/SeriesSocial.tsx (review form)',
        builder: 'shelves',
        label: 'Rating with this review',
        moved: true,
        fit: 'content',
        root: 'ancestor::form[1]',
        hit: true,
        keyboard: true,
        font14: true,
      },
    ],
  },
  {
    key: 'rankings',
    open: async (page) => {
      await page.goto('/library/rankings', { waitUntil: 'domcontentloaded' });
      await page.getByLabel('Kind').and(page.locator('select')).first().waitFor();
      await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'rankings-kind',
        file: 'library/Rankings.tsx',
        builder: 'shelves',
        label: 'Kind',
        moved: true,
        fit: 'content',
        root: 'ancestor::label[1]/../..',
        hit: true,
        keyboard: true,
        font14: true,
      },
    ],
  },
  {
    key: 'mylibrary',
    open: async (page) => {
      await page.goto('/library/me', { waitUntil: 'domcontentloaded' });
      await page.getByLabel('Sort by').and(page.locator('select')).first().waitFor();
      await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'mylibrary-sort',
        file: 'library/Pages.tsx',
        builder: 'shelves',
        label: 'Sort by',
        moved: true,
        fit: 'content',
        root: 'ancestor::label[1]/..',
        hit: true,
        keyboard: true,
        font14: true,
      },
    ],
  },
  {
    key: 'lists-new',
    open: async (page) => {
      await page.goto('/library/lists', { waitUntil: 'domcontentloaded' });
      await page.getByRole('button', { name: 'New list' }).first().click();
      await page.getByLabel('Who can see it').and(page.locator('select')).first().waitFor();
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'lists-new-visibility',
        file: 'library/Lists.tsx (new list form)',
        builder: 'shelves',
        label: 'Who can see it',
        moved: true,
        fit: 'content',
        root: 'ancestor::form[1]',
        hit: true,
        keyboard: true,
        font14: true,
      },
    ],
  },
  {
    key: 'lists-edit',
    open: async (page, ids) => {
      await page.goto(`/library/lists/${ids.listId}`, { waitUntil: 'domcontentloaded' });
      await page.getByRole('button', { name: 'Edit list' }).first().click();
      await page.getByLabel('Who can see it').and(page.locator('select')).first().waitFor();
      await page.waitForTimeout(600);
    },
    selects: [
      {
        key: 'lists-edit-visibility',
        file: 'library/Lists.tsx (edit list form)',
        builder: 'shelves',
        label: 'Who can see it',
        moved: true,
        fit: 'content',
        root: 'ancestor::form[1]',
        hit: true,
        font14: true,
      },
    ],
  },
];

/** The Reader's chapter picker and the end card's rating, shot in their own scenarios below. */
const PICKER: SelDef = {
  key: 'reader-chapter',
  file: 'library/reader/Reader.tsx',
  builder: 'reader',
  label: 'Chapter',
  moved: true,
  fit: 'content',
  root: 'ancestor::header[1]',
  hit: false,
  keyboard: true,
};
const END_RATE: SelDef = {
  key: 'chapter-rate',
  file: 'library/SeriesSocial.tsx (reader end card)',
  builder: 'already the kit',
  label: 'Rate this chapter',
  moved: false,
  fit: 'fill',
  root: 'ancestor::section[1]',
  hit: true,
  font14: true,
};

/* ----------------------------------------------------------------------------------------------- measuring */

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Neighbour {
  key: string;
  box: Box;
}
interface SelFig {
  key: string;
  file: string;
  builder: string;
  moved: boolean;
  fit: string;
  /** True when the select sits in the kit's wrapper span (it is the kit's `NativeSelect`). */
  wrapped: boolean;
  box: Box;
  wrap: Box | null;
  label: { text: string; lines: number; box: Box | null; name: string | null };
  fontSize: string;
  padStart: number;
  padEnd: number;
  /** Width of the selected option's text (canvas, the select's own font). */
  textW: number;
  selectedText: string;
  /** The room between the end of the text and the chevron's start edge (null for a raw select). */
  clear: number | null;
  chevron: { color: string; ground: string; ratio: number; box: Box } | null;
  root: Box;
  neighbours: Neighbour[];
  row: Neighbour[];
  overlaps: string[];
  scroll: [number, number];
  dataTheme: string | null;
  hit: HitSummary | null;
  extra: Record<string, unknown>;
}
interface HitSummary {
  pass: number;
  fail: number;
  notReached: number;
  skipped: number;
  rows: { key: string; status: string; reason?: string; takers: string[] }[];
  opened: unknown[];
  /** A control whose +-21 px point lies off the page (a bar at the viewport's edge), re-tested with a shorter reach. */
  offPage: { key: string; reach: number; pass: boolean; takers: string[] }[];
}

async function figure(sel: SelDef, locator: Locator): Promise<SelFig> {
  const m = await locator.evaluate(
    (el, args: { root: string }) => {
      const s = el as HTMLSelectElement;
      const rnd = (n: number) => Math.round(n * 10) / 10;
      const rootEl = document.evaluate(
        args.root,
        s,
        null,
        XPathResult.FIRST_ORDERED_NODE_TYPE,
        null,
      ).singleNodeValue as HTMLElement | null;
      const root = rootEl ?? document.body;
      const ro = root.getBoundingClientRect();
      const ox = ro.left;
      const oy = ro.top;
      const box = (e: Element): { x: number; y: number; w: number; h: number } => {
        const r = e.getBoundingClientRect();
        return { x: rnd(r.left - ox), y: rnd(r.top - oy), w: rnd(r.width), h: rnd(r.height) };
      };
      const visible = (e: Element) => e.getClientRects().length > 0;
      const cs = getComputedStyle(s);
      const label = s.closest('label');
      const wrapperEl =
        s.parentElement && s.parentElement !== label && s.parentElement.tagName === 'SPAN'
          ? s.parentElement
          : null;
      const span = label
        ? (Array.from(label.children).find((c) => c.tagName === 'SPAN' && !c.contains(s)) ?? null)
        : null;
      let lines = 0;
      if (span) {
        const rg = document.createRange();
        rg.selectNodeContents(span);
        lines = rg.getClientRects().length;
      }
      const selected = s.selectedOptions[0]?.textContent ?? '';
      const canvas = document.createElement('canvas').getContext('2d')!;
      canvas.font = `${cs.fontStyle} ${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const textW = rnd(canvas.measureText(selected).width);
      const sr = s.getBoundingClientRect();
      const padStart = parseFloat(cs.paddingInlineStart);
      const padEnd = parseFloat(cs.paddingInlineEnd);
      let clear: number | null = null;
      let chevron: {
        color: string;
        ground: string;
        box: { x: number; y: number; w: number; h: number };
      } | null = null;
      if (wrapperEl) {
        const wr = wrapperEl.getBoundingClientRect();
        const ac = getComputedStyle(wrapperEl, '::after');
        // The kit draws a 6 px box 14 px from the inline end, rotated 45 degrees: its drawing reaches about 4.3 px
        // either side of its centre, so its start edge is 14 + 3 + 4.3 = about 21.3 px from the wrapper's end.
        const chevLeft = wr.right - 22;
        const textEnd = sr.left + parseFloat(cs.borderInlineStartWidth) + padStart + textW;
        clear = rnd(chevLeft - textEnd);
        chevron = {
          color: ac.borderRightColor,
          ground: cs.backgroundColor,
          box: { x: rnd(chevLeft - ox), y: rnd(wr.top + wr.height / 2 - 4 - oy), w: 9, h: 9 },
        };
      }
      const els = Array.from(
        root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
      ).filter(visible);
      const key = (e: HTMLElement, i: number) =>
        `${i}:${e.tagName.toLowerCase()}#${e.id || ''}:${(e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24)}`;
      const neighbours = els.map((e, i) => ({ key: key(e, i), box: box(e) }));
      const row = neighbours.filter((n) => {
        const b = box(s);
        return n.box.y < b.y + b.h - 1 && n.box.y + n.box.h > b.y + 1;
      });
      const overlaps: string[] = [];
      for (const o of els) {
        if (o === s || s.contains(o) || o.contains(s)) continue;
        const b = o.getBoundingClientRect();
        const ix = Math.min(sr.right, b.right) - Math.max(sr.left, b.left);
        const iy = Math.min(sr.bottom, b.bottom) - Math.max(sr.top, b.top);
        if (ix > 1 && iy > 1)
          overlaps.push(
            `${o.tagName.toLowerCase()} "${(o.textContent ?? '').trim().slice(0, 20)}" (${Math.round(ix)}x${Math.round(iy)})`,
          );
      }
      const barBg = (() => {
        const bar = s.closest('header, footer');
        return bar ? getComputedStyle(bar).backgroundColor : null;
      })();
      const stage = s.closest('[data-bg]');
      return {
        wrapped: !!wrapperEl,
        box: box(s),
        wrap: wrapperEl ? box(wrapperEl) : null,
        label: {
          text: (span?.textContent ?? '').trim(),
          lines,
          box: label ? box(label) : null,
          name: s.labels?.[0]?.textContent?.trim() ?? null,
        },
        fontSize: cs.fontSize,
        padStart,
        padEnd,
        textW,
        selectedText: selected,
        clear,
        chevron,
        root: box(root),
        neighbours,
        row,
        overlaps,
        scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth] as [
          number,
          number,
        ],
        dataTheme: s.closest('[data-theme]')?.getAttribute('data-theme') ?? null,
        extra: {
          barBg,
          stageBg: stage ? getComputedStyle(stage).backgroundColor : null,
          selectBg: cs.backgroundColor,
          selectColor: cs.color,
          selectBorder: `${cs.borderTopWidth} ${cs.borderTopColor}`,
          maxInlineSize: cs.maxWidth,
        },
      };
    },
    { root: sel.root },
  );
  const fig: SelFig = {
    key: sel.key,
    file: sel.file,
    builder: sel.builder,
    moved: sel.moved,
    fit: sel.fit,
    wrapped: m.wrapped,
    box: m.box,
    wrap: m.wrap,
    label: m.label,
    fontSize: m.fontSize,
    padStart: m.padStart,
    padEnd: m.padEnd,
    textW: m.textW,
    selectedText: m.selectedText,
    clear: m.clear,
    chevron: null,
    root: m.root,
    neighbours: m.neighbours,
    row: m.row,
    overlaps: m.overlaps,
    scroll: m.scroll,
    dataTheme: m.dataTheme,
    hit: null,
    extra: m.extra,
  };
  if (m.chevron) {
    const ground = parseColor(m.chevron.ground);
    fig.chevron = {
      color: m.chevron.color,
      ground: m.chevron.ground,
      ratio: Math.round(ratio(over(parseColor(m.chevron.color), ground), ground) * 100) / 100,
      box: m.chevron.box,
    };
    const barBg = m.extra['barBg'] as string | null;
    const stageBg = m.extra['stageBg'] as string | null;
    if (barBg && stageBg) {
      const g = over(parseColor(barBg), over(parseColor(stageBg), { r: 0, g: 0, b: 0, a: 1 }));
      fig.extra['chevronOnBar'] =
        Math.round(ratio(over(parseColor(m.chevron.color), g), g) * 100) / 100;
    }
  }
  return fig;
}

async function hitOver(page: Page, root: Locator): Promise<HitSummary> {
  const opened = await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root);
  const offPage: HitSummary['offPage'] = [];
  for (const r of rows.filter(
    (x) => x.status === 'not-reached' && (x.reason ?? '').includes('off the page'),
  )) {
    const loc = root.locator(CONTROL_SELECTOR).nth(r.index);
    const reach = 12;
    try {
      const res = await hitTest(page, loc, { reach });
      offPage.push({
        key: `${r.tag} "${r.name}"`,
        reach,
        pass: res.pass,
        takers: res.points.filter((p) => !p.ok).map((p) => `${p.at}: ${p.taker?.tag}`),
      });
      r.status = 'skipped';
      r.reason = `off the page at 21 px; tested at ${reach} px: ${res.pass ? 'pass' : 'fail'}`;
    } catch {
      /* stays not-reached */
    }
  }
  return {
    pass: rows.filter((r) => r.status === 'pass').length,
    fail: rows.filter((r) => r.status === 'fail').length,
    notReached: rows.filter((r) => r.status === 'not-reached').length,
    skipped: rows.filter((r) => r.status === 'skipped').length,
    rows: rows.map((r) => ({
      key: `${r.tag} "${r.name}"`,
      status: r.status,
      reason: r.reason,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}: ${p.taker ? `${p.taker.tag} "${p.taker.name}" ${p.taker.path}` : 'null'}`,
        ),
    })),
    opened,
    offPage,
  };
}

/* --------------------------------------------------------------------------------------------- page set-up */

async function setPrefs(ctx: BrowserContext, section: string) {
  await apiPatch(ctx.request, '/users/@me/settings', {
    settings: {
      user_prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: section } },
    },
  });
  const back = (await (await ctx.request.get(`${API}/users/@me/settings`)).json()) as {
    settings?: { user_prefs?: { sectionThemes?: Record<string, string> } };
  };
  expect(back.settings?.user_prefs?.sectionThemes?.['library']).toBe(section);
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((sec) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({
          state: { prefs: { theme: 'lounge-dark', zoom: 100, sectionThemes: { library: sec } } },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  }, c.section);
  await ctx.route('**/cdn/p103-*.png', (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: Buffer.from(PIXEL, 'base64') }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
    const res = await r.fetch();
    if (!res.ok()) return r.fulfill({ response: res });
    const body = (await res.json()) as { pages: { url: string }[] };
    body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p103-p${i}.png`));
    return r.fulfill({ response: res, json: body });
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

/** The room's `data-theme` (the main region's closest ancestor that carries one), read back and checked. */
async function theme(page: Page, c: Cfg, problems: string[]) {
  const dt = await page
    .getByRole('main')
    .first()
    .evaluate((e) => e.closest('[data-theme]')?.getAttribute('data-theme') ?? null);
  say(`${c.tag}: data-theme ${dt}`);
  if (dt !== c.section) problems.push(`${c.tag}: data-theme is ${dt}, wanted ${c.section}`);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(500);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

const selectOf = (page: Page, label: string, within?: 'dialog') =>
  (within ? page.getByRole('dialog') : page).getByLabel(label).and(page.locator('select')).first();

/** Focus, ArrowDown changes the value, ArrowUp puts it back; what the page showed before and after. */
async function keyboardDrive(page: Page, sel: Locator, sig: () => Promise<string>) {
  await page.keyboard.press('Shift');
  await sel.focus();
  const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
  const before = await sel.inputValue();
  const sigBefore = await sig();
  await page.keyboard.press('ArrowDown');
  await expect(sel).not.toHaveValue(before);
  const changed = await sel.inputValue();
  await page.waitForTimeout(700);
  const reacted = (await sig()) !== sigBefore;
  await sel.focus();
  await page.keyboard.press('ArrowUp');
  await expect(sel).toHaveValue(before);
  await page.waitForTimeout(400);
  return { focusVisible, before, changed, reacted, back: await sel.inputValue() };
}

type Store = Record<string, SelFig | { missing: string }>;

async function runGroup(page: Page, c: Cfg, g: Group, ids: Ids, store: Store, problems: string[]) {
  try {
    await g.open(page, ids);
    await theme(page, c, problems);
  } catch (e) {
    for (const s of g.selects)
      store[s.key] = { missing: `not reached: ${String(e).slice(0, 200)}` };
    problems.push(`${c.tag} ${g.key}: not reached: ${String(e).slice(0, 200)}`);
    return;
  }
  let firstShot = true;
  for (const s of g.selects) {
    const loc = selectOf(page, s.label, s.within);
    try {
      await loc.scrollIntoViewIfNeeded();
      const fig = await figure(s, loc);
      if (c.hit && s.hit) {
        const root = loc.locator(`xpath=${s.root}`);
        fig.hit = await hitOver(page, root);
        say(
          `${c.tag} ${s.key}: hit ${fig.hit.pass}/${fig.hit.fail}/${fig.hit.notReached}/${fig.hit.skipped}`,
        );
        if (fig.hit.notReached)
          problems.push(
            `${c.tag} ${s.key}: ${fig.hit.notReached} controls not reached (probe error)`,
          );
        // opening a collapsed card moves things: measure again afterwards for the boxes
      }
      if (fig.scroll[0] > fig.scroll[1])
        problems.push(`${c.tag} ${s.key}: the page scrolls sideways ${fig.scroll.join('/')}`);
      store[s.key] = fig;
      say(
        `${c.tag} ${s.key}: wrapped ${fig.wrapped} box ${JSON.stringify(fig.box)} font ${fig.fontSize} clear ${fig.clear} label ${JSON.stringify(fig.label)}`,
      );
      if (firstShot) {
        await loc.scrollIntoViewIfNeeded();
        await shot(page, `${g.key}-${c.size}-${c.section === 'sideroom' ? 'sideroom' : 'light'}`);
        firstShot = false;
      }
    } catch (e) {
      store[s.key] = { missing: `not reached: ${String(e).slice(0, 200)}` };
      problems.push(`${c.tag} ${s.key}: not reached: ${String(e).slice(0, 200)}`);
    }
  }
  // keyboard, last (it changes values and puts them back)
  if (c.keyboard) {
    for (const s of g.selects.filter((x) => x.keyboard)) {
      const fig = store[s.key];
      if (!fig || 'missing' in fig) continue;
      try {
        const k = await keyboardDrive(page, selectOf(page, s.label, s.within), async () =>
          (
            await page
              .locator('main')
              .first()
              .innerText()
              .catch(() => '')
          )
            .replace(/\s+/g, ' ')
            .slice(0, 4000),
        );
        fig.extra['keyboard'] = k;
        say(`${c.tag} ${s.key}: keyboard ${JSON.stringify(k)}`);
        if (!k.focusVisible)
          problems.push(`${c.tag} ${s.key}: :focus-visible did not match after a key`);
      } catch (e) {
        problems.push(`${c.tag} ${s.key}: keyboard: ${String(e).slice(0, 300)}`);
      }
    }
  }
}

/* ------------------------------------------------------------------------------------------------ the reader */

/** Keeps the Reader's bars shown: a `mousemove` on the top bar every 400 ms bubbles to the root, which pokes. */
async function holdBars(page: Page) {
  await page.evaluate(() => {
    const w = window as unknown as { __holdBars?: number };
    if (w.__holdBars) return;
    const poke = () =>
      document
        .querySelector('header[aria-label]')
        ?.dispatchEvent(new MouseEvent('mousemove', { bubbles: true }));
    poke();
    w.__holdBars = window.setInterval(poke, 400);
  });
}

async function barsShown(page: Page) {
  return page.evaluate(() => {
    const h = document.querySelector('header[aria-label]');
    const f = document.querySelector('header[aria-label] ~ footer');
    const ok = (e: Element | null) =>
      !!e && getComputedStyle(e).opacity === '1' && getComputedStyle(e).pointerEvents !== 'none';
    return { top: ok(h), bottom: ok(f) };
  });
}

async function runReader(
  page: Page,
  c: Cfg,
  ids: Ids,
  store: Store,
  problems: string[],
): Promise<void> {
  const tag = `${c.tag} reader`;
  try {
    await page.goto(`/library/r/${ids.chapterId}/1`, { waitUntil: 'domcontentloaded' });
    const picker = selectOf(page, PICKER.label);
    await picker.waitFor({ state: 'visible', timeout: 30_000 });
    await holdBars(page);
    await theme(page, c, problems);
    await page.waitForTimeout(700);
    const shown = await barsShown(page);
    if (!shown.top || !shown.bottom)
      problems.push(`${tag}: the bars are not shown (${JSON.stringify(shown)})`);
    const fig = await figure(PICKER, picker);
    const top = page.locator('header[aria-label]').first();
    const bottom = page.locator('header[aria-label] ~ footer').first();
    const topHit = await hitOver(page, top);
    const bottomHit = await hitOver(page, bottom);
    const after = await barsShown(page);
    if (!after.top || !after.bottom)
      problems.push(`${tag}: the bars hid during the hit test (${JSON.stringify(after)})`);
    // the same hit tests are recorded on the figure: top bar in `hit`, bottom bar in `extra`
    fig.hit = topHit;
    fig.extra['bottomBar'] = bottomHit;
    fig.extra['bars'] = { before: shown, after };
    fig.extra['barBoxes'] = {
      top: await top.boundingBox(),
      bottom: await bottom.boundingBox(),
    };
    for (const [bar, h] of [
      ['top bar', topHit],
      ['bottom bar', bottomHit],
    ] as const) {
      say(`${tag}: ${bar} hit ${h.pass}/${h.fail}/${h.notReached}/${h.skipped}`);
      if (h.notReached)
        problems.push(`${tag}: ${bar}: ${h.notReached} controls not reached (probe error)`);
    }
    if (fig.scroll[0] > fig.scroll[1])
      problems.push(`${tag}: the page scrolls sideways ${fig.scroll.join('/')}`);
    store[PICKER.key] = fig;
    say(
      `${tag}: picker ${JSON.stringify({ box: fig.box, wrapped: fig.wrapped, clear: fig.clear, chevron: fig.chevron, onBar: fig.extra['chevronOnBar'] })}`,
    );
    await holdBars(page);
    await page.waitForTimeout(300);
    await shot(page, `reader-bars-${c.size}-${c.section === 'sideroom' ? 'sideroom' : 'light'}`);

    if (c.keyboard) {
      const url0 = page.url();
      await page.keyboard.press('Shift');
      await picker.focus();
      const focusVisible = await picker.evaluate((e) => e.matches(':focus-visible'));
      const before = await picker.inputValue();
      await page.keyboard.press('ArrowDown');
      await expect.poll(() => page.url(), { timeout: 10_000 }).not.toBe(url0);
      const url1 = page.url();
      const k = { focusVisible, before, urlChanged: url1 !== url0, url1 };
      fig.extra['keyboard'] = k;
      say(`${tag}: picker keyboard ${JSON.stringify(k)}`);
      if (!focusVisible) problems.push(`${tag}: :focus-visible did not match after a key`);
      // put it back: the chapter picker of the second chapter, ArrowUp
      const back = selectOf(page, PICKER.label);
      await back.waitFor({ state: 'visible', timeout: 15_000 });
      await holdBars(page);
      await back.focus();
      await page.keyboard.press('ArrowUp');
      await expect.poll(() => page.url(), { timeout: 10_000 }).not.toBe(url1);
    }
  } catch (e) {
    store[PICKER.key] = { missing: `not reached: ${String(e).slice(0, 200)}` };
    problems.push(`${tag}: not reached: ${String(e).slice(0, 300)}`);
  }

  // ---- the end card's rating
  try {
    await page.goto(`/library/r/${ids.chapterId}/1`, { waitUntil: 'domcontentloaded' });
    let end = false;
    for (let i = 0; i < 8 && !end; i++) {
      end = await selectOf(page, END_RATE.label)
        .isVisible()
        .catch(() => false);
      if (!end) {
        for (let k = 0; k < 3; k++) await page.keyboard.press('ArrowRight');
        await page.waitForTimeout(800);
      }
    }
    if (!end) throw new Error('the end card did not show a Rate this chapter select');
    await page.waitForTimeout(600);
    const loc = selectOf(page, END_RATE.label);
    const fig = await figure(END_RATE, loc);
    if (c.hit) fig.hit = await hitOver(page, loc.locator(`xpath=${END_RATE.root}`));
    if (fig.scroll[0] > fig.scroll[1])
      problems.push(`${c.tag} chapter-rate: the page scrolls sideways ${fig.scroll.join('/')}`);
    store[END_RATE.key] = fig;
    await shot(page, `reader-end-${c.size}-${c.section === 'sideroom' ? 'sideroom' : 'light'}`);
  } catch (e) {
    store[END_RATE.key] = { missing: `not reached: ${String(e).slice(0, 200)}` };
    problems.push(`${c.tag} chapter-rate: not reached: ${String(e).slice(0, 300)}`);
  }
}

async function runConfig(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  ids: Ids,
  problems: string[],
): Promise<Store> {
  await setPrefs(owner, c.section);
  const { ctx, page } = await newPage(browser, owner, c);
  const store: Store = {};
  try {
    for (const g of GROUPS) await runGroup(page, c, g, ids, store, problems);
    await runReader(page, c, ids, store, problems);
  } finally {
    await ctx.close();
  }
  return store;
}

/* -------------------------------------------------------------------------------------------------- compare */

const r1 = (n: number) => Math.round(n * 10) / 10;
const ALL: SelDef[] = [...GROUPS.flatMap((g) => g.selects), PICKER, END_RATE];

function compare(c: Cfg, base: Store, after: Store, problems: string[]) {
  const out: Record<string, unknown> = {};
  const min = c.touch ? 44 : 40;
  for (const def of ALL) {
    const a = after[def.key];
    const b = base[def.key];
    const tag = `${c.tag} ${def.key}`;
    if (!a || !b) continue;
    if ('missing' in a || 'missing' in b) {
      if ('missing' in a !== 'missing' in b) problems.push(`${tag}: reached in only one phase`);
      continue;
    }
    const d: Record<string, unknown> = {};
    // moved or not
    if (a.wrapped !== (def.moved ? true : b.wrapped))
      problems.push(
        `${tag}: ${def.moved ? "expected the kit's select, found a raw one" : 'expected as it was'} (wrapped ${a.wrapped})`,
      );
    if (def.moved && b.wrapped) problems.push(`${tag}: the base was already the kit's select`);
    d['box'] = { base: b.box, after: a.box };
    d['wrapped'] = { base: b.wrapped, after: a.wrapped };
    // height
    if (a.box.h < min - 0.5) problems.push(`${tag}: ${a.box.h} px tall, under ${min}`);
    if (a.box.h < b.box.h - 0.5)
      problems.push(`${tag}: shorter than the base (${a.box.h} < ${b.box.h})`);
    // width
    if (def.fit === 'fill' || !def.moved) {
      // D-974: the type size goes from 16 to 14 px, so a select as wide as its text may shrink by the text's difference
      const shrink = Math.max(0, b.textW - a.textW);
      d['allowedShrink'] = r1(shrink);
      if (a.box.w < b.box.w - shrink - 0.5)
        problems.push(
          `${tag}: narrower than the base (${a.box.w} < ${b.box.w}, the type change allows ${r1(shrink)})`,
        );
      if (!def.moved && a.box.w > b.box.w + 0.5)
        problems.push(
          `${tag}: a select that did not change is ${a.box.w} px wide, base ${b.box.w}`,
        );
    } else {
      if (a.clear !== null && a.clear < 0)
        problems.push(
          `${tag}: the text "${a.selectedText}" runs into the chevron (clear ${a.clear})`,
        );
      // the selected text must fit the select's text area (an ellipsis means it is cut)
      const room = r1(a.box.w - 2 - a.padStart - a.padEnd - a.textW);
      d['textRoom'] = room;
      if (room < -0.05)
        problems.push(
          `${tag}: the text "${a.selectedText}" (${a.textW} px at ${a.fontSize}) is cut: the text area is ${r1(a.box.w - 2 - a.padStart - a.padEnd)} px (the base's text was ${b.textW} px at ${b.fontSize})`,
        );
      if (a.box.w > b.box.w + 36 + 0.5)
        problems.push(`${tag}: ${a.box.w} px wide, base ${b.box.w}: more than 36 px wider`);
      if (def.key === 'reader-chapter') {
        if (!c.touch && a.box.w > 220.5)
          problems.push(`${tag}: ${a.box.w} px wide at 1024, over the 220 px cap`);
        if (c.touch && a.box.w < b.box.w - 36 - 0.5)
          problems.push(
            `${tag}: ${a.box.w} px at 375, narrower than the base ${b.box.w} by more than 36`,
          );
      }
    }
    // overlaps the base did not have
    for (const o of a.overlaps)
      if (!b.overlaps.includes(o)) problems.push(`${tag}: new overlap with ${o}`);
    // type size
    if (def.font14 && a.fontSize !== '14px')
      problems.push(`${tag}: font-size ${a.fontSize}, wanted 14px (D-974)`);
    // label
    if (a.label.lines > b.label.lines)
      problems.push(
        `${tag}: label "${a.label.text}" wraps (${b.label.lines} -> ${a.label.lines} lines)`,
      );
    if (a.label.text && !(a.label.name ?? '').startsWith(a.label.text))
      problems.push(`${tag}: label "${a.label.text}" is not the name (${a.label.name})`);
    // the root and its neighbours
    const dh = r1(a.box.h - b.box.h);
    const growth = r1(a.root.h - b.root.h);
    d['root'] = { base: b.root.h, after: a.root.h, growth, selectDh: dh };
    if (a.neighbours.length !== b.neighbours.length) {
      problems.push(`${tag}: control count ${b.neighbours.length} -> ${a.neighbours.length}`);
    } else {
      const moves: unknown[] = [];
      const selBottom = b.box.y + b.box.h;
      b.neighbours.forEach((bn, i) => {
        const an = a.neighbours[i]!;
        if (bn.key.includes(':select#')) return;
        const dy = r1(an.box.y - bn.box.y);
        const dx = r1(an.box.x - bn.box.x);
        const below = bn.box.y >= selBottom - 0.5;
        const inRow = b.row.some((r) => r.key === bn.key);
        if (dy !== 0 || dx !== 0) moves.push({ key: bn.key, dx, dy, below, inRow });
        if (below && (dy < -1 || dy > Math.max(growth, 0) + 1))
          problems.push(`${tag}: ${bn.key} (below) moved ${dy}, the root grew ${growth}`);
        if (!below && (dy < -1 || dy > Math.max(dh, 0) + 1))
          problems.push(`${tag}: ${bn.key} (above or level) moved ${dy}, the select grew ${dh}`);
        const dw = Math.abs(a.box.w - b.box.w);
        if (inRow && Math.abs(dx) > dw + 1)
          problems.push(
            `${tag}: ${bn.key} (same row) moved ${dx} across, the select changed ${r1(dw)}`,
          );
      });
      d['moves'] = moves;
    }
    // the browse toolbar: no taller than its base
    if (def.key === 'browse-sort') {
      if (growth > 0.5) problems.push(`${tag}: the toolbar is ${growth} px taller than its base`);
      if (a.box.w < 152 - 0.5) problems.push(`${tag}: the sort is ${a.box.w} px wide, under 152`);
    }
    // the rating rows (D-966): not one px taller than the base
    if (['series-your-rating', 'chapter-rate'].includes(def.key) && growth > 0.5)
      problems.push(`${tag}: the rating row grew ${growth} px against this wave's base`);
    // hit tests
    if (a.hit && b.hit) {
      const sum = (h: HitSummary) => h.pass + h.fail + h.notReached + h.skipped;
      d['hit'] = {
        base: [b.hit.pass, b.hit.fail, b.hit.notReached, b.hit.skipped],
        after: [a.hit.pass, a.hit.fail, a.hit.notReached, a.hit.skipped],
      };
      if (sum(a.hit) !== sum(b.hit))
        problems.push(`${tag}: hit test count ${sum(b.hit)} -> ${sum(a.hit)}`);
      if (a.hit.fail > b.hit.fail || a.hit.pass < b.hit.pass)
        problems.push(
          `${tag}: hit test pass/fail ${b.hit.pass}/${b.hit.fail} -> ${a.hit.pass}/${a.hit.fail}; takers ${JSON.stringify(a.hit.rows.filter((r) => r.status === 'fail'))}`,
        );
      const worse = a.hit.rows.filter(
        (r) => r.status === 'fail' && b.hit!.rows.find((x) => x.key === r.key)?.status === 'pass',
      );
      if (worse.length) problems.push(`${tag}: rows worse than the base: ${JSON.stringify(worse)}`);
    }
    // the reader's chrome
    if (def.key === 'reader-chapter') {
      const ab = a.extra['bottomBar'] as HitSummary | undefined;
      const bb = b.extra['bottomBar'] as HitSummary | undefined;
      if (ab && bb) {
        d['bottomHit'] = {
          base: [bb.pass, bb.fail, bb.notReached],
          after: [ab.pass, ab.fail, ab.notReached],
        };
        if (ab.fail > bb.fail || ab.pass < bb.pass)
          problems.push(
            `${tag}: bottom bar hit ${bb.pass}/${bb.fail} -> ${ab.pass}/${ab.fail}; takers ${JSON.stringify(ab.rows.filter((r) => r.status === 'fail'))}`,
          );
      }
    }
    // the keyboard behaves as before
    const ka = a.extra['keyboard'] as Record<string, unknown> | undefined;
    const kb = b.extra['keyboard'] as Record<string, unknown> | undefined;
    if (ka && kb && (ka['reacted'] !== kb['reacted'] || ka['urlChanged'] !== kb['urlChanged']))
      problems.push(
        `${tag}: keyboard behaved differently: base ${JSON.stringify(kb)} after ${JSON.stringify(ka)}`,
      );
    out[def.key] = d;
  }
  return out;
}

/** The reader chevron against the bar (SEC-AUD-468): at least 3:1, whatever the phase (the base has none). */
function chevronChecks(c: Cfg, s: Store, problems: string[]) {
  const f = s[PICKER.key];
  if (!f || 'missing' in f || !f.chevron) return;
  const onBar = f.extra['chevronOnBar'] as number | undefined;
  if (f.chevron.ratio < 3)
    problems.push(
      `${c.tag} reader-chapter: the chevron ${f.chevron.color} on the select's ground ${f.chevron.ground} is ${f.chevron.ratio}:1, under 3:1`,
    );
  if (onBar !== undefined && onBar < 3)
    problems.push(
      `${c.tag} reader-chapter: the chevron on the bar's ground is ${onBar}:1, under 3:1`,
    );
}

/* -------------------------------------------------------------------------------------------------- the test */

test(`p103 library: the Library's selects and the Reader's chapter picker, ${PHASE}`, async ({
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
      'e2e p103 library shots',
    ]);
  } catch (e) {
    say(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true }).catch(() => undefined);

  const stamp = String(Date.now()).slice(-6); // digits only: the same width every run
  const problems: string[] = [];
  const figs: Record<string, Store> = {};
  const ids: Ids = { seriesId: '', chapterId: '', chapter2Id: '', listId: '' };
  try {
    const series = await call<{ id: string }>(octx.request, 'POST', '/admin/library/series', {
      kind: 'manga',
      title: `p103-lib-${stamp}`,
      synopsis: 'A series made by the wave 103 library spec.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    });
    ids.seriesId = series.id;
    const mkEdition = async (language: string, label: string, isDefault: boolean) =>
      (
        await call<{ id: string }>(
          octx.request,
          'POST',
          `/admin/library/series/${ids.seriesId}/editions`,
          { language, label, is_default: isDefault },
        )
      ).id;
    const edition = await mkEdition('en', 'Official', true);
    const edition2 = await mkEdition('fr', 'Fan edition', false);
    const mkChapter = async (editionId: string, number: number, title: string) => {
      const ch = await call<{ id: string }>(
        octx.request,
        'POST',
        `/admin/library/editions/${editionId}/chapters`,
        { number, title, format: 'images' },
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
    ids.chapterId = await mkChapter(edition, 1, 'p103-lib first');
    ids.chapter2Id = await mkChapter(
      edition,
      2,
      'p103-lib second chapter with a long title that is wider than the picker',
    );
    await mkChapter(edition, 3, 'p103-lib third');
    await mkChapter(edition2, 1, 'p103-lib fan');
    await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/publish`);
    const list = await call<{ id: string }>(octx.request, 'POST', '/library/lists', {
      name: `p103-lib-list-${stamp}`,
      visibility: 'private',
    });
    ids.listId = list.id;
    say(`ids ${JSON.stringify(ids)}`);

    for (const c of CONFIGS) {
      try {
        figs[c.tag] = await runConfig(browser, octx, c, ids, problems);
      } catch (e) {
        problems.push(`${c.tag}: ${String(e).slice(0, 600)}`);
      }
    }
  } finally {
    if (ids.listId)
      await call(octx.request, 'DELETE', `/library/lists/${ids.listId}`).catch((e) =>
        say(`list delete: ${String(e).slice(0, 160)}`),
      );
    if (ids.seriesId) {
      await call(octx.request, 'POST', `/admin/library/series/${ids.seriesId}/unpublish`).catch(
        () => undefined,
      );
      await call(octx.request, 'DELETE', `/admin/library/series/${ids.seriesId}`).catch((e) =>
        say(`series delete: ${String(e).slice(0, 160)}`),
      );
    }
    await octx.close();
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  if (PHASE === 'after')
    for (const c of CONFIGS) if (figs[c.tag]) chevronChecks(c, figs[c.tag]!, problems);
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Store>;
    const cmp: Record<string, unknown> = {};
    for (const c of CONFIGS)
      if (base[c.tag] && figs[c.tag]) cmp[c.tag] = compare(c, base[c.tag]!, figs[c.tag]!, problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
