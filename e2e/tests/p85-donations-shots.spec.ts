import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-372 (wave 85; D-875). Admin > Pricing and perks and Admin > Donations: the per-tier perk fields and the two
 * pickers on the kit's look. Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited); the way into Admin is p84-admin-shots.
 *
 * Settings: 375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial (account theme by PATCH and
 * localStorage, `data-theme` read back and written into the file name). Per page and setting: one JSON line in
 * `measure.json` (bare controls outside a toolbar with box, ground, border and hit test; the kit probe's computed
 * values; the perk table's and the page's sideways scroll; what is cut), and the shots.
 *
 * Local only (SR_P85_DON=1); SR_P85_BASE=1 is the base case (shots named `before-`, nothing asserted);
 * SR_P85_OUT the shot folder; SR_P85_SETTINGS a comma list of `phone-light`, `phone-dark`, `desk`.
 */
const ON = process.env.SR_P85_DON === '1';
const BASE = process.env.SR_P85_BASE === '1';
const OUT =
  process.env.SR_P85_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p85-donations';
const WANT = (process.env.SR_P85_SETTINGS ?? 'phone-light,phone-dark,desk').split(',');
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
test.setTimeout(900_000);

interface Measured {
  vw: number;
  theme: string | null;
  doc: number;
  bare: {
    tag: string;
    cls: string;
    name: string;
    box: number[];
    bg: string;
    border: string;
    hit: boolean;
  }[];
  bareCount: number;
  otherControls: string[];
  failKit: { tag: string; text: string; h: number }[];
  failOther: number;
  table: { sw: number; cw: number; wrapOver: number; cut: string[]; display: string } | null;
  probe: Record<string, string> | null;
  perk: Record<string, string> | null;
  perkFocus: { probe: string; perk: string } | null;
}

