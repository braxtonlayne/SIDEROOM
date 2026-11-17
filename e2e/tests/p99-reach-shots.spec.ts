import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-440 (wave 99; D-948). Admin > Donations > Invoices: the table's wrapper is a named, focusable region, so the
 * keyboard can scroll it to the last columns (between 721 px and 1199 px the table scrolls sideways in its wrapper).
 * One run holds the base (the three attributes taken out of the DOM, the page then reloaded for the after) and the
 * after, in the same world. Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; never edited) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`,
 * `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and
 * `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`
 * (an invoice needs a price quote; a localhost stub is refused). Own account names (D-942); nothing is faked.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8772 / 5772):
 *   SR_P99_REACH_ON=1 SR_E2E_BASE_URL=http://localhost:5772 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P99_REACH_OUT=<folder> npx playwright test tests/p99-reach-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 * Settings: Lounge Light and Dark (account theme by PATCH and localStorage, `data-theme` read back into the file name)
 * at 1024x768; Lounge Light at 375x812 touch. One JSON line per measure in `measure.json`.
 */
const ON = process.env.SR_P99_REACH_ON === '1';
const OUT =
  process.env.SR_P99_REACH_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p99-reach';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const WRAP = '.adm-table-wrap:has(> .inv-table)';
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Runs in the page: the wrapper's figures and every header and row box. */
function measureWrap(sel: string) {
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const wrap = document.querySelector<HTMLElement>(sel);
  if (!wrap) return null;
  const de = document.documentElement;
  return {
    doc: de.scrollWidth - de.clientWidth,
    scrollWidth: wrap.scrollWidth,
    clientWidth: wrap.clientWidth,
    height: f(wrap.getBoundingClientRect().height),
    scrollLeft: f(wrap.scrollLeft),
    wrapBox: rect(wrap),
    role: wrap.getAttribute('role'),
    label: wrap.getAttribute('aria-label'),
    tabindex: wrap.getAttribute('tabindex'),
    bgImage: getComputedStyle(wrap).backgroundImage,
    heads: Array.from(wrap.querySelectorAll('thead th')).map((th) => ({
      text: (th.textContent ?? '').trim().slice(0, 20),
      box: rect(th),
    })),
    cells: Array.from(wrap.querySelectorAll('tbody tr, tbody td')).map(rect),
  };
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
      reason: 'wave 99 reach shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 99 reach shots: donations on for the dev stack',
    }),
  );
  const people: { req: APIRequestContext }[] = [];
  for (const [label, name] of [
    ['p99reachlong', 'Bartholomew Featherstonehaugh-Vi'],
    ['p99reachtwo', 'Juniper'],
    ['p99reachthree', 'Wren Alder-Okonkwo'],
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

const THEMES = [
  { key: 'light', theme: 'lounge-light' },
  { key: 'dark', theme: 'lounge-dark' },
] as const;

test('SEC-AUD-440: the Invoices wrapper is a named region the keyboard can scroll', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const notes: string[] = [];
  const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
  const seedLog = await seedRows(
    o,
    async () => (await browser.newContext({ baseURL: ORIGIN })).request,
  );
  for (const l of seedLog) console.log(`REPORT seed ${l}`);
  writeFileSync(resolve(OUT, 'seed-log.txt'), seedLog.join('\n'));
  if (seedLog.some((l) => l.startsWith('FAIL invoice')))
    problems.push('an invoice was not seeded (stack error: rate source?)');

  for (const s of THEMES) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
    const mkCtx = async (w: number, h: number, phone: boolean) => {
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
      }, s.theme);
      const page = await ctx.newPage();
      page.setDefaultTimeout(8000);
      return { ctx, page };
    };
    const open = async (page: Page) => {
      await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 4000);
      await page
        .locator(NAV)
        .filter({ hasText: /^Invoices$/ })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 2500);
      await page.locator(WRAP).first().waitFor({ timeout: 8000 });
    };
    const themeOf = (page: Page) =>
      page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
    const shot = async (page: Page, name: string) => {
      const file = `${name}-${await themeOf(page)}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      shots.push(file);
    };
    const hits = async (page: Page) => {
      await page.locator(WRAP).evaluate((w) => {
        w.scrollIntoView({ block: 'center' });
        w.scrollLeft = 0;
      });
      const wrap = page.locator(WRAP);
      await openCollapsed(page, wrap);
      const rows = await hitTestAll(page, wrap);
      return rows.map((r) => ({
        name: r.name,
        status: r.status,
        taker: r.result?.points.find((p) => !p.ok)?.taker?.path ?? null,
      }));
    };
    /** Focus the last focusable element before the wrapper, then Tab; one record per stop until focus leaves the wrapper. */
    const tabWalk = async (page: Page) => {
      await page.locator(WRAP).evaluate((w) => {
        w.scrollIntoView({ block: 'center' });
        const all = Array.from(
          document.querySelectorAll<HTMLElement>(
            'button,input,select,textarea,a[href],[tabindex]:not([tabindex="-1"])',
          ),
        ).filter(
          (e) =>
            e !== w &&
            !w.contains(e) &&
            !!(e.compareDocumentPosition(w) & Node.DOCUMENT_POSITION_FOLLOWING) &&
            e.getClientRects().length > 0,
        );
        all[all.length - 1]?.focus();
      });
      const stops: { tag: string; text: string; isWrap: boolean; inWrap: boolean }[] = [];
      for (let i = 0; i < 16; i++) {
        await page.keyboard.press('Tab');
        const st = await page.evaluate((sel) => {
          const w = document.querySelector(sel)!;
          const a = document.activeElement as HTMLElement | null;
          return {
            tag: a?.tagName.toLowerCase() ?? 'none',
            text: (a?.textContent ?? '').trim().slice(0, 18),
            isWrap: a === w,
            inWrap: !!a && w.contains(a),
          };
        }, WRAP);
        stops.push(st);
        if (!st.isWrap && !st.inWrap && stops.some((x) => x.inWrap || x.isWrap)) break;
      }
      return stops;
    };
    const wrapState = (page: Page) =>
      page.evaluate((sel) => {
        const w = document.querySelector<HTMLElement>(sel)!;
        const lastTh = Array.from(w.querySelectorAll('thead th')).pop()!.getBoundingClientRect();
        const wr = w.getBoundingClientRect();
        return {
          scrollLeft: Math.round(w.scrollLeft * 10) / 10,
          max: w.scrollWidth - w.clientWidth,
          lastInside: lastTh.right <= wr.right + 0.5 && lastTh.left >= wr.left - 0.5,
          focused: document.activeElement === w,
        };
      }, WRAP);

    // ------------------------------------------------------------ 1024 x 768
    const d = await mkCtx(1024, 768, false);
    const page = d.page;
    await open(page);
    // BASE: the attributes taken out of the DOM.
    await page.locator(WRAP).evaluate((w) => {
      w.removeAttribute('role');
      w.removeAttribute('aria-label');
      w.removeAttribute('tabindex');
    });
    const base = (await page.evaluate(measureWrap, WRAP))!;
    log({ id: 'base', s: s.key, ...base });
    const baseStops = await tabWalk(page);
    const baseBtns = baseStops.filter((x) => x.inWrap && !x.isWrap).length;
    const baseWrapFocused = baseStops.some((x) => x.isWrap);
    const lastBtn = page.locator(`${WRAP} tbody tr:last-child button`).first();
    await lastBtn.focus();
    const bBefore = await wrapState(page);
    await page.keyboard.press('ArrowRight');
    await page.keyboard.press('ArrowRight');
    const bAfter = await wrapState(page);
    console.log(
      `REPORT ${s.key} BASE 1024: client ${base.clientWidth} scroll ${base.scrollWidth} (+${base.scrollWidth - base.clientWidth}) h ${base.height}; tab stops in wrapper ${baseBtns} (buttons), wrapper focused ${baseWrapFocused}; stops ${JSON.stringify(baseStops.filter((x) => x.inWrap || x.isWrap))}`,
    );
    console.log(
      `REPORT ${s.key} BASE last id button focused: scrollLeft ${bBefore.scrollLeft} of ${bBefore.max}; after ArrowRight x2: ${bAfter.scrollLeft}; last header inside ${bAfter.lastInside}`,
    );
    const baseHits = await hits(page);
    console.log(`REPORT ${s.key} BASE hit ${JSON.stringify(baseHits)}`);
    if (baseWrapFocused) problems.push(`${s.key}: base wrapper focused (should be impossible)`);
    if (base.scrollWidth <= base.clientWidth + 1)
      problems.push(`${s.key}: the table fits at 1024: the fault is not as the plan says`);

    // AFTER: a fresh load with the attributes in.
    await open(page);
    const after = (await page.evaluate(measureWrap, WRAP))!;
    log({ id: 'after', s: s.key, ...after });
    console.log(
      `REPORT ${s.key} AFTER attrs: role ${after.role}, aria-label "${after.label}", tabindex ${after.tabindex}; client ${after.clientWidth} scroll ${after.scrollWidth} h ${after.height}; bg ${after.bgImage.slice(0, 60)}`,
    );
    if (after.role !== 'region') problems.push(`${s.key}: role ${after.role}`);
    if (after.label !== 'Invoices') notes.push(`${s.key}: aria-label is "${after.label}"`);
    const moved = after.heads.filter(
      (h, i) => JSON.stringify(h.box) !== JSON.stringify(base.heads[i]?.box),
    );
    console.log(`REPORT ${s.key} heads moved vs base: ${moved.length} of ${after.heads.length}`);
    if (moved.length) problems.push(`${s.key}: ${moved.length} header boxes moved`);
    if (JSON.stringify(after.cells) !== JSON.stringify(base.cells))
      problems.push(`${s.key}: row/cell boxes moved`);
    if (
      after.scrollWidth !== base.scrollWidth ||
      after.clientWidth !== base.clientWidth ||
      after.height !== base.height
    )
      problems.push(`${s.key}: wrapper figures differ from the base`);
    const named = await page
      .getByRole('region', { name: after.label ?? 'Invoices', exact: true })
      .count()
      .catch(() => -1);
    console.log(`REPORT ${s.key} getByRole region "${after.label}" count ${named}`);
    if (named < 1) problems.push(`${s.key}: no named region in the accessibility tree`);

    const stops = await tabWalk(page);
    const inW = stops.filter((x) => x.isWrap || x.inWrap);
    console.log(`REPORT ${s.key} AFTER tab stops ${JSON.stringify(inW)}`);
    if (!inW[0]?.isWrap) problems.push(`${s.key}: first stop in the table is not the wrapper`);
    if (inW.filter((x) => x.isWrap).length !== 1)
      problems.push(`${s.key}: the wrapper is not exactly one stop`);
    // back to the wrapper by keyboard (focus-visible) and read the ring
    await page.locator(WRAP).evaluate((w) => (w as HTMLElement).focus());
    await page.keyboard.press('Shift+Tab');
    await page.keyboard.press('Tab');
    const ring = await page.evaluate((sel) => {
      const w = document.querySelector<HTMLElement>(sel)!;
      const cs = getComputedStyle(w);
      return {
        focused: document.activeElement === w,
        visible: w.matches(':focus-visible'),
        outline: cs.outline,
        outlineOffset: cs.outlineOffset,
      };
    }, WRAP);
    console.log(`REPORT ${s.key} ring ${JSON.stringify(ring)}`);
    if (!ring.focused || !ring.visible) problems.push(`${s.key}: wrapper not focus-visible by Tab`);
    await page.mouse.move(0, 0);
    await shot(page, 'invoices-1024-focus');
    // ArrowRight until it stops rising
    const s0 = await wrapState(page);
    let prev = -1;
    let n = 0;
    while (n < 120) {
      const cur = (await wrapState(page)).scrollLeft;
      if (cur === prev) break;
      prev = cur;
      await page.keyboard.press('ArrowRight');
      await page.waitForTimeout(60);
      n++;
    }
    await page.waitForTimeout(300);
    const sEnd = await wrapState(page);
    console.log(
      `REPORT ${s.key} ArrowRight x${n}: scrollLeft ${s0.scrollLeft} -> ${sEnd.scrollLeft} (max ${sEnd.max}); last header inside ${sEnd.lastInside}; still focused ${sEnd.focused}`,
    );
    if (Math.abs(sEnd.scrollLeft - sEnd.max) > 1)
      problems.push(`${s.key}: ArrowRight ends at ${sEnd.scrollLeft}, max ${sEnd.max}`);
    if (!sEnd.lastInside) problems.push(`${s.key}: last header not inside the wrapper`);
    const bgEnd = await page.locator(WRAP).evaluate((w) => getComputedStyle(w).backgroundImage);
    log({ id: 'end', s: s.key, bgEnd, sEnd });
    await page.mouse.move(0, 0);
    if (s.key === 'light') await shot(page, 'invoices-1024-focus-end');
    let k = 0;
    while (k < 120 && (await wrapState(page)).scrollLeft > 0) {
      await page.keyboard.press('ArrowLeft');
      await page.waitForTimeout(60);
      k++;
    }
    await page.waitForTimeout(300);
    const sBack = await wrapState(page);
    console.log(`REPORT ${s.key} ArrowLeft x${k}: scrollLeft ${sBack.scrollLeft}`);
    if (sBack.scrollLeft !== 0) problems.push(`${s.key}: ArrowLeft ends at ${sBack.scrollLeft}`);
    const aftHits = await hits(page);
    console.log(`REPORT ${s.key} AFTER hit ${JSON.stringify(aftHits)}`);
    if (JSON.stringify(aftHits) !== JSON.stringify(baseHits))
      problems.push(`${s.key}: hit test after differs from the base`);
    if (aftHits.some((r) => r.status === 'not-reached'))
      problems.push(`${s.key}: hit test has not-reached rows (probe error)`);
    console.log(`REPORT ${s.key} hit rows ${aftHits.length}`);
    // other tables, for the proposed row
    if (s.key === 'light') {
      const labels = await page.locator(NAV).allTextContents();
      for (let i = 0; i < labels.length; i++) {
        await page
          .locator(NAV)
          .nth(i)
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 1200);
        const m = await page.evaluate(() =>
          Array.from(
            document.querySelectorAll<HTMLElement>('.adm-table-wrap:not(:has(> .inv-table))'),
          ).map((w) => `${w.scrollWidth}/${w.clientWidth}`),
        );
        if (m.length)
          notes.push(`other tables in nav "${labels[i]}": scroll/client ${m.join(' ')}`);
      }
    }
    await d.ctx.close().catch(() => undefined);

    // ------------------------------------------------------------ phone 375
    if (s.key === 'light') {
      const p = await mkCtx(375, 812, true);
      await open(p.page);
      await p.page.locator(WRAP).evaluate((w) => {
        w.removeAttribute('role');
        w.removeAttribute('aria-label');
        w.removeAttribute('tabindex');
      });
      const pb = (await p.page.evaluate(measureWrap, WRAP))!;
      await open(p.page);
      const pa = (await p.page.evaluate(measureWrap, WRAP))!;
      log({ id: 'phone', before: pb, after: pa });
      const eq = JSON.stringify(pa.cells) === JSON.stringify(pb.cells);
      console.log(
        `REPORT phone 375: wrapper client ${pa.clientWidth} scroll ${pa.scrollWidth}; cell boxes equal ${eq} (${pa.cells.length}); page sideways ${pa.doc}`,
      );
      if (!eq) problems.push('phone: row boxes differ');
      if (pa.scrollWidth !== pa.clientWidth) problems.push('phone: wrapper scrolls');
      if (pa.doc > 0) problems.push(`phone: page scrolls sideways ${pa.doc}`);
      await p.page.locator(WRAP).evaluate((w) => w.scrollIntoView({ block: 'start' }));
      await p.page.waitForTimeout(200);
      await shot(p.page, 'invoices-375');
      await p.ctx.close().catch(() => undefined);
    }
  }
  await ownerCtx.close();
  for (const n of notes) console.log(`REPORT note ${n}`);
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${JSON.stringify(problems)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify({ shots, notes, problems }, null, 1));
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
