import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';

/**
 * SEC-AUD-405 (wave 92; D-918). Admin > Donations > Invoices: (1) at 1280 the "Search" button sits on the controls' line
 * in the filters' row (or, if five items cannot share the row with the search field 200 px or more, at the start edge of
 * row two with no offset); (2) the Invoices table and the Entitlements list are looked at with real rows.
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json` and `world.json`, made by `page-sweep.spec.ts`'s
 * "seed the sweep world"; that spec is never edited). The way into Admin is p90-invoices-shots (the dashboard, then the
 * nav). The stack needs `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` (donations stay off
 * without a chain URL; the watcher fails to reach it, which only shows in Overview) and `SR_PAYMENTS_RATE_SOURCES` with the
 * Coinbase spot source (D-288: `[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]`;
 * a quote is needed to make an invoice; a read-only public price). The rows are made through the API: the owner sets a
 * signet watch-only wallet (the BIP-84 test vector as a tpub) and turns donations on, three accounts make invoices
 * (`POST /donations/invoices`), staff resolves one, staff grants entitlements. Nothing is faked.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database):
 *   SR_P92_INVOICES_ON=1 SR_E2E_BASE_URL=http://localhost:5704 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P92_INVOICES_OUT=<folder> npx playwright test tests/p92-invoices-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (add SR_P92_INVOICES_BASE=1 for the base case: toolbar measures only, shots named `before-`, nothing asserted, no
 *  rows; the base is the tree with `donations.css` as the base has it; SR_P92_INVOICES_SETTINGS = comma list of
 *  `desk`, `desk-light`, `tab`, `phone-light`, `phone-dark`).
 * Settings: 1280x800 Editorial (`desk`) and Lounge Light (`desk-light`), 1024x800 Editorial (`tab`), 375x812 touch in
 * Lounge Light and Lounge Dark (account theme by PATCH and localStorage, `data-theme` read back into the file name).
 * One JSON line per setting in `measure*.json`. Hit tests follow the brief: everything closed is opened, the control
 * is scrolled to the middle, its centre must return an element (else a probe error, never a fault).
 */
const ON = process.env.SR_P92_INVOICES_ON === '1';
const BASE = process.env.SR_P92_INVOICES_BASE === '1';
const OUT =
  process.env.SR_P92_INVOICES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-invoices';
const WANT = (
  process.env.SR_P92_INVOICES_SETTINGS ?? 'desk,desk-light,tab,phone-light,phone-dark'
).split(',');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'desk', w: 1280, h: 800, phone: false, theme: 'lounge-editorial' },
  { key: 'desk-light', w: 1280, h: 800, phone: false, theme: 'lounge-light' },
  { key: 'tab', w: 1024, h: 800, phone: false, theme: 'lounge-editorial' },
  { key: 'phone-light', w: 375, h: 812, phone: true, theme: 'lounge-light' },
  { key: 'phone-dark', w: 375, h: 812, phone: true, theme: 'lounge-dark' },
].filter((s) => WANT.includes(s.key));
// Which shots are taken (12 at most): the toolbar, then the tables.
const TOOLBAR_SHOT = BASE ? ['desk'] : ['desk', 'desk-light', 'tab', 'phone-light'];
const INV_SHOT = ['desk', 'phone-light', 'phone-dark'];
const ENT_SHOT = ['desk', 'phone-light'];
const DETAIL_SHOT = ['desk', 'phone-light'];
const NAV = '[role="dialog"] nav ul li > button';
// BIP-84 test vector as a signet tpub (the donations tests use the same key).
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Item {
  kind: string;
  label: string;
  box: Box;
  labelBox: Box | null;
  textTop: number | null;
  controlBox: Box | null;
  flex: string;
  minW: string;
  marginTop: string;
  hit: boolean | null;
}
interface Toolbar {
  vw: number;
  theme: string | null;
  doc: number;
  form: Box | null;
  formContentW: number;
  items: Item[];
}

