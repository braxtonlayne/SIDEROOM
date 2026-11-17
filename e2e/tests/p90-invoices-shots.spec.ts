import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-390 (wave 90; D-900). Admin > Donations > Invoices: the filter toolbar's labels share one line and its
 * controls share one line at 1280; on a phone each label sits directly above its own full-width 44 px control.
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the
 * sweep world"; that spec is never edited); the way into Admin is p87-donations-shots (the dashboard, then the nav).
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database):
 *   SR_P90_INVOICES_ON=1 SR_E2E_BASE_URL=http://localhost:5686 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P90_INVOICES_OUT=<folder> npx playwright test tests/p90-invoices-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (add SR_P90_INVOICES_BASE=1 for the base case: the form's `inv-toolbar` class is removed in the page, shots named
 *  `before-`, nothing asserted; SR_P90_INVOICES_SETTINGS = comma list of `phone-light`, `phone-dark`, `desk`).
 * Settings: 375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial (account theme by PATCH and
 * localStorage, `data-theme` read back into the file name). One JSON line per setting in `measure*.json`: the
 * Invoices toolbar's labels (box and text line) and controls, and Entitlements' toolbar boxes (must not move).
 */
const ON = process.env.SR_P90_INVOICES_ON === '1';
const BASE = process.env.SR_P90_INVOICES_BASE === '1';
const OUT =
  process.env.SR_P90_INVOICES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-invoices';
