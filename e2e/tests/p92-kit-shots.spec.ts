import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * Wave 92, kit builder: SEC-AUD-401 (D-913): the kit `Select`'s chip rows get a 44 px pitch on a phone. Local only.
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8702, Vite 5702, metrics 9402,
 * `SR_ADMIN_REQUIRE_REAUTH=0`):
 *   SR_E2E_BASE_URL=http://localhost:5702 SR_P92_KIT_ON=1 \
 *   SR_P92_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-kit \
 *   npx playwright test tests/p92-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * `SR_P92_KIT_BASE=1` is the base case: run it with the sheet's change switched off (files named `base-*`; it runs
 * the light phone cases and the 1280 case only). `SR_P92_KIT_ONLY=a,b` picks cases (a nine chips and taps, b one row
 * and none, c the open list, d dark, e 1280 Editorial, f the second multi-select).
 *
 * Channels p92-r1 to p92-r10 are made through the API (D-863). Onboarding "Default channels" (Server settings,
 * Community; never saved) is emptied, then p92-r1 to p92-rN are chosen. Per state: the control's box, every chip's
 * box, the hit test of every remove button (centre, 21 px above and below, with a 1 px walk), the chevron's three
 * points, the text input's box, the top of what lies below the field. The hit test follows the hit-test rule: the
 * control is scrolled into the middle first, and a null is a probe error (thrown), never a fault. The touch taps:
 * 20 px below the centre of the middle button of row 1, and at another button's centre. Shots go to SR_P92_KIT_OUT
 * with `data-theme` read back before each.
 */
const ON = process.env.SR_P92_KIT_ON === '1';
const BASE = process.env.SR_P92_KIT_BASE === '1';
const OUT =
  process.env.SR_P92_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-kit';
const ONLY = (process.env.SR_P92_KIT_ONLY ?? 'a,b,c,d,e,f').split(',').map((s) => s.trim());
const PRE = BASE ? 'base' : 'after';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const DESK = { width: 1280, height: 800 };

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const rows: unknown[] = [];
const log = (id: string, data: unknown) => {
  mkdirSync(OUT, { recursive: true });
  rows.push({ id, ...(data as object) });
  writeFileSync(resolve(OUT, `${PRE}-measure.json`), JSON.stringify(rows, null, 1));
  console.log(`REPORT ${id} ${JSON.stringify(data).slice(0, 700)}`);
};

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(`REPORT account theme read back: ${back.settings?.user_prefs?.theme}`);
  if (back.settings?.user_prefs?.theme !== theme) throw new Error('account theme not set');
}

function installHelpers() {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [r1(r.left), r1(r.top), r1(r.width), r1(r.height)];
  };
  const owns = (el: Element, t: Element | null) => !!t && (t === el || el.contains(t));
  /** A null or an element outside the control is reported as `null`/`false`; the caller throws on null (probe error). */
  const hit = (el: Element, dy: number) => {
    const b = el.getBoundingClientRect();
    if (b.width === 0 || b.height === 0) return null;
    const t = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2 + dy);
    if (!t) return null;
    return owns(el, t);
  };
  const center = (e: Element) => (e as HTMLElement).scrollIntoView({ block: 'center' });
  (window as unknown as { __m: unknown }).__m = { r1, box, owns, hit, center };
}

type M = {
  r1: (n: number) => number;
  box: (e: Element) => number[];
  owns: (el: Element, t: Element | null) => boolean;
  hit: (el: Element, dy: number) => boolean | null;
  center: (e: Element) => void;
};

async function newPage(browser: Browser, ownerCtx: BrowserContext, theme: string, phone: boolean) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? PHONE : DESK,
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
  }, theme);
  await ctx.addInitScript(installHelpers);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

const idle = async (page: Page, ms = 2500) => {
  await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
  await page.waitForTimeout(ms);
};
const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  const theme = await page.evaluate(
    () => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
  );
  const sw = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  console.log(`REPORT shot ${name} data-theme ${theme} scroll ${sw}`);
  await page.screenshot({ path: resolve(OUT, `${PRE}-${name}.png`), timeout: 10_000 });
  return { theme, scroll: sw };
};

const FIELD = 'Default channels';

