import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-376 (wave 86; D-882). Admin > Pricing and perks on a phone: "Donations are on" is a 44 px target and a
 * perk field's text is 16 px. Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited); the way into Admin is p85-donations-shots.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database):
 *   SR_P86_DONATIONS=1 SR_E2E_BASE_URL=http://localhost:5645 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
 *   SR_P86_DONATIONS_OUT=<folder> npx playwright test tests/p86-donations-shots.spec.ts --project=chromium --workers=1 \
 *   --retries=0 --reporter=line
 * (add SR_P86_DONATIONS_BASE=1 for the base case: shots named `before-`, nothing asserted;
 *  SR_P86_DONATIONS_SETTINGS = comma list of `phone-light`, `phone-dark`, `desk`).
 * Settings: 375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial (account theme by PATCH and
 * localStorage, `data-theme` read back into the file name). One JSON line per page and setting in `measure.json`.
 */
const ON = process.env.SR_P86_DONATIONS === '1';
const BASE = process.env.SR_P86_DONATIONS_BASE === '1';
const OUT =
  process.env.SR_P86_DONATIONS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-donations';
const WANT = (process.env.SR_P86_DONATIONS_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
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
test.setTimeout(600_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Check {
  label: string;
  box: Box;
  labelBox: Box | null;
  hit: boolean;
}
interface Measured {
  vw: number;
  theme: string | null;
  doc: number;
  checks: Check[];
  perk: { n: number; fonts: string[]; heights: number[] };
  tableOver: number;
}

function measureInPage(): Measured {
  const de = document.documentElement;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return {
      x: Math.round(r.left),
      y: Math.round(r.top),
      w: Math.round(r.width * 10) / 10,
      h: Math.round(r.height * 10) / 10,
    };
  };
  const hit = (e: HTMLElement) => {
    e.scrollIntoView({ block: 'center', inline: 'nearest' });
    const r = e.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const own = e.closest('label');
    const ok = (y: number) => {
      const h = document.elementFromPoint(cx, y);
      return !!h && (h === e || e.contains(h) || (own?.contains(h) ?? false));
    };
    return ok(cy - 21) && ok(cy + 21);
  };
  const checks: Check[] = Array.from(
    document.querySelectorAll<HTMLInputElement>('input[type=checkbox]'),
  )
    .filter((e) => e.getBoundingClientRect().width > 0)
    .map((e) => {
      const lab = e.closest('label');
      return {
        label: (lab?.textContent ?? e.getAttribute('aria-label') ?? '').trim().slice(0, 40),
        box: rect(e),
        labelBox: lab ? rect(lab) : null,
        hit: hit(e),
      };
    });
  const perks = Array.from(document.querySelectorAll<HTMLInputElement>('.adm-perk-input'));
  const tbl = document.querySelector<HTMLElement>('.adm-perk-table');
  const wrap = tbl?.closest<HTMLElement>('.adm-table-wrap');
  return {
    vw: de.clientWidth,
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    checks,
    perk: {
      n: perks.length,
      fonts: Array.from(new Set(perks.map((p) => getComputedStyle(p).fontSize))),
      heights: Array.from(
        new Set(perks.map((p) => Math.round(p.getBoundingClientRect().height * 10) / 10)),
      ),
    },
    tableOver: Math.max(
      tbl ? tbl.scrollWidth - tbl.clientWidth : 0,
      wrap ? wrap.scrollWidth - wrap.clientWidth : 0,
    ),
  };
}

test('SEC-AUD-376: Donations are on is a 44 px target, perk fields 16 px on a phone', async ({
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
    const shoot = async (name: string) => {
      const f = `${pre}${name}-${s.key}-${await themeOf()}.png`;
      await page.screenshot({ path: resolve(OUT, f), timeout: 10_000 });
      shots.push(f);
    };
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
    const scrollTo = async (sel: string) => {
      await page
        .locator(sel)
        .first()
        .evaluate((el) => el.scrollIntoView({ block: 'start' }));
      await page.waitForTimeout(300);
    };
    const measure = async (id: string) => {
      const m = await page.evaluate(measureInPage);
      appendFileSync(jsonl, `${JSON.stringify({ id, setting: s.key, ...m })}\n`);
      console.log(
        `REPORT ${s.key} ${m.theme} ${id}: doc ${m.doc} tableOver ${m.tableOver} perk ${JSON.stringify(m.perk)} checks ${JSON.stringify(m.checks)}`,
      );
      return m;
    };

    // ---- Pricing and perks
    await goTo(/^Pricing and perks$/);
    if ((await page.locator('h2#don-prices-h').count()) === 0)
      problems.push(`${s.key}: Pricing page did not open`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shoot('pricing-top');
    const on = page.getByRole('checkbox', { name: /Donations are on/ });
    const m = await measure('pricing');
    await scrollTo('.adm-table-wrap');
    await shoot('pricing-tiers');
    const mine = m.checks.find((c) => /Donations are on/.test(c.label));
    if (!mine) problems.push(`${s.key}: no "Donations are on" checkbox`);
    if (s.phone) {
      // A tap on the label's text toggles the checkbox (toggled back; nothing saved).
      await on.scrollIntoViewIfNeeded();
      const lab = page.locator('label.adm-check-row').first();
      const lb = (await lab.boundingBox())!;
      const was = await on.isChecked();
      await page.touchscreen.tap(lb.x + Math.min(lb.width - 8, 60), lb.y + lb.height / 2);
      const now = await on.isChecked();
      await page.touchscreen.tap(lb.x + Math.min(lb.width - 8, 60), lb.y + lb.height / 2);
      const again = await on.isChecked();
      console.log(`REPORT ${s.key} tap on label text: ${was} -> ${now} -> ${again}`);
      appendFileSync(jsonl, `${JSON.stringify({ id: 'tap', setting: s.key, was, now, again })}\n`);
      if (!BASE) {
        if (now === was || again !== was)
          problems.push(`${s.key}: tap on label text did not toggle`);
        if (!mine?.hit) problems.push(`${s.key}: "Donations are on" fails the hit test`);
        if (mine?.labelBox && mine.labelBox.h < 43.5)
          problems.push(`${s.key}: label row ${mine.labelBox.h} px`);
        if (m.perk.fonts.some((f) => f !== '16px'))
          problems.push(`${s.key}: perk fonts ${m.perk.fonts}`);
        if (m.perk.heights.some((h) => h < 43.5))
          problems.push(`${s.key}: perk heights ${m.perk.heights}`);
        if (m.doc > 0 || m.tableOver > 0)
          problems.push(`${s.key}: scrolls sideways doc ${m.doc} tbl ${m.tableOver}`);
      }
    } else {
      console.log(`REPORT desk perk fonts ${m.perk.fonts} heights ${m.perk.heights}`);
    }

    // ---- The other Donations pages' checkboxes (measured, listed, not changed)
    for (const [id, re] of [
      ['entitlements', /^Entitlements$/],
      ['invoices', /^Invoices$/],
    ] as const) {
      await goTo(re);
      await measure(id);
      if (id === 'entitlements') {
        const grant = page.getByRole('button', { name: /grant/i }).first();
        if (await grant.count()) {
          await grant.click().catch(() => undefined);
          await page.waitForTimeout(500);
          await measure('entitlements-dialog');
          await page.keyboard.press('Escape');
        }
      }
    }
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
