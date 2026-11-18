import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { edgeStrips } from '../support/edge-strips';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * Wave 100, rtl (D-953). Compare levels (Server settings > Server Support) and the Invoices wrapper (Admin > Donations >
 * Invoices) shot and measured left-to-right, then right-to-left, in one run and one world. A RECORD: it changes no app
 * file and a finding is a line in the handoff, not a failed test (the test fails only on a spec error: a screen that did
 * not open, a hit-test row that was not reached).
 *
 * THE APP HAS NO RTL MODE (no locale or setting sets `dir` on the root; `i18n/runtime.ts` sets only `lang`). So RTL here is
 * `document.documentElement.dir = 'rtl'` set after the page has loaded: a record of how ready the layout is under a forced
 * direction, not of a state a member can reach. Every finding is "info".
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/world.json` and `owner.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; that spec is never edited) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`,
 * `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`
 * (an invoice needs a price quote; D-288; a localhost stub is refused). The invoices are made through the API under
 * names of this spec's own (D-942); nothing is faked.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8785 / 5785 /
 * 9485):
 *   SR_P100_RTL_ON=1 SR_E2E_BASE_URL=http://localhost:5785 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P100_RTL_OUT=<folder> npx playwright test tests/p100-rtl-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 *
 * Settings: Lounge Light (account theme by PATCH and localStorage, `data-theme` read back and asserted). Compare levels at
 * 1024x768 and 375x812 touch; Invoices at 1024x768. Figures: `measure.json` (one JSON line per measure) and `totals.json`.
 * Mirror test: a box's distance from the inline-start edge of its reference (the table for a cell, the card for the rest)
 * in RTL equals its distance from the left in LTR, within 1 px. The shade is read by pixels: the two edge strips of the box
 * are cut from a screenshot with the rule switched off by an injected style and on, and compared byte for byte.
 */
const ON = process.env.SR_P100_RTL_ON === '1';
const OUT =
  process.env.SR_P100_RTL_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-rtl';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const THEME = 'lounge-light';
const WRAP = '.adm-table-wrap:has(> .inv-table)';
const COMPARE = 'table.sup-compare';
const INV_OFF_CSS = '.adm-table-wrap:has(> .inv-table){background-image:none !important}';
const CMP_OFF_CSS = 'table.sup-compare{background-image:none !important}';
const OFF_ID = 'p100-rtl-rule-off';
// BIP-84 test vector as a signet tpub (the donations tests use the same key).
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Dir = 'ltr' | 'rtl';
interface Item {
  tag: string;
  text: string;
  /** distance from the inline-start edge of the reference, signed so that LTR and RTL read alike */
  start: number;
  w: number;
  h: number;
  top: number;
  cut: number;
}
interface CompareM {
  dir: Dir;
  rootDir: string;
  vw: number;
  doc: number;
  tabScroller: number;
  theme: string | null;
  tableOver: number;
  tableClient: number;
  tableDisplay: string;
  tableOverflowX: string;
  scrollLeft: number;
  bgImage: string;
  matchesRtl: boolean;
  stickyPos: string | null;
  stickyStart: number | null;
  cardStart: number;
  cardW: number;
  items: Item[];
  overlaps: string[];
}

/** Runs in the page: Compare levels' boxes, measured from the inline-start edge of the table (cells) or the card. */
function measureCompare(dir: Dir): CompareM | null {
  const f = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const t = document.querySelector<HTMLElement>('table.sup-compare');
  if (!t) return null;
  const card = t.closest<HTMLElement>('.ss-card')!;
  const tr = t.getBoundingClientRect();
  const cr = card.getBoundingClientRect();
  const startOf = (r: DOMRect, ref: DOMRect) =>
    dir === 'rtl' ? ref.right - r.right : r.left - ref.left;
  const els: { el: HTMLElement; ref: DOMRect }[] = [];
  const cap = t.querySelector<HTMLElement>('caption');
  if (cap) els.push({ el: cap, ref: tr });
  for (const e of Array.from(t.querySelectorAll<HTMLElement>('th, td')))
    els.push({ el: e, ref: tr });
  const h2 = card.querySelector<HTMLElement>('h2');
  if (h2) els.push({ el: h2, ref: cr });
  for (const e of Array.from(
    card.querySelectorAll<HTMLElement>('button, a[href], input, select, textarea'),
  ))
    els.push({ el: e, ref: cr });
  const items: Item[] = els.map(({ el, ref }) => {
    const r = el.getBoundingClientRect();
    return {
      tag: el.tagName.toLowerCase(),
      text: (el.textContent ?? '').trim(),
      start: f(startOf(r, ref)),
      w: f(r.width),
      h: f(r.height),
      top: f(r.top - cr.top),
      cut: Math.max(0, el.scrollWidth - el.clientWidth),
    };
  });
  // Overlap: any two boxes of the same table row or the card's controls that share more than 1 px2 (no ancestors).
  const overlaps: string[] = [];
  const rects = els.map(({ el }) => ({ el, r: el.getBoundingClientRect() }));
  for (let i = 0; i < rects.length; i++)
    for (let j = i + 1; j < rects.length; j++) {
      const a = rects[i]!;
      const b = rects[j]!;
      if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
      const w = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
      const h = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
      if (w > 1 && h > 1)
        overlaps.push(
          `${(a.el.textContent ?? '').trim().slice(0, 14)} x ${(b.el.textContent ?? '').trim().slice(0, 14)} (${f(w)}x${f(h)})`,
        );
    }
  let tabScroller = 0;
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('[role=dialog] *')))
    if (
      !e.matches('table') &&
      /(auto|scroll)/.test(getComputedStyle(e).overflowY) &&
      e.clientWidth > 300
    )
      tabScroller = Math.max(tabScroller, e.scrollWidth - e.clientWidth);
  const cs = getComputedStyle(t);
  const first = t.querySelector<HTMLElement>('tbody tr > :first-child');
  return {
    dir,
    rootDir: de.dir || '(none)',
    vw: de.clientWidth,
    doc: de.scrollWidth - de.clientWidth,
    tabScroller,
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    tableOver: t.scrollWidth - t.clientWidth,
    tableClient: t.clientWidth,
    tableDisplay: cs.display,
    tableOverflowX: cs.overflowX,
    scrollLeft: f(t.scrollLeft),
    bgImage: cs.backgroundImage.replace(/color-mix\([^)]*\)/g, 'cm(..)'),
    matchesRtl: t.matches(':dir(rtl)'),
    stickyPos: first ? getComputedStyle(first).position : null,
    stickyStart: first ? f(startOf(first.getBoundingClientRect(), tr)) : null,
    cardStart: f(dir === 'rtl' ? de.clientWidth - cr.right : cr.left),
    cardW: f(cr.width),
    items,
    overlaps,
  };
}

interface WrapM {
  dir: string;
  doc: number;
  scrollWidth: number;
  clientWidth: number;
  scrollLeft: number;
  beyondLeft: number;
  beyondRight: number;
  layers: { image: string; position: string; size: string; attachment: string }[];
  cells: number;
  heads: { text: string; left: number; right: number }[];
  focusable: string | null;
  theme: string | null;
}

/** Runs in the page: the Invoices wrapper's scroll figures and its computed background layers. */
function measureWrap(sel: string): WrapM | null {
  const f = (n: number) => Math.round(n * 10) / 10;
  const w = document.querySelector<HTMLElement>(sel);
  if (!w) return null;
  const de = document.documentElement;
  const cs = getComputedStyle(w);
  const max = w.scrollWidth - w.clientWidth;
  const rtl = getComputedStyle(w).direction === 'rtl';
  const sl = w.scrollLeft;
  const split = (s: string) => {
    const out: string[] = [];
    let depth = 0;
    let cur = '';
    for (const ch of s) {
      if (ch === '(') depth++;
      if (ch === ')') depth--;
      if (ch === ',' && depth === 0) {
        out.push(cur.trim());
        cur = '';
      } else cur += ch;
    }
    out.push(cur.trim());
    return out;
  };
  const im = split(cs.backgroundImage);
  const po = split(cs.backgroundPosition);
  const sz = split(cs.backgroundSize);
  const at = split(cs.backgroundAttachment);
  return {
    dir: rtl ? 'rtl' : 'ltr',
    doc: de.scrollWidth - de.clientWidth,
    scrollWidth: w.scrollWidth,
    clientWidth: w.clientWidth,
    scrollLeft: f(sl),
    beyondLeft: f(rtl ? max + sl : sl),
    beyondRight: f(rtl ? -sl : max - sl),
    layers: im.map((image, i) => ({
      image,
      position: po[i] ?? '',
      size: sz[i] ?? '',
      attachment: at[i] ?? '',
    })),
    cells: w.querySelectorAll('tbody td').length,
    heads: Array.from(w.querySelectorAll('thead th')).map((th) => {
      const r = th.getBoundingClientRect();
      return {
        text: (th.textContent ?? '').trim().slice(0, 16),
        left: f(r.left),
        right: f(r.right),
      };
    }),
    focusable: w.getAttribute('tabindex'),
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
  };
}

async function seedInvoices(
  ownerReq: APIRequestContext,
  fresh: () => Promise<APIRequestContext>,
): Promise<string[]> {
  const log: string[] = [];
  const step = async <T>(name: string, fn: () => Promise<T>): Promise<T | null> => {
    try {
      const r = await fn();
      log.push(`ok ${name}`);
      return r;
    } catch (e) {
      log.push(`FAIL ${name}: ${String(e).slice(0, 300)}`);
      return null;
    }
  };
  await step('wallet', () =>
    apiPut(ownerReq, '/admin/donations/wallet', {
      descriptor: `wpkh(${TPUB}/0/*)`,
      network: 'signet',
      reason: 'wave 100 rtl shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 100 rtl shots: donations on for the dev stack',
    }),
  );
  const people: { req: APIRequestContext }[] = [];
  for (const [label, name] of [
    ['p100rtlone', 'Bartholomew Featherstonehaugh-Vi'],
    ['p100rtltwo', 'Juniper'],
  ] as const) {
    const user = await registerUser(await fresh(), label);
    const req = await fresh();
    await loginApi(req, user);
    await step(`name ${label}`, () => apiPatch(req, '/users/@me', { global_name: name }));
    people.push({ req });
  }
  const mk = (i: number, body: Record<string, unknown>) =>
    step(`invoice ${i} ${JSON.stringify(body)}`, () =>
      apiPost<{ id: string }>(people[i]!.req, '/donations/invoices', body),
    );
  await mk(0, { purpose: 'patron', months: 12 });
  await mk(0, { purpose: 'donate', usd_cents: 4999 });
  await mk(1, { purpose: 'invite_credit', credits: 3 });
  await mk(1, { purpose: 'donate', usd_cents: 500 });
  return log;
}

test('D-953: Compare levels and the Invoices shade, left-to-right and right-to-left', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const world = JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const stripClips: string[] = [];
  const findings: string[] = [];
  const specErrors: string[] = [];
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };

  // The theme: the account's by PATCH (read back) and localStorage; data-theme read back in each page.
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: THEME, zoom: 100 } },
  });
  const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme set ${THEME}, read back: ${back.settings?.user_prefs?.theme}`);
  if (back.settings?.user_prefs?.theme !== THEME)
    specErrors.push('the account theme did not read back');

  const seedLog = await seedInvoices(
    o,
    async () => (await browser.newContext({ baseURL: ORIGIN })).request,
  );
  for (const l of seedLog) console.log(`REPORT seed ${l}`);
  writeFileSync(resolve(OUT, 'seed-log.txt'), seedLog.join('\n'));

  const mkCtx = async (w: number, h: number, phone: boolean): Promise<BrowserContext> => {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: w, height: h },
      storageState: await ownerCtx.storageState(),
      bypassCSP: true,
      ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
    }, THEME);
    return ctx;
  };
  const shot = async (page: Page, name: string) => {
    const file = `${name}.png`;
    await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
    shots.push(file);
  };
  const setDir = async (page: Page, dir: Dir) => {
    await page.evaluate((d) => (document.documentElement.dir = d), dir);
    await page.waitForTimeout(350);
  };
  const setRuleOff = (page: Page, css: string, off: boolean) =>
    page.evaluate(
      ([id, c, want]) => {
        document.getElementById(id as string)?.remove();
        if (want) {
          const st = document.createElement('style');
          st.id = id as string;
          st.textContent = c as string;
          document.head.appendChild(st);
        }
      },
      [OFF_ID, css, off] as const,
    );
  /** The two edge strips (16 px) of a box, as screenshots, cut from the part inside the viewport (SEC-AUD-447, D-957). */
  const strips = async (page: Page, sel: string) => {
    const { l, r, clip } = await edgeStrips(page, sel, { width: 16, maxHeight: 120 });
    stripClips.push(`${sel} y ${clip.y} h ${clip.height} viewport h ${clip.viewportHeight}`);
    return { l, r };
  };
  const shadeAt = async (page: Page, sel: string, css: string) => {
    await setRuleOff(page, css, true);
    const off = await strips(page, sel);
    await setRuleOff(page, css, false);
    const on = await strips(page, sel);
    return { left: !on.l.equals(off.l), right: !on.r.equals(off.r) };
  };

  // ------------------------------------------------------------------ Compare levels
  const compareRun: Record<string, Record<string, CompareM>> = {};
  const compareHits: Record<string, Record<string, string[]>> = {};
  for (const [w, h, phone] of [
    [1024, 768, false],
    [375, 812, true],
  ] as const) {
    const key = String(w);
    compareRun[key] = {};
    compareHits[key] = {};
    const ctx = await mkCtx(w, h, phone);
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
    await page.goto(`/settings/server/${world.guildId}`, {
      waitUntil: 'domcontentloaded',
      timeout: 15_000,
    });
    await idle(page, 4000);
    const labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    const i = labels.findIndex((l) => /^Server Support$/i.test(l));
    if (i < 0) {
      specErrors.push(`${w}: no nav item Server Support: ${labels.join(' | ')}`);
      await ctx.close();
      continue;
    }
    await page
      .locator(NAV)
      .nth(i)
      .evaluate((el) => (el as HTMLElement).click());
    await idle(page, 2500);
    await page.evaluate(() => document.fonts?.ready);
    if ((await page.locator(COMPARE).count()) === 0) {
      specErrors.push(`${w}: Compare levels did not render`);
      await ctx.close();
      continue;
    }
    for (const dir of ['ltr', 'rtl'] as const) {
      await setDir(page, dir);
      const panel = page.locator('[role="dialog"]').first();
      for (const where of ['start', 'end'] as const) {
        await page.locator(COMPARE).evaluate(
          (t, [wh, d]) => {
            t.scrollIntoView({ block: 'center' });
            const max = t.scrollWidth - t.clientWidth;
            t.scrollLeft = wh === 'start' ? 0 : d === 'rtl' ? -max : max;
          },
          [where, dir] as const,
        );
        await page.mouse.move(0, 0);
        await page.waitForTimeout(250);
        const m = (await page.evaluate(measureCompare, dir))!;
        compareRun[key]![`${dir}-${where}`] = m;
        log({ id: 'compare', w, where, ...m });
        console.log(
          `REPORT compare ${w} ${dir} ${where}: theme ${m.theme}; doc ${m.doc} tabScroller ${m.tabScroller}; table ${m.tableDisplay}/${m.tableOverflowX} over ${m.tableOver} scrollLeft ${m.scrollLeft}; :dir(rtl) ${m.matchesRtl}; sticky ${m.stickyPos} start ${m.stickyStart}; overlaps ${m.overlaps.length}`,
        );
        if (m.theme !== THEME) specErrors.push(`${w} ${dir}: data-theme is ${m.theme}`);
        if (w === 375 || m.tableOver > 1) {
          const sh = await shadeAt(page, COMPARE, CMP_OFF_CSS);
          log({ id: 'compare-shade', w, dir, where, ...sh });
          console.log(
            `REPORT compare ${w} ${dir} ${where} shade: left ${sh.left} right ${sh.right}`,
          );
          (m as CompareM & { shade?: unknown }).shade = sh;
        }
        if (where === 'start') {
          await page.locator(COMPARE).evaluate((t) => t.scrollIntoView({ block: 'center' }));
          await shot(page, `compare-${w}-${dir}`);
        }
      }
      // Hit test over the whole panel, scroll at the start.
      await page.locator(COMPARE).evaluate((t) => {
        t.scrollIntoView({ block: 'center' });
        t.scrollLeft = 0;
      });
      const opened = await openCollapsed(page, panel);
      const rows = await hitTestAll(page, panel);
      // Shut again what the probe opened (the phone's Sections list), so the next direction's shot is of the same page.
      for (const op of opened)
        if (op.kind === 'aria-expanded')
          await panel
            .locator('button[aria-expanded="true"]')
            .filter({ hasText: op.text })
            .first()
            .click()
            .catch(() => undefined);
      await page.waitForTimeout(300);
      compareHits[key]![dir] = rows.map(
        (r) => `${r.status}|${r.name}|${r.result?.points.find((p) => !p.ok)?.taker?.path ?? ''}`,
      );
      const c = (s: string) => rows.filter((r) => r.status === s).length;
      console.log(
        `REPORT compare ${w} ${dir} hit rows ${rows.length}: ${c('pass')} pass, ${c('fail')} fail, ${c('not-reached')} not-reached, ${c('skipped')} skipped`,
      );
      if (c('not-reached'))
        specErrors.push(`${w} ${dir}: hit test has not-reached rows (probe error)`);
    }
    // Back to LTR so nothing leaks, then compare the two directions.
    await setDir(page, 'ltr');
    for (const where of ['start', 'end'] as const) {
      const a = compareRun[key]![`ltr-${where}`];
      const b = compareRun[key]![`rtl-${where}`];
      if (!a || !b) continue;
      if (a.items.length !== b.items.length) {
        findings.push(`${w} ${where}: ${a.items.length} items LTR, ${b.items.length} RTL`);
        continue;
      }
      const notMirrored: string[] = [];
      const textDiff: string[] = [];
      a.items.forEach((it, k) => {
        const jt = b.items[k]!;
        if (
          Math.abs(it.start - jt.start) > 1 ||
          Math.abs(it.w - jt.w) > 1 ||
          Math.abs(it.top - jt.top) > 1
        )
          notMirrored.push(
            `${it.tag} "${it.text.slice(0, 18)}": start ${it.start} vs ${jt.start}, w ${it.w} vs ${jt.w}, top ${it.top} vs ${jt.top}`,
          );
        if (it.text !== jt.text)
          textDiff.push(`"${it.text.slice(0, 20)}" vs "${jt.text.slice(0, 20)}"`);
      });
      const cutL = a.items
        .filter((it) => it.cut > 1)
        .map((it) => `${it.tag} "${it.text.slice(0, 18)}" +${it.cut}`);
      const cutR = b.items
        .filter((it) => it.cut > 1)
        .map((it) => `${it.tag} "${it.text.slice(0, 18)}" +${it.cut}`);
      console.log(
        `REPORT compare ${w} ${where}: ${a.items.length} boxes; not mirrored ${notMirrored.length}; text differs ${textDiff.length}; cut LTR ${cutL.length} RTL ${cutR.length}; page scroll LTR ${a.doc} RTL ${b.doc}; overlaps LTR ${a.overlaps.length} RTL ${b.overlaps.length}; card start ${a.cardStart} vs ${b.cardStart} (w ${a.cardW} vs ${b.cardW}); table over ${a.tableOver} vs ${b.tableOver}`,
      );
      for (const n of notMirrored) console.log(`REPORT   not mirrored ${w} ${where}: ${n}`);
      for (const n of cutL) console.log(`REPORT   cut LTR ${w} ${where}: ${n}`);
      for (const n of cutR) console.log(`REPORT   cut RTL ${w} ${where}: ${n}`);
      if (notMirrored.length)
        findings.push(
          `${w} ${where}: ${notMirrored.length} boxes not mirrored: ${notMirrored.slice(0, 6).join(' ; ')}`,
        );
      if (textDiff.length)
        findings.push(`${w} ${where}: text differs: ${textDiff.slice(0, 4).join(' ; ')}`);
      if (cutR.length > cutL.length)
        findings.push(`${w} ${where}: text cut in RTL only: ${cutR.join(' ; ')}`);
      if (b.doc > 0) findings.push(`${w} ${where}: RTL page scrolls sideways by ${b.doc}`);
      if (a.doc > 0)
        findings.push(`${w} ${where}: LTR page scrolls sideways by ${a.doc} (before RTL)`);
      if (b.tabScroller > 0)
        findings.push(`${w} ${where}: RTL tab scroller overflows by ${b.tabScroller}`);
      if (b.overlaps.length > a.overlaps.length)
        findings.push(`${w} ${where}: overlaps in RTL only: ${b.overlaps.join(' ; ')}`);
      if (where === 'start' && b.overlaps.length)
        findings.push(`${w} start RTL overlaps: ${b.overlaps.join(' ; ')}`);
    }
    const hl = compareHits[key]!['ltr'] ?? [];
    const hr = compareHits[key]!['rtl'] ?? [];
    const sameHits = JSON.stringify(hl) === JSON.stringify(hr);
    console.log(
      `REPORT compare ${w} hit test LTR ${hl.length} rows, RTL ${hr.length} rows, same statuses and takers: ${sameHits}`,
    );
    if (!sameHits)
      findings.push(`${w}: hit test differs LTR ${hl.length} vs RTL ${hr.length} rows`);
    log({ id: 'compare-hits', w, ltr: hl, rtl: hr });
    // The shade: expected inline-end at the start, none at the end.
    const sS = (
      compareRun[key]!['ltr-start'] as
        (CompareM & { shade?: { left: boolean; right: boolean } }) | undefined
    )?.shade;
    const sE = (
      compareRun[key]!['ltr-end'] as
        (CompareM & { shade?: { left: boolean; right: boolean } }) | undefined
    )?.shade;
    const rS = (
      compareRun[key]!['rtl-start'] as
        (CompareM & { shade?: { left: boolean; right: boolean } }) | undefined
    )?.shade;
    const rE = (
      compareRun[key]!['rtl-end'] as
        (CompareM & { shade?: { left: boolean; right: boolean } }) | undefined
    )?.shade;
    if (sS && sE && rS && rE) {
      const line = `${w} shade (strip differs from rule off): LTR start L${sS.left} R${sS.right}, end L${sE.left} R${sE.right}; RTL start L${rS.left} R${rS.right}, end L${rE.left} R${rE.right}`;
      console.log(`REPORT compare ${line}`);
      if (w === 375) {
        if (sS.left || !sS.right || sE.left || sE.right)
          findings.push(`${line}: LTR not as expected (shade at the right at the start only)`);
        if (rS.right || !rS.left || rE.left || rE.right)
          findings.push(`${line}: RTL not as expected (shade at the left at the start only)`);
      }
    }
    await ctx.close();
  }

  // ------------------------------------------------------------------ Invoices wrapper, 1024
  {
    const ctx = await mkCtx(1024, 768, false);
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
    await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    await page
      .locator(NAV)
      .filter({ hasText: /^Invoices$/ })
      .first()
      .evaluate((el) => (el as HTMLElement).click());
    await idle(page, 2500);
    const present = await page
      .locator(WRAP)
      .first()
      .waitFor({ timeout: 8000 })
      .then(() => true)
      .catch(() => false);
    if (!present)
      specErrors.push('Invoices: no table wrapper (are the invoices seeded? see seed-log.txt)');
    else {
      const scrollTo = async (where: 'start' | 'end', dir: Dir) => {
        await page.locator(WRAP).evaluate(
          (w, [wh, d]) => {
            w.scrollIntoView({ block: 'center' });
            const max = w.scrollWidth - w.clientWidth;
            w.scrollLeft = wh === 'start' ? 0 : d === 'rtl' ? -max : max;
          },
          [where, dir] as const,
        );
        await page.mouse.move(0, 0);
        await page.waitForTimeout(250);
      };
      const inv: Record<string, { m: WrapM; shade: { left: boolean; right: boolean } }> = {};
      for (const dir of ['ltr', 'rtl'] as const) {
        await setDir(page, dir);
        for (const where of ['start', 'end'] as const) {
          await scrollTo(where, dir);
          const m = (await page.evaluate(measureWrap, WRAP))!;
          const shade = await shadeAt(page, WRAP, INV_OFF_CSS);
          inv[`${dir}-${where}`] = { m, shade };
          log({ id: 'invoices', where, ...m, shade });
          console.log(
            `REPORT invoices ${dir} ${where}: direction ${m.dir}; client ${m.clientWidth} scroll ${m.scrollWidth}; scrollLeft ${m.scrollLeft}; more table beyond left ${m.beyondLeft}, beyond right ${m.beyondRight}; shade left ${shade.left} right ${shade.right}; doc ${m.doc}; theme ${m.theme}`,
          );
          if (m.theme !== THEME) specErrors.push(`invoices ${dir}: data-theme is ${m.theme}`);
          if (dir === 'rtl') await shot(page, `invoices-1024-rtl-${where}`);
          // The rule: a shade exactly on the edge that has more table beyond it.
          const wantL = m.beyondLeft > 1;
          const wantR = m.beyondRight > 1;
          if (shade.left !== wantL || shade.right !== wantR)
            findings.push(
              `invoices ${dir} ${where}: shade L${shade.left} R${shade.right}, more table beyond L${wantL} R${wantR}`,
            );
        }
      }
      const rs = inv['rtl-start']!.m;
      console.log(`REPORT invoices layers (RTL start): ${JSON.stringify(rs.layers)}`);
      console.log(`REPORT invoices heads (RTL start): ${JSON.stringify(rs.heads)}`);
      // Arrow keys in RTL: the wrapper is not a keyboard stop in this base (tables' part is not here); the probe makes it
      // focusable for the test only, in the page, to read which way ArrowLeft and ArrowRight move it.
      await setDir(page, 'rtl');
      await scrollTo('start', 'rtl');
      const keys = await page.locator(WRAP).evaluate(async (w) => {
        w.setAttribute('tabindex', '0');
        w.focus();
        const sl0 = w.scrollLeft;
        return { sl0, focused: document.activeElement === w };
      });
      await page.keyboard.press('ArrowLeft');
      await page.waitForTimeout(500);
      const afterLeft = await page.locator(WRAP).evaluate((w) => w.scrollLeft);
      await page.keyboard.press('ArrowRight');
      await page.waitForTimeout(500);
      const afterRight = await page.locator(WRAP).evaluate((w) => w.scrollLeft);
      await page.locator(WRAP).evaluate((w) => w.removeAttribute('tabindex'));
      console.log(
        `REPORT invoices RTL keys (wrapper made focusable for the probe only): focused ${keys.focused}; scrollLeft ${keys.sl0} -> ArrowLeft ${afterLeft} -> ArrowRight ${afterRight}`,
      );
      log({ id: 'invoices-keys', ...keys, afterLeft, afterRight });
      await setDir(page, 'ltr');
    }
    await ctx.close();
  }

  await ownerCtx.close();
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT strips ${JSON.stringify(stripClips)}`);
  console.log(`REPORT findings ${JSON.stringify(findings)}`);
  console.log(`REPORT specErrors ${JSON.stringify(specErrors)}`);
  writeFileSync(
    resolve(OUT, 'totals.json'),
    JSON.stringify({ shots, findings, specErrors }, null, 1),
  );
  if (specErrors.length) throw new Error(`spec errors: ${specErrors.join(' ; ')}`);
});
