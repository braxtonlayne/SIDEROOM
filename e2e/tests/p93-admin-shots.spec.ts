import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-409, 410, 411, 423 (wave 93; D-925, D-929). Admin > Donations: Invoices stacks on a phone, the Entitlements
 * badges wrap, "Resolve by hand" has 12 px above it, a long payer name is not cut.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json` and `world.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; that spec is never edited). The way into Admin is p90-invoices-shots (the dashboard, then the
 * nav). The stack needs `SR_ADMIN_REQUIRE_REAUTH=0` and `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` (donations stay off
 * without a chain URL). D-929: NO outside price service, so no real invoice can be made (a quote needs a price source,
 * and the server's price client refuses a local address: it is always the strict policy, `payments_jobs.rs`). So the
 * Invoices list and the invoice detail are FAKED with `page.route` (five invoices; payers: a 32-character name with no
 * space, a 32-character name with spaces and a hyphen, short names, a detached payer); the grants (Entitlements) are
 * real: the owner sets a signet wallet, turns donations on, three accounts, two patron grants and a server pool grant.
 * Everything else is real. The stack gets no `SR_PAYMENTS_RATE_SOURCES`.
 *
 * The base case (`SR_P93_ADMIN_BASE=1`) is run in the same round on the same code: it takes the three classes this wave
 * added (`adm-table-stack`, `inv-table`, `inv-payer`, `ent-list`) off the elements and sets the audit margin back to 0
 * before it measures, which is the base's CSS to the pixel (every rule of this wave is scoped to those classes or to
 * `.adm-audit + .adm-actions`). It seeds; the after run is given `SR_P93_ADMIN_NOSEED=1` (the seed is made once).
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; the stack also
 * needs `SR_ADMIN_REQUIRE_REAUTH=0` and `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`; ports 8714 / 5714):
 *   SR_P93_ADMIN_ON=1 SR_E2E_BASE_URL=http://localhost:5714 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P93_ADMIN_OUT=<folder> npx playwright test tests/p93-admin-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (add `SR_P93_ADMIN_BASE=1` for the base case; run it first, then the after run with `SR_P93_ADMIN_NOSEED=1`).
 * Hit tests go through `e2e/support/hit-test.ts` (`openCollapsed`, `hitTestAll`); a `not-reached` is a spec error.
 */