function measureToolbar(): Toolbar {
  const de = document.documentElement;
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const form = document.querySelector<HTMLElement>('.ss-root .ss-toolbar');
  const items: Item[] = [];
  const textTop = (lab: Element) => {
    const w = document.createTreeWalker(lab, NodeFilter.SHOW_TEXT);
    let n: Node | null;
    while ((n = w.nextNode())) {
      if ((n.textContent ?? '').trim()) {
        const r = document.createRange();
        r.selectNodeContents(n);
        return f(r.getBoundingClientRect().top);
      }
    }
    return null;
  };
  const touch = de.clientWidth <= 720;
  let formContentW = 0;
  if (form) {
    const cs = getComputedStyle(form);
    formContentW = f(
      form.clientWidth - parseFloat(cs.paddingLeft || '0') - parseFloat(cs.paddingRight || '0'),
    );
    for (const c of Array.from(form.children)) {
      const ctl = c.matches('input,select,button')
        ? c
        : c.querySelector('select, input:not([type=checkbox]), input[type=checkbox], button');
      const lab = c.tagName === 'LABEL' ? c : c.querySelector('label');
      let hit: boolean | null = null;
      if (ctl && touch) {
        (ctl as HTMLElement).scrollIntoView({ block: 'center' });
        const r = ctl.getBoundingClientRect();
        const cx = r.left + r.width / 2;
        const cy = r.top + r.height / 2;
        const own = ctl.closest('label');
        const at = (y: number) => {
          const h = document.elementFromPoint(cx, y);
          return !!h && (h === ctl || ctl.contains(h) || (own?.contains(h) ?? false));
        };
        hit =
          r.width > 0 &&
          r.height > 0 &&
          !!document.elementFromPoint(cx, cy) &&
          at(cy - 21) &&
          at(cy + 21);
      }
      const cs2 = getComputedStyle(c);
      const isCheck = !!ctl && (ctl as HTMLInputElement).type === 'checkbox';
      items.push({
        kind: ctl ? `${ctl.tagName.toLowerCase()}${isCheck ? ':checkbox' : ''}` : c.tagName,
        label: (lab?.textContent ?? c.textContent ?? '').trim().slice(0, 24),
        box: rect(c),
        labelBox: lab ? rect(lab) : null,
        textTop: lab ? textTop(lab) : null,
        controlBox: ctl ? rect(ctl) : null,
        flex: `${cs2.flexGrow} ${cs2.flexShrink} ${cs2.flexBasis}`,
        minW: cs2.minWidth,
        marginTop: cs2.marginBlockStart,
        hit,
      });
    }
  }
  return {
    vw: de.clientWidth,
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    form: form ? rect(form) : null,
    formContentW,
    items,
  };
}

interface Ctl {
  text: string;
  box: Box;
  hit: boolean | null;
  probe: string | null;
}
interface Rows {
  vw: number;
  theme: string | null;
  doc: number;
  sideways: { el: string; client: number; scroll: number }[];
  wrap: { client: number; scroll: number } | null;
  table: Box | null;
  rows: number;
  display: string | null;
  cut: { text: string; client: number; scroll: number }[];
  ctls: Ctl[];
  firstRowText: string[];
}

function measureRows(sel: { rows: string; ctl: string; wrap: string; table: string }): Rows {
  const de = document.documentElement;
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const touch = de.clientWidth <= 720;
  // The hit-test rule: open what is closed first.
  for (const d of Array.from(document.querySelectorAll('details:not([open])')))
    (d as HTMLDetailsElement).open = true;
  const table = document.querySelector<HTMLElement>(sel.table);
  const wrap = document.querySelector<HTMLElement>(sel.wrap);
  const sideways: Rows['sideways'] = [];
  for (let e: HTMLElement | null = table ?? wrap; e; e = e.parentElement) {
    if (e.scrollWidth - e.clientWidth > 1)
      sideways.push({
        el: `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`,
        client: e.clientWidth,
        scroll: e.scrollWidth,
      });
  }
  const cut: Rows['cut'] = [];
  const scope = table ?? wrap ?? document.body;
  for (const e of Array.from(scope.querySelectorAll<HTMLElement>('*'))) {
    if (
      e.scrollWidth - e.clientWidth > 1 &&
      e.children.length === 0 &&
      (e.textContent ?? '').trim() &&
      !e.closest('.us-sr')
    )
      cut.push({
        text: (e.textContent ?? '').trim().slice(0, 40),
        client: e.clientWidth,
        scroll: e.scrollWidth,
      });
  }
  const ctls: Ctl[] = [];
  for (const c of Array.from(document.querySelectorAll<HTMLElement>(sel.ctl))) {
    c.scrollIntoView({ block: 'center', inline: 'center' });
    const r = c.getBoundingClientRect();
    let probe: string | null = null;
    let hit: boolean | null = null;
    if (r.width <= 0 || r.height <= 0) probe = 'empty box';
    else {
      const cx = r.left + r.width / 2;
      const cy = r.top + r.height / 2;
      if (cy < 0 || cy > de.clientHeight || cx < 0 || cx > de.clientWidth) probe = 'off screen';
      else if (!document.elementFromPoint(cx, cy)) probe = 'centre returns null';
      else if (touch) {
        const own = c.closest('label');
        const at = (y: number) => {
          const h = document.elementFromPoint(cx, y);
          return !!h && (h === c || c.contains(h) || (own?.contains(h) ?? false));
        };
        hit = at(cy - 21) && at(cy + 21);
      }
    }
    ctls.push({ text: (c.textContent ?? '').trim().slice(0, 30), box: rect(c), hit, probe });
  }
  const trs = Array.from(document.querySelectorAll<HTMLElement>(sel.rows));
  return {
    vw: de.clientWidth,
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    sideways,
    wrap: wrap ? { client: wrap.clientWidth, scroll: wrap.scrollWidth } : null,
    table: table ? rect(table) : null,
    rows: trs.length,
    display: trs[0] ? getComputedStyle(trs[0]).display : null,
    cut,
    ctls,
    firstRowText: trs[0]
      ? (trs[0].innerText ?? '')
          .split('\n')
          .map((x) => x.trim())
          .filter(Boolean)
      : [],
  };
}

