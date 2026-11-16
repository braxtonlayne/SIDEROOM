import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';

/**
 * Wave 84, support builder. SEC-AUD-358 (D-862): Support / Patron on the kit's fields and the plan buttons in an
 * even grid. SEC-AUD-359 (D-863): the in-app template page's ready state, shot in the member's theme.
 *
 * The Owner on the visual sweep's seeded world (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts -g "seed
 * the sweep world"`; this spec never edits that spec). Three settings: 375x812 (touch, a mobile user agent) in Lounge
 * Light and Lounge Dark, 1280x800 in Editorial; the main theme is set with the account's settings (PATCH) and
 * localStorage and read back, and `data-theme` is read from the page's root before each shot.
 *
 * Test 1 walks the member's Settings > Support / Patron (the viewport and one shot per card), `/support` and a server's
 * Server Support tab, and writes one JSON line per page and setting to `measure.jsonl`: every `.sup-choices` row's
 * buttons with the line each is on, every input / select / combobox in the panel (its tag, whether it is the kit's, its
 * box, computed background and border) against a kit field of another tab, every `.sup-custom` row's button bottom
 * against its field's bottom, the plan and preset buttons' hit test at 375, any box that leaves its card, and the
 * sideways scroll of the document and of the tab's scroller. Then (after only) it types an amount under and over the
 * range, reads the error, and opens each Select and picks a value. Donations are off in the seeded world: the buttons
 * are disabled, so nothing is sent.
 *
 * Test 2 makes a template of the seeded server through the API, opens it inside the app and shoots its ready state in
 * the three themes at both sizes: `data-theme`, every text's contrast against the pixels behind it, the name field's and
 * the button's boxes inside the card, the hit test at 375, the sideways scroll. It does not press the create button and
 * deletes the template at its end.
 *
 * Local only (SR_P84_SUPPORT=1); shots go to SR_P84_SUPPORT_OUT. SR_P84_BASE=1 records the base (out folder
 * `<OUT>-base`, no assertion, no typing or picking: the controls are bare there).
 */
const ON = process.env.SR_P84_SUPPORT === '1';
const BASE = process.env.SR_P84_BASE === '1';
const OUT_AFTER =
  process.env.SR_P84_SUPPORT_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p84-support';
const OUT = BASE ? `${OUT_AFTER}-base` : OUT_AFTER;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

interface Setting {
  key: string;
  theme: string;
  size: { width: number; height: number };
  phone: boolean;
}
const SETTINGS: Setting[] = [
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
  {
    key: 'desk-editorial',
    theme: 'lounge-editorial',
    size: { width: 1280, height: 800 },
    phone: false,
  },
];
const DESK_LIGHT: Setting = {
  key: 'desk-light',
  theme: 'lounge-light',
  size: { width: 1280, height: 800 },
  phone: false,
};
const DESK_DARK: Setting = {
  key: 'desk-dark',
  theme: 'lounge-dark',
  size: { width: 1280, height: 800 },
  phone: false,
};

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
}
const loadWorld = (): World => {
  if (!existsSync(WORLD_FILE)) throw new Error('no sweep world: run the sweep seed first');
  return JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
};

async function newPage(browser: Browser, s: Setting) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: s.size,
    storageState: OWNER_STATE,
    bypassCSP: true,
    ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  const got = back.settings?.user_prefs?.theme ?? null;
  console.log(`REPORT account theme read back: ${got}`);
  expect(got).toBe(theme);
}