function measureInPage(phone: boolean): Measured {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const themed =
    document.querySelector('.ss-root[data-theme]') ?? document.querySelector('[data-theme]');
  const isKit = (e: Element) =>
    typeof e.className === 'string' &&
    e.className.split(/\s+/).some((c) => /^_/.test(c) || /module/.test(c));
  const hit = (e: HTMLElement) => {
    e.scrollIntoView({ block: 'center', inline: 'nearest' });
    const r = e.getBoundingClientRect();
    const cx = r.left + r.width / 2;
    const cy = r.top + r.height / 2;
    const labels = Array.from((e as HTMLInputElement).labels ?? []);
    const own = e.closest('label');
    const ok = (y: number) => {
      const h = document.elementFromPoint(cx, y);
      return (
        !!h &&
        (h === e ||
          e.contains(h) ||
          labels.some((l) => l === h || l.contains(h)) ||
          (own?.contains(h) ?? false))
      );
    };
    return ok(cy - 21) && ok(cy + 21);
  };
  const vis = (e: Element) => {
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(e).visibility !== 'hidden';
  };
  const all = Array.from(document.querySelectorAll<HTMLElement>('input, select, textarea')).filter(
    (e) => vis(e) && !(e instanceof HTMLInputElement && e.type === 'hidden'),
  );
  const bare: Measured['bare'] = [];
  const otherControls: string[] = [];
  for (const e of all) {
    if (isKit(e)) continue;
    const type = e instanceof HTMLInputElement ? e.type : e.tagName.toLowerCase();
    if (type === 'checkbox' || type === 'file' || type === 'radio') {
      otherControls.push(
        `${type}:${e.getAttribute('aria-label') ?? e.closest('label')?.textContent?.trim() ?? ''}`,
      );
      continue;
    }
    if (e.closest('.ss-toolbar')) continue;
    const cs = getComputedStyle(e);
    const r = e.getBoundingClientRect();
    bare.push({
      tag: e.tagName.toLowerCase(),
      cls: typeof e.className === 'string' ? e.className : '',
      name: (e.getAttribute('aria-label') ?? '').slice(0, 40),
      box: [
        Math.round(r.left),
        Math.round(r.top),
        Math.round(r.width),
        Math.round(r.height * 10) / 10,
      ],
      bg: cs.backgroundColor,
      border: `${cs.borderTopWidth} ${cs.borderTopStyle} ${cs.borderTopColor}`,
      hit: phone ? hit(e) : true,
    });
  }
  // Kit controls and bare ones that fail the hit test (a phone only).
  const failKit: Measured['failKit'] = [];
  let failOther = 0;
  if (phone) {
    for (const e of Array.from(
      document.querySelectorAll<HTMLElement>(
        'button, a[href], input, select, textarea, [role=combobox]',
      ),
    )) {
      if (!vis(e) || (e instanceof HTMLInputElement && e.type === 'hidden')) continue;
      const r0 = e.getBoundingClientRect();
      if (r0.right <= 0 || r0.left >= vw || r0.height >= 43.5) continue;
      if (e.closest('.ss-sr, .us-sr') || e.closest('nav')) continue;
      if (hit(e)) continue;
      if (isKit(e))
        failKit.push({
          tag: e.tagName.toLowerCase(),
          text: (e.textContent ?? '').trim().slice(0, 30),
          h: Math.round(r0.height * 10) / 10,
        });
      else failOther++;
    }
  }
  const pick = (e: Element) => {
    const cs = getComputedStyle(e);
    return {
      bg: cs.backgroundColor,
      border: `${cs.borderTopWidth} ${cs.borderTopStyle} ${cs.borderTopColor}`,
      radius: cs.borderTopLeftRadius,
      color: cs.color,
      padInline: `${cs.paddingLeft}/${cs.paddingRight}`,
      font: `${cs.fontSize} ${cs.fontFamily.slice(0, 20)}`,
      height: String(Math.round(e.getBoundingClientRect().height * 10) / 10),
    };
  };
  const probeEl = document.querySelector<HTMLInputElement>(
    'input[class*="_control_"]:not([disabled])',
  );
  const perkEl = document.querySelector<HTMLInputElement>(
    '.adm-perk-table input, table input[inputmode=numeric]',
  );
  const focusOutline = (e: HTMLElement | null) => {
    if (!e) return '';
    e.focus();
    const cs = getComputedStyle(e);
    return `${cs.outlineWidth} ${cs.outlineStyle} ${cs.outlineColor} off ${cs.outlineOffset}`;
  };
  const perkFocus =
    probeEl && perkEl ? { probe: focusOutline(probeEl), perk: focusOutline(perkEl) } : null;
  (document.activeElement as HTMLElement | null)?.blur();
  const tbl =
    document.querySelector<HTMLElement>('.adm-perk-table') ??
    perkEl?.closest<HTMLElement>('table') ??
    null;
  let table: Measured['table'] = null;
  if (tbl) {
    const wrap = tbl.closest<HTMLElement>('.adm-table-wrap');
    const cut: string[] = [];
    for (const c of Array.from(tbl.querySelectorAll<HTMLElement>('input, td, th'))) {
      const r = c.getBoundingClientRect();
      if (r.width === 0) continue;
      if (r.left < -0.5 || r.right > vw + 0.5)
        cut.push(
          `${c.tagName.toLowerCase()}:${(c.getAttribute('aria-label') ?? c.textContent ?? '').slice(0, 24)}@${Math.round(r.left)}..${Math.round(r.right)}`,
        );
    }
    table = {
      sw: tbl.scrollWidth,
      cw: tbl.clientWidth,
      wrapOver: wrap ? wrap.scrollWidth - wrap.clientWidth : 0,
      cut: cut.slice(0, 12),
      display: getComputedStyle(tbl).display,
    };
  }
  return {
    vw,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - de.clientWidth,
    bare,
    bareCount: bare.length,
    otherControls,
    failKit,
    failOther,
    table,
    probe: probeEl ? pick(probeEl) : null,
    perk: perkEl ? pick(perkEl) : null,
    perkFocus,
  };
}