const WANT = (process.env.SR_P90_INVOICES_SETTINGS ?? 'desk,phone-light,phone-dark').split(',');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SETTINGS = [
  { key: 'phone-light', phone: true, theme: 'lounge-light' },
  { key: 'phone-dark', phone: true, theme: 'lounge-dark' },
  { key: 'desk', phone: false, theme: 'lounge-editorial' },
].filter((s) => WANT.includes(s.key));
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(300_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Item {
  kind: string;
  label: string;
  labelBox: Box | null;
  textTop: number | null;
  textBottom: number | null;
  controlBox: Box | null;
  hit: boolean | null;
}
interface Measured {
  vw: number;
  theme: string | null;
  doc: number;
  form: Box | null;
  items: Item[];
  boxes: { tag: string; text: string; box: Box }[];
}

function measureInPage(): Measured {
  const de = document.documentElement;
  const f = (n: number) => Math.round(n * 10) / 10;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return { x: f(r.left), y: f(r.top), w: f(r.width), h: f(r.height) };
  };
  const form = document.querySelector<HTMLElement>('.ss-root .ss-toolbar');
  const items: Item[] = [];
  const textLine = (lab: Element) => {
    const w = document.createTreeWalker(lab, NodeFilter.SHOW_TEXT);
    let n: Node | null;
    while ((n = w.nextNode())) {
      if ((n.textContent ?? '').trim()) {
        const r = document.createRange();
        r.selectNodeContents(n);
        const b = r.getBoundingClientRect();
        return { top: f(b.top), bottom: f(b.bottom) };
      }
    }
    return { top: null, bottom: null };
  };
  const touch = de.clientWidth <= 720;
  if (form) {
    for (const c of Array.from(form.children)) {
      const ctl = c.matches('input,select,button')
        ? c
        : c.querySelector('select, input:not([type=checkbox]), input[type=checkbox], button');
      const lab = c.tagName === 'LABEL' ? c : c.querySelector('label');
      const l = lab ? textLine(lab) : { top: null, bottom: null };
      let hit: boolean | null = null;
      if (ctl && touch) {
        const r = ctl.getBoundingClientRect();
        const cx = r.left + r.width / 2;
        const cy = r.top + r.height / 2;
        const own = ctl.closest('label');
        const ok = (y: number) => {
          const h = document.elementFromPoint(cx, y);
          return !!h && (h === ctl || ctl.contains(h) || (own?.contains(h) ?? false));
        };
        hit = ok(cy - 21) && ok(cy + 21);
      }
      const isCheck = !!ctl && (ctl as HTMLInputElement).type === 'checkbox';
      items.push({
        kind: ctl ? `${ctl.tagName.toLowerCase()}${isCheck ? ':checkbox' : ''}` : c.tagName,
        label: (lab?.textContent ?? c.textContent ?? '').trim().slice(0, 30),
        labelBox: lab ? rect(lab) : null,
        textTop: l.top,
        textBottom: l.bottom,
        controlBox: ctl ? rect(ctl) : null,
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
    items,
    boxes: form
      ? Array.from(form.querySelectorAll<HTMLElement>('label, input, select, button')).map((e) => ({
          tag: e.tagName.toLowerCase(),
          text: (e.textContent ?? '').trim().slice(0, 24),
          box: rect(e),
        }))
      : [],
  };
}

test('SEC-AUD-390: the Invoices filter labels share a line', async ({ browser }) => {
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
      viewport: s.phone ? { width: 375, height: 812 } : { width: 1280, height: 800 },
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
    const measure = async (id: string) => {
      const m = await page.evaluate(measureInPage);
      appendFileSync(jsonl, `${JSON.stringify({ id, setting: s.key, ...m })}\n`);
      console.log(`REPORT ${s.key} ${m.theme} ${id}: doc ${m.doc} form ${JSON.stringify(m.form)}`);
      for (const i of m.items) console.log(`REPORT   ${s.key} ${id} ${JSON.stringify(i)}`);
      return m;
    };

    // ---- Invoices
    await goTo(/^Invoices$/);
    if (BASE)
      await page.evaluate(() =>
        document.querySelector('form.ss-toolbar')?.classList.remove('inv-toolbar'),
      );
    await page.waitForTimeout(300);
    const inv = await measure('invoices');
    const form = page.locator('form.ss-toolbar').first();
    await form.scrollIntoViewIfNeeded();
    const file = `${pre}invoices-toolbar-${s.key}-${await themeOf()}.png`;
    await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
    shots.push(file);

    if (!BASE) {
      const sel = inv.items.filter(
        (i) => i.controlBox && i.kind !== 'button' && i.kind !== 'input:checkbox',
      );
      if (!s.phone) {
        const span = (a: number[]) => Math.max(...a) - Math.min(...a);
        const lt = span(sel.map((i) => i.textTop ?? NaN));
        const ct = span(sel.map((i) => i.controlBox!.y));
        if (!(lt <= 1)) problems.push(`${s.key}: label tops differ by ${lt}`);
        if (!(ct <= 1)) problems.push(`${s.key}: control tops differ by ${ct}`);
      } else {
        for (const i of sel) {
          const lb = i.textBottom ?? 0;
          if (Math.abs(i.controlBox!.w - (inv.form?.w ?? 0)) > 1)
            problems.push(
              `${s.key}: "${i.label}" control ${i.controlBox!.w} wide, form ${inv.form?.w}`,
            );
          if (i.controlBox!.h < 43.5)
            problems.push(`${s.key}: "${i.label}" control ${i.controlBox!.h} high`);
          if (i.hit === false) problems.push(`${s.key}: "${i.label}" fails the hit test`);
          if (i.controlBox!.y - lb < 0 || i.controlBox!.y - lb > 16)
            problems.push(
              `${s.key}: "${i.label}" label ends ${lb}, control starts ${i.controlBox!.y}`,
            );
        }
      }
      if (inv.doc > 0) problems.push(`${s.key}: page scrolls sideways ${inv.doc}`);
    }

    // The filter still works: choose a State, the request goes out with it.
    const stateSel = form.locator('select').first();
    const opts = await stateSel
      .locator('option')
      .evaluateAll((os) => os.map((x) => (x as HTMLOptionElement).value));
    const pick = opts.find((v) => v);
    if (pick) {
      const req = page.waitForRequest(
        (r) => /\/invoices/.test(r.url()) && r.url().includes(`state=${pick}`),
        { timeout: 6000 },
      );
      await stateSel.selectOption(pick);
      const got = await req.then(
        () => true,
        () => false,
      );
      await idle(page, 2500);
      const empty = await page.getByText(/no invoices|nothing/i).count();
      const rows = await page.locator('.adm-table tbody tr').count();
      console.log(
        `REPORT ${s.key} filter state=${pick}: request sent ${got}, rows ${rows}, empty-state text ${empty}`,
      );
      if (!got) problems.push(`${s.key}: choosing State ${pick} sent no request with it`);
    }

    // ---- Entitlements' toolbar (the reference: it must not move; no change touches it)
    await goTo(/^Entitlements$/);
    await measure('entitlements');
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