const settle = async (page: Page, ms = 2500) => {
  await page.waitForLoadState('networkidle', { timeout: ms + 3000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(500);
};

// ---------------------------------------------------------------------------------------------------------------------
// In the page

interface CtlInfo {
  tag: string;
  kind: string;
  kit: boolean;
  label: string;
  box: number[];
  bg: string;
  border: string;
  borderW: string;
  radius: string;
}
interface PanelMeasure {
  theme: string | null;
  doc: number;
  scroller: string | null;
  scrollerOver: number;
  choices: { label: string; n: number; rows: number[]; boxes: number[][] }[];
  controls: CtlInfo[];
  bare: number;
  custom: {
    label: string;
    fieldBottom: number;
    buttonBottom: number;
    diff: number;
    sameRow: boolean;
  }[];
  hit: { text: string; h: number; up: boolean; down: boolean }[];
  outside: { card: string; el: string; l: number; r: number }[];
  texts: number;
}

/** Everything the table needs for a page that holds the donation controls. */
function measurePanel(arg: { phone: boolean }): PanelMeasure {
  const de = document.documentElement;
  const dlg = document.querySelector<HTMLElement>('[role="dialog"]');
  const root: HTMLElement = dlg ?? document.querySelector<HTMLElement>('main') ?? document.body;
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 2) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const vis = (e: Element) => {
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  const rd = (n: number) => Math.round(n * 10) / 10;
  const boxOf = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [rd(r.left), rd(r.top + scrollY), rd(r.width), rd(r.height)];
  };
  const scrollers = Array.from(root.querySelectorAll<HTMLElement>('*'))
    .filter((e) => {
      if (e.closest('nav')) return false;
      return (
        /(auto|scroll|overlay)/.test(getComputedStyle(e).overflowY) &&
        e.clientWidth * e.clientHeight > 20000
      );
    })
    .sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight);
  const sc = scrollers[0] ?? null;

  const choices: PanelMeasure['choices'] = [];
  for (const g of Array.from(root.querySelectorAll<HTMLElement>('.sup-choices'))) {
    const btns = Array.from(g.querySelectorAll<HTMLElement>('button')).filter(vis);
    const tops = new Map<number, number>();
    for (const b of btns) {
      const t = Math.round(b.getBoundingClientRect().top);
      tops.set(t, (tops.get(t) ?? 0) + 1);
    }
    choices.push({
      label: g.getAttribute('aria-label') ?? '',
      n: btns.length,
      rows: Array.from(tops.entries())
        .sort((a, b) => a[0] - b[0])
        .map((x) => x[1]),
      boxes: btns.map(boxOf),
    });
  }

  const controls: CtlInfo[] = [];
  const ctlSel =
    'input:not([type=hidden]):not([type=checkbox]):not([type=radio]), select, textarea, [role=combobox]';
  for (const e of Array.from(root.querySelectorAll<HTMLElement>(ctlSel))) {
    if (e.closest('nav') || !vis(e)) continue;
    const isCombo = e.getAttribute('role') === 'combobox';
    // The kit's Select draws its box on the wrapper around the trigger; the kit's TextInput on the input itself.
    const drawn = isCombo ? (e.closest<HTMLElement>('[class*="control"]') ?? e) : e;
    const kit = isCombo
      ? /control/i.test(drawn.className) && drawn !== e
      : /control/i.test(String(e.className));
    const cs = getComputedStyle(drawn);
    controls.push({
      tag: e.tagName.toLowerCase(),
      kind: isCombo ? 'combobox' : ((e as HTMLInputElement).type ?? ''),
      kit,
      label:
        e.getAttribute('aria-label') ??
        document.querySelector(`label[for="${e.id}"]`)?.textContent?.trim() ??
        '',
      box: boxOf(drawn),
      bg: cs.backgroundColor,
      border: cs.borderTopColor,
      borderW: cs.borderTopWidth,
      radius: cs.borderTopLeftRadius,
    });
  }

  const custom: PanelMeasure['custom'] = [];
  for (const row of Array.from(root.querySelectorAll<HTMLElement>('.sup-custom'))) {
    const btn = row.querySelector<HTMLElement>('button');
    const field =
      row.querySelector<HTMLElement>('input') ??
      row.querySelector<HTMLElement>('[role=combobox]')?.closest<HTMLElement>('[class*="control"]');
    if (!btn || !field || !vis(btn) || !vis(field)) continue;
    const fb = field.getBoundingClientRect();
    const bb = btn.getBoundingClientRect();
    custom.push({
      label: row.querySelector('label')?.textContent?.trim() ?? '',
      fieldBottom: rd(fb.bottom),
      buttonBottom: rd(bb.bottom),
      diff: rd(bb.bottom - fb.bottom),
      sameRow: Math.abs(bb.top + bb.height / 2 - (fb.top + fb.height / 2)) < fb.height,
    });
  }

  const hit: PanelMeasure['hit'] = [];
  if (arg.phone) {
    const own = (target: Element, hitEl: Element | null) =>
      !!hitEl &&
      (hitEl === target || target.contains(hitEl) || !!hitEl.closest('label')?.contains(target));
    for (const b of Array.from(
      root.querySelectorAll<HTMLElement>('.sup-choices button, .sup-custom button'),
    )) {
      if (!vis(b)) continue;
      // Only a control whose centre is well inside the viewport can be hit-tested: bring it there.
      b.scrollIntoView({ block: 'center' });
      const r2 = b.getBoundingClientRect();
      const y = r2.top + r2.height / 2;
      const x = r2.left + r2.width / 2;
      hit.push({
        text: (b.textContent ?? '').trim().slice(0, 24),
        h: rd(r2.height),
        up: own(b, document.elementFromPoint(x, y - 21)),
        down: own(b, document.elementFromPoint(x, y + 21)),
      });
    }
    if (sc) sc.scrollTop = 0;
    scrollTo(0, 0);
  }

  const outside: PanelMeasure['outside'] = [];
  for (const card of Array.from(
    root.querySelectorAll<HTMLElement>('.us-card, section[id^=sup-]'),
  )) {
    const cr = card.getBoundingClientRect();
    if (cr.width === 0) continue;
    for (const e of Array.from(card.querySelectorAll<HTMLElement>('*'))) {
      if (!vis(e) || e.classList.contains('us-sr')) continue;
      const r = e.getBoundingClientRect();
      if (r.left < cr.left - 1 || r.right > cr.right + 1)
        outside.push({ card: card.id || sel(card), el: sel(e), l: rd(r.left), r: rd(r.right) });
    }
  }

  const themed = root.closest('[data-theme]') ?? document.querySelector('[data-theme]');
  return {
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    scroller: sc ? sel(sc) : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    choices,
    controls,
    bare: controls.filter((c) => !c.kit).length,
    custom,
    hit,
    outside,
    texts: 0,
  };
}

