import { mkdirSync, readFileSync, writeFileSync, existsSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-441 (wave 100; D-949). Every Admin table wrapper (`.adm-table-wrap`) is a named, focusable region while its
 * table scrolls sideways and a plain wrapper while it fits. One walk over every Admin page the nav lists (and, where it
 * can be reached, the tabs of the page and the detail of the first Crash report or Cinema title), at 1024x768 and
 * 900x768 (Lounge Light), then 1024 in Lounge Dark and 375x812 touch (Lounge Light). Per wrapper: scrollWidth,
 * clientWidth, height, role, tabindex, whether a Tab walk from the control before it ever lands on the wrapper, whether
 * the last body cell holds a control, every header and row box. Needs the visual sweep's seeded world
 * (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; never edited) and a stack with
 * `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and
 * `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`
 * (an invoice needs a price quote; a localhost stub is refused). Own account names (D-942); nothing is faked.
 *
 * SR_P100_TABLES_PHASE=base (default) measures and seeds the five invoices (write `base-<w>.json` in the out folder);
 * SR_P100_TABLES_PHASE=after runs the same walk, compares every box with the base files, and checks the keyboard
 * (one stop, role, name, ArrowRight to the end, ArrowLeft back) and the shots. Run both on the same database.
 *
 * The command the commander runs after merging (from `e2e/`, sweep world seeded on the same database; ports
 * 8781 / 5781 / 9481), once with PHASE=base on the unmerged tree only if wanted; on the merged tree PHASE=after
 * still compares with `base-*.json` if the out folder holds them, else it only measures:
 *   SR_P100_TABLES_ON=1 SR_P100_TABLES_PHASE=after SR_E2E_BASE_URL=http://localhost:5781 SR_CLI_BIN=<sr-cli> \
 *   SR_DATABASE__URL=<db> DATABASE_URL=<db> SR_P100_TABLES_OUT=<folder> npx playwright test \
 *   tests/p100-tables-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P100_TABLES_ON === '1';
const PHASE = process.env.SR_P100_TABLES_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P100_TABLES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-tables';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const WRAPS = '.adm-table-wrap';
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';

test.skip(!ON, 'local visual check only');
test.setTimeout(560_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Rec {
  key: string;
  nav: string;
  step: string;
  idx: number;
  name: string;
  scrollWidth: number;
  clientWidth: number;
  height: number;
  role: string | null;
  label: string | null;
  tabindex: string | null;
  lastColControl: boolean;
  isStop: boolean;
  stopCount: number;
  heads: Box[];
  cells: Box[];
}

/** Runs in the page: one record per wrapper that is on screen now (no tab walk yet). */
function measureAll() {
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  return Array.from(document.querySelectorAll<HTMLElement>('.adm-table-wrap')).map((w, idx) => {
    const tbl = w.querySelector('table');
    const cap = tbl?.querySelector('caption')?.textContent?.trim();
    const lastRow = w.querySelector('tbody tr:not([class*="frames"])');
    const lastCell = lastRow ? Array.from(lastRow.querySelectorAll('td')).pop() : undefined;
    return {
      idx,
      name: tbl?.getAttribute('aria-label') ?? cap ?? '(unnamed)',
      scrollWidth: w.scrollWidth,
      clientWidth: w.clientWidth,
      height: f(w.getBoundingClientRect().height),
      role: w.getAttribute('role'),
      label: w.getAttribute('aria-label'),
      tabindex: w.getAttribute('tabindex'),
      lastColControl: !!lastCell?.querySelector('button,a[href],input,select,textarea,[tabindex]'),
      heads: Array.from(w.querySelectorAll('thead th')).map(rect),
      cells: Array.from(w.querySelectorAll('tbody tr, tbody td')).map(rect),
    };
  });
}

async function seedRows(ownerReq: APIRequestContext, browserNew: () => Promise<APIRequestContext>) {
  const log: string[] = [];
  const step = async (name: string, fn: () => Promise<unknown>) => {
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
      reason: 'wave 100 tables shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 100 tables shots: donations on for the dev stack',
    }),
  );
  const people: { req: APIRequestContext }[] = [];
  for (const [label, name] of [
    ['p100tablesone', 'Bartholomew Featherstonehaugh-Vi'],
    ['p100tablestwo', 'Juniper'],
    ['p100tablesthree', 'Wren Alder-Okonkwo'],
  ] as const) {
    const anon = await browserNew();
    const user = await registerUser(anon, label);
    const req = await browserNew();
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
  await mk(2, { purpose: 'donate', usd_cents: 500 });
  await mk(0, { purpose: 'invite_credit', credits: 1 });
  return log;
}

const SETTINGS = [
  { key: 'light-1024', theme: 'lounge-light', w: 1024, h: 768, phone: false },
  { key: 'light-900', theme: 'lounge-light', w: 900, h: 768, phone: false },
  { key: 'dark-1024', theme: 'lounge-dark', w: 1024, h: 768, phone: false },
  { key: 'light-375', theme: 'lounge-light', w: 375, h: 812, phone: true },
] as const;
const WANT = (process.env.SR_P100_TABLES_SETTINGS ?? SETTINGS.map((s) => s.key).join(',')).split(
  ',',
);

// The shots the task lists: [file prefix, setting key, nav regexp, table-name regexp, end-of-scroll too]
const SHOTS = [
  { file: 'accounts-1024-focus', set: 'light-1024', nav: /^Accounts$/i, end: true },
  { file: 'crash-1024-focus', set: 'dark-1024', nav: /crash/i, end: false },
  { file: 'pricing-900-focus', set: 'light-900', nav: /pricing/i, end: false },
  { file: 'audit-1024-focus', set: 'light-1024', nav: /audit/i, end: false },
];

test('SEC-AUD-441: every Admin table wrapper that scrolls is a named keyboard stop', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const notes: string[] = [];
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(400);
  };
  if (PHASE === 'base' && process.env.SR_P100_TABLES_NOSEED !== '1') {
    const seedLog = await seedRows(
      o,
      async () => (await browser.newContext({ baseURL: ORIGIN })).request,
    );
    for (const l of seedLog) console.log(`REPORT seed ${l}`);
    writeFileSync(resolve(OUT, 'seed-log.txt'), seedLog.join('\n'));
    if (seedLog.some((l) => l.startsWith('FAIL invoice')))
      problems.push('an invoice was not seeded (stack error: rate source?)');
  }

  for (const s of SETTINGS.filter((x) => WANT.includes(x.key))) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT [${s.key}] account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: { width: s.w, height: s.h },
      storageState: await ownerCtx.storageState(),
      bypassCSP: true,
      ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
    }, s.theme);
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
    const themeOf = () =>
      page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
    const shot = async (name: string) => {
      const file = `${name}-${await themeOf()}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      shots.push(file);
    };
    await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    await page.locator(NAV).first().waitFor({ state: 'attached', timeout: 20_000 });
    const labels = (await page.locator(NAV).allTextContents()).map((t) => t.trim());
    console.log(`REPORT [${s.key}] nav pages ${labels.length}: ${labels.join(' | ')}`);
    const recs = new Map<string, Rec>();
    const emptyPages: string[] = [];

    const walkStops = async (idx: number) => {
      // focus the last focusable before the wrapper, then Tab through the wrapper's own focusables + 1.
      const info = await page.evaluate((i) => {
        const w = document.querySelectorAll<HTMLElement>('.adm-table-wrap')[i]!;
        w.scrollIntoView({ block: 'center' });
        const sel = 'button,input,select,textarea,a[href],[tabindex]:not([tabindex="-1"])';
        const vis = (e: HTMLElement) => e.getClientRects().length > 0 && e.tabIndex >= 0;
        const before = Array.from(document.querySelectorAll<HTMLElement>(sel)).filter(
          (e) =>
            e !== w &&
            !w.contains(e) &&
            !!(e.compareDocumentPosition(w) & Node.DOCUMENT_POSITION_FOLLOWING) &&
            vis(e),
        );
        before[before.length - 1]?.focus();
        const inside = Array.from(w.querySelectorAll<HTMLElement>(sel)).filter(vis).length;
        return { hadBefore: before.length > 0, inside };
      }, idx);
      let wrapStops = 0;
      for (let k = 0; k < info.inside + 2; k++) {
        await page.keyboard.press('Tab');
        const isWrap = await page.evaluate(
          (i) => document.activeElement === document.querySelectorAll('.adm-table-wrap')[i],
          idx,
        );
        if (isWrap) wrapStops++;
      }
      return { ...info, wrapStops };
    };

    const collect = async (nav: string, step: string) => {
      const list = (await page.evaluate(measureAll)) as Omit<
        Rec,
        'key' | 'nav' | 'step' | 'isStop' | 'stopCount'
      >[];
      for (const m of list) {
        const key = `${nav} / ${step} / ${m.name}`;
        if (recs.has(key)) continue;
        const wk = await walkStops(m.idx);
        recs.set(key, { key, nav, step, ...m, isStop: wk.wrapStops > 0, stopCount: wk.wrapStops });
        await page.evaluate((i) => {
          const w = document.querySelectorAll<HTMLElement>('.adm-table-wrap')[i];
          if (w) w.scrollLeft = 0;
          (document.activeElement as HTMLElement | null)?.blur();
        }, m.idx);
      }
      return list.length;
    };
    const clickNav = async (label: string) => {
      await page
        .locator(NAV)
        .filter({ hasText: new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`) })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 2500);
    };

    for (const label of labels) {
      try {
        await clickNav(label);
      } catch (e) {
        notes.push(`[${s.key}] nav "${label}" not clicked: ${String(e).slice(0, 80)}`);
        continue;
      }
      let n = await collect(label, 'page');
      // tabs of the page
      const tabs = await page.locator('[role="dialog"] [role="tab"]').allTextContents();
      for (const tab of tabs.map((t) => t.trim()).filter(Boolean)) {
        try {
          await page
            .locator('[role="dialog"] [role="tab"]')
            .filter({ hasText: new RegExp(`^${tab.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`) })
            .first()
            .evaluate((el) => (el as HTMLElement).click());
          await idle(page, 1500);
          n += await collect(label, `tab ${tab}`);
        } catch {
          /* tab gone */
        }
      }
      // the detail of the first row (Crash reports: Open; Cinema catalog: Edit)
      if (/crash|catalog/i.test(label)) {
        try {
          await clickNav(label);
          const btn = page.locator(`${WRAPS} tbody td[data-actions] button`).first();
          if (await btn.count()) {
            await btn.evaluate((el) => (el as HTMLElement).click());
            await idle(page, 2500);
            n += await collect(label, 'detail');
            const dtabs = await page.locator('[role="dialog"] [role="tab"]').allTextContents();
            for (const tab of dtabs.map((t) => t.trim()).filter(Boolean)) {
              try {
                await page
                  .locator('[role="dialog"] [role="tab"]')
                  .filter({ hasText: new RegExp(`^${tab.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`) })
                  .first()
                  .evaluate((el) => (el as HTMLElement).click());
                await idle(page, 1500);
                n += await collect(label, `detail tab ${tab}`);
              } catch {
                /* tab gone */
              }
            }
          }
        } catch (e) {
          notes.push(`[${s.key}] ${label} detail not reached: ${String(e).slice(0, 80)}`);
        }
      }
      if (n === 0) emptyPages.push(label);
    }
    console.log(
      `REPORT [${s.key}] pages with no wrapper (no rows: not measured): ${emptyPages.join(' | ')}`,
    );

    // ---- figures
    const all = Array.from(recs.values());
    writeFileSync(resolve(OUT, `${PHASE}-${s.key}.json`), JSON.stringify(all, null, 1));
    for (const r of all) {
      console.log(
        `REPORT [${s.key}] ${r.key}: ${r.scrollWidth}/${r.clientWidth} h ${r.height}; scrolls ${r.scrollWidth > r.clientWidth}; role ${r.role}; label ${JSON.stringify(r.label)}; tabindex ${r.tabindex}; stops ${r.stopCount}; last col control ${r.lastColControl}`,
      );
    }
    if (PHASE === 'after') {
      const baseFile = resolve(OUT, `base-${s.key}.json`);
      const base: Rec[] | null = existsSync(baseFile)
        ? (JSON.parse(readFileSync(baseFile, 'utf8')) as Rec[])
        : null;
      if (!base) notes.push(`[${s.key}] no base file: boxes not compared`);
      for (const r of all) {
        const scrolls = r.scrollWidth > r.clientWidth;
        if (scrolls) {
          if (r.role !== 'region') problems.push(`[${s.key}] ${r.key}: scrolls, role ${r.role}`);
          if (!r.label) problems.push(`[${s.key}] ${r.key}: scrolls, no name`);
          if (r.stopCount !== 1)
            problems.push(`[${s.key}] ${r.key}: scrolls, ${r.stopCount} stops`);
          if (r.label && r.name !== '(unnamed)' && r.label !== r.name)
            notes.push(
              `[${s.key}] ${r.key}: region name "${r.label}" differs from the table's "${r.name}"`,
            );
        } else if (r.role || r.tabindex !== null || r.stopCount > 0) {
          problems.push(
            `[${s.key}] ${r.key}: fits but role ${r.role} tabindex ${r.tabindex} stops ${r.stopCount}`,
          );
        }
        const b = base?.find((x) => x.key === r.key);
        if (base && !b) notes.push(`[${s.key}] ${r.key}: not in the base`);
        // The two live logs gain rows from the run itself (the walk opens pages that are logged), so their boxes
        // are reported, not compared (commander, wave 100 integration re-shoot).
        const live = /Platform audit log|Access log/.test(r.key);
        if (b && live)
          notes.push(`[${s.key}] ${r.key}: live log, boxes not compared with the base`);
        if (b && !live) {
          const moved =
            JSON.stringify(b.heads) !== JSON.stringify(r.heads) ||
            JSON.stringify(b.cells) !== JSON.stringify(r.cells);
          if (moved) problems.push(`[${s.key}] ${r.key}: header/row boxes moved vs the base`);
          if (
            b.scrollWidth !== r.scrollWidth ||
            b.clientWidth !== r.clientWidth ||
            b.height !== r.height
          )
            problems.push(`[${s.key}] ${r.key}: wrapper figures differ from the base`);
        }
      }
      if (s.phone) {
        const de = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        console.log(`REPORT [${s.key}] page sideways ${de}`);
        if (de > 0) problems.push(`[${s.key}] page scrolls sideways ${de}`);
        // Invoices at 375, shot
        await clickNav(labels.find((l) => /invoices/i.test(l)) ?? 'Invoices');
        await page
          .locator(WRAPS)
          .first()
          .evaluate((w) => w.scrollIntoView({ block: 'start' }));
        await page.waitForTimeout(200);
        await shot('invoices-375');
      }
      // keyboard + shots at the desktop widths
      if (!s.phone) {
        const scrolling = all.filter((r) => r.scrollWidth > r.clientWidth);
        for (const r of scrolling) {
          try {
            await clickNav(r.nav);
            if (r.step.startsWith('tab ') || r.step.startsWith('detail')) {
              notes.push(`[${s.key}] ${r.key}: keyboard run skipped (reached through a ${r.step})`);
              continue;
            }
            const idx = (await page.evaluate(measureAll)).findIndex((m) => m.name === r.name);
            if (idx < 0) continue;
            const wk = await walkStops(idx);
            // walkStops pressed Tab past the wrapper: bring focus back by keyboard (focus-visible)
            await page.evaluate((i) => {
              const w = document.querySelectorAll<HTMLElement>('.adm-table-wrap')[i]!;
              w.scrollLeft = 0;
              w.focus();
            }, idx);
            await page.keyboard.press('Shift+Tab');
            await page.keyboard.press('Tab');
            const st = () =>
              page.evaluate((i) => {
                const w = document.querySelectorAll<HTMLElement>('.adm-table-wrap')[i]!;
                const lastTh = Array.from(w.querySelectorAll('thead th'))
                  .pop()!
                  .getBoundingClientRect();
                const wr = w.getBoundingClientRect();
                const cs = getComputedStyle(w);
                return {
                  scrollLeft: Math.round(w.scrollLeft * 10) / 10,
                  max: w.scrollWidth - w.clientWidth,
                  lastInside: lastTh.right <= wr.right + 0.5 && lastTh.left >= wr.left - 0.5,
                  focused: document.activeElement === w,
                  visible: w.matches(':focus-visible'),
                  outline: cs.outline,
                  offset: cs.outlineOffset,
                };
              }, idx);
            const s0 = await st();
            const want = SHOTS.find((x) => x.set === s.key && x.nav.test(r.nav));
            if (!s0.focused) problems.push(`[${s.key}] ${r.key}: wrapper not focused by Tab`);
            console.log(`REPORT [${s.key}] ${r.key} focus: ${JSON.stringify(s0)}`);
            if (want && r.step === 'page') {
              await page.mouse.move(0, 0);
              await shot(want.file);
            }
            let prev = -1;
            let n = 0;
            while (n < 150) {
              const cur = (await st()).scrollLeft;
              if (cur === prev) break;
              prev = cur;
              await page.keyboard.press('ArrowRight');
              await page.waitForTimeout(50);
              n++;
            }
            await page.waitForTimeout(250);
            const e = await st();
            console.log(
              `REPORT [${s.key}] ${r.key} ArrowRight x${n}: ${s0.scrollLeft} -> ${e.scrollLeft} of ${e.max}; last header inside ${e.lastInside}; focused ${e.focused}; wrapper stops ${wk.wrapStops}`,
            );
            if (Math.abs(e.scrollLeft - e.max) > 1)
              problems.push(`[${s.key}] ${r.key}: ArrowRight ends ${e.scrollLeft} of ${e.max}`);
            if (!e.lastInside) problems.push(`[${s.key}] ${r.key}: last header not inside`);
            if (want?.end && r.step === 'page') {
              await page.mouse.move(0, 0);
              await shot(`${want.file}-end`);
            }
            let k = 0;
            while (k < 150 && (await st()).scrollLeft > 0) {
              await page.keyboard.press('ArrowLeft');
              await page.waitForTimeout(50);
              k++;
            }
            await page.waitForTimeout(250);
            const z = await st();
            if (z.scrollLeft !== 0)
              problems.push(`[${s.key}] ${r.key}: ArrowLeft ends ${z.scrollLeft}`);
          } catch (err) {
            notes.push(`[${s.key}] ${r.key}: keyboard run error ${String(err).slice(0, 100)}`);
          }
        }
      }
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  for (const n of notes) console.log(`REPORT note ${n}`);
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${JSON.stringify(problems)}`);
  appendFileSync(
    resolve(OUT, 'totals.jsonl'),
    `${JSON.stringify({ PHASE, shots, notes, problems })}\n`,
  );
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