test('SEC-AUD-372: Pricing and perks, Donations: fields and pickers on the kit look', async ({
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
    const measure = async (id: string) => {
      const m = await page.evaluate(measureInPage, s.phone);
      await page.evaluate(() =>
        (document.querySelector('main, [role=region]') ?? document.body).scrollTo?.(0, 0),
      );
      appendFileSync(jsonl, `${JSON.stringify({ id, setting: s.key, ...m })}\n`);
      console.log(
        `REPORT ${s.key} ${m.theme} ${id}: bare ${m.bareCount} (hit fails ${m.bare.filter((b) => !b.hit).length}) failKit ${JSON.stringify(m.failKit)} failOther ${m.failOther} doc ${m.doc} table ${JSON.stringify(m.table)} others ${m.otherControls.length}`,
      );
      return m;
    };
    const scrollTo = async (sel: string) => {
      await page
        .locator(sel)
        .first()
        .evaluate((el) => el.scrollIntoView({ block: 'start' }));
      await page.waitForTimeout(300);
    };

    // ---- Pricing and perks
    await goTo(/^Pricing and perks$/);
    const top = await page.locator('h2#don-prices-h').count();
    if (top === 0) problems.push(`${s.key}: Pricing page did not open`);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shoot('pricing-top');
    await scrollTo('h3');
    const m = await measure('pricing');
    await scrollTo('.adm-table-wrap');
    await shoot('pricing-table');
    if (!BASE) {
      await scrollTo('#don-mod-h');
      await shoot('pricing-calc');
    }
    if (!BASE) {
      if (m.bareCount !== 0) problems.push(`${s.key}: ${m.bareCount} bare controls on Pricing`);
      if (m.bare.some((b) => !b.hit)) problems.push(`${s.key}: a bare control fails the hit test`);
      if (m.probe && m.perk) {
        for (const k of ['bg', 'border', 'radius', 'color', 'padInline'] as const) {
          if (m.probe[k] !== m.perk[k])
            problems.push(`${s.key}: perk ${k} ${m.perk[k]} != kit ${m.probe[k]}`);
        }
      }
      if (
        s.phone &&
        m.table &&
        (m.table.sw > m.table.cw || m.table.wrapOver > 0 || m.table.cut.length)
      )
        problems.push(`${s.key}: table scrolls or is cut ${JSON.stringify(m.table)}`);
      if (s.phone && m.doc > 0) problems.push(`${s.key}: page scrolls sideways ${m.doc}`);
      const one = page.getByRole('textbox', { name: 'Level 2 Upload MB' });
      if ((await one.count()) !== 1)
        problems.push(`${s.key}: textbox "Level 2 Upload MB" count ${await one.count()}`);
    }

    // ---- Click-through and the open pickers (phone, Lounge Light, once)
    if (!BASE && s.key === 'phone-light') {
      await scrollTo('.adm-table-wrap');
      const f = page.getByRole('textbox', { name: 'Level 2 Upload MB' });
      const before = await f.inputValue();
      const save = page.getByRole('button', { name: 'Save changes' });
      const reset = page.getByRole('button', { name: 'Reset' });
      console.log(
        `REPORT click: before buttons enabled save=${await save.isEnabled()} reset=${await reset.isEnabled()}`,
      );
      await f.fill('777');
      console.log(
        `REPORT click: after change save=${await save.isEnabled()} reset=${await reset.isEnabled()}`,
      );
      if (!(await save.isEnabled()) || !(await reset.isEnabled()))
        problems.push('click: buttons did not enable');
      await shoot('click-changed');
      await reset.click();
      const back2 = await f.inputValue();
      console.log(`REPORT click: reset value ${before} -> ${back2}`);
      if (back2 !== before) problems.push(`click: Reset left ${back2}, expected ${before}`);
      await f.fill('778');
      await save.click();
      const dlg = page.getByRole('dialog', { name: 'Save donation settings' });
      const opened = await dlg.isVisible().catch(() => false);
      console.log(`REPORT click: reason dialog open ${opened}`);
      if (!opened) problems.push('click: reason dialog did not open');
      await shoot('click-dialog');
      await page.keyboard.press('Escape');
      await page.waitForTimeout(300);
      if (await dlg.isVisible().catch(() => false))
        await dlg.getByRole('button', { name: /cancel/i }).click();
      // The Who picker's list.
      await scrollTo('#don-mod-h');
      await page.getByRole('combobox', { name: 'Who' }).click();
      const lb = page.getByRole('listbox');
      await lb
        .first()
        .waitFor({ timeout: 4000 })
        .catch(() => undefined);
      const lr = await lb.first().boundingBox();
      const vh = page.viewportSize()?.height ?? 0;
      const inside =
        !!lr && lr.x >= 0 && lr.y >= 0 && lr.x + lr.width <= 375 && lr.y + lr.height <= vh;
      console.log(`REPORT who list box ${JSON.stringify(lr)} inside ${inside}`);
      if (!inside) problems.push(`who list outside the viewport ${JSON.stringify(lr)}`);
      await shoot('who-open');
      await page.keyboard.press('Escape');
    }

    // ---- Donations overview and its wallet form
    await goTo(/^Donations$/);
    const wallet = await page.locator('h2#don-wallet-h').count();
    if (wallet === 0) problems.push(`${s.key}: Donations overview has no wallet form`);
    await scrollTo('#don-wallet-h');
    const d = await measure('donations');
    await shoot('donations-wallet');
    if (!BASE) {
      if (d.bareCount !== 0) problems.push(`${s.key}: ${d.bareCount} bare controls on Donations`);
      if (s.phone && d.doc > 0) problems.push(`${s.key}: Donations scrolls sideways ${d.doc}`);
      if (s.key === 'phone-light') {
        await page.getByRole('combobox', { name: 'Network' }).click();
        const lb = page.getByRole('listbox');
        await lb
          .first()
          .waitFor({ timeout: 4000 })
          .catch(() => undefined);
        const lr = await lb.first().boundingBox();
        const vh = page.viewportSize()?.height ?? 0;
        const inside =
          !!lr && lr.x >= 0 && lr.y >= 0 && lr.x + lr.width <= 375 && lr.y + lr.height <= vh;
        console.log(`REPORT network list box ${JSON.stringify(lr)} inside ${inside}`);
        if (!inside) problems.push(`network list outside the viewport ${JSON.stringify(lr)}`);
        await shoot('network-open');
        await page.keyboard.press('Escape');
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