const ON = process.env.SR_P93_ADMIN_ON === '1';
const BASE = process.env.SR_P93_ADMIN_BASE === '1';
const NOSEED = process.env.SR_P93_ADMIN_NOSEED === '1';
const OUT =
  process.env.SR_P93_ADMIN_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-admin';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'desk', w: 1280, h: 800, phone: false, theme: 'lounge-editorial' },
  { key: 'tab', w: 1024, h: 800, phone: false, theme: 'lounge-editorial' },
  { key: 'phone-light', w: 375, h: 812, phone: true, theme: 'lounge-light' },
  { key: 'phone-dark', w: 375, h: 812, phone: true, theme: 'lounge-dark' },
];
// The 8 shots: Invoices rows (phone light base + after, phone dark after, desk after), Entitlements (phone light base +
// after), the invoice detail (desk after, phone light after).
const INV_SHOT = BASE ? ['phone-light'] : ['phone-light', 'phone-dark', 'desk'];
const ENT_SHOT = ['phone-light'];
const DETAIL_SHOT = BASE ? [] : ['desk', 'phone-light'];
const NAV = '[role="dialog"] nav ul li > button';
// BIP-84 test vector as a signet tpub (the donations tests use the same key).
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';
const NAME_NOSPACE = 'Averyverylongmembernameforinvo'.padEnd(32, 'x'); // 32 characters, no space
const NAME_SPACES = 'Bartholomew Featherstonehaugh-Vi'; // 32 characters, spaces and a hyphen

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const NOW = Date.now();
function fakeInvoice(n: number) {
  const base = [
    { purpose: 'patron', state: 'confirmed', who: NAME_SPACES, uid: '9100', cents: 6000 },
    { purpose: 'donate', state: 'open', who: NAME_NOSPACE, uid: '9101', cents: 4999 },
    { purpose: 'invite_credit', state: 'underpaid', who: 'Juniper', uid: '9102', cents: 300 },
    { purpose: 'donate', state: 'resolved', who: 'Wren Alder-Okonkwo', uid: '9103', cents: 500 },
    { purpose: 'invite_credit', state: 'expired', who: null, uid: '0', cents: 100 },
  ][n]!;
  const id = `11623080489962700${n}`;
  return {
    id,
    purpose: base.purpose,
    state: base.state,
    usd_cents: base.cents,
    sats: Math.round(base.cents * 1666.6),
    btc: '0.00100000',
    address: 'tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx',
    bip21: 'bitcoin:tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx',
    network: 'signet',
    guild_id: null,
    months: base.purpose === 'patron' ? 12 : null,
    credits: base.purpose === 'invite_credit' ? 3 : null,
    anonymous: false,
    rate: { cents_per_btc: 6000000, source: 'local', age_secs: 5, stale: false },
    confirmations_required: 1,
    confirmations: base.state === 'confirmed' ? 3 : null,
    received_sats: base.state === 'open' ? 0 : 1000,
    confirmed_sats: base.state === 'confirmed' ? 1000 : 0,
    shortfall_sats: base.state === 'underpaid' ? 200 : 0,
    excess_sats: 0,
    late: false,
    created_at_ms: NOW - n * 86_400_000,
    expires_at_ms: NOW + 3_600_000,
    quote_expires_at_ms: NOW + 600_000,
    credited_at_ms: null,
    explorer_url: null,
    txs: [],
    resolution: base.state === 'resolved' ? 'mark_paid' : null,
    user_id: base.uid,
    user_name: base.who,
    derivation_index: n,
    resolution_note: base.state === 'resolved' ? 'Paid in person at the meetup.' : null,
    resolved_by: null,
    audit: [
      {
        at_ms: NOW - n * 86_400_000,
        kind: 'created',
        detail: { purpose: base.purpose, usd_cents: base.cents, rate_source: 'local' },
      },
      {
        at_ms: NOW - n * 86_400_000 + 60_000,
        kind: 'state_changed',
        detail: { from: 'open', to: base.state, reason: 'watcher', txids: ['aaaa', 'bbbb'] },
      },
    ],
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
      reason: 'wave 93 shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 93 shots: donations on for the dev stack',
    }),
  );
  const people: {
    user: { id: string; username: string; password: string };
    req: APIRequestContext;
  }[] = [];
  for (const [label, name] of [
    ['averyverylongmembernameforinvoices', NAME_SPACES],
    ['invtwo', 'Juniper'],
    ['invthree', 'Wren Alder-Okonkwo'],
  ] as const) {
    const anon = await browserNew();
    const user = await registerUser(anon, label);
    const req = await browserNew();
    await loginApi(req, user);
    await step(`name ${label}`, () => apiPatch(req, '/users/@me', { global_name: name }));
    people.push({ user, req });
  }
  await step('grant patron 30 days', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[0]!.user.id,
      days: 30,
      reason:
        'wave 93 shots: thanks for the long-running help with the translation work on the help centre',
    }),
  );
  await step('grant patron permanent', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[1]!.user.id,
      permanent: true,
      reason: 'wave 93 shots',
    }),
  );
  const g = await step('server for pool grant', () =>
    apiPost<{ id: string }>(people[2]!.req, '/guilds', {
      name: 'The Unreasonably Long Name Of A Very Small Reading Circle',
    }),
  );
  if (g)
    await step('grant pool', () =>
      apiPost(ownerReq, '/admin/donations/entitlements', {
        kind: 'server_pool',
        guild_id: (g as { id: string }).id,
        usd_cents: 2500,
        reason: 'wave 93 shots',
      }),
    );
  return log;
}

// The base case: the wave's classes off the elements, the audit margin back to 0 (the base's CSS).
const BASE_SCRIPT = `
(() => {
  const strip = () => {
    for (const c of ['adm-table-stack', 'inv-table', 'inv-payer', 'ent-list'])
      for (const e of Array.from(document.getElementsByClassName(c))) e.classList.remove(c);
  };
  new MutationObserver(strip).observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ['class'] });
  document.addEventListener('DOMContentLoaded', () => {
    const st = document.createElement('style');
    st.textContent = '.adm-audit + .adm-actions { margin-block-start: 0 !important; }';
    document.head.appendChild(st);
  });
})();
`;