/** Open Onboarding, empty the field, choose p92-r1 .. p92-rN. */
async function prepare(page: Page, n: number) {
  const w = world();
  await page.goto(`/settings/server/${w.guildId}/community`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await idle(page);
  await page.getByRole('tab', { name: 'Onboarding' }).click();
  const field = page.locator(`[aria-label="${FIELD}"]`).first();
  await field.scrollIntoViewIfNeeded();
  for (let k = 0; k < 40; k++) {
    const gone = await page.evaluate((label) => {
      const ctl = document
        .querySelector(`[aria-label="${label}"]`)
        ?.closest('div[class*="control"]');
      const b = ctl?.querySelector<HTMLElement>('button[aria-label^="Remove "]');
      if (!b) return false;
      b.click();
      return true;
    }, FIELD);
    if (!gone) break;
    await page.waitForTimeout(120);
  }
  for (let i = 1; i <= n; i++) {
    if ((await page.getByRole('listbox').count()) === 0) await field.click();
    await page
      .locator('[role="option"]', { hasText: new RegExp(`p92-r${i}$`) })
      .first()
      .click();
    await page.waitForTimeout(150);
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);
  return field;
}

/** The probe of one control: boxes, hit tests (an unreached point throws), the walks, the input, what lies below. */
function probe(label: string) {
  const M = (window as unknown as { __m: M }).__m;
  const input = document.querySelector(`[aria-label="${label}"]`) as HTMLInputElement | null;
  const control = input?.closest('div[class*="control"]') as HTMLElement | null;
  if (!input || !control) throw new Error(`probe error: no control for ${label}`);
  M.center(control);
  const btns = Array.from(control.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'));
  const need = (v: boolean | null, what: string) => {
    if (v === null) throw new Error(`probe error: no box or no element at the point (${what})`);
    return v;
  };
  const chips = btns.map((b, i) => {
    const bb = b.getBoundingClientRect();
    const cx = bb.left + bb.width / 2;
    const cy = bb.top + bb.height / 2;
    const own = (y: number) => {
      const t = document.elementFromPoint(cx, y);
      return !!t && (t === b || b.contains(t));
    };
    let down = 0;
    for (let s = 0; s <= 60 && own(cy + s); s++) down = s;
    let up = 0;
    for (let s = 0; s <= 60 && own(cy - s); s++) up = s;
    return {
      i,
      label: b.getAttribute('aria-label'),
      chip: M.box(b.parentElement!),
      button: M.box(b),
      up: need(M.hit(b, -21), `${i} up`),
      centre: need(M.hit(b, 0), `${i} centre`),
      down: need(M.hit(b, 21), `${i} down`),
      ownRunUp: up,
      ownRunDown: down,
    };
  });
  const tops = Array.from(new Set(chips.map((c) => c.chip[1]!))).sort((p, q) => p - q);
  const chev = control.querySelector('button[class*="chevron"]');
  let up: Element = control;
  while (up.parentElement && !up.nextElementSibling) up = up.parentElement;
  const after = up.nextElementSibling;
  return {
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    control: M.box(control),
    rows: tops.length,
    rowPitch: tops.slice(1).map((tp, k) => M.r1(tp - tops[k]!)),
    chips,
    allPass: chips.every((c) => c.up && c.centre && c.down),
    failing: chips.filter((c) => !(c.up && c.centre && c.down)).map((c) => c.label),
    chevron: chev
      ? {
          box: M.box(chev),
          up: need(M.hit(chev, -21), 'chevron up'),
          centre: need(M.hit(chev, 0), 'chevron centre'),
          down: need(M.hit(chev, 21), 'chevron down'),
        }
      : null,
    input: M.box(input),
    placeholder: input.placeholder,
    belowTop: after ? M.r1(after.getBoundingClientRect().top + window.scrollY) : null,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
  };
}

const labels = (page: Page) =>
  page.evaluate((f) => {
    const ctl = document.querySelector(`[aria-label="${f}"]`)?.closest('div[class*="control"]');
    return Array.from(ctl?.querySelectorAll('button[aria-label^="Remove "]') ?? []).map(
      (b) => b.getAttribute('aria-label') ?? '',
    );
  }, FIELD);

async function tapState(page: Page, id: string) {
  const before = await labels(page);
  const at = await page.evaluate((f) => {
    const M = (window as unknown as { __m: M }).__m;
    const ctl = document.querySelector(`[aria-label="${f}"]`)!.closest('div[class*="control"]')!;
    const b = Array.from(ctl.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]')).find(
      (x) => x.getAttribute('aria-label') === 'Remove #p92-r2',
    )!;
    M.center(b);
    return b.getAttribute('aria-label');
  }, FIELD);
  await page.waitForTimeout(300);
  const pt = await page.evaluate((f) => {
    const ctl = document.querySelector(`[aria-label="${f}"]`)!.closest('div[class*="control"]')!;
    const b = Array.from(ctl.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]')).find(
      (x) => x.getAttribute('aria-label') === 'Remove #p92-r2',
    )!;
    const r = b.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, FIELD);
  await page.touchscreen.tap(pt.x, pt.y + 20);
  await page.waitForTimeout(500);
  const mid = await labels(page);
  log(`${id}-tap-20-below-r2`, {
    tapped: at,
    at: { x: pt.x, y: pt.y + 20 },
    removed: before.filter((l) => !mid.includes(l)),
  });
  // A tap at another button's centre: which chip goes?
  const pt2 = await page.evaluate((f) => {
    const ctl = document.querySelector(`[aria-label="${f}"]`)!.closest('div[class*="control"]')!;
    const b = Array.from(ctl.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]')).find(
      (x) => x.getAttribute('aria-label') === 'Remove #p92-r6',
    )!;
    (b as HTMLElement).scrollIntoView({ block: 'center' });
    const r = b.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, FIELD);
  await page.waitForTimeout(300);
  const pt3 = await page.evaluate((f) => {
    const ctl = document.querySelector(`[aria-label="${f}"]`)!.closest('div[class*="control"]')!;
    const b = Array.from(ctl.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]')).find(
      (x) => x.getAttribute('aria-label') === 'Remove #p92-r6',
    )!;
    const r = b.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, FIELD);
  void pt2;
  await page.touchscreen.tap(pt3.x, pt3.y);
  await page.waitForTimeout(500);
  const end = await labels(page);
  log(`${id}-tap-centre-r6`, { at: pt3, removed: mid.filter((l) => !end.includes(l)) });
}

async function states(
  browser: Browser,
  owner: BrowserContext,
  theme: string,
  phone: boolean,
  tag: string,
  opts: { taps?: boolean; one?: boolean; list?: boolean },
) {
  await setMain(owner, theme);
  let { ctx, page } = await newPage(browser, owner, theme, phone);
  await prepare(page, 9);
  const m9 = await page.evaluate(probe, FIELD);
  log(`nine-${tag}`, m9);
  await shot(page, `nine-${tag}`);
  if (opts.taps) await tapState(page, tag);
  if (opts.one) {
    await prepare(page, 2);
    log(`two-${tag}`, await page.evaluate(probe, FIELD));
    await shot(page, `two-${tag}`);
    await ctx.close();
    ({ ctx, page } = await newPage(browser, owner, theme, phone));
    await prepare(page, 0);
    await shot(page, `none-${tag}`);
    log(`none-${tag}`, await page.evaluate(probe, FIELD));
  }
  if (opts.list) {
    await prepare(page, 9);
    const field = page.locator(`[aria-label="${FIELD}"]`).first();
    await field.click();
    await page.waitForTimeout(500);
    const place = await page.evaluate((f) => {
      const M = (window as unknown as { __m: M }).__m;
      const c = document.querySelector(`[aria-label="${f}"]`)!.closest('div[class*="control"]')!;
      const l = document.querySelector('[role="listbox"]');
      return { control: M.box(c), list: l ? M.box(l) : null };
    }, FIELD);
    log(`list-${tag}`, place);
    await shot(page, `list-open-${tag}`);
    await page
      .locator('[role="option"]', { hasText: /p92-r10$/ })
      .first()
      .click();
    await page.waitForTimeout(300);
    await page.keyboard.press('Escape');
    const withTenth = await labels(page);
    log(`tenth-${tag}`, { has10: withTenth.includes('Remove #p92-r10'), n: withTenth.length });
    await field.click();
    await page.keyboard.press('Backspace');
    await page.waitForTimeout(300);
    const afterBs = await labels(page);
    log(`backspace-${tag}`, { before: withTenth.length, after: afterBs.length });
  }
  await ctx.close();
}

async function secondSelect(browser: Browser, owner: BrowserContext) {
  const theme = 'lounge-light';
  await setMain(owner, theme);
  const { ctx, page } = await newPage(browser, owner, theme, true);
  const w = world();
  await page.goto(`/settings/server/${w.guildId}/community`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await idle(page);
  await page.getByRole('tab', { name: 'Onboarding' }).click();
  await page.getByRole('button', { name: 'Add question' }).first().click();
  await page.getByRole('button', { name: 'Add answer' }).first().click();
  const label = 'Channels for answer 1 of question 1';
  const field = page.locator(`[aria-label="${label}"]`).first();
  await field.scrollIntoViewIfNeeded();
  for (let i = 1; i <= 6; i++) {
    if ((await page.getByRole('listbox').count()) === 0) await field.click();
    await page
      .locator('[role="option"]', { hasText: new RegExp(`p92-r${i}$`) })
      .first()
      .click();
    await page.waitForTimeout(150);
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);
  const m = await page.evaluate(probe, label);
  log('second-multiselect-375-lounge-light', m);
  await shot(page, 'second-multiselect-375-lounge-light');
  await ctx.close();
}

test('p92 kit: the Select chip rows at 375 and 1280', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  for (let i = 1; i <= 10; i++)
    await apiPost(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p92-r${i}`,
      type: 0,
    }).catch(() => undefined);
  const errors: string[] = [];
  const run = async (id: string, fn: () => Promise<void>) => {
    if (!ONLY.includes(id)) return;
    try {
      await fn();
    } catch (e) {
      errors.push(`case ${id}: ${String(e).slice(0, 300)}`);
      console.log(`REPORT case ${id} FAILED ${String(e).slice(0, 300)}`);
    }
  };
  await run('a', () =>
    states(browser, owner, 'lounge-light', true, '375-lounge-light', { taps: true }),
  );
  await run('b', () =>
    states(browser, owner, 'lounge-light', true, '375-lounge-light-rows', { one: true }),
  );
  if (!BASE) {
    await run('c', () =>
      states(browser, owner, 'lounge-light', true, '375-lounge-light-list', { list: true }),
    );
    await run('d', () => states(browser, owner, 'lounge-dark', true, '375-lounge-dark', {}));
  }
  await run('e', () => states(browser, owner, 'lounge-editorial', false, '1280-editorial', {}));
  if (!BASE) await run('f', () => secondSelect(browser, owner));
  await owner.close();
  log('totals', { errors });
  if (errors.length) throw new Error(errors.join('\n'));
});
