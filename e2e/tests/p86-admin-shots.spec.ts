import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-375 (wave 86; D-881): Admin's three hand-made tables (Access log, the Crash reports list, the Cinema catalog)
 * as stacked rows on a phone. NEEDS THE VISUAL SWEEP'S SEEDED WORLD: the Owner's state is
 * `e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep world" (never edited); this spec reads
 * only the Owner's session from it, and makes one crash report through the API (POST /crash-reports, one call) so the
 * list has a row. The Cinema catalog has no row in the world (a title takes several calls): its empty state is shot
 * and listed.
 *
 * Settings: 375x812 (touch, a mobile user agent) in Lounge Light and Lounge Dark, and 1280x800 in Editorial (the
 * account's main theme by PATCH and localStorage, `data-theme` read back from the page and written into the file name).
 * Pages: Access log, Crash reports, Cinema catalog (the three), Cinema transcoding and Donation invoices (plain
 * `.adm-table` users that must not move), and, at 1280 only, Accounts (the caption probe).
 *
 * The BASE case is the same build with the class `adm-table-stack` taken off the tables: every rule of the change is
 * scoped to that class, so the page draws as the base tree did. Per page and setting the spec measures the base (class
 * off), puts the class back and measures again. Per measurement: the wrapper's sideways scroll, cells and actions past
 * the viewport edge, the hit test of D-856 on each action, the contrast of each label, the lines of each value, the
 * boxes of table and wrapper and of the first six rows' cells (compare before and after at 1280: 0 px moved).
 * Caption probe (Accounts, 1280, Editorial): the table caption's box and `clip`. Click (Crash reports, phone): open the
 * first report from its row's button, come back to the list.
 *
 * Local only. The commander's command (from `e2e/`, on a stack with the sweep world seeded; ports 8644/5644):
 *   SR_P86_ADMIN=1 SR_E2E_BASE_URL=http://localhost:5644 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<url> DATABASE_URL=<url> \
 *   SR_P86_ADMIN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-admin \
 *   npx playwright test tests/p86-admin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Variables (all optional except SR_P86_ADMIN=1): SR_P86_ADMIN_OUT; SR_P86_ADMIN_ONLY a regex over page ids;
 * SR_P86_ADMIN_SETTINGS a comma list of `phone-light`, `phone-dark`, `desk` (default all); SR_P86_ADMIN_BASE=1
 * measures and shoots only the base case (class off).
 */
const ON = process.env.SR_P86_ADMIN === '1';
const OUT =
  process.env.SR_P86_ADMIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-admin';
const ONLY = process.env.SR_P86_ADMIN_ONLY ? new RegExp(process.env.SR_P86_ADMIN_ONLY) : null;
const BASE_ONLY = process.env.SR_P86_ADMIN_BASE === '1';
const WANT = (process.env.SR_P86_ADMIN_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;
const SETTINGS = [
  { key: 'phone-light', size: 'phone' as Size, theme: 'lounge-light' },
  { key: 'phone-dark', size: 'phone' as Size, theme: 'lounge-dark' },
  { key: 'desk', size: 'desktop' as Size, theme: 'lounge-editorial' },
].filter((s) => WANT.includes(s.key));

interface PageDef {
  id: string;
  path: string;
  /** The Review hub's tab to open after the route loads. */
  tab?: RegExp;
}
const PAGES: PageDef[] = [
  { id: 'access-log', path: '/admin/access-log' },
  { id: 'crash-reports', path: '/admin/crash-reports' },
  { id: 'cinema-catalog', path: '/admin/cinema-catalog' },
  { id: 'cinema-transcoding', path: '/admin/cinema-transcoding' },
  { id: 'donation-invoices', path: '/admin/donation-invoices' },
  { id: 'accounts', path: '/admin/accounts' },
];
/** The three pages: shot before the change as well (phone, Lounge Dark). */
const THE_THREE = ['access-log', 'crash-reports', 'cinema-catalog'];

test.skip(!ON, 'local visual check only');
test.setTimeout(1_200_000);

interface Measured {
  vw: number;
  theme: string | null;
  tables: number;
  rows: number;
  empty: boolean;
  doc: number;
  wrapOver: number;
  tableBox: number[] | null;
  wrapBox: number[] | null;
  cellsOff: string[];
  actionsOff: string[];
  actions: number;
  hitFail: string[];
  minContrast: number | null;
  lowContrast: string[];
  tallValues: string[];
  emptyLabelLines: number;
  firstRow: { title: string; lines: string[] } | null;
  tools: Record<string, number[] | null>;
  cells: number[][];
}

/** In the page: everything one measurement logs. `phone` turns on the hit test and the contrast. */
function measureInPage(phone: boolean): Measured {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const rnd = (n: number) => Math.round(n * 10) / 10;
  const vp = vw * de.clientHeight;
  const sc =
    Array.from(document.querySelectorAll<HTMLElement>('body *'))
      .filter((e) => {
        if (e.clientWidth === 0 || e.closest('nav')) return false;
        const cs = getComputedStyle(e);
        return (
          cs.display !== 'none' &&
          /(auto|scroll|overlay)/.test(cs.overflowY) &&
          e.clientWidth * e.clientHeight >= vp * 0.2
        );
      })
      .sort((a, b) => b.scrollHeight - a.scrollHeight)[0] ?? null;
  if (sc) sc.scrollTop = 0;
  const top0 = sc ? sc.getBoundingClientRect().top : 0;
  const left0 = sc ? sc.getBoundingClientRect().left : 0;
  const sbox = (e: Element | null) => {
    if (!e) return null;
    const r = e.getBoundingClientRect();
    return [rnd(r.left - left0), rnd(r.top - top0), rnd(r.width), rnd(r.height)];
  };
  const table = document.querySelector<HTMLElement>('table.adm-table[aria-label]');
  const wrap = table?.closest<HTMLElement>('.adm-table-wrap') ?? null;
  const themed =
    document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]');
  const out: Measured = {
    vw,
    theme: themed?.getAttribute('data-theme') ?? null,
    tables: document.querySelectorAll('table.adm-table').length,
    rows: table ? table.querySelectorAll('tbody tr').length : 0,
    empty: !table,
    doc: de.scrollWidth - vw,
    wrapOver: wrap ? wrap.scrollWidth - wrap.clientWidth : 0,
    tableBox: sbox(table),
    wrapBox: sbox(wrap),
    cellsOff: [],
    actionsOff: [],
    actions: 0,
    hitFail: [],
    minContrast: null,
    lowContrast: [],
    tallValues: [],
    emptyLabelLines: 0,
    firstRow: null,
    tools: {},
    cells: [],
  };
  const btnBox = (re: RegExp) =>
    sbox(
      Array.from(document.querySelectorAll('button')).find((b) =>
        re.test((b.textContent ?? '').trim()),
      ) ?? null,
    );
  out.tools = {
    columns: btnBox(/^Columns$/),
    exportCsv: btnBox(/^Export CSV$/),
    loadMore: btnBox(/^Load more$/),
  };
  if (!table) return out;

  // Contrast: a computed colour (any syntax) to RGBA through a canvas pixel.
  const cv = document.createElement('canvas');
  cv.width = 1;
  cv.height = 1;
  const ctx = cv.getContext('2d', { willReadFrequently: true })!;
  const rgba = (c: string): number[] => {
    ctx.clearRect(0, 0, 1, 1);
    ctx.fillStyle = '#000';
    ctx.fillStyle = c;
    ctx.fillRect(0, 0, 1, 1);
    const d = ctx.getImageData(0, 0, 1, 1).data;
    return [d[0]!, d[1]!, d[2]!, d[3]! / 255];
  };
  const ground = (e: Element): number[] => {
    let acc = [255, 255, 255];
    const stack: number[][] = [];
    for (let n: Element | null = e; n; n = n.parentElement) {
      const c = rgba(getComputedStyle(n).backgroundColor);
      if (c[3]! > 0) stack.push(c);
      if (c[3]! >= 1) break;
    }
    for (const c of stack.reverse())
      acc = [0, 1, 2].map((i) => acc[i]! * (1 - c[3]!) + c[i]! * c[3]!);
    return acc;
  };
  const lum = (c: number[]) => {
    const f = (v: number) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * f(c[0]!) + 0.7152 * f(c[1]!) + 0.0722 * f(c[2]!);
  };
  const ratio = (a: number[], b: number[]) => {
    const [l1, l2] = [lum(a), lum(b)].sort((x, y) => y - x);
    return (l1! + 0.05) / (l2! + 0.05);
  };
  const lineCount = (e: Element) => {
    const rg = document.createRange();
    rg.selectNodeContents(e);
    const tops: number[] = [];
    for (const q of Array.from(rg.getClientRects()).sort((a, b) => a.top - b.top))
      if (q.width > 0 && (tops.length === 0 || q.top - tops[tops.length - 1]! > 6))
        tops.push(q.top);
    return tops.length;
  };

  const trs = Array.from(table.querySelectorAll('tbody tr'));
  trs.forEach((tr, ri) => {
    Array.from(tr.children).forEach((td) => {
      const cs = getComputedStyle(td);
      if (cs.display === 'none') return;
      const r = td.getBoundingClientRect();
      if (r.width === 0) return;
      const isAct = td.hasAttribute('data-actions') || td === tr.lastElementChild;
      const kind =
        td.getAttribute('data-label') ?? (td.hasAttribute('data-actions') ? 'actions' : 'cell');
      const tag = `${ri}:${kind}`;
      if (ri < 6) out.cells.push(Array.from(sbox(td) ?? []));
      if (r.left < -0.5 || r.right > vw + 0.5) out.cellsOff.push(tag);
      const txt = (td.textContent ?? '').trim();
      const ctls = Array.from(td.querySelectorAll<HTMLElement>('button, a[href], select, input'));
      if (txt === '' && ctls.length === 0) out.emptyLabelLines++;
      // Value lines (ids and hashes, in .adm-mono or by shape, may wrap).
      if (!isAct) {
        const mono = td.querySelector('.adm-mono') !== null || /^[0-9a-f-]{20,}$/i.test(txt);
        if (!mono && lineCount(td) > 3) out.tallValues.push(`${tag}=${lineCount(td)}`);
      }
      // The label's contrast (phone: the ::before text).
      if (phone && td.hasAttribute('data-label')) {
        const pb = getComputedStyle(td, '::before');
        if (pb.content !== 'none' && pb.content !== 'normal') {
          const c = ratio(rgba(pb.color), ground(td));
          out.minContrast = out.minContrast === null ? c : Math.min(out.minContrast, c);
          if (c < 4.5) out.lowContrast.push(`${tag}=${rnd(c)}`);
        }
      }
      {
        for (const e of ctls.filter((x) => isAct || x.tagName === 'BUTTON')) {
          const b = e.getBoundingClientRect();
          if (b.width === 0) continue;
          out.actions++;
          if (b.left < -0.5 || b.right > vw + 0.5)
            out.actionsOff.push(`${ri}:${(e.textContent ?? '').trim()}`);
          if (phone) {
            e.scrollIntoView({ block: 'center' });
            const q = e.getBoundingClientRect();
            const cx = q.left + q.width / 2;
            const cy = q.top + q.height / 2;
            const ok = (y: number) => {
              const h = document.elementFromPoint(cx, y);
              return h !== null && (h === e || e.contains(h) || h.closest('label') !== null);
            };
            if (!(ok(cy - 21) && ok(cy + 21)))
              out.hitFail.push(`${ri}:${(e.textContent ?? '').trim()} h=${rnd(q.height)}`);
          }
        }
      }
    });
  });
  const tr0 = trs[0];
  if (tr0) {
    out.firstRow = {
      title: (tr0.firstElementChild?.textContent ?? '').trim().slice(0, 40),
      lines: Array.from(tr0.children)
        .slice(1)
        .filter((c) => getComputedStyle(c).display !== 'none')
        .map(
          (c) =>
            `${c.getAttribute('data-label') ?? 'actions'}: ${(c.textContent ?? '').trim().slice(0, 30)}`,
        ),
    };
  }
  if (sc) sc.scrollTop = 0;
  window.scrollTo(0, 0);
  return out;
}

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

async function newPage(browser: Browser, size: Size, theme: string, base: BrowserContext) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: await base.storageState(),
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
  }, theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

/** Only the tables that carry the class when the page loads (DataTable's own) are switched; plain tables are left alone. */
const setStack = (page: Page, on: boolean) =>
  page.evaluate((v) => {
    document.querySelectorAll('table.adm-table[aria-label]').forEach((t) => {
      if (!t.hasAttribute('data-dt'))
        t.setAttribute('data-dt', t.classList.contains('adm-table-stack') ? '1' : '0');
      if (t.getAttribute('data-dt') === '1') t.classList.toggle('adm-table-stack', v);
    });
  }, on);

test('SEC-AUD-375: Access log, Crash reports and Cinema catalog, stacked rows on a phone', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const totals = { shots: 0, lines: 0, failed: [] as string[] };
  // One crash report through the API (one call), so the Crash reports list has a row to stack and to open.
  try {
    const sent = await apiPost<{ id: string; count: number }>(ownerCtx.request, '/crash-reports', {
      commit: '0a1b2c3d4e5f6a7b8c9d',
      kind: 'error',
      route: 'lounge.:guildId.channels.:channelId',
      at: Date.now(),
      frames: [{ file: 'assets/index-Bx3k9Qa1.js', line: 12, col: 3, fn: 'renderChannel' }],
    });
    console.log(`REPORT crash report made: ${JSON.stringify(sent)}`);
  } catch (e) {
    console.log(`REPORT crash report not made: ${String(e).slice(0, 160)}`);
  }
  const themeOf = (page: Page) =>
    page.evaluate(
      () =>
        (
          document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
        )?.getAttribute('data-theme') ?? 'no-theme',
    );

  const open = async (page: Page, p: PageDef) => {
    await page.goto(p.path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    if (p.tab) {
      await page
        .getByRole('tab', { name: p.tab })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 3000);
    }
    await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  };

  for (const s of SETTINGS) {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
    const { ctx, page } = await newPage(browser, s.size, s.theme, ownerCtx);
    const phone = s.size === 'phone';
    for (const p of PAGES) {
      if (ONLY && !ONLY.test(p.id)) continue;
      if (p.id === 'accounts' && phone) continue;
      try {
        await open(page, p);
        const th = await themeOf(page);
        const stem = `${p.id}-${phone ? '375' : '1280'}-${th}`;
        const shoot = async (tag: string) => {
          await page.evaluate(() => window.scrollTo(0, 0));
          await page.screenshot({ path: resolve(OUT, `${stem}-${tag}.png`), timeout: 10_000 });
          totals.shots++;
        };
        for (const mode of BASE_ONLY ? ['before'] : ['before', 'after']) {
          // before: the class off (the base tree's drawing); after: the change as built.
          await setStack(page, mode === 'after');
          await page.waitForTimeout(250);
          const m = await page.evaluate(measureInPage, phone);
          const wanted =
            (mode === 'after' && THE_THREE.includes(p.id)) ||
            (mode === 'after' &&
              p.id === 'donation-invoices' &&
              phone &&
              s.theme === 'lounge-dark') ||
            (mode === 'before' && phone && s.theme === 'lounge-dark' && THE_THREE.includes(p.id));
          if (wanted) await shoot(mode);
          appendFileSync(jsonl, `${JSON.stringify({ id: p.id, key: s.key, mode, ...m })}\n`);
          totals.lines++;
          const mc = m.minContrast === null ? '-' : String(Math.round(m.minContrast * 100) / 100);
          console.log(
            `REPORT ${s.key} ${th} ${p.id} ${mode}: tables ${m.tables} rows ${m.rows} empty ${m.empty} doc ${m.doc} wrapOver ${m.wrapOver} cellsOff ${m.cellsOff.length} actions ${m.actions} actionsOff ${m.actionsOff.length} hitFail ${m.hitFail.length} minContrast ${mc} tall ${m.tallValues.join(',')} emptyLines ${m.emptyLabelLines}`,
          );
        }
        await setStack(page, true);

        // Caption probe: Accounts at 1280 (wave 85's .adm-vh, never seen in a browser).
        if (!phone && p.id === 'accounts') {
          const probe = await page.evaluate(() => {
            const cap = document.querySelector('table.adm-table caption');
            const vh = document.querySelector('table.adm-table .adm-vh, table.adm-table .us-sr');
            const info = (e: Element | null) => {
              if (!e) return null;
              const r = e.getBoundingClientRect();
              const cs = getComputedStyle(e);
              return {
                box: [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10),
                clip: cs.clip,
                position: cs.position,
                overflow: cs.overflow,
                text: (e.textContent ?? '').trim().slice(0, 40),
              };
            };
            return { caption: info(cap), vh: info(vh) };
          });
          console.log(`REPORT caption probe ${JSON.stringify(probe)}`);
          appendFileSync(jsonl, `${JSON.stringify({ probe: true, ...probe })}\n`);
        }

        // Click-through: open the first crash report from its row, come back to the list (phone, once per theme).
        if (phone && p.id === 'crash-reports' && !BASE_ONLY) {
          const btn = page.locator('table.adm-table td[data-actions] button').first();
          const rowsBefore = await page.locator('table.adm-table tbody tr').count();
          if ((await btn.count()) === 0) {
            console.log(`REPORT ${s.key} click: no row to open (rows ${rowsBefore})`);
          } else {
            await btn.scrollIntoViewIfNeeded();
            await btn.click();
            await page.waitForTimeout(1500);
            const back = page.getByRole('button', { name: 'Back to the list' });
            const detail = await back.count();
            const frames = await page
              .locator('table[aria-label="Stack frames, newest first"]')
              .count();
            await page.screenshot({
              path: resolve(OUT, `${stem}-click-report.png`),
              timeout: 10_000,
            });
            totals.shots++;
            if (detail) await back.click();
            await page.waitForTimeout(800);
            const tableBack = await page.locator('table.adm-table tbody tr').count();
            console.log(
              `REPORT ${s.key} click: detail opened ${detail === 1} frames table ${frames} back rows ${tableBack}`,
            );
            appendFileSync(
              jsonl,
              `${JSON.stringify({ click: true, key: s.key, detail, frames, tableBack })}\n`,
            );
          }
        }
      } catch (e) {
        totals.failed.push(`${s.key} ${p.id}`);
        console.log(`REPORT ${s.key} ${p.id}: NOT MEASURED ${String(e).slice(0, 200)}`);
      }
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify(totals, null, 1));
});
