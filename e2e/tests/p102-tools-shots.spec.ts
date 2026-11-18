import { deflateSync } from 'node:zlib';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-958 (wave 102, pod tools; builders calc, cards, studio): the raw `<select>` of the Tools drawer's pages move to the
 * kit's `NativeSelect`. This is the pod's one spec (written by the shots builder `toolshots`). For every tool that has
 * a raw select, in the drawer opened by Ctrl+K and `>name` (never by a class: the class is what changes), it records
 * every select's box, the boxes of the controls around it, the page's and the drawer's sideways overflow, the exact hit
 * test over the drawer (`openCollapsed` then `hitTestAll`, takers named), a shot, and, at 1024 only, a keyboard case per
 * select (focus, ArrowDown changes the value, ArrowUp puts it back). Base then after, in two runs on one database.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited). Data (D-942): one guild named `P102 tools shots` (found again by that whole name, else made)
 * and, per run and configuration, one text channel with its own name (`p102-<phase>-<run id>-<config>`) in which the
 * Poll and the Tier list start their rooms; nothing in the sweep's own guild is touched. The currency feed is a
 * `page.route` fake (the stack has no rate source): the select boxes do not depend on its values. The Studio is fed a
 * PNG the spec makes itself. Nothing is saved that another spec reads: the keyboard cases put every value back.
 *
 * Phases: `SR_P102_TOOLS_PHASE=base` records the unmerged tree (shots get a `base-` prefix, figures go to
 * `measure-base.json`); the default `after` records the same and, when `measure-base.json` is in the folder, compares
 * (a select's block size not under its base, not under 40 px, not under 44 px at 375 touch; inline size not under the
 * base; a control below or beside moved by no more than the height differences above it; no label taller than its
 * select's growth; no overlap; the drawer and the document not scrolling sideways when the base did not; the hit test
 * equal in count and no worse in status) and writes `report-after.json` and `pairs` (base beside after).
 * Configs: 1024 x 768 Lounge Editorial (the shipped theme; keyboard cases), 375 x 812 touch Lounge Editorial, 1024 x 768
 * Lounge Light (a subset of tools). The account theme is set by PATCH and localStorage and `data-theme` is read back.
 *
 * Local only (SR_P102_TOOLS_ON=1). SR_P102_TOOLS_OUT is the folder of the shots and the figures. From e2e/, on a stack
 * with the sweep's world (API 8797, Vite 5797, metrics 9497, `SR_ADMIN_REQUIRE_REAUTH=0`; `DATABASE_URL` and
 * `SR_CLI_BIN` set to the stack's database and sr-cli):
 *   SR_E2E_BASE_URL=http://localhost:5797 SR_P102_TOOLS_ON=1 SR_P102_TOOLS_PHASE=base \
 *   SR_P102_TOOLS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-tools \
 *   npx playwright test tests/p102-tools-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * then the same with SR_P102_TOOLS_PHASE=after (or unset) on the merged tree. After the merge, with no base from this
 * tree, delete `measure-base.json` first if the folder holds one from another tree.
 */
const ON = process.env.SR_P102_TOOLS_ON === '1';
const PHASE = process.env.SR_P102_TOOLS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P102_TOOLS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p102-tools';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const GUILD_NAME = 'P102 tools shots';
const ONLY = process.env.SR_P102_TOOLS_ONLY?.split(',');
const RUN = Date.now().toString(36);

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: 'lounge-editorial' | 'lounge-light';
  /** Target ids shot and measured in this config; undefined = all. */
  only?: string[];
  keyboard: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-editorial',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-editorial',
    keyboard: true,
  },
  {
    tag: '375-editorial',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-editorial',
    keyboard: false,
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    only: ['units', 'invite', 'json', 'poll', 'studio-caption', 'tierlist'],
    keyboard: false,
  },
];

