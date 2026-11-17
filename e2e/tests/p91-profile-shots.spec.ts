import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 91, profile builder: SEC-AUD-393 (D-910): Profile's page-block selects ("Your page" to "Status rings") and the
 * D-856 hit test at 375 with touch. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/owner.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account. Local only.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8693, Vite 5693):
 *   SR_E2E_BASE_URL=http://localhost:5693 SR_P91_PROFILE_ON=1 \
 *   SR_P91_PROFILE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-profile \
 *   npx playwright test tests/p91-profile-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (SR_P91_PROFILE_BASE=1 names the shots and the json "base-" instead of "after-": the run before the fix.)
 *
 * What it does: Settings > Profile at 375x812 with touch in Lounge Light and Lounge Dark and at 1280x800 in Editorial
 * (the account's main theme by PATCH and localStorage, read back), "Advanced" opened. For every kit select of the
 * page: scrolled to the middle of the viewport, the control's and trigger's boxes, the field's, label's and hint's
 * boxes, `elementsFromPoint` (tag, classes, position, z-index, pointer-events, box) at the centre, 21 px above and
 * 21 px below, and the D-856 verdict; then the card's height, the sticky or fixed nodes and the sideways scroll.
 * At 375 Lounge Light one select is opened (shot), another value chosen and put back. Shots and the probe json go to
 * SR_P91_PROFILE_OUT.
 */
const ON = process.env.SR_P91_PROFILE_ON === '1';
const BASE = process.env.SR_P91_PROFILE_BASE === '1';
const OUT =
  process.env.SR_P91_PROFILE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-profile';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
const CASES = [
  { size: 'phone', theme: 'lounge-light', tag: '375-lounge-light' },
  { size: 'phone', theme: 'lounge-dark', tag: '375-lounge-dark' },
  { size: 'desktop', theme: 'lounge-editorial', tag: '1280-editorial' },
] as const;
const PREFIX = BASE ? 'base-' : 'after-';

test.skip(!ON, 'local visual check only');
test.setTimeout(300_000);

const result: Record<string, unknown> = {};
const save = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, `${PREFIX}probe.json`), JSON.stringify(result, null, 1));
};

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme read back: ${back.settings?.user_prefs?.theme}`);
}

const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  await page.screenshot({ path: resolve(OUT, `${PREFIX}${name}.png`), timeout: 10_000 });
};

function probe() {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [r1(r.left), r1(r.top), r1(r.width), r1(r.height)];
  };
  const desc = (e: Element) => {
    const cs = getComputedStyle(e);
    const c = typeof e.className === 'string' ? e.className.trim() : '';
    return {
      el: `${e.tagName.toLowerCase()}${c ? `.${c.split(/\s+/).join('.')}` : ''}${e.id ? `#${e.id}` : ''}`,
      position: cs.position,
      z: cs.zIndex,
      pe: cs.pointerEvents,
      box: box(e),
    };
  };
  const owns = (el: Element, t: Element | null) =>
    !!t &&
    (t === el ||
      el.contains(t) ||
      (!!el.closest('label') && t.closest('label') === el.closest('label')));
  const at = (el: Element, dy: number) => {
    const b = el.getBoundingClientRect();
    const x = b.left + b.width / 2;
    const y = b.top + b.height / 2 + dy;
    const stack = document.elementsFromPoint(x, y);
    return {
      y: r1(y),
      ok: owns(el, stack[0] ?? null),
      top: stack[0] ? desc(stack[0]) : null,
      stack: stack.slice(0, 6).map(desc),
    };
  };
  const details = document.querySelector('details.us-advanced') as HTMLDetailsElement | null;
  if (details) details.open = true;
  const triggers = Array.from(
    document.querySelectorAll<HTMLElement>('div[class*="control"] button[class*="trigger"]'),
  ).filter((t) => t.getBoundingClientRect().width > 0);
  const items = triggers.map((trig) => {
    const control = trig.closest('div[class*="control"]') as HTMLElement;
    const field = control.closest('.us-field') as HTMLElement | null;
    const labelId = trig.getAttribute('aria-labelledby');
    const label = labelId ? document.getElementById(labelId) : null;
    control.scrollIntoView({ block: 'center' });
    const hint = field?.querySelector('p') ?? null;
    const nextField = field?.nextElementSibling ?? null;
    const cb = control.getBoundingClientRect();
    return {
      name: (label?.textContent ?? trig.getAttribute('aria-label') ?? trig.textContent ?? '')
        .trim()
        .slice(0, 30),
      inAdvanced: !!trig.closest('.us-advanced'),
      control: box(control),
      trigger: box(trig),
      controlCs: { minH: getComputedStyle(control).minHeight, h: getComputedStyle(control).height },
      field: field ? box(field) : null,
      fieldCs: field
        ? { pad: getComputedStyle(field).padding, gap: getComputedStyle(field).gap }
        : null,
      label: label ? box(label) : null,
      hint: hint ? box(hint) : null,
      nextField: nextField ? { tag: nextField.tagName.toLowerCase(), box: box(nextField) } : null,
      gapToNext: nextField
        ? r1(nextField.getBoundingClientRect().top - (cb.top + cb.height))
        : null,
      triggerHit: { up: at(trig, -21), centre: at(trig, 0), down: at(trig, 21) },
      controlHit: { up: at(control, -21), centre: at(control, 0), down: at(control, 21) },
    };
  });
  const card = document.getElementById('us-profile-privacy');
  return {
    theme: document.documentElement.getAttribute('data-theme'),
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
    card: card ? box(card) : null,
    advanced: details ? box(details) : null,
    stickyOrFixed: Array.from(document.querySelectorAll<HTMLElement>('body *'))
      .filter((e) => ['sticky', 'fixed'].includes(getComputedStyle(e).position))
      .filter((e) => e.getBoundingClientRect().width > 0)
      .map(desc),
    items,
  };
}

/** The wave 90 way: "Advanced" still closed. Every select of the card is scrolled to the middle and hit-tested. */
function closedProbe() {
  const details = document.querySelector('details.us-advanced') as HTMLDetailsElement | null;
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const out = Array.from(
    document.querySelectorAll<HTMLElement>('div[class*="control"] button[class*="trigger"]'),
  ).map((trig) => {
    const control = trig.closest('div[class*="control"]') as HTMLElement;
    const before = control.getBoundingClientRect();
    control.scrollIntoView({ block: 'center' });
    const b = control.getBoundingClientRect();
    const t = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2 + 21);
    return {
      inAdvanced: !!trig.closest('.us-advanced'),
      boxBefore: [before.left, before.top, before.width, before.height].map(r1),
      boxAfter: [b.left, b.top, b.width, b.height].map(r1),
      inViewport: b.top >= 0 && b.bottom <= window.innerHeight,
      downReturns: t ? `${t.tagName.toLowerCase()}.${String(t.className)}` : null,
    };
  });
  return { open: details?.open ?? null, out };
}

const TRIGGERS = 'details.us-advanced button[class*="trigger"]';

async function reopen(page: Page) {
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('details.us-advanced');
  await page.locator('details.us-advanced > summary').click();
  await page.waitForTimeout(800);
}

test('p91 profile: the page-block selects and the hit test', async ({ browser }) => {
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  for (const c of CASES) {
    await setMain(owner, c.theme);
    const phone = c.size === 'phone';
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: SIZES[c.size],
      storageState: await owner.storageState(),
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
    }, c.theme);
    const page = await ctx.newPage();
    page.setDefaultTimeout(8000);
    await page.goto('/settings/user/profile', { waitUntil: 'domcontentloaded', timeout: 20_000 });
    await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
    await page.waitForSelector('details.us-advanced', { timeout: 15_000 });
    await page.waitForTimeout(1500);
    const closed = await page.evaluate(closedProbe);
    result[`${c.tag}-closed`] = closed;
    console.log(`REPORT ${c.tag} closed ${JSON.stringify(closed)}`.slice(0, 1200));
    if (phone && c.theme === 'lounge-light') {
      await page.evaluate(() => {
        const d = document.querySelector('details.us-advanced') as HTMLElement | null;
        if (d) d.style.outline = '3px solid #e11d48';
        d?.scrollIntoView({ block: 'center' });
      });
      await page.waitForTimeout(300);
      await shot(page, 'closed-outlined-375-lounge-light');
      await page.evaluate(() => {
        const d = document.querySelector('details.us-advanced') as HTMLElement | null;
        if (d) d.style.outline = '';
      });
    }
    const m = await page.evaluate(probe);
    result[c.tag] = m;
    save();
    console.log(
      `REPORT ${c.tag} theme=${m.theme} scroll=${m.scroll} card=${m.card} items=${m.items.length}`,
    );
    for (const it of m.items)
      console.log(
        `REPORT   ${it.name} | ctl ${it.control.join(',')} | trig ${it.trigger.join(',')} | up ${it.triggerHit.up.ok}(${it.triggerHit.up.top?.el}) down ${it.triggerHit.down.ok}(${it.triggerHit.down.top?.el}) | ctlUp ${it.controlHit.up.ok} ctlDown ${it.controlHit.down.ok} | gapNext ${it.gapToNext}`,
      );
    await page.evaluate((sel) => {
      document.querySelector(sel)?.scrollIntoView({ block: 'center' });
    }, TRIGGERS);
    await page.waitForTimeout(300);
    await shot(page, `advanced-${c.tag}`);
    if (phone && c.theme === 'lounge-light') {
      // One select opened, another value chosen, put back.
      const sel = page.locator(TRIGGERS).nth(2);
      await sel.scrollIntoViewIfNeeded();
      const before = ((await sel.textContent()) ?? '').trim();
      await sel.click();
      await page.waitForTimeout(400);
      await shot(page, 'open-375-lounge-light');
      const opts = page.getByRole('option');
      const n = await opts.count();
      let chosen = '';
      for (let i = 0; i < n; i++) {
        const o = opts.nth(i);
        const txt = ((await o.textContent()) ?? '').replace('✓', '').trim();
        if (txt !== before && (await o.getAttribute('aria-disabled')) !== 'true') {
          chosen = txt;
          await o.click();
          break;
        }
      }
      await page.waitForTimeout(1200);
      const mid = ((await sel.textContent()) ?? '').trim();
      await reopen(page);
      const afterReload = ((await page.locator(TRIGGERS).nth(2).textContent()) ?? '').trim();
      await page.locator(TRIGGERS).nth(2).click();
      await page.getByRole('option', { name: before, exact: true }).click();
      await page.waitForTimeout(1200);
      await reopen(page);
      const restored = ((await page.locator(TRIGGERS).nth(2).textContent()) ?? '').trim();
      result.choose = { before, chosen, mid, afterReload, restored };
      save();
      console.log(`REPORT choose ${JSON.stringify(result.choose)}`);
    }
    await ctx.close();
  }
  await owner.close();
  save();
});