test('SEC-AUD-409/410/411/423: Invoices stacked, Entitlements badges, room above Resolve, payer name', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, BASE ? 'measure-before.json' : 'measure.json');
  const pre = BASE ? 'before-' : '';
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };

  if (!NOSEED) {
    const log = await seedRows(
      o,
      async () => (await browser.newContext({ baseURL: ORIGIN })).request,
    );
    for (const l of log) console.log(`REPORT seed ${l}`);
    writeFileSync(resolve(OUT, 'seed-log.txt'), log.join('\n'));
  }

  for (const s of SETTINGS) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${s.theme}, read back: ${back.settings?.user_prefs?.theme}`,
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
    if (BASE) await ctx.addInitScript(BASE_SCRIPT);
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
    // D-929: the invoices are faked; the list and the detail.
    await page.route(/\/api\/v1\/admin\/donations\/invoices(\/[^/?]+)?(\?.*)?$/, async (route) => {
      const u = new URL(route.request().url());
      const m = /\/invoices\/(\d+)$/.exec(u.pathname);
      if (route.request().method() !== 'GET') return route.continue();
      if (m) return route.fulfill({ json: fakeInvoice(Number(m[1].slice(-1))) });
      return route.fulfill({
        json: { items: [0, 1, 2, 3, 4].map(fakeInvoice), next_before: null },
      });
    });
    const themeOf = () =>
      page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
    const goTo = async (label: RegExp) => {
      await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 4000);
      await page
        .locator(NAV)
        .filter({ hasText: label })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 2500);
    };
    const shot = async (name: string) => {
      const file = `${pre}${name}-${s.key}-${await themeOf()}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      shots.push(file);
    };
    const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);

    // ---- Invoices: the table with rows
    await goTo(/^Invoices$/);
    await page.locator('.adm-table tbody tr').first().waitFor({ timeout: 8000 });
    const inv = await page.evaluate(() => {
      const de = document.documentElement;
      const f = (n: number) => Math.round(n * 10) / 10;
      const rect = (e: Element) => {
        const r = e.getBoundingClientRect();
        return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
      };
      const table = document.querySelector<HTMLElement>('.adm-table')!;
      const wrap = document.querySelector<HTMLElement>('.adm-table-wrap')!;
      const rows = Array.from(table.querySelectorAll<HTMLElement>('tbody tr'));
      const cell = (c: HTMLElement) => {
        const cs = getComputedStyle(c);
        return {
          tag: c.tagName.toLowerCase(),
          label: c.getAttribute('data-label'),
          text: (c.innerText ?? '').replace(/\s+/g, ' ').trim().slice(0, 60),
          box: rect(c),
          client: c.clientWidth,
          scroll: c.scrollWidth,
          overflow: cs.overflowX,
          whiteSpace: cs.whiteSpace,
          wordBreak: cs.wordBreak,
          overflowWrap: cs.overflowWrap,
          hyphens: cs.hyphens,
          textOverflow: cs.textOverflow,
          display: cs.display,
        };
      };
      // The payer name: is it whole? Its text range's width against its cell's content width.
      const payerRows = rows.map((r) => {
        const c = r.querySelectorAll<HTMLElement>('td')[0]!;
        const rg = document.createRange();
        rg.selectNodeContents(c);
        const rects = Array.from(rg.getClientRects()).map((q) => ({ l: f(q.left), r: f(q.right) }));
        const cr = c.getBoundingClientRect();
        return {
          text: (c.textContent ?? '').trim(),
          cellL: f(cr.left),
          cellR: f(cr.right),
          lines: rects.length,
          inside: rects.every((q) => q.l >= cr.left - 0.5 && q.r <= cr.right + 0.5),
          cell: cell(c),
        };
      });
      return {
        vw: de.clientWidth,
        theme:
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? null,
        doc: de.scrollWidth - de.clientWidth,
        wrap: { client: wrap.clientWidth, scroll: wrap.scrollWidth },
        table: { box: rect(table), scrollWidth: table.scrollWidth, clientWidth: table.clientWidth },
        heads: Array.from(table.querySelectorAll<HTMLElement>('thead th')).map((h) => ({
          text: (h.textContent ?? '').trim(),
          w: f(h.getBoundingClientRect().width),
        })),
        rowBoxes: rows.map(rect),
        firstTwoRows: rows
          .slice(0, 2)
          .map((r) => Array.from(r.children).map((c) => cell(c as HTMLElement))),
        payerRows,
        rowText: rows.slice(0, 2).map((r) =>
          (r.innerText ?? '')
            .split('\n')
            .map((x) => x.trim())
            .filter(Boolean),
        ),
        classes: table.className,
      };
    });
    log({ id: 'invoices-rows', setting: s.key, ...inv });
    console.log(
      `REPORT ${s.key} invoices doc ${inv.doc} wrap ${JSON.stringify(inv.wrap)} table ${JSON.stringify(inv.table)} classes "${inv.classes}"`,
    );
    console.log(`REPORT   heads ${JSON.stringify(inv.heads)}`);
    for (const p of inv.payerRows)
      console.log(
        `REPORT   payer ${JSON.stringify({ t: p.text, lines: p.lines, inside: p.inside, cell: p.cell.box, client: p.cell.client, scroll: p.cell.scroll, ov: p.cell.overflow, ws: p.cell.whiteSpace, wb: p.cell.wordBreak, ow: p.cell.overflowWrap, hy: p.cell.hyphens, to: p.cell.textOverflow })}`,
      );
    for (const r of inv.firstTwoRows)
      console.log(`REPORT   row ${JSON.stringify(r.map((c) => [c.tag, c.label, c.text, c.box]))}`);
    console.log(`REPORT   rowText ${JSON.stringify(inv.rowText)}`);
    if (inv.doc > 0) problems.push(`${s.key}: invoices page scrolls sideways ${inv.doc}`);
    if (!BASE) {
      if (s.phone) {
        if (inv.table.scrollWidth > inv.wrap.client)
          problems.push(
            `${s.key}: table scrollWidth ${inv.table.scrollWidth} > wrapper ${inv.wrap.client}`,
          );
        for (const r of inv.firstTwoRows) {
          const labels = r.map((c) => c.label).filter(Boolean);
          if (labels.length !== 5) problems.push(`${s.key}: a row has ${labels.length} labels`);
        }
      }
      for (const p of inv.payerRows)
        if (!p.inside) problems.push(`${s.key}: payer "${p.text}" runs outside its cell`);
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    if (s.phone) {
      const main = page.locator('.adm-table-wrap').first();
      await openCollapsed(page, main);
      const rows = await hitTestAll(page, main);
      const fails = rows.filter((r) => r.status === 'fail');
      const nr = rows.filter((r) => r.status === 'not-reached');
      console.log(
        `REPORT ${s.key} invoices hit test: ${rows.length} controls, ${fails.length} fail, ${nr.length} not-reached`,
      );
      for (const r of fails)
        console.log(
          `REPORT   FAIL ${r.name} ${JSON.stringify(r.result?.points.filter((p) => !p.ok).map((p) => [p.at, p.taker]))}`,
        );
      log({
        id: 'invoices-hit',
        setting: s.key,
        rows: rows.map((r) => [r.name, r.status, r.result?.box]),
      });
      if (!BASE) {
        if (fails.length) problems.push(`${s.key}: invoices hit test ${fails.length} fail`);
        if (nr.length) problems.push(`${s.key}: invoices hit test not-reached (spec error)`);
      }
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    await page
      .locator('.adm-table')
      .first()
      .evaluate((e) => e.scrollIntoView({ block: 'start' }));
    if (INV_SHOT.includes(s.key)) await shot('invoices-rows');

    // ---- The invoice detail (the gap above "Resolve by hand"; its hit test)
    const first = page.locator('.adm-table tbody tr th button').first();
    await first.scrollIntoViewIfNeeded();
    await first.click();
    await page.locator('[role="dialog"] .adm-dl').first().waitFor({ timeout: 8000 });
    await idle(page, 2000);
    const det = await page.evaluate(() => {
      const d = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop()!;
      const f = (n: number) => Math.round(n * 10) / 10;
      const lis = Array.from(d.querySelectorAll<HTMLElement>('.adm-audit li'));
      const last = lis[lis.length - 1];
      const act = d.querySelector<HTMLElement>('.adm-actions');
      return {
        gap:
          last && act
            ? f(act.getBoundingClientRect().top - last.getBoundingClientRect().bottom)
            : null,
        margin: act ? getComputedStyle(act).marginBlockStart : null,
        doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      };
    });
    console.log(`REPORT ${s.key} detail ${JSON.stringify(det)}`);
    log({ id: 'invoice-detail', setting: s.key, det });
    if (!BASE && det.gap !== 12) problems.push(`${s.key}: gap above Resolve by hand ${det.gap}`);
    if (det.doc > 0) problems.push(`${s.key}: detail scrolls sideways ${det.doc}`);
    if (s.phone) {
      const dlg = page.locator('[role="dialog"]').last();
      const rows = await hitTestAll(page, dlg);
      const fails = rows.filter((r) => r.status === 'fail');
      const nr = rows.filter((r) => r.status === 'not-reached');
      console.log(
        `REPORT ${s.key} detail hit test: ${rows.length} controls, ${fails.length} fail, ${nr.length} not-reached`,
      );
      for (const r of fails)
        console.log(
          `REPORT   FAIL ${r.name} ${JSON.stringify(r.result?.points.filter((p) => !p.ok).map((p) => [p.at, p.taker]))}`,
        );
      // The kit Modal's own footer "Close" fails on the base as well (a report, not this wave's row).
      const own = fails.filter((r) => r.name !== 'Close');
      if (!BASE && (own.length || nr.length))
        problems.push(`${s.key}: detail hit test ${own.length} fail ${nr.length} not-reached`);
    }
    await page.evaluate(() => {
      const d = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop();
      const a = d?.querySelector<HTMLElement>('.adm-actions');
      a?.scrollIntoView({ block: 'end' });
    });
    await page.waitForTimeout(200);
    if (DETAIL_SHOT.includes(s.key)) await shot('invoice-detail');
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);

    // ---- Entitlements: the list with rows
    await goTo(/^Entitlements$/);
    await page.locator('.ss-list > li').first().waitFor({ timeout: 8000 });
    const ent = await page.evaluate(() => {
      const f = (n: number) => Math.round(n * 10) / 10;
      const rect = (e: Element) => {
        const r = e.getBoundingClientRect();
        return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
      };
      const lis = Array.from(document.querySelectorAll<HTMLElement>('.ss-list > li'));
      return {
        doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
        classes: document.querySelector('.ss-list')?.className ?? '',
        rows: lis.map((li) => {
          const nm = li.querySelector<HTMLElement>('.ss-name')!;
          const cs = getComputedStyle(nm);
          const nb = nm.getBoundingClientRect();
          const badges = Array.from(nm.children).map((b) => {
            const r = b.getBoundingClientRect();
            return {
              text: (b.textContent ?? '').trim(),
              box: rect(b),
              whole: r.right <= nb.right + 0.5 && r.left >= nb.left - 0.5,
            };
          });
          return {
            row: rect(li),
            name: rect(nm),
            client: nm.clientWidth,
            scroll: nm.scrollWidth,
            overflow: cs.overflowX,
            whiteSpace: cs.whiteSpace,
            text: (nm.innerText ?? '').replace(/\s+/g, ' ').trim(),
            badges,
          };
        }),
      };
    });
    log({ id: 'entitlements-rows', setting: s.key, ...ent });
    console.log(`REPORT ${s.key} entitlements doc ${ent.doc} classes "${ent.classes}"`);
    for (const r of ent.rows.slice(0, 2))
      console.log(
        `REPORT   name ${JSON.stringify({ n: r.name, client: r.client, scroll: r.scroll, ov: r.overflow, ws: r.whiteSpace, t: r.text, b: r.badges.map((b) => [b.text, b.box, b.whole]) })}`,
      );
    if (ent.doc > 0) problems.push(`${s.key}: entitlements page scrolls sideways ${ent.doc}`);
    if (!BASE)
      for (const r of ent.rows.slice(0, 2)) {
        if (r.scroll > r.client + 1)
          problems.push(`${s.key}: entitlement name scroll ${r.scroll} > ${r.client}`);
        for (const b of r.badges.filter((b) => b.text))
          if (!b.whole) problems.push(`${s.key}: badge "${b.text}" is cut`);
      }
    if (s.phone) {
      const list = page.locator('.ss-list').first();
      const rows = await hitTestAll(page, list);
      const fails = rows.filter((r) => r.status === 'fail');
      const nr = rows.filter((r) => r.status === 'not-reached');
      console.log(
        `REPORT ${s.key} entitlements hit test: ${rows.length} controls, ${fails.length} fail, ${nr.length} not-reached`,
      );
      for (const r of fails)
        console.log(
          `REPORT   FAIL ${r.name} ${JSON.stringify(r.result?.points.filter((p) => !p.ok).map((p) => [p.at, p.taker]))}`,
        );
      if (!BASE && (fails.length || nr.length))
        problems.push(
          `${s.key}: entitlements hit test ${fails.length} fail ${nr.length} not-reached`,
        );
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    await page
      .locator('.ss-list')
      .first()
      .evaluate((e) => e.scrollIntoView({ block: 'start' }));
    if (ENT_SHOT.includes(s.key)) await shot('entitlements-rows');
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT problems ${JSON.stringify(problems)}`);
  writeFileSync(
    resolve(OUT, BASE ? 'totals-before.json' : 'totals.json'),
    JSON.stringify({ shots, problems }, null, 1),
  );
  if (!BASE && problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
