import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-371 (wave 85; D-874): Admin's shared table (`DataTable`) as stacked rows on a phone. Needs the visual sweep's
 * seeded world: the Owner's state is `e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world" (that spec is never edited); this spec reads only the Owner's session from it.
 *
 * Settings: 375x812 (touch, a mobile user agent) in Lounge Light and Lounge Dark, and 1280x800 in Editorial (the
 * account's main theme by PATCH and localStorage, `data-theme` read back from the page and written into the file name).
 * Pages (Admin routes): Accounts, Servers, Applications (the Review hub's tab), Platform audit log and Library catalog
 * (the five that draw `DataTable`), Access log and Invoices (plain `.adm-table`: they must not move).
 *
 * The BASE case (before the change) is the same build with the class `adm-table-stack` taken off the tables: every rule
 * of the change is scoped to that class, so the page then draws exactly as the base tree did (the `data-label`
 * attributes draw nothing). Per page and setting the spec measures the base (class off), puts the class back and
 * measures again, so "before" and "after" come from one page load. Per measurement: the wrapper's sideways scroll, the
 * boxes of the cells and of the actions, those past the viewport's edge, the hit test of D-856 on each action (box
 * centre -21 / +21 px), the contrast of each label (computed colours composited over the first opaque ground behind
 * it), the lines of each value (ids and hashes apart), empty label lines, the boxes of the table, its wrapper, the
 * toolbar's buttons and "Load more". Chooser (Accounts, phone): hide a column, its label line is gone; show it, the
 * line is back. Click (Accounts, phone): open the first account from its row's action, come back to the list.
 *
 * Local only (SR_P85_ADMIN=1); out SR_P85_ADMIN_OUT; SR_P85_ONLY a regex over page ids; SR_P85_SETTINGS a comma list of
 * `phone-light`, `phone-dark`, `desk` (default all); SR_P85_BASE=1 measures and shoots only the base case (class off).
 */
const ON = process.env.SR_P85_ADMIN === '1';
const OUT =
  process.env.SR_P85_ADMIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-admin';
const ONLY = process.env.SR_P85_ONLY ? new RegExp(process.env.SR_P85_ONLY) : null;
const BASE_ONLY = process.env.SR_P85_BASE === '1';
const WANT = (process.env.SR_P85_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
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
  { id: 'accounts', path: '/admin/accounts' },
  { id: 'servers', path: '/admin/servers' },
  { id: 'applications', path: '/admin/review', tab: /^All applications$/ },
  { id: 'audit-log', path: '/admin/audit-log' },
  { id: 'library-catalog', path: '/admin/library-catalog' },
  { id: 'access-log', path: '/admin/access-log' },
  { id: 'donation-invoices', path: '/admin/donation-invoices' },
];
/** Shot before the change as well (phone, Lounge Dark): the two pages the fault was found on, and the two that stay. */
const BEFORE_SHOTS = ['accounts', 'servers', 'access-log', 'donation-invoices'];

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

test('SEC-AUD-371: the shared Admin table, stacked rows on a phone', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const totals = { shots: 0, lines: 0, failed: [] as string[] };
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
            mode === 'after' ||
            (phone && s.theme === 'lounge-dark' && BEFORE_SHOTS.includes(p.id)) ||
            (!phone && p.id === 'accounts');
          if (wanted) await shoot(mode);
          if (phone && mode === 'after' && m.rows > 3 && p.id === 'accounts') {
            await page.evaluate(() =>
              document
                .querySelector('table.adm-table tbody tr:nth-child(4)')
                ?.scrollIntoView({ block: 'start' }),
            );
            await page.waitForTimeout(250);
            await page.screenshot({
              path: resolve(OUT, `${stem}-after-rows.png`),
              timeout: 10_000,
            });
            totals.shots++;
          }
          appendFileSync(jsonl, `${JSON.stringify({ id: p.id, key: s.key, mode, ...m })}\n`);
          totals.lines++;
          const mc = m.minContrast === null ? '-' : String(Math.round(m.minContrast * 100) / 100);
          console.log(
            `REPORT ${s.key} ${th} ${p.id} ${mode}: tables ${m.tables} rows ${m.rows} empty ${m.empty} doc ${m.doc} wrapOver ${m.wrapOver} cellsOff ${m.cellsOff.length} actions ${m.actions} actionsOff ${m.actionsOff.length} hitFail ${m.hitFail.length} minContrast ${mc} tall ${m.tallValues.join(',')} emptyLines ${m.emptyLabelLines}`,
          );
        }
        await setStack(page, true);

        // Chooser and click-through: Accounts on a phone, once per theme.
        if (phone && p.id === 'accounts' && !BASE_ONLY) {
          const lab = () =>
            page.evaluate(() => {
              const t = document.querySelector('table.adm-table[aria-label]')!;
              const tds = Array.from(t.querySelectorAll('tbody tr:first-child td[data-label]'));
              const shown = tds.filter((e) => getComputedStyle(e).display !== 'none');
              return {
                emailCells: t.querySelectorAll('tbody td[data-label="Email"]').length,
                lines: shown.map((e) => e.getAttribute('data-label')),
                emptyShown: shown.filter((e) => (e.textContent ?? '').trim() === '').length,
              };
            });
          const was = await lab();
          const cols = page.getByRole('button', { name: /^Columns$/ });
          await cols.click();
          const box = page.getByRole('checkbox', { name: 'Email' });
          await box.evaluate((e) => (e as HTMLElement).click());
          await page.waitForTimeout(300);
          const hiddenState = await lab();
          await page.screenshot({
            path: resolve(OUT, `${stem}-chooser-hidden.png`),
            timeout: 10_000,
          });
          totals.shots++;
          await box.evaluate((e) => (e as HTMLElement).click());
          await page.waitForTimeout(300);
          const shownAgain = await lab();
          await cols.click();
          console.log(
            `REPORT ${s.key} chooser: was ${JSON.stringify(was)} hidden ${JSON.stringify(hiddenState)} again ${JSON.stringify(shownAgain)}`,
          );
          appendFileSync(
            jsonl,
            `${JSON.stringify({ chooser: true, key: s.key, was, hiddenState, shownAgain })}\n`,
          );

          const view = page.locator('table.adm-table td[data-actions] button').first();
          const name =
            (await page.locator('table.adm-table tbody tr td').first().textContent()) ?? '';
          await view.scrollIntoViewIfNeeded();
          await view.click();
          await page.waitForTimeout(1500);
          const dlg = await page.locator('[role="dialog"]').count();
          const text = await page
            .locator('[role="dialog"]')
            .last()
            .evaluate((e) => (e.textContent ?? '').slice(0, 200));
          await page.screenshot({
            path: resolve(OUT, `${stem}-click-account.png`),
            timeout: 10_000,
          });
          totals.shots++;
          await page.keyboard.press('Escape');
          await page.waitForTimeout(800);
          const tableBack = await page.locator('table.adm-table tbody tr').count();
          console.log(
            `REPORT ${s.key} click: row "${name.trim()}" dialogs ${dlg} text "${text.replace(/\s+/g, ' ').slice(0, 120)}" table back rows ${tableBack}`,
          );
          appendFileSync(
            jsonl,
            `${JSON.stringify({ click: true, key: s.key, name, dlg, text, tableBack })}\n`,
          );
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
