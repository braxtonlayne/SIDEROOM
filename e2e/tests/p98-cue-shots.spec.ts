import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { edgeStrips } from '../support/edge-strips';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-438 (wave 98; D-945). Admin > Donations > Invoices: between 721 px and the width where the table fits, the table
 * scrolls in its own wrapper (D-817); the wrapper shows a soft shade at each inline edge that has more table beyond it.
 * One run holds the base (the rule switched off by an injected style) and the after, so the figures are compared in the
 * same world. Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed
 * the sweep world"; that spec is never edited) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`,
 * `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`
 * (an invoice needs a price quote; D-288). The rows are made through the API as wave 92's spec makes them (own account
 * names, D-942); nothing is faked.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8762 / 5762):
 *   SR_P98_CUE_ON=1 SR_E2E_BASE_URL=http://localhost:5762 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P98_CUE_OUT=<folder> npx playwright test tests/p98-cue-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 * Settings: Lounge Light and Lounge Dark (account theme by PATCH and localStorage, `data-theme` read back into the file
 * name) at 1024x768 and 900x768 and wider (to find where the table fits), 375x812 touch. The shade is checked by pixels:
 * the two edge strips of the wrapper are cut out of a screenshot with the rule off and on, at scroll start, middle and end,
 * and compared byte for byte (a strip that differs has a shade). One JSON line per measure in `measure.json`.
 */
const ON = process.env.SR_P98_CUE_ON === '1';
const OUT =
  process.env.SR_P98_CUE_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p98-cue';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const OFF_ID = 'p98-cue-rule-off';
const OFF_CSS = '.adm-table-wrap:has(> .inv-table){background-image:none !important}';
const WRAP = '.adm-table-wrap:has(> .inv-table)';
const STEPS = [1024, 1100, 1200, 1280, 1366, 1440, 1600, 1920];
// BIP-84 test vector as a signet tpub (the donations tests use the same key).
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
interface Wrapm {
  vw: number;
  doc: number;
  scrollWidth: number;
  clientWidth: number;
  height: number;
  scrollLeft: number;
  wrapBox: Box;
  theme: string | null;
  heads: { text: string; box: Box; out: boolean }[];
  cells: Box[];
  grounds: Record<string, string>;
  tokens: Record<string, string>;
  bgImage: string;
}

