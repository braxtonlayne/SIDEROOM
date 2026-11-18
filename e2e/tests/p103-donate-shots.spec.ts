import { existsSync, mkdirSync, readFileSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, apiPut, loginApi, registerUser, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-465 item 8 (wave 103; D-972). Admin > Donations: the five raw `<select>` (Invoices filters State and Purpose,
 * the resolve dialog's Resolution, Entitlements' Kind filter, the grant dialog's Kind). The page's two label layout
 * rules (`admin.css` `.ss-toolbar > label:has(> select)`, `donations.css` `.inv-toolbar > label:has(> select)`) stop
 * matching a select that sits in the kit's wrapper span, so only the two dialog selects move to `NativeSelect`; the three
 * toolbar selects stay raw. The spec measures all five, and the Invoices table, in either phase:
 *   - after (default): the tree as it is. Shots named `<name>.png`.
 *   - base (`SR_P103_DONATE_BASE=1`): run with the two `.tsx` files put back to the base's text (the builder does that
 *     with copies, never a stash). Shots named `base-<name>.png`; the figures go to `measure-base.json`.
 * The base run seeds two invoices (own account names, D-942) and writes `seed.json`; the after run reads it (one world
 * allows four registering specs, so the pair of runs registers two accounts). Invoices cannot be deleted; nothing else is
 * kept.
 *
 * Needs the sweep's seeded world (`e2e/.state/sweep/owner.json`) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`,
 * `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9` and `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`.
 *
 * The command the commander runs (from `e2e/`, sweep world seeded on the same database; ports 8806 / 5806):
 *   SR_P103_DONATE_ON=1 SR_E2E_BASE_URL=http://localhost:5806 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P103_DONATE_OUT=<folder> npx playwright test tests/p103-donate-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 */
const ON = process.env.SR_P103_DONATE_ON === '1';
const BASE = process.env.SR_P103_DONATE_BASE === '1';
const OUT =
  process.env.SR_P103_DONATE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-donate';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const NAV = '[role="dialog"] nav ul li > button';
const WRAP = '.adm-table-wrap:has(> .inv-table)';
const TPUB =
  'tpubDCxX2sYFS5bDkSe5GKKYHjBW7tgyN1R3UchpLJvdbf54ohxeGRtd8MbDUe1cguVHe4vnK68DsuD5MXjxi9EXx16rb9EnNsaF5KT99CinaJz';
const PFX = BASE ? 'base-' : '';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface SelFig {
  name: string;
  box: Box;
  wrapper: boolean;
  label: Box | null;
  labelDisplay: string | null;
  labelFontSize: string | null;
  textTransform: string;
  fontSize: string;
  selectedText: string;
  textClear: boolean;
  chevronClass: string;
}
interface Fig {
  vw: number;
  doc: number;
  selects: SelFig[];
  rowTops: number[];
  items: { tag: string; text: string; box: Box }[];
  table: { box: Box; scrollWidth: number; clientWidth: number } | null;
}

const THEMES = [
  { key: 'light', theme: 'lounge-light' },
  { key: 'dark', theme: 'lounge-dark' },
] as const;

/** Runs in the page: figures of every select under `rootSel` (the toolbar or the dialog) and its neighbours. */
function figuresInPage([rootSel, wrapSel]: [string, string]): Fig {
  const box = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    const f = (n: number) => Math.round(n * 10) / 10;
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const root =
    rootSel === 'dialog'
      ? (Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).pop() ??
        document.body)
      : (document.querySelector<HTMLElement>(rootSel) ?? document.body);
  const canvas = document.createElement('canvas').getContext('2d')!;
  const selects: SelFig[] = Array.from(root.querySelectorAll<HTMLSelectElement>('select')).map(
    (s) => {
      const cs = getComputedStyle(s);
      const lab = s.closest('label');
      const lcs = lab ? getComputedStyle(lab) : null;
      canvas.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
      const sel = s.options[s.selectedIndex]?.textContent ?? '';
      const tw = canvas.measureText(sel).width;
      const inner =
        s.getBoundingClientRect().width -
        parseFloat(cs.paddingLeft) -
        parseFloat(cs.paddingRight) -
        parseFloat(cs.borderLeftWidth) -
        parseFloat(cs.borderRightWidth);
      return {
        name:
          lab?.textContent?.replace(sel, '').trim().slice(0, 24) ??
          s.getAttribute('aria-label') ??
          '',
        box: box(s),
        wrapper: s.parentElement?.tagName === 'SPAN' && s.parentElement !== lab,
        label: lab ? box(lab) : null,
        labelDisplay: lcs?.display ?? null,
        labelFontSize: lcs?.fontSize ?? null,
        textTransform: cs.textTransform,
        fontSize: cs.fontSize,
        selectedText: sel,
        textClear: tw <= inner + 0.5,
        chevronClass: s.className.trim().split(/\s+/)[0] ?? '',
      };
    },
  );
  const kids = Array.from(root.children).filter(
    (c) => c.getBoundingClientRect().height > 0,
  ) as HTMLElement[];
  const items = kids.map((k) => ({
    tag: k.tagName.toLowerCase(),
    text: (k.textContent ?? '').trim().slice(0, 22),
    box: box(k),
  }));
  const rowTops = Array.from(new Set(items.map((i) => Math.round(i.box.y / 4) * 4))).sort(
    (a, b) => a - b,
  );
  const wrap = document.querySelector<HTMLElement>(wrapSel);
  const t = wrap?.querySelector<HTMLElement>('.inv-table') ?? null;
  return {
    vw: document.documentElement.clientWidth,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    selects,
    rowTops,
    items,
    table:
      wrap && t
        ? { box: box(t), scrollWidth: wrap.scrollWidth, clientWidth: wrap.clientWidth }
        : null,
  };
}

