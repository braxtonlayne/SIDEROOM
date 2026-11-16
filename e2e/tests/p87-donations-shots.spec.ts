import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-378 (wave 87; D-888). Admin > Entitlements, the grant dialog on a phone: "No end date" is a 44 px row and
 * its label is the target. Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited); the way into Admin is p86-donations-shots.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database):
 *   SR_P87_DONATIONS_ON=1 SR_E2E_BASE_URL=http://localhost:5653 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P87_DONATIONS_OUT=<folder> npx playwright test tests/p87-donations-shots.spec.ts \
 *   --project=chromium --workers=1 --retries=0 --reporter=line
 * (add SR_P87_DONATIONS_BASE=1 for the base case: shots named `before-`, nothing asserted;
 *  SR_P87_DONATIONS_SETTINGS = comma list of `phone-light`, `phone-dark`, `desk`).
 * Settings: 375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial (account theme by PATCH and
 * localStorage, `data-theme` read back into the file name). One JSON line per setting and state in `measure*.json`
 * (the dialog's controls' boxes, so base and after can be compared; Pricing's "Donations are on" row once).
 */
const ON = process.env.SR_P87_DONATIONS_ON === '1';
const BASE = process.env.SR_P87_DONATIONS_BASE === '1';
const OUT =
  process.env.SR_P87_DONATIONS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-donations';
const WANT = (process.env.SR_P87_DONATIONS_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
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
test.setTimeout(420_000);

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  vw: number;
  vh: number;
  theme: string | null;
  doc: number;
  dialog: Box | null;
  dialogScroll: { client: number; scroll: number } | null;
  check: { box: Box; labelBox: Box | null; labelClass: string | null; hit: boolean } | null;
  controls: { tag: string; text: string; box: Box }[];
  daysField: boolean;
}

function measureInPage(): Measured {
  const de = document.documentElement;
  const rect = (e: Element): Box => {
    const r = e.getBoundingClientRect();
    return {
      x: Math.round(r.left * 10) / 10,
      y: Math.round(r.top * 10) / 10,
      w: Math.round(r.width * 10) / 10,
      h: Math.round(r.height * 10) / 10,
    };
  };
  const dlg = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).find((d) =>
    /No end date/.test(d.textContent ?? ''),
  );
  const root: ParentNode = dlg ?? document;
  const cb =
    Array.from(root.querySelectorAll<HTMLInputElement>('input[type=checkbox]')).find((e) =>
      /No end date|Donations are on/.test(e.closest('label')?.textContent ?? ''),
    ) ?? null;
  let check: Measured['check'] = null;
  if (cb) {
    cb.scrollIntoView({ block: 'center', inline: 'nearest' });
    const r = cb.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const own = cb.closest('label');
    const ok = (y: number) => {
      const h = document.elementFromPoint(cx, y);
      return !!h && (h === cb || cb.contains(h) || (own?.contains(h) ?? false));
    };
    check = {
      box: rect(cb),
      labelBox: own ? rect(own) : null,
      labelClass: own?.className ?? null,
      hit: ok(cy - 21) && ok(cy + 21),
    };
  }
  const controls = dlg
    ? Array.from(
        dlg.querySelectorAll<HTMLElement>('label, input, select, textarea, button, h2, h3'),
      ).map((e) => ({
        tag: e.tagName.toLowerCase(),
        text: (e.textContent ?? '').trim().slice(0, 24),
        box: rect(e),
      }))
    : [];
  return {
    vw: de.clientWidth,
    vh: window.innerHeight,
    theme:
      (
        document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]')
      )?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    dialog: dlg ? rect(dlg) : null,
    dialogScroll: dlg ? { client: dlg.clientHeight, scroll: dlg.scrollHeight } : null,
    check,
    controls,
    daysField:
      !!dlg &&
      Array.from(dlg.querySelectorAll('label')).some((l) => /days/i.test(l.textContent ?? '')),
  };
}