/** A kit field of another tab: the first TextInput and the first Select drawn there. */
function kitProbe(): { input: CtlInfo | null; select: CtlInfo | null } {
  const root = document.querySelector<HTMLElement>('[role="dialog"]') ?? document.body;
  const read = (e: HTMLElement): CtlInfo => {
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    return {
      tag: e.tagName.toLowerCase(),
      kind: '',
      kit: true,
      label: '',
      box: [r.left, r.top, r.width, r.height],
      bg: cs.backgroundColor,
      border: cs.borderTopColor,
      borderW: cs.borderTopWidth,
      radius: cs.borderTopLeftRadius,
    };
  };
  const input = Array.from(
    root.querySelectorAll<HTMLInputElement>(
      'input:not([type=hidden]):not([type=checkbox]):not([type=radio])',
    ),
  ).find((e) => /control/i.test(String(e.className)) && e.getBoundingClientRect().width > 0);
  const combo = Array.from(root.querySelectorAll<HTMLElement>('[role=combobox]')).find(
    (e) => e.getBoundingClientRect().width > 0,
  );
  const wrap = combo?.closest<HTMLElement>('[class*="control"]');
  return { input: input ? read(input) : null, select: wrap ? read(wrap) : null };
}

interface TplMeasure {
  rootTheme: string | null;
  doc: number;
  card: number[] | null;
  field: { box: number[]; bg: string; border: string; color: string; inside: boolean } | null;
  button: { box: number[]; bg: string; inside: boolean; upOk: boolean; downOk: boolean } | null;
  ground: string;
  texts: { k: string; fg: string; bg: string; ratio: number; size: number }[];
}