const INV = {
  rows: '.adm-table tbody tr',
  ctl: '.adm-table tbody button, .adm-table tbody a',
  wrap: '.adm-table-wrap',
  table: '.adm-table',
};
const ENT = {
  rows: '.ss-list > li',
  ctl: '.ss-list > li button',
  wrap: '.ss-list',
  table: '.ss-list',
};

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
      reason: 'wave 92 shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 92 shots: donations on for the dev stack',
    }),
  );
  // Three accounts; the first with the longest plausible display name.
  const longName = 'Bartholomew Featherstonehaugh-Vi';
  const people: {
    user: { id: string; username: string; password: string };
    req: APIRequestContext;
  }[] = [];
  for (const [label, name] of [
    ['averyverylongmembernameforinvoices', longName],
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
  const mk = (i: number, body: Record<string, unknown>) =>
    step(`invoice ${i} ${JSON.stringify(body)}`, () =>
      apiPost<{ id: string }>(people[i]!.req, '/donations/invoices', body),
    );
  const made = [
    await mk(0, { purpose: 'patron', months: 12 }),
    await mk(0, { purpose: 'donate', usd_cents: 4999 }),
    await mk(1, { purpose: 'invite_credit', credits: 3 }),
    await mk(2, { purpose: 'donate', usd_cents: 500 }),
    await mk(0, { purpose: 'invite_credit', credits: 1 }),
  ] as ({ id: string } | null)[];
  const second = made[3];
  if (second)
    await step('resolve one by hand', () =>
      apiPost(ownerReq, `/admin/donations/invoices/${second.id}/resolve`, {
        resolution: 'mark_paid',
        note: 'Paid in person at the meetup; the payer sent the txid by message.',
        reason: 'wave 92 shots',
      }),
    );
  // Entitlements: patron grants for two accounts, one permanent; a pool grant for a server the third account makes.
  await step('grant patron 30 days', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[0]!.user.id,
      days: 30,
      reason:
        'wave 92 shots: thanks for the long-running help with the translation work on the help centre',
    }),
  );
  await step('grant patron permanent', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[1]!.user.id,
      permanent: true,
      reason: 'wave 92 shots',
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
        reason: 'wave 92 shots',
      }),
    );
  return log;
}

