import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-379 (wave 87; D-889): the Crash reports list has no sideways scroll at 1280. NEEDS THE VISUAL SWEEP'S SEEDED
 * WORLD: the Owner's state is `e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"
 * (never edited); this spec reads only the Owner's session from it and makes one crash report through the API
 * (POST /crash-reports, one call) so the list has a row with a long route.
 *
 * Settings: 1280x800 in Editorial and in Lounge Light, 375x812 (touch) in Lounge Dark (the account's main theme by PATCH
 * and localStorage, `data-theme` read back from the page and written into the file name). Page: /admin/crash-reports.
 * The BASE case is the same build with a style tag that gives the route cell `white-space: nowrap` back (what
 * crash.css did before the change; on the phone the stacked rows already wrap by admin.css, so the base there is the
 * page as it is without the new rule: no tag); the spec measures with it (before), removes it, measures again (after). Per
 * measurement: the wrapper's sideways scroll, each column's width, the wide cell, the actions cell against the wrapper,
 * each row's height, the boxes of every cell, the lines of the route. Click: open the first report at 1280 and
 * compare its tables' boxes before and after (0 px moved), shot the report.
 *
 * Local only. The commander's command (from `e2e/`, on a stack with the sweep world seeded; ports 8654/5654):
 *   SR_P87_ADMIN_ON=1 SR_E2E_BASE_URL=http://localhost:5654 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<url> DATABASE_URL=<url> \
 *   SR_P87_ADMIN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-admin \
 *   npx playwright test tests/p87-admin-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Variables (all optional except SR_P87_ADMIN_ON=1): SR_P87_ADMIN_OUT; SR_P87_ADMIN_SETTINGS a comma list of
 * `desk-editorial`, `desk-light`, `phone-dark` (default all); SR_P87_ADMIN_BASE=1 measures and shoots only the base case.
 */
const ON = process.env.SR_P87_ADMIN_ON === '1';
const OUT =
  process.env.SR_P87_ADMIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-admin';
const BASE_ONLY = process.env.SR_P87_ADMIN_BASE === '1';
const WANT = (process.env.SR_P87_ADMIN_SETTINGS ?? 'desk-editorial,desk-light,phone-dark').split(
  ',',
);
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'desk-editorial', phone: false, theme: 'lounge-editorial' },
  { key: 'desk-light', phone: false, theme: 'lounge-light' },
  { key: 'phone-dark', phone: true, theme: 'lounge-dark' },
].filter((s) => WANT.includes(s.key));

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

const BASE_CSS =
  '.adm-crash-table td.adm-mono:nth-last-child(2){white-space:nowrap!important;overflow-wrap:normal!important}';

function measureInPage() {
  const rnd = (n: number) => Math.round(n * 10) / 10;
  const de = document.documentElement;
  const table = document.querySelector<HTMLElement>('table.adm-crash-table');
  const wrap = table?.closest<HTMLElement>('.adm-table-wrap') ?? null;
  if (!table || !wrap) return null;
  const wr = wrap.getBoundingClientRect();
  const heads = Array.from(table.querySelectorAll('thead th')).map((th) =>
    rnd(th.getBoundingClientRect().width),
  );
  const rows = Array.from(table.querySelectorAll('tbody tr'));
  const first = Array.from(rows[0]?.children ?? []);
  const wide = first
    .map((c, i) => ({ i, w: rnd(c.getBoundingClientRect().width) }))
    .sort((a, b) => b.w - a.w)[0];
  const act = table.querySelector<HTMLElement>('td[data-actions] button');
  const ar = act?.getBoundingClientRect();
  const route = table.querySelector<HTMLElement>('td.adm-mono:nth-last-child(2)');
  const rg = document.createRange();
  const lines = (e: Element | null) => {
    if (!e) return 0;
    rg.selectNodeContents(e);
    const tops: number[] = [];
    for (const q of Array.from(rg.getClientRects()).sort((a, b) => a.top - b.top))
      if (q.width > 0 && (tops.length === 0 || q.top - tops[tops.length - 1]! > 6))
        tops.push(q.top);
    return tops.length;
  };
  return {
    vw: de.clientWidth,
    doc: de.scrollWidth - de.clientWidth,
    wrapOver: wrap.scrollWidth - wrap.clientWidth,
    wrapW: rnd(wr.width),
    tableW: rnd(table.getBoundingClientRect().width),
    heads,
    wideCol: wide ?? null,
    rows: rows.length,
    rowH: rows.map((r) => rnd(r.getBoundingClientRect().height)),
    routeLines: lines(route),
    routeText: (route?.textContent ?? '').trim(),
    actionInside: ar ? ar.left >= wr.left - 0.5 && ar.right <= wr.right + 0.5 : null,
    actionBox: ar ? [rnd(ar.left), rnd(ar.top), rnd(ar.width), rnd(ar.height)] : null,
    cells: rows.flatMap((r) =>
      Array.from(r.children).map((c) => {
        const b = c.getBoundingClientRect();
        return [rnd(b.left), rnd(b.top - wr.top), rnd(b.width), rnd(b.height)];
      }),
    ),
  };
}