test('SEC-AUD-378: "No end date" is a 44 px row on a phone', async ({ browser }) => {
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
    const measure = async (id: string) => {
      const m = await page.evaluate(measureInPage);
      appendFileSync(jsonl, `${JSON.stringify({ id, setting: s.key, ...m })}\n`);
      console.log(
        `REPORT ${s.key} ${m.theme} ${id}: doc ${m.doc} dialog ${JSON.stringify(m.dialog)} scroll ${JSON.stringify(m.dialogScroll)} check ${JSON.stringify(m.check)} days ${m.daysField}`,
      );
      return m;
    };

    // ---- Entitlements, the grant dialog, kind "patron" (the default)
    await goTo(/^Entitlements$/);
    await page.getByRole('button', { name: /grant/i }).first().click();
    await page.waitForTimeout(600);
    const dlg = page
      .locator('[role="dialog"]')
      .filter({ hasText: /No end date/ })
      .first();
    const noEnd = dlg.getByRole('checkbox', { name: /No end date/ });
    if ((await noEnd.count()) === 0) problems.push(`${s.key}: no "No end date" checkbox`);
    const unchecked = await measure('grant-unchecked');
    await page.evaluate(() => {
      const d = Array.from(document.querySelectorAll<HTMLElement>('[role="dialog"]')).find((e) =>
        /No end date/.test(e.textContent ?? ''),
      );
      d?.scrollTo(0, 0);
    });
    await shoot('grant-unchecked');
    if (s.phone) {
      // A touch tap on the label's text toggles the checkbox; the days field goes and comes back.
      const lab = dlg
        .locator('label')
        .filter({ hasText: /No end date/ })
        .first();
      const lb = (await lab.boundingBox())!;
      const tapAt = { x: lb.x + Math.min(lb.width - 8, 90), y: lb.y + lb.height / 2 };
      const was = await noEnd.isChecked();
      await page.touchscreen.tap(tapAt.x, tapAt.y);
      await page.waitForTimeout(300);
      const now = await noEnd.isChecked();
      const checked = await measure('grant-checked');
      await shoot('grant-checked');
      const lb2 = (await lab.boundingBox())!;
      await page.touchscreen.tap(lb2.x + Math.min(lb2.width - 8, 90), lb2.y + lb2.height / 2);
      await page.waitForTimeout(300);
      const again = await noEnd.isChecked();
      console.log(`REPORT ${s.key} tap on label text: ${was} -> ${now} -> ${again}`);
      appendFileSync(jsonl, `${JSON.stringify({ id: 'tap', setting: s.key, was, now, again })}\n`);
      if (!BASE) {
        if (now === was || again !== was)
          problems.push(`${s.key}: tap on label text did not toggle`);
        if (!unchecked.daysField || checked.daysField)
          problems.push(
            `${s.key}: days field unchecked ${unchecked.daysField}, checked ${checked.daysField}`,
          );
        const c = unchecked.check;
        if (!c?.hit) problems.push(`${s.key}: "No end date" fails the hit test`);
        if (c?.labelBox && c.labelBox.h < 43.5)
          problems.push(`${s.key}: label row ${c.labelBox.h} px`);
        if (unchecked.doc > 0) problems.push(`${s.key}: page scrolls sideways ${unchecked.doc}`);
        const d = unchecked.dialog;
        if (d && (d.y < -0.5 || d.y + d.h > unchecked.vh + 0.5))
          problems.push(
            `${s.key}: dialog ${JSON.stringify(d)} outside the viewport (scroll ${JSON.stringify(unchecked.dialogScroll)})`,
          );
      }
    }
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);

    // ---- Pricing's "Donations are on" row, one measurement (unchanged by this wave)
    await goTo(/^Pricing and perks$/);
    const pm = await measure('pricing');
    console.log(`REPORT ${s.key} pricing check ${JSON.stringify(pm.check)}`);
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