interface Ctx {
  page: Page;
  drawer: Locator;
  cfg: Cfg;
  roomChannel: string;
  guildId: string;
}
interface Target {
  id: string;
  query: string;
  pick: RegExp;
  /** How many selects the drawer holds when the way in is done. */
  selects: number;
  /** Goes to the room channel (Poll, Tier list) instead of the guild's first one. */
  room?: boolean;
  setup?: (c: Ctx) => Promise<void>;
}

/** A 240 x 120 PNG made here: gradient, no library. */
function makePng(w = 240, h = 120): Buffer {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (b: Buffer) => {
    let c = 0xffffffff;
    for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const raw = Buffer.alloc((w * 3 + 1) * h);
  for (let y = 0; y < h; y++) {
    raw[y * (w * 3 + 1)] = 0;
    for (let x = 0; x < w; x++) {
      const o = y * (w * 3 + 1) + 1 + x * 3;
      raw[o] = Math.round((x / w) * 255);
      raw[o + 1] = Math.round((y / h) * 255);
      raw[o + 2] = 160;
    }
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const startRoom = async (c: Ctx) => {
  await c.drawer.getByRole('button', { name: 'Start a new one' }).click();
};
const openStudio = async (c: Ctx) => {
  await c.drawer
    .getByLabel('Choose an image')
    .setInputFiles({ name: 'p102.png', mimeType: 'image/png', buffer: makePng() });
  await expect(c.drawer.getByRole('button', { name: 'Caption', exact: true })).toBeVisible({
    timeout: 20_000,
  });
};

const TARGETS: Target[] = [
  { id: 'units', query: 'unit', pick: /unit converter/i, selects: 3 },
  { id: 'currency', query: 'currency', pick: /currency converter/i, selects: 2 },
  { id: 'percent', query: 'percent', pick: /percentage calculator/i, selects: 1 },
  { id: 'programmer', query: 'programmer', pick: /programmer calculator/i, selects: 1 },
  { id: 'timezone', query: 'time zones', pick: /time zones/i, selects: 2 },
  {
    id: 'loot',
    query: 'loot',
    pick: /loot tables/i,
    selects: 2,
    setup: async (c) => {
      await expect(c.drawer.locator('select')).toHaveCount(2, { timeout: 20_000 });
    },
  },
  { id: 'invite', query: 'invite card', pick: /invite card/i, selects: 3 },
  {
    id: 'json',
    query: 'json',
    pick: /^json/i,
    selects: 1,
  },
  { id: 'diff', query: 'diff', pick: /diff/i, selects: 1 },
  { id: 'url', query: 'url encode', pick: /url encode/i, selects: 1 },
  {
    id: 'mdtable',
    query: 'markdown table',
    pick: /markdown table/i,
    selects: 3,
    setup: async (c) => {
      await c.drawer.getByRole('textbox').first().fill('a\tb\tc\n1\t2\t3');
      await expect(c.drawer.locator('select')).toHaveCount(3);
    },
  },
  {
    id: 'poll',
    query: 'poll',
    pick: /poll builder/i,
    selects: 2,
    room: true,
    setup: async (c) => {
      await startRoom(c);
      await expect(c.drawer.locator('select')).toHaveCount(2, { timeout: 20_000 });
    },
  },
  {
    id: 'tierlist',
    query: 'tier list',
    pick: /tier list/i,
    selects: 3,
    room: true,
    setup: async (c) => {
      await startRoom(c);
      const rows = c.drawer.getByRole('button', { name: 'Add the rows S to D' });
      if (await rows.isVisible().catch(() => false)) await rows.click();
      await c.drawer.getByLabel('Item', { exact: true }).fill('P102 item');
      await c.drawer.getByRole('button', { name: 'Add item' }).click();
      await c.drawer.getByRole('button', { name: 'Image from a bucket' }).click();
      await expect(c.drawer.locator('select')).toHaveCount(3, { timeout: 20_000 });
    },
  },
  {
    id: 'studio-caption',
    query: 'image studio',
    pick: /image studio/i,
    selects: 1,
    setup: async (c) => {
      await openStudio(c);
      await c.drawer.getByRole('button', { name: 'Caption', exact: true }).click();
      await expect(c.drawer.locator('select')).toHaveCount(1);
    },
  },
  {
    id: 'studio-export',
    query: 'image studio',
    pick: /image studio/i,
    selects: 1,
    setup: async (c) => {
      await openStudio(c);
      await c.drawer.getByRole('button', { name: 'Export', exact: true }).click();
      await expect(c.drawer.locator('select')).toHaveCount(1);
    },
  },
];

const say = (s: string) => console.log(`REPORT ${s}`);
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

const FX = {
  enabled: true,
  base: 'EUR',
  as_of: '2026-11-17',
  fetched_at: 1_795_000_000,
  stale: false,
  rates: {
    USD: 1.08,
    GBP: 0.85,
    JPY: 160.2,
    CHF: 0.96,
    CAD: 1.47,
    AUD: 1.63,
    SEK: 11.4,
    NOK: 11.6,
  },
};

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
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
  await ctx.route('**/tools/fx', (route) => route.fulfill({ json: FX }));
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
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
  drawerScroll: number[];
  root: Box | null;
  selects: {
    key: string;
    box: Box;
    label: Box | null;
    options: number;
    value: string;
    kit: boolean;
  }[];
  controls: { key: string; box: Box }[];
  overlaps: string[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitRows: Record<string, string>;
  hitFails: unknown[];
  opened: unknown[];
  keyboard?: Record<string, unknown>[];
}

/** Boxes in the root's coordinates are page coordinates plus every scrolled ancestor's offset. */
async function measure(drawer: Locator): Promise<Measured> {
  return drawer.evaluate((root) => {
    // Every scroller up to the top: boxes are read with the drawer scrolled to its start.
    for (let p: Element | null = root; p; p = p.parentElement) {
      p.scrollTop = 0;
      p.scrollLeft = 0;
    }
    root.querySelectorAll('*').forEach((e) => {
      e.scrollTop = 0;
    });
    const box = (e: Element) => {
      const r = e.getBoundingClientRect();
      return {
        x: Math.round(r.left * 10) / 10,
        y: Math.round(r.top * 10) / 10,
        w: Math.round(r.width * 10) / 10,
        h: Math.round(r.height * 10) / 10,
      };
    };
    const els = Array.from(
      root.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    const name = (e: HTMLElement) => {
      const aria = e.getAttribute('aria-label');
      if (aria) return aria;
      const lab = e.closest('label');
      if (lab) {
        let s = '';
        lab.childNodes.forEach((n) => {
          if (n === e || (n.nodeType === 1 && (n as Element).contains(e))) return;
          s += n.textContent ?? '';
        });
        if (s.trim()) return s.trim().slice(0, 30);
      }
      return (e.textContent ?? '').trim().slice(0, 30);
    };
    const controls = els.map((e, i) => ({
      key: `${i}:${e.tagName.toLowerCase()}:${name(e)}`,
      box: box(e),
    }));
    const selEls = els.filter((e): e is HTMLSelectElement => e.tagName === 'SELECT');
    const selects = selEls.map((e) => {
      const lab = e.closest('label');
      return {
        key: `${els.indexOf(e)}:select:${name(e)}`,
        box: box(e),
        label: lab ? box(lab) : null,
        options: e.options.length,
        kit: e.parentElement?.tagName === 'SPAN' && /wrap/i.test(e.parentElement.className),
        value: e.value,
      };
    });
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
            `select(${name(s)}) x ${o.tagName.toLowerCase()}(${name(o)}) ${Math.round(ix)}x${Math.round(iy)}`,
          );
      }
    }
    // The widest scroller inside the drawer: does anything in it scroll sideways?
    let widest = [root.scrollWidth, root.clientWidth];
    for (const e of Array.from(root.querySelectorAll<HTMLElement>('*')))
      if (e.scrollWidth - e.clientWidth > widest[0]! - widest[1]! && e.clientWidth > 0)
        widest = [e.scrollWidth, e.clientWidth];
    return {
      theme: {
        html: document.documentElement.getAttribute('data-theme'),
        first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      },
      scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      drawerScroll: widest,
      root: box(root),
      selects,
      controls,
      overlaps,
    };
  });
}

const count = (rows: ControlRow[]) => ({
  pass: rows.filter((r) => r.status === 'pass').length,
  fail: rows.filter((r) => r.status === 'fail').length,
  notReached: rows.filter((r) => r.status === 'not-reached').length,
  skipped: rows.filter((r) => r.status === 'skipped').length,
});

async function hit(page: Page, root: Locator, tag: string, problems: string[]) {
  const opened = await openCollapsed(page, root);
  const rows = await hitTestAll(page, root);
  const h = count(rows);
  const hitRows: Record<string, string> = {};
  rows.forEach((r, i) => {
    hitRows[`${i}:${r.tag}:${r.name}`] = r.status;
  });
  const hitFails = rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));
  say(`${tag}: hit ${JSON.stringify(h)}`);
  if (h.notReached) problems.push(`${tag}: ${h.notReached} controls not reached (probe error)`);
  return { h, hitRows, hitFails, opened };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(350);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function openTool(page: Page, t: Target) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${t.query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: t.pick }).first().click();
  await page.waitForTimeout(900);
}