const measureDetail = () => {
  const rnd = (n: number) => Math.round(n * 10) / 10;
  return Array.from(document.querySelectorAll('table')).map((t) => {
    const r = t.getBoundingClientRect();
    return {
      label: t.getAttribute('aria-label'),
      box: [rnd(r.left), rnd(r.top), rnd(r.width), rnd(r.height)],
      cells: Array.from(t.querySelectorAll('td,th')).map((c) => {
        const b = c.getBoundingClientRect();
        return [rnd(b.left), rnd(b.top), rnd(b.width), rnd(b.height)];
      }),
    };
  });
};

const setBase = (page: Page, base: boolean) =>
  page.evaluate(
    ([on, css]) => {
      document.getElementById('p87-base')?.remove();
      if (on) {
        const s = document.createElement('style');
        s.id = 'p87-base';
        s.textContent = css as string;
        document.head.appendChild(s);
      }
    },
    [base, BASE_CSS] as const,
  );

async function newPage(browser: Browser, phone: boolean, theme: string, base: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
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

test('SEC-AUD-379: the Crash reports list has no sideways scroll at 1280', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const totals = { shots: 0, lines: 0, failed: [] as string[] };
  try {
    const sent = await apiPost<{ id: string; count: number }>(ownerCtx.request, '/crash-reports', {
      commit: '0a1b2c3d4e5f6a7b8c9d',
      kind: 'error',
      route: 'lounge.:guildId.channels.:channelId.threads.:threadId',
      at: Date.now(),
      frames: [{ file: 'assets/index-Bx3k9Qa1.js', line: 12, col: 3, fn: 'renderChannel' }],
    });
    console.log(`REPORT crash report made: ${JSON.stringify(sent)}`);
  } catch (e) {
    console.log(`REPORT crash report not made: ${String(e).slice(0, 160)}`);
  }
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
    const { ctx, page } = await newPage(browser, s.phone, s.theme, ownerCtx);
    try {
      await page.goto('/admin/crash-reports', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
      await page.waitForSelector('table.adm-crash-table', { timeout: 8000 });
      await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
      const th = await page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
      const stem = `crash-reports-${s.phone ? '375' : '1280'}-${th}`;
      const want = (mode: string) =>
        mode === 'after' || (mode === 'before' && s.key === 'desk-editorial');
      for (const mode of BASE_ONLY ? ['before'] : ['before', 'after']) {
        await setBase(page, mode === 'before' && !s.phone);
        await page.waitForTimeout(250);
        const m = await page.evaluate(measureInPage);
        await page.evaluate(() => window.scrollTo(0, 0));
        if (want(mode)) {
          await page.screenshot({ path: resolve(OUT, `${stem}-${mode}.png`), timeout: 10_000 });
          totals.shots++;
        }
        appendFileSync(jsonl, `${JSON.stringify({ key: s.key, mode, th, ...m })}\n`);
        totals.lines++;
        console.log(`REPORT ${s.key} ${th} ${mode}: ${JSON.stringify({ ...m, cells: undefined })}`);
      }
      // Click: open the first report (desktop only), its tables before and after.
      if (!s.phone && !BASE_ONLY) {
        const btn = page.locator('table.adm-crash-table td[data-actions] button').first();
        await btn.click();
        await page.waitForTimeout(1500);
        const detail = await page.getByRole('button', { name: 'Back to the list' }).count();
        const figs: Record<string, unknown> = {};
        for (const mode of ['before', 'after']) {
          await setBase(page, mode === 'before');
          await page.waitForTimeout(200);
          figs[mode] = await page.evaluate(measureDetail);
        }
        const moved = JSON.stringify(figs.before) === JSON.stringify(figs.after) ? 0 : 1;
        if (s.key === 'desk-editorial') {
          await page.evaluate(() => window.scrollTo(0, 0));
          await page.screenshot({
            path: resolve(OUT, `${stem}-click-report.png`),
            timeout: 10_000,
          });
          totals.shots++;
        }
        const tables = (figs.after as { label: string | null }[]).map((t) => t.label);
        console.log(
          `REPORT ${s.key} click: detail opened ${detail === 1} tables ${JSON.stringify(tables)} detailMoved ${moved}`,
        );
        appendFileSync(
          jsonl,
          `${JSON.stringify({ click: true, key: s.key, detail, tables, moved })}\n`,
        );
      }
    } catch (e) {
      totals.failed.push(s.key);
      console.log(`REPORT ${s.key}: NOT MEASURED ${String(e).slice(0, 200)}`);
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify(totals, null, 1));
});
