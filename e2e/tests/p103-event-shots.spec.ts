import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-978 / SEC-AUD-459 (wave 103, event): the Create Event dialog's body never scrolls sideways; the Starts and Ends
 * fields share a line only where both fit whole, otherwise Ends goes under Starts. A record of the dialog body's
 * `scrollWidth` against `clientWidth`, the two date fields' boxes and their own `scrollWidth`, the grid, the radios'
 * left edge against the body's, the dialog against the viewport, and the exact hit test (shared tool), for the plain
 * dialog and for a large case (a "Somewhere else" event with a long name, a long location, a long description and a
 * cover picked). NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited). It creates no data and saves nothing (D-942): the dialog is opened and closed without its
 * Create button; the fields are only filled.
 *
 * Phases: `SR_P103_EVENT_PHASE=base` records the unedited sheet (shots get a `base-` prefix, figures go to
 * `figures-base.json`); the default `after` records the same and, when `figures-base.json` is in the folder, compares
 * (375: boxes equal to the base, hit test equal in count and no row worse). Configs: 1024 x 768 Lounge Light, 1024 x
 * 768 Lounge Dark, 1280 x 800 Lounge Light, 375 x 812 touch Lounge Light. The account theme is set by PATCH and
 * localStorage and read back. The dialog is opened through the dev server's module graph (`/src/lounge/store.ts`,
 * `openDialog`), as wave 102's lounge spec does.
 *
 * Local only (SR_P103_EVENT_ON=1). SR_P103_EVENT_OUT is the folder of the shots and the figures.
 */
const ON = process.env.SR_P103_EVENT_ON === '1';
const PHASE = process.env.SR_P103_EVENT_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P103_EVENT_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-event';
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
  theme: 'lounge-light' | 'lounge-dark';
  /** Cases that get a shot in this config. */
  shots: ('plain' | 'large')[];
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: ['plain', 'large'],
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: ['plain'],
  },
  {
    tag: '1280-lounge-light',
    width: 1280,
    height: 800,
    touch: false,
    theme: 'lounge-light',
    shots: ['plain'],
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: ['plain'],
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
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

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  theme: { html: string | null; first: string | null };
  doc: number[];
  viewport: number[];
  dialog: Box | null;
  /** The element inside the dialog that scrolls (or the dialog itself): its scroll and client width and height. */
  body: { sw: number; cw: number; sh: number; ch: number; box: Box } | null;
  radiosLeft: number[];
  radiosSpan: Box[];
  grid: Box | null;
  fields: { label: string; box: Box; sw: number; cw: number; text: string }[];
  /** Every element in the dialog whose box runs past the body's content box on the inline end. */
  past: string[];
  controls: { key: string; box: Box }[];
}
interface Figures extends Measured {
  hit: { pass: number; fail: number; notReached: number; skipped: number };
  hitFails: unknown[];
  opened: unknown[];
}