/** Runs in the page: the wrapper's figures, the header boxes, the grounds between the wrapper and the eye. */
function measureWrap(sel: string): Wrapm | null {
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const wrap = document.querySelector<HTMLElement>(sel);
  if (!wrap) return null;
  const de = document.documentElement;
  const wr = wrap.getBoundingClientRect();
  const bg = (e: Element | null) => (e ? getComputedStyle(e).backgroundColor : 'none');
  const grounds: Record<string, string> = {
    wrapper: bg(wrap),
    table: bg(wrap.querySelector('table')),
    thead: bg(wrap.querySelector('thead')),
    theadTr: bg(wrap.querySelector('thead tr')),
    th: bg(wrap.querySelector('thead th')),
    tbodyTr: bg(wrap.querySelector('tbody tr')),
    td: bg(wrap.querySelector('tbody td')),
    tdImage: wrap.querySelector('tbody td')
      ? getComputedStyle(wrap.querySelector('tbody td')!).backgroundImage
      : 'none',
  };
  let k = 0;
  for (let e: Element | null = wrap.parentElement; e && k < 12; e = e.parentElement, k++) {
    const c = getComputedStyle(e).backgroundColor;
    if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') {
      grounds.firstOpaqueAncestor = `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)} ${c}`;
      break;
    }
  }
  const tokens: Record<string, string> = {};
  for (const t of ['--ui-bg', '--ui-bg', '--ui-bg-raised', '--ui-text', '--ui-border']) {
    const p = document.createElement('div');
    p.style.cssText = `position:absolute;background:var(${t});color:var(${t})`;
    wrap.appendChild(p);
    tokens[t] = `${getComputedStyle(p).backgroundColor}`;
    p.remove();
  }
  return {
    vw: de.clientWidth,
    doc: de.scrollWidth - de.clientWidth,
    scrollWidth: wrap.scrollWidth,
    clientWidth: wrap.clientWidth,
    height: f(wr.height),
    scrollLeft: f(wrap.scrollLeft),
    wrapBox: rect(wrap),
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? null,
    heads: Array.from(wrap.querySelectorAll('thead th')).map((th) => {
      const r = th.getBoundingClientRect();
      return {
        text: (th.textContent ?? '').trim().slice(0, 20),
        box: rect(th),
        out: r.right > wr.right + 0.5 || r.left < wr.left - 0.5,
      };
    }),
    cells: Array.from(wrap.querySelectorAll('tbody tr, tbody td')).map(rect),
    grounds,
    tokens,
    bgImage: getComputedStyle(wrap).backgroundImage,
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
      reason: 'wave 98 cue shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 98 cue shots: donations on for the dev stack',
    }),
  );
  // Three accounts; the first with the longest plausible display name.
  const longName = 'Bartholomew Featherstonehaugh-Vi';
  const people: {
    user: { id: string; username: string; password: string };
    req: APIRequestContext;
  }[] = [];
  for (const [label, name] of [
    ['p98cuelongname', longName],
    ['p98cuetwo', 'Juniper'],
    ['p98cuethree', 'Wren Alder-Okonkwo'],
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
        reason: 'wave 98 cue shots',
      }),
    );
  // Entitlements: patron grants for two accounts, one permanent; a pool grant for a server the third account makes.
  await step('grant patron 30 days', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[0]!.user.id,
      days: 30,
      reason:
        'wave 98 cue shots: thanks for the long-running help with the translation work on the help centre',
    }),
  );
  await step('grant patron permanent', () =>
    apiPost(ownerReq, '/admin/donations/entitlements', {
      kind: 'patron',
      user_id: people[1]!.user.id,
      permanent: true,
      reason: 'wave 98 cue shots',
    }),
  );
  const g = await step('server for pool grant', () =>
    apiPost<{ id: string }>(people[2]!.req, '/guilds', {
      name: 'P98 Cue Reading Circle',
    }),
  );
  if (g)
    await step('grant pool', () =>
      apiPost(ownerReq, '/admin/donations/entitlements', {
        kind: 'server_pool',
        guild_id: (g as { id: string }).id,
        usd_cents: 2500,
        reason: 'wave 98 cue shots',
      }),
    );
  return log;
}

type Edge = { l: Buffer; r: Buffer };

const THEMES = [
  { key: 'light', theme: 'lounge-light' },
  { key: 'dark', theme: 'lounge-dark' },
] as const;