async function closeTool(page: Page) {
  const dlg = page.getByRole('dialog', { name: 'Drawer', exact: true });
  for (let i = 0; i < 3 && (await dlg.isVisible().catch(() => false)); i++) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

/** Focus, ArrowDown changes the value, ArrowUp puts it back; the drawer's text must react. */
async function keyboardCases(page: Page, drawer: Locator, problems: string[], tag: string) {
  const out: Record<string, unknown>[] = [];
  const n = await drawer.locator('select').count();
  for (let i = 0; i < n; i++) {
    const sel = drawer.locator('select').nth(i);
    const options = await sel.locator('option').count();
    if (options < 2) {
      out.push({ i, options, note: 'one option: nothing to change' });
      continue;
    }
    await sel.scrollIntoViewIfNeeded();
    await page.keyboard.press('Shift');
    await sel.focus();
    const focusVisible = await sel.evaluate((e) => e.matches(':focus-visible'));
    const ring = await sel.evaluate((e) => {
      const cs = getComputedStyle(e);
      return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} off ${cs.outlineOffset}`;
    });
    const textBefore = await drawer.innerText();
    const before = await sel.inputValue();
    const lastOpt = await sel.evaluate(
      (e) => (e as HTMLSelectElement).selectedIndex === (e as HTMLSelectElement).options.length - 1,
    );
    const fwd = lastOpt ? 'ArrowUp' : 'ArrowDown';
    const rev = lastOpt ? 'ArrowDown' : 'ArrowUp';
    await page.keyboard.press(fwd);
    await page.waitForTimeout(150);
    const changed = await sel.inputValue();
    const textAfter = await drawer.innerText();
    await page.keyboard.press(rev);
    await page.waitForTimeout(150);
    const back = await sel.inputValue();
    out.push({
      i,
      options,
      focusVisible,
      ring,
      before,
      changed,
      back,
      reacted: textAfter !== textBefore,
    });
    if (!focusVisible)
      problems.push(`${tag}: select ${i} :focus-visible did not match after a key`);
    if (changed === before)
      problems.push(`${tag}: select ${i} the arrow key did not change the value`);
    if (back !== before)
      problems.push(`${tag}: select ${i} the way back did not restore the value`);
  }
  return out;
}

function checkTheme(m: Measured, c: Cfg, tag: string, problems: string[]) {
  say(`${tag}: data-theme html=${m.theme.html} first=${m.theme.first}`);
  if (m.theme.html !== c.theme && m.theme.first !== c.theme)
    problems.push(`${tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
}

/** Boxes in the drawer's own coordinates: the drawer is centred, so a taller body moves the whole dialog. */
const toRoot = (f: Figures): Figures => {
  const r = f.root;
  if (!r) return f;
  const mv = (b: Box): Box => ({
    ...b,
    x: Math.round((b.x - r.x) * 10) / 10,
    y: Math.round((b.y - r.y) * 10) / 10,
  });
  return {
    ...f,
    selects: f.selects.map((x) => ({ ...x, box: mv(x.box), label: x.label ? mv(x.label) : null })),
    controls: f.controls.map((c) => ({ ...c, box: mv(c.box) })),
  };
};

function compare(label: string, base: Figures, after: Figures, touch: boolean, problems: string[]) {
  return compareRaw(label, toRoot(base), toRoot(after), touch, problems);
}

function compareRaw(
  label: string,
  base: Figures,
  after: Figures,
  touch: boolean,
  problems: string[],
) {
  const diffs: Record<string, unknown> = {};
  if (base.selects.length !== after.selects.length) {
    problems.push(`${label}: select count ${base.selects.length} -> ${after.selects.length}`);
    return diffs;
  }
  base.selects.forEach((b, i) => {
    const a = after.selects[i]!;
    diffs[b.key] = { base: b.box, after: a.box };
    if (a.box.h < b.box.h - 0.5)
      problems.push(`${label}: ${b.key} shorter than base (${a.box.h} < ${b.box.h})`);
    if (a.kit && (touch ? a.box.h < 43.5 : a.box.h < 39.5))
      problems.push(`${label}: ${b.key} is ${a.box.h} px tall (${touch ? '44' : '40'} wanted)`);
    if (!a.kit && (Math.abs(a.box.h - b.box.h) > 0.5 || Math.abs(a.box.w - b.box.w) > 0.5))
      problems.push(
        `${label}: ${b.key} is still raw but its box changed ${b.box.w}x${b.box.h} -> ${a.box.w}x${a.box.h} (a sheet rule it lost or gained)`,
      );
    if (a.box.w < b.box.w - 0.5)
      problems.push(`${label}: ${b.key} narrower than base (${a.box.w} < ${b.box.w})`);
    if (b.label && a.label && a.label.h - b.label.h > a.box.h - b.box.h + 1)
      problems.push(
        `${label}: ${b.key} label grew ${a.label.h - b.label.h} px, the select ${a.box.h - b.box.h} (wraps?)`,
      );
  });
  for (const o of after.overlaps)
    if (!base.overlaps.includes(o)) problems.push(`${label}: new overlap ${o}`);
  if (base.root && after.root)
    diffs['root'] = { base: base.root, after: after.root, dh: after.root.h - base.root.h };
  if (base.controls.length !== after.controls.length) {
    problems.push(`${label}: control count ${base.controls.length} -> ${after.controls.length}`);
    return diffs;
  }
  const moved: { key: string; dx: number; dy: number; allowed: number }[] = [];
  base.controls.forEach((bc, i) => {
    const ac = after.controls[i]!;
    // The summed growth of the selects above it, plus the growth of the one beside it in its row (a bottom-aligned
    // neighbour follows the row's whole growth).
    let allowed = 0;
    let beside = 0;
    base.selects.forEach((b, j) => {
      const a = after.selects[j]!;
      if (b.box.y + b.box.h <= bc.box.y + 0.5) allowed += a.box.h - b.box.h;
      else if (b.box.y < bc.box.y + bc.box.h && bc.box.y < b.box.y + b.box.h)
        beside = Math.max(beside, a.box.h - b.box.h);
    });
    allowed += beside;
    const dy = Math.round((ac.box.y - bc.box.y) * 10) / 10;
    const dx = Math.round((ac.box.x - bc.box.x) * 10) / 10;
    if (Math.abs(dy) > allowed + 0.5 || Math.abs(dx) > 1.5)
      moved.push({ key: bc.key, dx, dy, allowed: Math.round(allowed * 10) / 10 });
  });
  diffs['moved'] = moved;
  for (const m of moved) {
    if (Math.abs(m.dy) > m.allowed + 0.5)
      problems.push(`${label}: ${m.key} moved dy ${m.dy} (allowed ${m.allowed})`);
    else if (Math.abs(m.dx) > 1.5) say(`${label}: note: ${m.key} moved sideways dx ${m.dx}`);
  }
  const over = (f: Figures) =>
    f.scroll[0]! > f.scroll[1]! || f.drawerScroll[0]! > f.drawerScroll[1]!;
  if (over(after) && !over(base))
    problems.push(
      `${label}: scrolls sideways now (doc ${after.scroll.join('/')}, drawer ${after.drawerScroll.join('/')}), base ${base.scroll.join('/')} ${base.drawerScroll.join('/')}`,
    );
  if (base.hit.pass + base.hit.fail !== after.hit.pass + after.hit.fail)
    problems.push(
      `${label}: hit-tested count ${base.hit.pass + base.hit.fail} -> ${after.hit.pass + after.hit.fail}`,
    );
  if (after.hit.fail > base.hit.fail)
    problems.push(
      `${label}: hit fails ${base.hit.fail} -> ${after.hit.fail}: ${JSON.stringify(after.hitFails)}`,
    );
  return diffs;
}

/** Base beside after in one PNG, so each pair is opened once. */
async function pair(browser: Browser, name: string) {
  const a = resolve(OUT, `base-${name}.png`);
  const b = resolve(OUT, `${name}.png`);
  if (!existsSync(a) || !existsSync(b)) return;
  const p = await browser.newPage();
  const uri = (f: string) => `data:image/png;base64,${readFileSync(f).toString('base64')}`;
  await p.setContent(
    `<body style="margin:0;background:#888;font:12px sans-serif;color:#fff"><div style="display:flex;gap:8px"><div>base<br><img src="${uri(a)}"></div><div>after<br><img src="${uri(b)}"></div></div></body>`,
  );
  await p.waitForLoadState('load');
  mkdirSync(resolve(OUT, 'pairs'), { recursive: true });
  await p.screenshot({ path: resolve(OUT, 'pairs', `${name}.png`), fullPage: true });
  await p.close();
}

test('Tools drawer: raw selects measured, base then after', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const problems: string[] = [];
  const unreached: string[] = [];
  const all: Record<string, Figures> = {};
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    // The pod's own guild, found again by its whole name, else made.
    const mine = (await (await owner.request.get('/api/v1/users/@me/guilds')).json()) as {
      id: string;
      name: string;
    }[];
    let guildId = mine.find((g) => g.name === GUILD_NAME)?.id;
    let firstChannel = '';
    if (!guildId) {
      const g = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
        owner.request,
        '/guilds',
        { name: GUILD_NAME },
      );
      guildId = g.id;
      firstChannel = g.channels.find((c) => c.type === 0)!.id;
    } else {
      const g = (await (await owner.request.get(`/api/v1/guilds/${guildId}/channels`)).json()) as {
        id: string;
        type: number;
      }[];
      firstChannel = g.find((c) => c.type === 0)!.id;
    }
    say(`guild ${guildId} first channel ${firstChannel}`);

    const baseFile = resolve(OUT, 'measure-base.json');
    const baseFigs: Record<string, Figures> | null =
      PHASE === 'after' && existsSync(baseFile)
        ? (JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>)
        : null;
    if (PHASE === 'after' && !baseFigs)
      say('no measure-base.json: figures recorded, nothing compared');

    // Written after every configuration, so a stack that dies late keeps what it measured.
    const outFile = resolve(OUT, `measure-${PHASE}.json`);
    const prev =
      ONLY && existsSync(outFile)
        ? (JSON.parse(readFileSync(outFile, 'utf8')) as Record<string, Figures>)
        : {};
    const flush = () => {
      writeFileSync(outFile, JSON.stringify({ ...prev, ...all }, null, 1));
      if (PHASE === 'after')
        writeFileSync(
          resolve(OUT, 'report-after.json'),
          JSON.stringify({ problems, unreached }, null, 1),
        );
    };
    for (const cfg of CONFIGS) {
      await setMain(owner, cfg.theme);
      const room = await apiPost<{ id: string }>(owner.request, `/guilds/${guildId}/channels`, {
        name: `p102-${PHASE}-${RUN}-${cfg.tag}`.toLowerCase(),
        type: 0,
      });
      const { ctx, page } = await newPage(browser, owner, cfg);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      try {
        for (const t of TARGETS) {
          if (cfg.only && !cfg.only.includes(t.id)) continue;
          if (ONLY && !ONLY.includes(t.id)) continue;
          const key = `${cfg.tag}|${t.id}`;
          const tag = `${t.id} ${cfg.tag}`;
          try {
            const channel = t.room ? room.id : firstChannel;
            if (!page.url().endsWith(`/channels/${guildId}/${channel}`)) {
              await page.goto(`/channels/${guildId}/${channel}`, {
                waitUntil: 'domcontentloaded',
                timeout: 30_000,
              });
            }
            await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
            await openTool(page, t);
            const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
            await expect(drawer).toBeVisible();
            await t.setup?.({ page, drawer, cfg, roomChannel: room.id, guildId });
            await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
            await page.waitForTimeout(500);
            const sels = await drawer.locator('select').count();
            if (sels !== t.selects) {
              unreached.push(`${tag}: ${sels} selects, wanted ${t.selects}`);
              say(`NOT REACHED ${tag}: ${sels} selects, wanted ${t.selects}`);
              await closeTool(page);
              continue;
            }
            const m = await measure(drawer);
            checkTheme(m, cfg, tag, problems);
            say(
              `${tag}: selects ${JSON.stringify(m.selects.map((s) => [s.key, s.box.w, s.box.h]))}`,
            );
            const h0 = await drawer.locator('select').first();
            await h0.scrollIntoViewIfNeeded();
            await shot(page, `${t.id}-${cfg.tag}`);
            const fig: Figures = {
              ...m,
              hit: { pass: 0, fail: 0, notReached: 0, skipped: 0 },
              hitRows: {},
              hitFails: [],
              opened: [],
            };
            if (cfg.keyboard) {
              fig.keyboard = await keyboardCases(page, drawer, problems, tag);
              say(`${tag}: keyboard ${JSON.stringify(fig.keyboard)}`);
            }
            const hh = await hit(page, drawer, tag, problems);
            fig.hit = hh.h;
            fig.hitRows = hh.hitRows;
            fig.hitFails = hh.hitFails;
            fig.opened = hh.opened;
            all[key] = fig;
            if (baseFigs?.[key]) {
              const d = compare(tag, baseFigs[key], fig, cfg.touch, problems);
              say(`${tag}: compared ${JSON.stringify(d)}`);
            }
            await closeTool(page);
          } catch (e) {
            const msg = String(e instanceof Error ? e.message : e).split('\n')[0];
            unreached.push(`${tag}: ${msg}`);
            say(`NOT REACHED ${tag}: ${msg}`);
            await closeTool(page).catch(() => undefined);
          }
        }
      } finally {
        if (errors.length) say(`page errors ${cfg.tag}: ${errors.join('; ')}`);
        await ctx.close();
        flush();
      }
    }
    flush();
    if (PHASE === 'after' && baseFigs) {
      for (const cfg of CONFIGS) for (const t of TARGETS) await pair(browser, `${t.id}-${cfg.tag}`);
    }
    say(
      `${PHASE}: ${Object.keys(all).length} figures; unreached ${unreached.length}; problems ${problems.length}`,
    );
    for (const u of unreached) say(`UNREACHED ${u}`);
    for (const p of problems) say(`PROBLEM ${p}`);
    expect(unreached, 'every select reached').toEqual([]);
    expect(problems, 'no select breaks the rule').toEqual([]);
  } finally {
    await owner.close();
  }
});