async function seedRows(
  ownerReq: APIRequestContext,
  fresh: () => Promise<APIRequestContext>,
): Promise<{ log: string[]; ids: string[] }> {
  const log: string[] = [];
  const ids: string[] = [];
  const step = async <T>(name: string, fn: () => Promise<T>) => {
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
      reason: 'wave 103 donate shots: signet test wallet',
    }),
  );
  await step('enable donations', () =>
    apiPatch(ownerReq, '/admin/donations/config', {
      changes: { enabled: true },
      reason: 'wave 103 donate shots: donations on for the dev stack',
    }),
  );
  const people: { req: APIRequestContext }[] = [];
  for (const [label, name] of [
    ['p103donateone', 'Ottoline Brightwater-Nwosu'],
    ['p103donatetwo', 'Caspian Merrow-Delacroix'],
  ] as const) {
    const anon = await fresh();
    const user = await registerUser(anon, label);
    const req = await fresh();
    await loginApi(req, user);
    await step(`name ${label}`, () => apiPatch(req, '/users/@me', { global_name: name }));
    people.push({ req });
  }
  const a = await step('invoice 0', () =>
    apiPost<{ id: string }>(people[0]!.req, '/donations/invoices', {
      purpose: 'patron',
      months: 12,
    }),
  );
  const b = await step('invoice 1', () =>
    apiPost<{ id: string }>(people[1]!.req, '/donations/invoices', {
      purpose: 'donate',
      usd_cents: 4999,
    }),
  );
  if (a) ids.push(a.id);
  if (b) ids.push(b.id);
  return { log, ids };
}