async function measure(page: Page): Promise<Measured> {
  return page.evaluate(() => {
    const dlg = document.querySelector<HTMLElement>('[role="dialog"]');
    const round = (n: number) => Math.round(n * 10) / 10;
    const box = (e: Element): Box => {
      const r = e.getBoundingClientRect();
      return { x: round(r.left), y: round(r.top), w: round(r.width), h: round(r.height) };
    };
    if (!dlg) {
      return {
        theme: { html: null, first: null },
        doc: [],
        viewport: [],
        dialog: null,
        body: null,
        radiosLeft: [],
        radiosSpan: [],
        grid: null,
        fields: [],
        past: [],
        controls: [],
      };
    }
    // The body is the form's nearest scrolling ancestor (or the dialog).
    const form = dlg.querySelector('form') ?? dlg;
    let bodyEl: HTMLElement = dlg;
    for (let p: HTMLElement | null = form.parentElement; p; p = p.parentElement) {
      const o = getComputedStyle(p);
      if (/(auto|scroll)/.test(o.overflowX + o.overflowY)) {
        bodyEl = p;
        break;
      }
      if (p === dlg) break;
    }
    const bb = bodyEl.getBoundingClientRect();
    const inputs = Array.from(
      dlg.querySelectorAll<HTMLInputElement>('input[type="datetime-local"]'),
    );
    const fields = inputs.map((i) => ({
      label: (i.closest('label')?.textContent ?? i.getAttribute('aria-label') ?? '')
        .trim()
        .slice(0, 30),
      box: box(i),
      sw: i.scrollWidth,
      cw: i.clientWidth,
      text: i.value,
    }));
    const grid = dlg.querySelector('.lng-event-times');
    const radios = Array.from(dlg.querySelectorAll<HTMLInputElement>('input[type="radio"]'));
    const past: string[] = [];
    const cs = getComputedStyle(bodyEl);
    const contentRight = bb.right - parseFloat(cs.paddingRight) - parseFloat(cs.borderRightWidth);
    for (const e of Array.from(dlg.querySelectorAll<HTMLElement>('*'))) {
      if (e.getClientRects().length === 0) continue;
      const r = e.getBoundingClientRect();
      if (r.width > 0 && r.right > contentRight + 1)
        past.push(
          `${e.tagName.toLowerCase()}.${(e.getAttribute('class') ?? '').slice(0, 30)} right=${round(r.right)} (limit ${round(contentRight)})`,
        );
    }
    const els = Array.from(
      dlg.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
    ).filter((e) => e.getClientRects().length > 0);
    return {
      theme: {
        html: document.documentElement.getAttribute('data-theme'),
        first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
      },
      doc: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
      viewport: [window.innerWidth, window.innerHeight],
      dialog: box(dlg),
      body: {
        sw: bodyEl.scrollWidth,
        cw: bodyEl.clientWidth,
        sh: bodyEl.scrollHeight,
        ch: bodyEl.clientHeight,
        box: box(bodyEl),
      },
      radiosLeft: radios.map((r) => round(r.getBoundingClientRect().left)),
      radiosSpan: radios.map((r) => box(r.closest('label') ?? r)),
      grid: grid ? box(grid) : null,
      fields,
      past: past.slice(0, 12),
      controls: els.map((e, i) => ({
        key: `${i}:${e.tagName.toLowerCase()}:${(
          e.getAttribute('aria-label') ??
          e.closest('label')?.firstElementChild?.textContent ??
          e.textContent ??
          ''
        )
          .trim()
          .slice(0, 24)}`,
        box: box(e),
      })),
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
  return { h, hitFails, opened };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(400);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function openEvent(page: Page) {
  const w = world();
  await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await page
    .getByTestId('channel-sidebar')
    .first()
    .waitFor({ timeout: 20_000 })
    .catch(() => undefined);
  await page.waitForTimeout(1200);
  await page.evaluate(
    async (d) => {
      const m = (await import(/* @vite-ignore */ String('/src/lounge/store.ts'))) as {
        useLoungeUi: { getState: () => { openDialog: (x: unknown) => void } };
      };
      m.useLoungeUi.getState().openDialog(d);
    },
    { kind: 'createEvent', guildId: w.guildId },
  );
  const dlg = page.getByRole('dialog', { name: /Create Event/ });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('input[type="datetime-local"]').first().waitFor({ timeout: 15_000 });
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.waitForTimeout(600);
  return dlg;
}

/** 1 x 1 PNG for the cover picker. */
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==',
  'base64',
);

async function fillLarge(dlg: Locator) {
  await dlg.getByText('Somewhere else', { exact: true }).click();
  await dlg.getByRole('textbox', { name: 'Location', exact: true }).fill('L'.repeat(100));
  await dlg
    .getByRole('textbox', { name: 'Event topic', exact: true })
    .fill('Event-name-without-any-break-'.repeat(4).slice(0, 100));
  await dlg
    .getByRole('textbox', { name: 'Description (optional)', exact: true })
    .fill('A long description line. '.repeat(40).slice(0, 1000));
  await dlg.locator('input[type="file"]').setInputFiles({
    name: 'cover.png',
    mimeType: 'image/png',
    buffer: PNG,
  });
  await dlg
    .locator('.lng-event-cover-preview img')
    .waitFor({ timeout: 5000 })
    .catch(() => undefined);
}

async function scenario(
  browser: Browser,
  owner: BrowserContext,
  c: Cfg,
  kind: 'plain' | 'large',
  problems: string[],
) {
  const tag = `${kind} ${c.tag}`;
  await setMain(owner, c.theme);
  const { ctx, page } = await newPage(browser, owner, c);
  try {
    const dlg = await openEvent(page);
    if (kind === 'large') {
      await fillLarge(dlg);
      await page.waitForTimeout(400);
    }
    const m = await measure(page);
    say(`${tag}: data-theme html=${m.theme.html} first=${m.theme.first}`);
    if (m.theme.html !== c.theme && m.theme.first !== c.theme)
      problems.push(`${tag}: data-theme is ${m.theme.html}/${m.theme.first}, wanted ${c.theme}`);
    say(
      `${tag}: dialog ${JSON.stringify(m.dialog)} body ${JSON.stringify(m.body)} grid ${JSON.stringify(m.grid)}`,
    );
    say(`${tag}: fields ${JSON.stringify(m.fields)}`);
    say(`${tag}: radiosLeft ${JSON.stringify(m.radiosLeft)} past ${JSON.stringify(m.past)}`);
    if (m.doc[0]! > m.doc[1]!)
      problems.push(`${tag}: the document scrolls sideways ${m.doc.join('/')}`);
    if (m.body && m.body.sw > m.body.cw)
      problems.push(`${tag}: the dialog body scrolls sideways ${m.body.sw}/${m.body.cw}`);
    for (const f of m.fields)
      if (f.sw > f.cw) problems.push(`${tag}: field "${f.label}" clips its value ${f.sw}/${f.cw}`);
    if (m.body) {
      for (const x of m.radiosLeft)
        if (x < m.body.box.x - 0.5)
          problems.push(`${tag}: a radio starts left of the body (${x} < ${m.body.box.x})`);
    }
    const h = await hit(page, dlg, tag, problems);
    const fig: Figures = { ...m, hit: h.h, hitFails: h.hitFails, opened: h.opened };
    await page.evaluate(() => window.scrollTo(0, 0));
    await dlg.evaluate((e) => {
      for (const el of Array.from(e.querySelectorAll('*'))) {
        (el as HTMLElement).scrollTop = 0;
        (el as HTMLElement).scrollLeft = 0;
      }
    });
    if (c.shots.includes(kind))
      await shot(page, `event-${kind === 'large' ? '1024-large-lounge-light' : c.tag}`);
    return fig;
  } finally {
    await ctx.close();
  }
}

function compare(label: string, base: Figures, after: Figures, problems: string[]) {
  const diffs: Record<string, unknown> = {
    body: { base: base.body, after: after.body },
    grid: { base: base.grid, after: after.grid },
    fields: { base: base.fields, after: after.fields },
  };
  if (!label.includes('375')) return diffs;
  // 375 equals the base (boxes) and the hit test is equal in count, no row worse.
  if (base.controls.length !== after.controls.length) {
    problems.push(`${label}: control count ${base.controls.length} -> ${after.controls.length}`);
  } else {
    base.controls.forEach((b, i) => {
      const a = after.controls[i]!;
      for (const k of ['x', 'y', 'w', 'h'] as const)
        if (Math.abs(a.box[k] - b.box[k]) > 1)
          problems.push(`${label}: ${b.key} ${k} ${b.box[k]} -> ${a.box[k]}`);
    });
  }
  if (
    after.hit.fail > base.hit.fail ||
    after.hit.notReached > 0 ||
    after.hit.pass + after.hit.fail !== base.hit.pass + base.hit.fail
  )
    problems.push(`${label}: hit test ${JSON.stringify(base.hit)} -> ${JSON.stringify(after.hit)}`);
  return diffs;
}

test(`p103 event: the Create Event dialog fits its box, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const problems: string[] = [];
  const figs: Record<string, Figures> = {};
  try {
    for (const c of CONFIGS) {
      for (const kind of ['plain', 'large'] as const) {
        // The large case is measured at 1024 Light and 375 only.
        if (kind === 'large' && c.tag !== '1024-lounge-light' && c.tag !== '375-lounge-light')
          continue;
        try {
          figs[`${kind} ${c.tag}`] = await scenario(browser, owner, c, kind, problems);
        } catch (e) {
          problems.push(`${kind} ${c.tag}: ${String(e).slice(0, 500)}`);
        }
      }
    }
  } finally {
    await owner.close();
  }
  writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(figs, null, 1));
  const baseFile = resolve(OUT, 'figures-base.json');
  if (PHASE === 'after' && existsSync(baseFile)) {
    const base = JSON.parse(readFileSync(baseFile, 'utf8')) as Record<string, Figures>;
    const cmp: Record<string, unknown> = {};
    for (const k of Object.keys(figs))
      if (base[k]) cmp[k] = compare(k, base[k]!, figs[k]!, problems);
    writeFileSync(resolve(OUT, 'compare.json'), JSON.stringify(cmp, null, 1));
  } else if (PHASE === 'after') say('no figures-base.json: nothing to compare against');
  say(`problems: ${JSON.stringify(problems)}`);
  if (PHASE === 'base') say('base phase: problems above are the base faults, not a failure');
  else expect(problems).toEqual([]);
});