/** The template's ready state: boxes against the card, the hit test and every text's contrast. */
function measureTemplate(): TplMeasure {
  const page = document.querySelector<HTMLElement>('.ent-invite-page');
  const card = document.querySelector<HTMLElement>('.ent-invite-card');
  const de = document.documentElement;
  const rd = (n: number) => Math.round(n * 10) / 10;
  const bx = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [rd(r.left), rd(r.top), rd(r.width), rd(r.height)];
  };
  const parse = (c: string) => {
    const m = /(rgba?|color)\(([^)]+)\)/.exec(c);
    if (!m) return null;
    const k = m[1] === 'color' ? 255 : 1;
    const v = m[2]!
      .replace('srgb', '')
      .split(/[ ,/]+/)
      .filter(Boolean)
      .map((x, i) => (i < 3 ? Number(x) * k : Number(x)));
    return {
      r: v[0]!,
      g: v[1]!,
      b: v[2]!,
      a: v[3] === undefined || Number.isNaN(v[3]) ? 1 : v[3]!,
    };
  };
  const lum = (c: { r: number; g: number; b: number }) => {
    const f = (x: number) => {
      const y = x / 255;
      return y <= 0.03928 ? y / 12.92 : Math.pow((y + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  /** The colour behind an element: its ancestors' backgrounds composited, nearest first, up to an opaque one. */
  const behind = (e: Element) => {
    const layers: { r: number; g: number; b: number; a: number }[] = [];
    for (let a: Element | null = e; a; a = a.parentElement) {
      const bg = parse(getComputedStyle(a).backgroundColor);
      if (bg && bg.a > 0) layers.push(bg);
      if (bg && bg.a >= 1) break;
    }
    let base = { r: 255, g: 255, b: 255 };
    for (const l of layers.reverse())
      base = {
        r: l.r * l.a + base.r * (1 - l.a),
        g: l.g * l.a + base.g * (1 - l.a),
        b: l.b * l.a + base.b * (1 - l.a),
      };
    return base;
  };
  const ratioOf = (fgc: string, base: { r: number; g: number; b: number }, op: number) => {
    const fg = parse(fgc);
    if (!fg) return null;
    const k = fg.a * op;
    const mix = {
      r: fg.r * k + base.r * (1 - k),
      g: fg.g * k + base.g * (1 - k),
      b: fg.b * k + base.b * (1 - k),
    };
    const l1 = lum(mix);
    const l2 = lum(base);
    return (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
  };
  const opacityOf = (e: Element) => {
    let op = 1;
    for (let a: Element | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    return op;
  };

  const texts: TplMeasure['texts'] = [];
  const scope = card ?? page ?? document.body;
  for (const e of Array.from(scope.querySelectorAll<HTMLElement>('*'))) {
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const own = Array.from(e.childNodes)
      .filter((n) => n.nodeType === 3)
      .map((n) => (n.textContent ?? '').trim())
      .join(' ')
      .trim();
    const isField = e.tagName === 'INPUT';
    const value = isField ? (e as HTMLInputElement).value : '';
    if (!own && !value) continue;
    const base = behind(e);
    const ratio = ratioOf(getComputedStyle(e).color, base, opacityOf(e));
    if (ratio === null) continue;
    texts.push({
      k: `${e.tagName.toLowerCase()} "${(own || value).replace(/\s+/g, ' ').slice(0, 28)}"`,
      fg: getComputedStyle(e).color,
      bg: `rgb(${Math.round(base.r)},${Math.round(base.g)},${Math.round(base.b)})`,
      ratio: rd(ratio),
      size: parseFloat(getComputedStyle(e).fontSize),
    });
  }

  const cr = card?.getBoundingClientRect();
  const inside = (r: DOMRect) =>
    !!cr &&
    r.left >= cr.left - 1 &&
    r.right <= cr.right + 1 &&
    r.top >= cr.top - 1 &&
    r.bottom <= cr.bottom + 1;
  const input = card?.querySelector<HTMLInputElement>('input') ?? null;
  const btn = card?.querySelector<HTMLButtonElement>('form button[type=submit]') ?? null;
  const own = (target: Element, hitEl: Element | null) =>
    !!hitEl &&
    (hitEl === target || target.contains(hitEl) || !!hitEl.closest('label')?.contains(target));
  const hitAt = (e: Element, dy: number) => {
    const r = e.getBoundingClientRect();
    return own(e, document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2 + dy));
  };
  const ic = input ? getComputedStyle(input) : null;
  const bc = btn ? getComputedStyle(btn) : null;
  const ground = page ? behind(page) : { r: 255, g: 255, b: 255 };
  return {
    rootTheme:
      page?.closest('[data-theme]')?.getAttribute('data-theme') ??
      page?.querySelector('[data-theme]')?.getAttribute('data-theme') ??
      null,
    doc: de.scrollWidth - de.clientWidth,
    card: card ? bx(card) : null,
    field:
      input && ic
        ? {
            box: bx(input),
            bg: ic.backgroundColor,
            border: ic.borderTopColor,
            color: ic.color,
            inside: inside(input.getBoundingClientRect()),
          }
        : null,
    button:
      btn && bc
        ? {
            box: bx(btn),
            bg: bc.backgroundColor,
            inside: inside(btn.getBoundingClientRect()),
            upOk: hitAt(btn, -21),
            downOk: hitAt(btn, 21),
          }
        : null,
    ground: `rgb(${Math.round(ground.r)},${Math.round(ground.g)},${Math.round(ground.b)})`,
    texts,
  };
}

// ---------------------------------------------------------------------------------------------------------------------
// Test 1

test('SEC-AUD-358: Support / Patron, /support and Server Support: controls, grid, shots', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.jsonl');
  writeFileSync(jsonl, '');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  let shots = 0;
  // At 375 Lounge Light the base is shot too (before); the after run shoots the three settings.
  const settings = [...SETTINGS, DESK_LIGHT, DESK_DARK];
  for (const s of settings) {
    // The desktop Light and Dark settings are extra: only the member's tab, for the grid at 1280 in every theme.
    const extra = s === DESK_LIGHT || s === DESK_DARK;
    await setMain(ownerCtx, s.theme);
    const { ctx, page } = await newPage(browser, s);
    const shoot = async (name: string) => {
      const theme = await page.evaluate(
        () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? 'none',
      );
      const file = `${name}-${s.key}-${theme}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      shots++;
      return { file, theme };
    };
    const record = async (
      id: string,
      url: string,
      m: PanelMeasure,
      probe: unknown,
      files: string[],
    ) => {
      appendFileSync(
        jsonl,
        `${JSON.stringify({ id, setting: s.key, want: s.theme, url, ...m, probe, files })}\n`,
      );
      const grid = m.choices.map((c) => `${c.label}:${c.rows.join('+')}`).join(' ');
      console.log(
        `REPORT ${s.key} ${id}: theme ${m.theme}${m.theme === s.theme ? '' : ' MISMATCH'} doc ${m.doc} scroller ${m.scrollerOver} controls ${m.controls.length} bare ${m.bare} rows [${grid}] custom ${JSON.stringify(m.custom.map((c) => c.diff))} hit-fail ${m.hit.filter((h) => !h.up || !h.down).length}/${m.hit.length} outside ${m.outside.length}`,
      );
      if (!BASE) {
        expect.soft(m.theme, `${s.key} ${id} data-theme`).toBe(s.theme);
        expect.soft(m.doc, `${s.key} ${id} document sideways`).toBeLessThanOrEqual(0);
        expect.soft(m.scrollerOver, `${s.key} ${id} scroller sideways`).toBeLessThanOrEqual(0);
        expect.soft(m.bare, `${s.key} ${id} bare controls`).toBe(0);
        expect.soft(m.outside, `${s.key} ${id} boxes outside their card`).toEqual([]);
      }
    };

    // The kit's own field on other tabs: its ground and border under this theme.
    const probes: Record<string, { input: CtlInfo | null; select: CtlInfo | null }> = {};
    for (const tab of ['account', 'appearance']) {
      await page.goto(`/settings/user/${tab}`, { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await settle(page);
      probes[tab] = await page.evaluate(kitProbe);
    }
    const probe = {
      input: probes.account?.input ?? probes.appearance?.input ?? null,
      select: probes.appearance?.select ?? probes.account?.select ?? null,
    };
    console.log(
      `REPORT ${s.key} kit probe ${JSON.stringify({ i: probe.input && [probe.input.bg, probe.input.border], s: probe.select && [probe.select.bg, probe.select.border] })}`,
    );

    // Settings > Support / Patron.
    await page.goto('/settings/user/support', { waitUntil: 'domcontentloaded', timeout: 20_000 });
    await page.locator('#sup-patron, .sup-choices').first().waitFor({ timeout: 15_000 });
    await settle(page);
    const m = await page.evaluate(measurePanel, { phone: s.phone });
    const files: string[] = [];
    files.push((await shoot('01-member-support-top')).file);
    if (!extra) {
      for (const [n, id] of [
        ['02-patron', 'sup-patron'],
        ['03-donate', 'sup-donate'],
        ['04-server', 'sup-server'],
        ['05-credits', 'sup-credits'],
      ] as const) {
        await page
          .locator(`#${id}`)
          .first()
          .evaluate((el) => el.scrollIntoView({ block: 'start' }));
        await page.waitForTimeout(200);
        files.push((await shoot(`${n}-member-support`)).file);
      }
    } else {
      await page
        .locator('#sup-donate')
        .first()
        .evaluate((el) => el.scrollIntoView({ block: 'start' }));
      await page.waitForTimeout(200);
      files.push((await shoot('03-donate-member-support')).file);
    }
    await record('member-support', '/settings/user/support', m, probe, files);

    if (!BASE) {
      // After only: type under and over the range, open each Select and pick, in this setting.
      await page
        .locator('#sup-donate')
        .first()
        .evaluate((el) => el.scrollIntoView({ block: 'start' }));
      const field = page.locator('#sup-donate input[inputmode="decimal"]').first();
      await field.fill('0.50');
      const under = await page.evaluate(() => {
        const i = document.querySelector<HTMLInputElement>(
          '#sup-donate input[inputmode="decimal"]',
        );
        const d = i?.getAttribute('aria-describedby') ?? '';
        const err = d ? document.getElementById(d.split(' ').pop()!) : null;
        return {
          invalid: i?.getAttribute('aria-invalid'),
          error: err?.textContent ?? null,
          alert: !!err?.querySelector('[role=alert]'),
          border: i ? getComputedStyle(i).borderTopColor : null,
          errBox: err ? err.getBoundingClientRect().toJSON() : null,
          fieldBox: i ? i.getBoundingClientRect().toJSON() : null,
          btn: (() => {
            const b = document.querySelector<HTMLElement>('#sup-donate .sup-custom button');
            return b ? b.getBoundingClientRect().toJSON() : null;
          })(),
        };
      });
      console.log(`REPORT ${s.key} amount 0.50: ${JSON.stringify(under)}`);
      expect.soft(under.invalid, 'under range is invalid').toBe('true');
      expect.soft(under.error ?? '', 'under range error').toContain('Use an amount from');
      files.push((await shoot('06-amount-under')).file);
      await field.fill('20000');
      const over = await page.evaluate(() => {
        const i = document.querySelector<HTMLInputElement>(
          '#sup-donate input[inputmode="decimal"]',
        );
        return { invalid: i?.getAttribute('aria-invalid'), val: i?.value };
      });
      console.log(`REPORT ${s.key} amount 20000: ${JSON.stringify(over)}`);
      expect.soft(over.invalid, 'over range is invalid').toBe('true');
      await field.fill('7.50');
      const ok = await page.evaluate(() =>
        document
          .querySelector('#sup-donate input[inputmode="decimal"]')
          ?.getAttribute('aria-invalid'),
      );
      console.log(`REPORT ${s.key} amount 7.50 aria-invalid: ${ok}`);
      expect.soft(ok, 'a valid amount is not invalid').toBeNull();

      for (const [id, pick] of [
        ['sup-guild', 0],
        ['sup-credit-n', 2],
      ] as const) {
        const combo = page.locator(`#${id}`).first();
        await combo.evaluate((el) => el.scrollIntoView({ block: 'center' }));
        const before = ((await combo.textContent()) ?? '').trim();
        await combo.click();
        const list = page.getByRole('listbox').first();
        await list.waitFor();
        const opts = await list.getByRole('option').allTextContents();
        const inView = await list.evaluate((el) => {
          const r = el.getBoundingClientRect();
          return r.left >= 0 && r.right <= innerWidth && r.top >= 0 && r.bottom <= innerHeight;
        });
        files.push((await shoot(`07-open-${id}`)).file);
        await list
          .getByRole('option')
          .nth(Math.min(pick, opts.length - 1))
          .click();
        const after = ((await combo.textContent()) ?? '').trim();
        const extraText =
          id === 'sup-credit-n'
            ? await page.locator('#sup-credits .sup-custom button').first().textContent()
            : '';
        console.log(
          `REPORT ${s.key} select ${id}: before "${before}" options ${JSON.stringify(opts)} popup inside viewport ${inView} picked "${after}" ${extraText ? `button "${extraText.trim()}"` : ''}`,
        );
        expect.soft(inView, `${id} popup inside the viewport`).toBe(true);
        expect.soft(opts.length, `${id} has options`).toBeGreaterThan(0);
        if (opts.length > 1) expect.soft(after, `${id} changed`).not.toBe(before);
      }
    }

    if (!extra) {
      // /support: the page that follows the main theme.
      await page.goto('/support', { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.locator('main.sup-page').first().waitFor({ timeout: 15_000 });
      await settle(page);
      const mp = await page.evaluate(measurePanel, { phone: s.phone });
      const f1 = (await shoot('10-support-page')).file;
      await record('support-page', '/support', mp, probe, [f1]);

      // A server's Server Support tab (the Owner manages the seeded server).
      const sp = `/settings/server/${world.guildId}/support`;
      await page.goto(sp, { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page
        .locator('.sup-choices')
        .first()
        .waitFor({ timeout: 15_000 })
        .catch(() => undefined);
      await settle(page);
      const ms = await page.evaluate(measurePanel, { phone: s.phone });
      const f2 = (await shoot('11-server-support-top')).file;
      const f3: string[] = [f2];
      if (await page.locator('.sup-custom').first().count()) {
        await page
          .locator('.sup-custom')
          .first()
          .evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(200);
        f3.push((await shoot('12-server-support-amount')).file);
      }
      await record('server-support', sp, ms, probe, f3);
      if (!BASE) {
        const f = page.locator('.sup-custom input[inputmode="decimal"]').first();
        if (await f.count()) {
          await f.fill('0.50');
          const e = await page.evaluate(
            () => document.querySelector('.sup-custom [role=alert]')?.textContent ?? null,
          );
          console.log(`REPORT ${s.key} server-support amount 0.50 error: ${e}`);
          expect.soft(e ?? '', 'server support range error').toContain('Use an amount from');
        }
      }
    }
    await ctx.close();
  }
  await ownerCtx.close();
  console.log(`REPORT shots ${shots}`);
});

// ---------------------------------------------------------------------------------------------------------------------
// Test 2

test('SEC-AUD-359: the template page, ready state, in the three themes', async ({ browser }) => {
  test.skip(BASE, 'the template page is not changed by this wave: one run');
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'template.jsonl');
  writeFileSync(jsonl, '');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  let code: string | null = null;
  try {
    const tpl = await apiPost<{ code: string; name: string }>(
      ownerCtx.request,
      `/guilds/${world.guildId}/templates`,
      {
        name: 'P84 shots template',
        description: 'Made by the wave 84 support spec; deleted at its end.',
      },
    );
    code = tpl.code;
    console.log(`REPORT template made: code ${code} name ${tpl.name}`);
    let shots = 0;
    for (const theme of ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const) {
      await setMain(ownerCtx, theme);
      for (const phone of [true, false]) {
        const s: Setting = {
          key: phone ? 'phone' : 'desk',
          theme,
          size: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
          phone,
        };
        const { ctx, page } = await newPage(browser, s);
        await page.goto(`/template/${code}`, { waitUntil: 'domcontentloaded', timeout: 20_000 });
        // The ready state: the server's name, the name field and the create button.
        await page.locator('.ent-invite-card h1').first().waitFor({ timeout: 15_000 });
        await page.locator('.ent-invite-card form input').first().waitFor();
        await page.locator('.ent-invite-card form button[type=submit]').first().waitFor();
        await settle(page);
        const m = await page.evaluate(measureTemplate);
        const tag = `template-${s.key}-${theme}`;
        await page.screenshot({ path: resolve(OUT, `${tag}.png`), timeout: 10_000 });
        shots++;
        appendFileSync(jsonl, `${JSON.stringify({ tag, theme, ...m })}\n`);
        const low = m.texts.filter((t) => t.ratio < (t.size >= 24 ? 3 : 4.5));
        console.log(
          `REPORT ${tag}: data-theme ${m.rootTheme} doc ${m.doc} texts ${m.texts.length} under-4.5 ${low.length} ${JSON.stringify(low)} field ${JSON.stringify(m.field)} button ${JSON.stringify(m.button)} card ${JSON.stringify(m.card)} ground ${m.ground}`,
        );
        expect.soft(m.rootTheme, `${tag} data-theme`).toBe(theme);
        expect.soft(m.doc, `${tag} sideways`).toBeLessThanOrEqual(0);
        expect.soft(low, `${tag} texts under 4.5:1`).toEqual([]);
        expect.soft(m.field?.inside, `${tag} field inside the card`).toBe(true);
        expect.soft(m.button?.inside, `${tag} button inside the card`).toBe(true);
        if (phone) {
          // The hit test is reported (the box is the kit's: a 40 px field and a 36 px button are kit faults).
          console.log(
            `REPORT ${tag} hit test: button up ${m.button?.upOk} down ${m.button?.downOk}`,
          );
        }
        await ctx.close();
      }
    }
    console.log(`REPORT template shots ${shots}`);
  } finally {
    if (code) {
      const tok = (await (
        await ownerCtx.request.get('/api/v1/sr/csrf', { headers: { Origin: ORIGIN } })
      ).json()) as {
        token: string;
      };
      const res = await ownerCtx.request.delete(
        `/api/v1/guilds/${world.guildId}/templates/${code}`,
        {
          headers: { Origin: ORIGIN, 'X-SR-CSRF': tok.token },
        },
      );
      console.log(`REPORT template deleted: ${res.status()}`);
      const gone = await ownerCtx.request.get(`/api/v1/guilds/templates/${code}`);
      console.log(`REPORT template read back after delete: ${gone.status()}`);
    }
    await ownerCtx.close();
  }
});