test('SEC-AUD-405: the Invoices toolbar and the two tables', async ({ browser }) => {
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

  if (!BASE) {
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
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
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

    // ---- Invoices: the toolbar
    await goTo(/^Invoices$/);
    await page.waitForTimeout(300);
    const tb = await page.evaluate(measureToolbar);
    log({ id: 'toolbar', setting: s.key, ...tb });
    console.log(
      `REPORT ${s.key} ${tb.theme} toolbar: doc ${tb.doc} form ${JSON.stringify(tb.form)} content ${tb.formContentW}`,
    );
    for (const i of tb.items) console.log(`REPORT   ${s.key} ${JSON.stringify(i)}`);
    await page
      .locator('form.ss-toolbar')
      .first()
      .evaluate((e) => e.scrollIntoView({ block: 'start' }));
    await page.evaluate(() => window.scrollTo(0, 0));
    if (TOOLBAR_SHOT.includes(s.key)) await shot('invoices-toolbar');

    if (!BASE) {
      const sel = tb.items.filter((i) => i.controlBox && i.kind.startsWith('select'));
      const search = tb.items.find((i) => i.kind === 'input');
      const btn = tb.items.find((i) => i.kind === 'button');
      const state = sel[0];
      if (!s.phone && state && search && btn) {
        const span = (a: number[]) => Math.max(...a) - Math.min(...a);
        const first = tb.items.filter((i) => i.controlBox && i.box.y < state.controlBox!.y + 30);
        const lt = span([...sel.map((i) => i.textTop ?? NaN), search.textTop ?? NaN]);
        const ct = span([...sel.map((i) => i.controlBox!.y), search.controlBox!.y]);
        // Labels on one line and controls on one line (D-900) where the three share the first row.
        if (s.w >= 1280) {
          if (!(lt <= 1)) problems.push(`${s.key}: label tops differ by ${lt}`);
          if (!(ct <= 1)) problems.push(`${s.key}: control tops differ by ${ct}`);
          // The fallback: Search on row two at the start edge, no block offset, level with the checkbox row.
          const chk = tb.items.find((i) => i.kind === 'input:checkbox');
          if (btn.box.y < state.controlBox!.y + 30)
            problems.push(`${s.key}: Search is not on row two (top ${btn.box.y})`);
          if (
            Math.abs(btn.box.x - (tb.form?.x ?? 0)) > 1 &&
            Math.abs(chk!.box.x - (tb.form?.x ?? 0)) > 1
          )
            problems.push(`${s.key}: row two does not start at the start edge`);
          if (btn.marginTop !== '0px')
            problems.push(`${s.key}: Search keeps a block offset ${btn.marginTop}`);
          if (chk && Math.abs(chk.box.y - btn.box.y) > 1)
            problems.push(`${s.key}: checkbox row top ${chk.box.y}, Search top ${btn.box.y}`);
          if (Math.abs(btn.controlBox!.h - state.controlBox!.h) > 1)
            problems.push(
              `${s.key}: Search height ${btn.controlBox!.h}, State ${state.controlBox!.h}`,
            );
          if ((search.controlBox?.w ?? 0) < 200)
            problems.push(`${s.key}: search field ${search.controlBox?.w} wide`);
        }
        void first;
      }
      if (s.phone)
        for (const i of tb.items) {
          if (i.controlBox && i.kind.startsWith('select') && i.controlBox.h < 43.5)
            problems.push(`${s.key}: "${i.label}" ${i.controlBox.h} high`);
          if (i.hit === false) problems.push(`${s.key}: "${i.label}" fails the hit test`);
        }
      if (tb.doc > 0) problems.push(`${s.key}: page scrolls sideways ${tb.doc}`);
    }
    if (BASE) {
      await ctx.close().catch(() => undefined);
      continue;
    }

    // ---- Invoices: the table with rows
    const inv = await page.evaluate(measureRows, INV);
    log({ id: 'invoices-rows', setting: s.key, ...inv });
    console.log(
      `REPORT ${s.key} invoices rows ${inv.rows} display ${inv.display} doc ${inv.doc} wrap ${JSON.stringify(inv.wrap)} table ${JSON.stringify(inv.table)}`,
    );
    console.log(`REPORT   sideways ${JSON.stringify(inv.sideways)} cut ${JSON.stringify(inv.cut)}`);
    console.log(`REPORT   first row ${JSON.stringify(inv.firstRowText)}`);
    for (const c of inv.ctls) console.log(`REPORT   ctl ${JSON.stringify(c)}`);
    if (inv.rows === 0) problems.push(`${s.key}: no invoice rows`);
    if (inv.doc > 0) problems.push(`${s.key}: invoices page scrolls sideways ${inv.doc}`);
    for (const c of inv.ctls) {
      if (c.probe) problems.push(`${s.key}: invoices probe error "${c.text}": ${c.probe}`);
      else if (s.phone && c.hit === false)
        problems.push(`${s.key}: invoices row control "${c.text}" ${c.box.h} high, hit ${c.hit}`);
    }
    await page.evaluate(() => window.scrollTo(0, 0));
    await page
      .locator('.adm-table')
      .first()
      .evaluate((e) => e.scrollIntoView({ block: 'center' }));
    if (INV_SHOT.includes(s.key)) await shot('invoices-rows');

    // The detail of one invoice (its transactions and audit trail).
    const first = page.locator('.adm-table tbody tr th button').first();
    if (await first.count()) {
      await first.scrollIntoViewIfNeeded();
      await first.click();
      await page
        .locator('[role="dialog"] .adm-dl')
        .first()
        .waitFor({ timeout: 8000 })
        .catch(() => undefined);
      await idle(page, 2000);
      const det = await page.evaluate(() => {
        const d = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop();
        const de = document.documentElement;
        if (!d) return null;
        const bs = Array.from(d.querySelectorAll<HTMLElement>('button')).map((b) => {
          b.scrollIntoView({ block: 'center' });
          const r = b.getBoundingClientRect();
          const cx = r.left + r.width / 2;
          const cy = r.top + r.height / 2;
          const at = (y: number) => {
            const h = document.elementFromPoint(cx, y);
            return !!h && (h === b || b.contains(h));
          };
          return {
            text: (b.textContent ?? '').trim().slice(0, 24),
            h: Math.round(r.height * 10) / 10,
            hit: r.width > 0 ? at(cy - 21) && at(cy + 21) : null,
          };
        });
        const cut = Array.from(d.querySelectorAll<HTMLElement>('*'))
          .filter(
            (e) =>
              e.scrollWidth - e.clientWidth > 1 &&
              e.children.length === 0 &&
              (e.textContent ?? '').trim(),
          )
          .map((e) => ({
            text: (e.textContent ?? '').trim().slice(0, 40),
            client: e.clientWidth,
            scroll: e.scrollWidth,
          }));
        return {
          doc: de.scrollWidth - de.clientWidth,
          w: d.getBoundingClientRect().width,
          dScroll: d.scrollWidth - d.clientWidth,
          bs,
          cut,
          text: (d.innerText ?? '').slice(0, 600),
        };
      });
      console.log(`REPORT ${s.key} detail ${JSON.stringify(det)}`);
      log({ id: 'invoice-detail', setting: s.key, det });
      if (det && det.doc > 0) problems.push(`${s.key}: detail scrolls sideways ${det.doc}`);
      if (det && s.phone)
        for (const b of det.bs)
          if (b.hit === false)
            problems.push(`${s.key}: detail button "${b.text}" ${b.h} high, hit ${b.hit}`);
      await page.evaluate(() => {
        const d = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop();
        d?.scrollTo(0, 0);
        d?.querySelectorAll<HTMLElement>('*').forEach((e) => {
          if (e.scrollTop > 0) e.scrollTop = 0;
        });
      });
      if (DETAIL_SHOT.includes(s.key)) await shot('invoice-detail');
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
    } else problems.push(`${s.key}: no invoice id button to open`);

    // ---- Entitlements: the list with rows
    await goTo(/^Entitlements$/);
    const ent = await page.evaluate(measureRows, ENT);
    log({ id: 'entitlements-rows', setting: s.key, ...ent });
    console.log(
      `REPORT ${s.key} entitlements rows ${ent.rows} display ${ent.display} doc ${ent.doc} wrap ${JSON.stringify(ent.wrap)}`,
    );
    console.log(`REPORT   sideways ${JSON.stringify(ent.sideways)} cut ${JSON.stringify(ent.cut)}`);
    console.log(`REPORT   first row ${JSON.stringify(ent.firstRowText)}`);
    for (const c of ent.ctls) console.log(`REPORT   ctl ${JSON.stringify(c)}`);
    if (ent.rows === 0) problems.push(`${s.key}: no entitlement rows`);
    if (ent.doc > 0) problems.push(`${s.key}: entitlements page scrolls sideways ${ent.doc}`);
    for (const c of ent.ctls) {
      if (c.probe) problems.push(`${s.key}: entitlements probe error "${c.text}": ${c.probe}`);
      else if (s.phone && c.hit === false)
        problems.push(
          `${s.key}: entitlements row control "${c.text}" ${c.box.h} high, hit ${c.hit}`,
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