test('SEC-AUD-465 item 8: Admin > Donations selects, base and after', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, BASE ? 'measure-base.json' : 'measure.json');
  const seedFile = resolve(OUT, 'seed.json');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const problems: string[] = [];
  const notes: string[] = [];
  const flag = BASE ? notes : problems; // the base is measured, not judged
  let ids: string[] = [];
  if (!BASE && existsSync(seedFile)) {
    ids = (JSON.parse(readFileSync(seedFile, 'utf8')) as { ids: string[] }).ids;
    console.log(`REPORT seed read from seed.json: ${ids.join(',')}`);
  } else {
    const seeded = await seedRows(
      o,
      async () => (await browser.newContext({ baseURL: ORIGIN })).request,
    );
    ids = seeded.ids;
    for (const l of seeded.log) console.log(`REPORT seed ${l}`);
    writeFileSync(seedFile, JSON.stringify({ ids, log: seeded.log }));
    if (ids.length < 2) problems.push('an invoice was not seeded (stack error: rate source?)');
  }
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };

  try {
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
      const sizes: { w: number; h: number; phone: boolean; tag: string }[] = [
        { w: 1024, h: 768, phone: false, tag: '1024' },
        ...(s.key === 'light'
          ? [
              { w: 900, h: 768, phone: false, tag: '900' },
              { w: 1280, h: 800, phone: false, tag: '1280' },
              { w: 375, h: 812, phone: true, tag: '375' },
            ]
          : []),
      ];
      for (const z of sizes) {
        const ctx = await browser.newContext({
          baseURL: ORIGIN,
          viewport: { width: z.w, height: z.h },
          storageState: await ownerCtx.storageState(),
          bypassCSP: true,
          ...(z.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
        const theme = () =>
          page.evaluate(
            () =>
              (
                document.querySelector('.ss-root[data-theme]') ??
                document.querySelector('[data-theme]')
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
        const measure = async (id: string, root: string) => {
          const f = await page.evaluate(figuresInPage, [root, WRAP] as [string, string]);
          appendFileSync(
            jsonl,
            `${JSON.stringify({ id, setting: s.key, size: z.tag, base: BASE, ...f })}\n`,
          );
          console.log(
            `REPORT ${PFX}${s.key} ${z.tag} ${id}: doc ${f.doc} rowTops ${JSON.stringify(f.rowTops)} table ${JSON.stringify(f.table?.box)} wrap ${f.table?.clientWidth}/${f.table?.scrollWidth}`,
          );
          for (const q of f.selects)
            console.log(
              `REPORT ${PFX}${s.key} ${z.tag} ${id} select "${q.name}" box ${JSON.stringify(q.box)} wrapper ${q.wrapper} label ${JSON.stringify(q.label)} ${q.labelDisplay} ${q.labelFontSize} text ${q.fontSize} ${q.textTransform} "${q.selectedText}" clear ${q.textClear}`,
            );
          if (f.doc > 0)
            problems.push(`${PFX}${s.key} ${z.tag} ${id}: page scrolls sideways by ${f.doc}`);
          for (const q of f.selects) {
            if (q.textTransform !== 'none')
              flag.push(
                `${PFX}${s.key} ${z.tag} ${id} "${q.name}": text-transform ${q.textTransform}`,
              );
            if (q.fontSize !== '14px')
              flag.push(`${PFX}${s.key} ${z.tag} ${id} "${q.name}": font-size ${q.fontSize}`);
            if (!q.textClear) flag.push(`${PFX}${s.key} ${z.tag} ${id} "${q.name}": text cut`);
            const want = z.phone ? 44 : 40;
            if (q.box.h < want - 0.5)
              notes.push(
                `${PFX}${s.key} ${z.tag} ${id} "${q.name}": block size ${q.box.h} (< ${want})`,
              );
          }
          return f;
        };
        const shoot = async (name: string, wanted: boolean) => {
          if (!wanted) return;
          const th = await theme();
          console.log(`REPORT ${PFX}${name}: data-theme ${th}`);
          const file = `${PFX}${name}.png`;
          await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
          shots.push(file);
        };
        const hits = async (id: string, rootLoc: ReturnType<Page['locator']>) => {
          if (!z.phone) return;
          const opened = await openCollapsed(page, rootLoc);
          const rows = await hitTestAll(page, rootLoc);
          const bad = rows.filter((r) => r.status === 'fail');
          const lost = rows.filter((r) => r.status === 'not-reached');
          console.log(
            `REPORT ${PFX}${s.key} ${z.tag} hit ${id}: ${rows.length} rows, opened ${opened.length}, pass ${rows.filter((r) => r.status === 'pass').length}, fail ${bad.length}${bad
              .map(
                (r) =>
                  ` [${r.name}: ${r.result?.points
                    .filter((p) => !p.taker || p.taker.tag)
                    .map((p) => `${p.at}:${p.taker?.tag}.${p.taker?.name}`)
                    .join(',')}]`,
              )
              .join('')}, not-reached ${lost.length}`,
          );
          if (lost.length)
            problems.push(`${s.key} ${z.tag} hit ${id}: not-reached ${lost.length} (probe error)`);
          for (const r of bad) notes.push(`${PFX}${s.key} ${z.tag} hit ${id}: fail ${r.name}`);
        };

        // ---------------------------------------------------------------- Invoices
        await goTo(/^Invoices$/);
        await page.locator(WRAP).first().waitFor({ timeout: 8000 });
        await page.locator('form.inv-toolbar').scrollIntoViewIfNeeded();
        const inv = await measure('invoices', 'form.inv-toolbar');
        if (inv.selects.length !== 2)
          problems.push(`${s.key} ${z.tag}: ${inv.selects.length} filters`);
        await shoot(
          `invoices-${z.tag}-lounge-${s.key}`,
          (z.tag === '1024' && s.key === 'light') ||
            (z.tag === '1024' && s.key === 'dark') ||
            z.tag === '1280' ||
            z.tag === '375',
        );
        if (BASE && z.tag === '1024' && s.key === 'light')
          await page.screenshot({ path: resolve(OUT, 'base-invoices-1024.png') });
        await hits('invoices-toolbar', page.locator('form.inv-toolbar'));
        // keyboard: the State filter (one per file)
        if (z.tag === '1024' && s.key === 'light') {
          const state = page.locator('form.inv-toolbar select').first();
          const rowsBefore = await page.locator('.inv-table tbody tr').count();
          await state.focus();
          await page.keyboard.press('ArrowDown');
          await idle(page, 3000);
          const v = await state.inputValue();
          const rowsAfter = await page.locator('.inv-table tbody tr').count();
          const states = await page
            .locator('.inv-table tbody tr td[data-label="State"]')
            .allInnerTexts();
          console.log(
            `REPORT ${PFX}keyboard State filter: value "${v}", rows ${rowsBefore} -> ${rowsAfter}, states ${JSON.stringify(states.slice(0, 4))}`,
          );
          if (v === '') problems.push('keyboard: ArrowDown did not change the State filter');
          await state.selectOption('');
          await idle(page, 3000);
        }
        // the resolve dialog (an open invoice of my own)
        if (ids[0]) {
          await page.locator('form.inv-toolbar').scrollIntoViewIfNeeded();
          const idBtn = page.getByRole('button', { name: ids[0], exact: true }).first();
          if ((await idBtn.count()) === 0)
            notes.push(`${s.key} ${z.tag}: invoice ${ids[0]} not in the first page`);
          else {
            await idBtn.evaluate((el) => (el as HTMLElement).click());
            await idle(page, 2500);
            const byHand = page.getByRole('button', { name: /resolve by hand/i }).first();
            if ((await byHand.count()) === 0)
              problems.push(`${s.key} ${z.tag}: no Resolve by hand button`);
            else {
              await byHand.evaluate((el) => (el as HTMLElement).click());
              await page.waitForTimeout(600);
              const dlg = page.locator('[role="dialog"]').last();
              await measure('resolve-dialog', 'dialog');
              await shoot(`resolve-dialog-${z.tag}`, z.tag === '1024' || z.tag === '375');
              await hits('resolve-dialog', dlg);
              if (z.tag === '1024' && s.key === 'light') {
                const sel = dlg.locator('select').first();
                await sel.focus();
                await page.keyboard.press('ArrowDown');
                const v = await sel.inputValue();
                console.log(`REPORT ${PFX}keyboard Resolution: value "${v}"`);
                if (v !== 'refund_noted') problems.push(`keyboard: Resolution is "${v}"`);
                await sel.selectOption('mark_paid');
              }
              await dlg
                .getByRole('button', { name: /^cancel$/i })
                .evaluate((el) => (el as HTMLElement).click());
              await page.waitForTimeout(400);
            }
          }
        }

        // ---------------------------------------------------------------- Entitlements
        await goTo(/^Entitlements$/);
        await page.locator('.ss-toolbar').first().waitFor({ timeout: 8000 });
        await measure('entitlements', '.ss-toolbar');
        await shoot(
          `entitlements-${z.tag}-lounge-${s.key}`,
          (z.tag === '1024' && s.key === 'light') || z.tag === '375',
        );
        if (BASE && z.tag === '1024' && s.key === 'light')
          await page.screenshot({ path: resolve(OUT, 'base-entitlements-1024.png') });
        await hits('entitlements-toolbar', page.locator('.ss-toolbar').first());
        if (z.tag === '1024' && s.key === 'light') {
          const kind = page.locator('.ss-toolbar select').first();
          await kind.focus();
          await page.keyboard.press('ArrowDown');
          await idle(page, 2500);
          const v = await kind.inputValue();
          console.log(`REPORT ${PFX}keyboard Entitlements Kind filter: value "${v}"`);
          if (v === '') problems.push('keyboard: ArrowDown did not change the Kind filter');
          await kind.selectOption('');
          await idle(page, 2500);
        }
        await page
          .getByRole('button', { name: /grant/i })
          .first()
          .evaluate((el) => (el as HTMLElement).click());
        await page.waitForTimeout(700);
        const gd = page.locator('[role="dialog"]').last();
        await measure('grant-dialog', 'dialog');
        await shoot(`grant-dialog-${z.tag}`, z.tag === '1024' || z.tag === '375');
        await hits('grant-dialog', gd);
        if (z.tag === '1024' && s.key === 'light') {
          const sel = gd.locator('select').first();
          const before = await gd
            .getByText(/account id|server id/i)
            .first()
            .innerText();
          await sel.focus();
          await page.keyboard.press('ArrowDown');
          const v = await sel.inputValue();
          const after = await gd
            .getByText(/account id|server id/i)
            .first()
            .innerText();
          console.log(
            `REPORT ${PFX}keyboard grant Kind: value "${v}", field "${before}" -> "${after}"`,
          );
          if (v !== 'server_pool' || before === after)
            problems.push('keyboard: grant Kind did not switch the field');
          await sel.selectOption('patron');
        }
        await gd
          .getByRole('button', { name: /^cancel$/i })
          .evaluate((el) => (el as HTMLElement).click());
        await ctx.close().catch(() => undefined);
      }
    }
  } finally {
    await ownerCtx.close().catch(() => undefined);
    for (const n of notes) console.log(`REPORT note ${n}`);
    console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
    console.log(`REPORT problems ${JSON.stringify(problems)}`);
    writeFileSync(
      resolve(OUT, BASE ? 'totals-base.json' : 'totals.json'),
      JSON.stringify({ shots, notes, problems }, null, 1),
    );
  }
  if (problems.length) throw new Error(`problems: ${problems.join(' ; ')}`);
});