test('SEC-AUD-438: the Invoices wrapper shows an edge shade where the table scrolls sideways', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const stripClips: string[] = [];
  const problems: string[] = [];
  const notes: string[] = [];
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
  let fitW = 0;

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
    const goTo = async (page: Page, label: RegExp) => {
      await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 4000);
      await page
        .locator(NAV)
        .filter({ hasText: label })
        .first()
        .evaluate((el) => (el as HTMLElement).click());
      await idle(page, 2500);
    };
    const setRule = (page: Page, on: boolean) =>
      page.evaluate(
        ([id, css, want]) => {
          document.getElementById(id as string)?.remove();
          if (!want) {
            const st = document.createElement('style');
            st.id = id as string;
            st.textContent = css as string;
            document.head.appendChild(st);
          }
        },
        [OFF_ID, OFF_CSS, on] as const,
      );
    const themeOf = (page: Page) =>
      page.evaluate(
        () =>
          (
            document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
          )?.getAttribute('data-theme') ?? 'no-theme',
      );
    const log = (rec: unknown) => appendFileSync(jsonl, `${JSON.stringify(rec)}\n`);
    const scrollTo = async (page: Page, where: 'start' | 'mid' | 'end') => {
      await page.locator(WRAP).evaluate((w, wh) => {
        w.scrollIntoView({ block: 'center' });
        const max = w.scrollWidth - w.clientWidth;
        w.scrollLeft = wh === 'start' ? 0 : wh === 'mid' ? max / 2 : max;
      }, where);
      await page.mouse.move(0, 0);
      await page.waitForTimeout(150);
    };
    const shot = async (page: Page, name: string, setting: string) => {
      const file = `${name}-${setting}-${await themeOf(page)}.png`;
      await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
      shots.push(file);
    };
    const edges = async (page: Page, where: 'start' | 'mid' | 'end'): Promise<Edge> => {
      await scrollTo(page, where);
      // SEC-AUD-447 (D-957): cut from the part of the wrapper that is inside the viewport.
      const { l, r, clip } = await edgeStrips(page, WRAP, { width: 16, maxHeight: 200 });
      stripClips.push(`${where} y ${clip.y} h ${clip.height} viewport h ${clip.viewportHeight}`);
      return { l, r };
    };
    const hits = async (page: Page) => {
      await scrollTo(page, 'start');
      const wrap = page.locator(WRAP);
      await openCollapsed(page, wrap);
      const rows = await hitTestAll(page, wrap);
      return rows.map((r) => ({
        name: r.name,
        status: r.status,
        taker: r.result?.points.find((p) => !p.ok)?.taker?.path ?? null,
      }));
    };

    // ---------------------------------------------------------------- desktop 1024 x 768
    const d = await mkCtx(1024, 768, false);
    const page = d.page;
    await goTo(page, /^Invoices$/);
    await page.locator(WRAP).first().waitFor({ timeout: 8000 });
    // BASE: the rule is switched off.
    await setRule(page, false);
    await scrollTo(page, 'start');
    const b1024 = (await page.evaluate(measureWrap, WRAP))!;
    log({ id: 'base', s: s.key, ...b1024 });
    console.log(
      `REPORT ${s.key} BASE 1024: wrapper client ${b1024.clientWidth} scroll ${b1024.scrollWidth} (+${b1024.scrollWidth - b1024.clientWidth}) h ${b1024.height} doc ${b1024.doc}`,
    );
    console.log(`REPORT   heads ${JSON.stringify(b1024.heads)}`);
    console.log(`REPORT   grounds ${JSON.stringify(b1024.grounds)}`);
    console.log(`REPORT   tokens ${JSON.stringify(b1024.tokens)}`);
    if (s.key === 'light') await shot(page, 'before-invoices', '1024-start');
    const baseEdges = {
      start: await edges(page, 'start'),
      mid: await edges(page, 'mid'),
      end: await edges(page, 'end'),
    };
    const baseHits = await hits(page);
    console.log(`REPORT ${s.key} BASE hit ${JSON.stringify(baseHits)}`);
    await page.setViewportSize({ width: 900, height: 768 });
    await page.waitForTimeout(400);
    await scrollTo(page, 'start');
    const b900 = (await page.evaluate(measureWrap, WRAP))!;
    log({ id: 'base900', s: s.key, ...b900 });
    console.log(
      `REPORT ${s.key} BASE 900: client ${b900.clientWidth} scroll ${b900.scrollWidth} (+${b900.scrollWidth - b900.clientWidth}) h ${b900.height}; out of sight: ${b900.heads.filter((h) => h.out).map((h) => h.text)}`,
    );
    console.log(
      `REPORT ${s.key} BASE 1024 out of sight: ${b1024.heads.filter((h) => h.out).map((h) => h.text)}`,
    );
    if (b1024.scrollWidth <= b1024.clientWidth + 1 && b900.scrollWidth <= b900.clientWidth + 1)
      problems.push(`${s.key}: the table fits at 900 and 1024: the fault is not as the plan says`);
    if (s.key === 'light') {
      for (const w of STEPS) {
        await page.setViewportSize({ width: w, height: 768 });
        await page.waitForTimeout(400);
        const m = (await page.evaluate(measureWrap, WRAP))!;
        console.log(`REPORT light BASE ${w}: client ${m.clientWidth} scroll ${m.scrollWidth}`);
        if (!fitW && m.scrollWidth <= m.clientWidth + 1) fitW = w;
      }
      console.log(`REPORT table fits from ${fitW || 'more than 1920'}`);
    }
    // 900 after
    await setRule(page, true);
    await page.setViewportSize({ width: 900, height: 768 });
    await page.waitForTimeout(400);
    await scrollTo(page, 'start');
    const a900 = (await page.evaluate(measureWrap, WRAP))!;
    if (
      a900.scrollWidth !== b900.scrollWidth ||
      a900.clientWidth !== b900.clientWidth ||
      a900.height !== b900.height
    )
      problems.push(`${s.key}: 900 wrapper figures differ from the base`);
    // AFTER at 1024
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.waitForTimeout(400);
    await scrollTo(page, 'start');
    const a1024 = (await page.evaluate(measureWrap, WRAP))!;
    log({ id: 'after', s: s.key, ...a1024 });
    console.log(
      `REPORT ${s.key} AFTER 1024: client ${a1024.clientWidth} scroll ${a1024.scrollWidth} h ${a1024.height}; bg ${a1024.bgImage.slice(0, 80)}...`,
    );
    const moved = a1024.heads.filter(
      (h, i) => JSON.stringify(h.box) !== JSON.stringify(b1024.heads[i]?.box),
    );
    console.log(`REPORT ${s.key} heads moved vs base: ${moved.length} of ${a1024.heads.length}`);
    if (moved.length) problems.push(`${s.key}: ${moved.length} header boxes moved`);
    if (
      a1024.scrollWidth !== b1024.scrollWidth ||
      a1024.clientWidth !== b1024.clientWidth ||
      a1024.height !== b1024.height
    )
      problems.push(`${s.key}: 1024 wrapper figures differ from the base`);
    if (JSON.stringify(a1024.cells) !== JSON.stringify(b1024.cells))
      problems.push(`${s.key}: row/cell boxes moved`);
    for (const [k, v] of Object.entries(a1024.grounds))
      if (
        ['table', 'thead', 'theadTr', 'th', 'tbodyTr', 'td'].includes(k) &&
        v !== 'rgba(0, 0, 0, 0)'
      )
        problems.push(`${s.key}: ${k} paints a ground ${v}`);
    const anc = a1024.grounds.firstOpaqueAncestor ?? '';
    console.log(
      `REPORT ${s.key} first opaque ancestor: ${anc}; --ui-bg ${a1024.tokens['--ui-bg']}`,
    );
    if (!anc.endsWith(a1024.tokens['--ui-bg'] ?? '?'))
      problems.push(`${s.key}: the cover token is not the ground under the wrapper (${anc})`);
    const aft = {
      start: await edges(page, 'start'),
      mid: await edges(page, 'mid'),
      end: await edges(page, 'end'),
    };
    const diff = (k: 'start' | 'mid' | 'end') => ({
      l: !aft[k].l.equals(baseEdges[k].l),
      r: !aft[k].r.equals(baseEdges[k].r),
    });
    const dS = diff('start');
    const dM = diff('mid');
    const dE = diff('end');
    console.log(
      `REPORT ${s.key} shade (strip differs from base) start L${dS.l} R${dS.r}; mid L${dM.l} R${dM.r}; end L${dE.l} R${dE.r}`,
    );
    if (dS.l || !dS.r) problems.push(`${s.key}: scroll start: shade L${dS.l} R${dS.r}`);
    if (!dM.l || !dM.r) problems.push(`${s.key}: middle: shade L${dM.l} R${dM.r}`);
    if (!dE.l || dE.r) problems.push(`${s.key}: scroll end: shade L${dE.l} R${dE.r}`);
    log({ id: 'shade', s: s.key, start: dS, mid: dM, end: dE });
    await scrollTo(page, 'start');
    await shot(page, 'invoices', '1024-start');
    if (s.key === 'light') {
      await scrollTo(page, 'end');
      await shot(page, 'invoices', '1024-end');
    }
    const aftHits = await hits(page);
    console.log(`REPORT ${s.key} AFTER hit ${JSON.stringify(aftHits)}`);
    if (JSON.stringify(aftHits) !== JSON.stringify(baseHits))
      problems.push(`${s.key}: hit test after differs from the base`);
    if (aftHits.some((r) => r.status === 'not-reached'))
      problems.push(`${s.key}: hit test has not-reached rows (probe error)`);
    console.log(
      `REPORT ${s.key} hit rows ${aftHits.length}: ${aftHits.filter((r) => r.status === 'pass').length} pass, ${aftHits.filter((r) => r.status === 'fail').length} fail, ${aftHits.filter((r) => r.status === 'not-reached').length} not-reached`,
    );

    if (s.key === 'light') {
      // The width where the table fits: no scroll, no shade.
      if (fitW) {
        await page.setViewportSize({ width: fitW, height: 768 });
        await page.waitForTimeout(400);
        await setRule(page, false);
        const fb = await edges(page, 'start');
        await setRule(page, true);
        const fa = await edges(page, 'start');
        const fm = (await page.evaluate(measureWrap, WRAP))!;
        console.log(
          `REPORT light FIT ${fitW}: client ${fm.clientWidth} scroll ${fm.scrollWidth}; shade L${!fa.l.equals(fb.l)} R${!fa.r.equals(fb.r)}`,
        );
        if (fm.scrollWidth > fm.clientWidth + 1) problems.push('fit width still scrolls');
        if (!fa.l.equals(fb.l) || !fa.r.equals(fb.r))
          problems.push('a shade is drawn where the table fits');
        log({ id: 'fit', fitW, ...fm });
        await scrollTo(page, 'start');
        await shot(page, 'invoices', `fit-${fitW}`);
      } else notes.push('the table does not fit at any width up to 1920');
      // RTL: one measurement (both edges alike).
      await page.setViewportSize({ width: 1024, height: 768 });
      await page.waitForTimeout(400);
      try {
        await page.evaluate(() => (document.documentElement.dir = 'rtl'));
        await page.waitForTimeout(300);
        await setRule(page, false);
        const rb = await edges(page, 'start');
        await setRule(page, true);
        const ra = await edges(page, 'start');
        const rwm = (await page.evaluate(measureWrap, WRAP))!;
        const msg = `RTL (dir=rtl on the root, scrollLeft 0): wrapper client ${rwm.clientWidth} scroll ${rwm.scrollWidth}; shade L${!ra.l.equals(rb.l)} R${!ra.r.equals(rb.r)}`;
        console.log(`REPORT light ${msg}`);
        notes.push(msg);
        await page.evaluate(() => (document.documentElement.dir = 'ltr'));
      } catch (e) {
        notes.push(`RTL not measured: ${String(e).slice(0, 120)}`);
      }
      // Other Admin tables: another wrapper's computed background-image.
      const labels = await page.locator(NAV).allTextContents();
      let other: string | null = null;
      for (let i = 0; i < labels.length && !other; i++) {
        await page
          .locator(NAV)
          .nth(i)
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 1500);
        other = await page.evaluate(() => {
          const w = document.querySelector<HTMLElement>('.adm-table-wrap:not(:has(> .inv-table))');
          return w
            ? `${(w.parentElement?.className ?? '').slice(0, 30)} bg ${getComputedStyle(w).backgroundImage}`
            : null;
        });
        if (other) notes.push(`other Admin table (nav "${labels[i]}"): wrapper in ${other}`);
      }
      if (!other) notes.push('no other .adm-table-wrap found in the nav');
      else if (!other.endsWith('none'))
        problems.push(`another Admin table wrapper has a background: ${other}`);
    }
    await d.ctx.close().catch(() => undefined);

    // ---------------------------------------------------------------- phone 375 x 812, touch
    if (s.key === 'light') {
      const p = await mkCtx(375, 812, true);
      await goTo(p.page, /^Invoices$/);
      await p.page.locator(WRAP).first().waitFor({ timeout: 8000 });
      await setRule(p.page, false);
      const pb = (await p.page.evaluate(measureWrap, WRAP))!;
      await setRule(p.page, true);
      const pa = (await p.page.evaluate(measureWrap, WRAP))!;
      log({ id: 'phone', before: pb, after: pa });
      console.log(
        `REPORT phone 375: wrapper client ${pa.clientWidth} scroll ${pa.scrollWidth} bg ${pa.bgImage}; cell boxes equal ${JSON.stringify(pa.cells) === JSON.stringify(pb.cells)} (${pa.cells.length})`,
      );
      if (pa.bgImage !== 'none') problems.push(`phone: wrapper background-image ${pa.bgImage}`);
      if (JSON.stringify(pa.cells) !== JSON.stringify(pb.cells))
        problems.push('phone: row boxes differ');
      if (pa.doc > 0) problems.push(`phone: page scrolls sideways ${pa.doc}`);
      await p.page.locator(WRAP).evaluate((w) => w.scrollIntoView({ block: 'start' }));
      await p.page.waitForTimeout(200);
      await shot(p.page, 'invoices', '375');
      await p.ctx.close().catch(() => undefined);
    }
  }
  await ownerCtx.close();
  for (const n of notes) console.log(`REPORT note ${n}`);
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  console.log(`REPORT strips ${JSON.stringify(stripClips)}`);
  console.log(`REPORT problems ${JSON.stringify(problems)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify({ shots, notes, problems }, null, 1));
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
