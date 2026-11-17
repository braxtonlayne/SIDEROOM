import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * Wave 90, measure builder: SEC-AUD-392 (D-902): the five re-measure items, measured on the merged tree. Nothing is
 * built and nothing is asserted but "the page was reached and the probe returned boxes": the verdicts are in the
 * handoff. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account. Local only.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8687, Vite 5687):
 *   SR_E2E_BASE_URL=http://localhost:5687 SR_P90_MEASURE_ON=1 \
 *   SR_P90_MEASURE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-measure \
 *   npx playwright test tests/p90-measure-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (SR_P90_MEASURE_ONLY=1,3 runs only those items; items are numbered as in the task: 1 chips, 2 Select trigger,
 * 3 template page, 4 Private content, 5 Sounds.)
 *
 * What it does, at 375x812 with touch in Lounge Light (the account's main theme by PATCH and localStorage, read back)
 * and, for items 1 and 2, at 1280x800 in Editorial:
 *  1. Server settings > Community > Onboarding "Default channels": extra channels are made through the API (and
 *     the server's own are all chosen) until the chips wrap to three rows. Per chip: the chip's box, the remove
 *     button's drawn box, the hit area's box (read from the ::after rule), the hit test at centre and 21 px above
 *     and below with what each returns, a scan of the area (which button owns each point), the overlaps of the rows'
 *     areas and who wins them, whether any button's centre lies inside another's area, the row gaps.
 *  2. Every kit `Select` and every kit text field on Support, Profile, Notifications and Activity: boxes, the hit
 *     test, and the pair of a Select and its nearest field. The page where the two sit closest is shot.
 *  3. A template made through the API (D-863, deleted at the end): `/template/<code>` in the app: field, button,
 *     card, form boxes, hit tests, sideways overflow, truncated text.
 *  4. Admin > Private content: the segmented control, its options' boxes, rows, hit tests.
 *  5. Settings > Voice & Video > Sounds, and the switch stacks of Notifications, Streamer Mode and Accessibility:
 *     every card's direct children with the gap to the previous one.
 * `data-theme` is read back before each shot. No colour ratio is taken (the task asks for none). Shots and
 * measure.json go to SR_P90_MEASURE_OUT.
 */
const ON = process.env.SR_P90_MEASURE_ON === '1';
const OUT =
  process.env.SR_P90_MEASURE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-measure';
const ONLY = (process.env.SR_P90_MEASURE_ONLY ?? '1,2,3,4,5').split(',').map((s) => s.trim());
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;
const SETTINGS = {
  phone: { size: 'phone' as Size, theme: 'lounge-light', tag: '375-lounge-light' },
  desk: { size: 'desktop' as Size, theme: 'lounge-editorial', tag: '1280-editorial' },
};

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const rows: unknown[] = [];
const log = (id: string, data: unknown) => {
  mkdirSync(OUT, { recursive: true });
  rows.push({ id, ...(data as object) });
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(rows, null, 1));
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
}

/** Helpers every probe uses, installed in the page before its own scripts. */
function installHelpers() {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const r = e.getBoundingClientRect();
    return [r1(r.left), r1(r.top), r1(r.width), r1(r.height)];
  };
  const d = (e: Element | null) => {
    if (!e) return null;
    const c = typeof e.className === 'string' ? e.className.trim().split(/\s+/)[0] : '';
    const txt = (e.getAttribute('aria-label') ?? e.textContent ?? '').trim().slice(0, 24);
    return `${e.tagName.toLowerCase()}${c ? `.${c.replace(/_[a-z0-9]{4,}$/i, '')}` : ''}${txt ? ` "${txt}"` : ''}`;
  };
  const owns = (el: Element, t: Element | null) =>
    !!t &&
    (t === el ||
      el.contains(t) ||
      (!!el.closest('label') && t.closest('label') === el.closest('label')));
  const hit = (el: Element, dy: number) => {
    const b = el.getBoundingClientRect();
    const t = document.elementFromPoint(b.left + b.width / 2, b.top + b.height / 2 + dy);
    return { ok: owns(el, t), got: d(t) };
  };
  const three = (el: Element) => ({ up: hit(el, -21), centre: hit(el, 0), down: hit(el, 21) });
  const root = () => ({
    html: document.documentElement.getAttribute('data-theme'),
    first: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
  });
  const scroll = () => [document.documentElement.scrollWidth, document.documentElement.clientWidth];
  const center = (e: Element) => (e as HTMLElement).scrollIntoView({ block: 'center' });
  (window as unknown as { __m: unknown }).__m = { box, d, owns, hit, three, root, scroll, center };
}

type M = {
  box: (e: Element) => number[];
  d: (e: Element | null) => string | null;
  owns: (el: Element, t: Element | null) => boolean;
  hit: (el: Element, dy: number) => { ok: boolean; got: string | null };
  three: (el: Element) => Record<'up' | 'centre' | 'down', { ok: boolean; got: string | null }>;
  root: () => { html: string | null; first: string | null };
  scroll: () => number[];
  center: (e: Element) => void;
};

async function newPage(
  browser: Browser,
  s: { size: Size; theme: string },
  ownerCtx: BrowserContext,
) {
  const phone = s.size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[s.size],
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
  await page.screenshot({ path: resolve(OUT, `${name}.png`), timeout: 10_000 });
};
const go = async (page: Page, path: string) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await idle(page);
};

/* ---------- item 1: wrapped chips ---------- */
function chipsProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const first = document.querySelector('button[aria-label^="Remove "]');
  const control = first?.closest('div[class*="control"]') as HTMLElement | null;
  if (!control) return null;
  M.center(control);
  const btns = Array.from(control.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'));
  const areaOf = (b: HTMLElement) => {
    const bb = b.getBoundingClientRect();
    const cs = getComputedStyle(b, '::after');
    const mt = parseFloat(cs.marginTop) || 0;
    const ml = parseFloat(cs.marginLeft) || 0;
    const left = bb.left + (parseFloat(cs.left) || 0) + ml;
    const top = bb.top + (parseFloat(cs.top) || 0) + mt;
    const w = parseFloat(cs.width) || 0;
    const h = parseFloat(cs.height) || 0;
    return { left, top, w, h, has: cs.content !== 'none' && cs.content !== 'normal' };
  };
  const areas = btns.map(areaOf);
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const owner = (t: Element | null) => {
    if (!t) return 'none';
    const i = btns.findIndex((b) => b === t || b.contains(t));
    return i >= 0 ? `btn${i}` : `other:${M.d(t)}`;
  };
  const chips = btns.map((b, i) => {
    const a = areas[i]!;
    // Which buttons does a scan of this button's area hand to whom?
    const tally: Record<string, number> = {};
    for (let x = a.left + 1; x < a.left + a.w; x += 2)
      for (let y = a.top + 1; y < a.top + a.h; y += 2) {
        const k = owner(document.elementFromPoint(x, y));
        tally[k] = (tally[k] ?? 0) + 1;
      }
    const bb = b.getBoundingClientRect();
    const cx = bb.left + bb.width / 2;
    const cy = bb.top + bb.height / 2;
    const coveredBy = areas
      .map((o, j) => ({ j, o }))
      .filter(
        ({ j, o }) =>
          j !== i && cx >= o.left && cx <= o.left + o.w && cy >= o.top && cy <= o.top + o.h,
      )
      .map(({ j }) => j);
    return {
      i,
      label: b.getAttribute('aria-label'),
      chip: M.box(b.parentElement!),
      button: M.box(b),
      area: [r1(a.left), r1(a.top), r1(a.w), r1(a.h)],
      hasArea: a.has,
      hit: M.three(b),
      centreOwner: owner(document.elementFromPoint(cx, cy)),
      centreInsideOthersArea: coveredBy,
      scanOwners: tally,
    };
  });
  // Overlaps between two areas and who wins each (by a scan of the intersection).
  const overlaps: unknown[] = [];
  for (let i = 0; i < areas.length; i++)
    for (let j = i + 1; j < areas.length; j++) {
      const a = areas[i]!;
      const o = areas[j]!;
      const l = Math.max(a.left, o.left);
      const r = Math.min(a.left + a.w, o.left + o.w);
      const t = Math.max(a.top, o.top);
      const b = Math.min(a.top + a.h, o.top + o.h);
      if (r - l > 0.5 && b - t > 0.5) {
        const tally: Record<string, number> = {};
        for (let x = l + 0.5; x < r; x += 1)
          for (let y = t + 0.5; y < b; y += 1) {
            const k = owner(document.elementFromPoint(x, y));
            tally[k] = (tally[k] ?? 0) + 1;
          }
        overlaps.push({ a: i, b: j, box: [r1(l), r1(t), r1(r - l), r1(b - t)], winners: tally });
      }
    }
  const tops = Array.from(new Set(chips.map((c) => c.chip[1]!))).sort((p, q) => p - q);
  const rowsB = tops.map((tp) => {
    const inRow = chips.filter((c) => c.chip[1] === tp);
    return { top: tp, bottom: Math.max(...inRow.map((c) => c.chip[1]! + c.chip[3]!)) };
  });
  const gaps = rowsB.slice(1).map((rw, k) => r1(rw.top - rowsB[k]!.bottom));
  const chev = control.querySelector('button[class*="chevron"]');
  return {
    theme: M.root(),
    control: M.box(control),
    rows: tops.length,
    rowGaps: gaps,
    chips,
    overlaps,
    chevron: chev ? { box: M.box(chev), hit: M.three(chev) } : null,
    scroll: M.scroll(),
  };
}

async function item1(browser: Browser, owner: BrowserContext, which: keyof typeof SETTINGS) {
  const w = world();
  const s = SETTINGS[which];
  await setMain(owner, s.theme);
  const { ctx, page } = await newPage(browser, s, owner);
  await go(page, `/settings/server/${w.guildId}/community`);
  await page.getByRole('tab', { name: 'Onboarding' }).click();
  const field = page.locator('[aria-label="Default channels"]').first();
  await field.scrollIntoViewIfNeeded();
  await field.click();
  const opts = page.getByRole('option');
  console.log(`REPORT item1 ${which} options: ${await opts.count()}`);
  for (let n = 0; n < 14; n++) {
    const free = page.locator('[role="option"]:not([aria-selected="true"])');
    if ((await page.getByRole('listbox').count()) === 0) await field.click();
    if ((await free.count()) === 0) break;
    await free.first().click();
    await page.waitForTimeout(150);
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  const m = await page.evaluate(chipsProbe);
  if (!m) throw new Error('item1: no chips reached');
  log(`item1-chips-${s.tag}`, m);
  await shot(page, `1-chips-${s.tag}`);
  await ctx.close();
}

/* ---------- item 2: Select trigger against a text field ---------- */
function selectProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const vis = (e: Element) => {
    const r = e.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  };
  const controls = Array.from(
    document.querySelectorAll<HTMLElement>('div[class*="control"]'),
  ).filter((c) => c.querySelector('button[class*="trigger"]') && vis(c));
  const fields = Array.from(
    document.querySelectorAll<HTMLInputElement>('input[class*="control"]'),
  ).filter((i) => vis(i) && !['checkbox', 'radio', 'range', 'file', 'hidden'].includes(i.type));
  const pre = (e: Element) => M.box(e);
  const sBoxes = controls.map(pre);
  const fBoxes = fields.map(pre);
  const card = (e: Element) =>
    e.closest('.us-card, fieldset, form, [class*="card"], section')?.id ||
    e.closest('.us-card, fieldset, form, [class*="card"], section')?.className ||
    null;
  const selects = controls.map((c, i) => {
    let best = -1;
    let bd = 1e9;
    fBoxes.forEach((fb, j) => {
      const dy = Math.abs(fb[1]! + fb[3]! / 2 - (sBoxes[i]![1]! + sBoxes[i]![3]! / 2));
      if (dy < bd) {
        bd = dy;
        best = j;
      }
    });
    return { c, i, best, dist: Math.round(bd) };
  });
  const out = selects.map(({ c, i, best, dist }) => {
    const trig = c.querySelector('button[class*="trigger"]')!;
    const f = best >= 0 ? fields[best]! : null;
    const fb = f ? fBoxes[best]! : null;
    const label = (e: Element | null) =>
      e
        ? (
            e.getAttribute('aria-label') ??
            document.getElementById(e.getAttribute('aria-labelledby') ?? '-')?.textContent ??
            e.closest('label')?.textContent ??
            document.querySelector(`label[for="${e.id}"]`)?.textContent ??
            ''
          )
            .trim()
            .slice(0, 40)
        : null;
    return {
      select: {
        label: label(trig) || (trig.textContent ?? '').trim().slice(0, 24),
        control: sBoxes[i],
        trigger: M.box(trig),
      },
      nearestField: f ? { label: label(f), box: fb, type: f.type } : null,
      distanceY: dist,
      sameCard: !!f && card(c) === card(f),
      heightDiff: fb ? Math.round((sBoxes[i]![3]! - fb[3]!) * 10) / 10 : null,
    };
  });
  // The hit tests, after scrolling each into the middle.
  const hits = controls.map((c, i) => {
    const trig = c.querySelector('button[class*="trigger"]')!;
    M.center(c);
    const f = selects[i]!.best >= 0 ? fields[selects[i]!.best]! : null;
    const tHit = M.three(trig);
    const cHit = M.three(c);
    let fHit = null;
    if (f) {
      M.center(f);
      fHit = M.three(f);
    }
    return { trigger: tHit, control: cHit, field: fHit };
  });
  return {
    theme: M.root(),
    scroll: M.scroll(),
    selects: out.map((o, i) => ({ ...o, hit: hits[i] })),
    fieldCount: fields.length,
  };
}

async function item2(browser: Browser, owner: BrowserContext, which: keyof typeof SETTINGS) {
  const s = SETTINGS[which];
  await setMain(owner, s.theme);
  const { ctx, page } = await newPage(browser, s, owner);
  const pages = ['support', 'profile', 'notifications', 'activity'];
  let best: { path: string; dist: number } | null = null;
  for (const p of pages) {
    const path = `/settings/user/${p}`;
    try {
      await go(page, path);
      const m = await page.evaluate(selectProbe);
      log(`item2-${p}-${s.tag}`, m);
      for (const sel of m.selects)
        if (sel.nearestField && (!best || sel.distanceY < best.dist))
          best = { path, dist: sel.distanceY };
    } catch (e) {
      log(`item2-${p}-${s.tag}`, { notReached: String(e).slice(0, 160) });
    }
  }
  log(`item2-best-${s.tag}`, { best });
  if (!best) throw new Error('item2: no page with a Select and a field');
  await go(page, best.path);
  // Scroll the closest pair into the middle of the viewport.
  await page.evaluate(() => {
    const M = (window as unknown as { __m: M }).__m;
    const cs = Array.from(document.querySelectorAll<HTMLElement>('div[class*="control"]')).filter(
      (c) => c.querySelector('button[class*="trigger"]'),
    );
    const fs = Array.from(document.querySelectorAll<HTMLElement>('input[class*="control"]'));
    let pair: [HTMLElement, HTMLElement] | null = null;
    let bd = 1e9;
    for (const c of cs)
      for (const f of fs) {
        const dd = Math.abs(c.getBoundingClientRect().top - f.getBoundingClientRect().top);
        if (dd < bd) {
          bd = dd;
          pair = [c, f];
        }
      }
    if (pair) M.center(pair[0]);
  });
  await page.waitForTimeout(500);
  log(`item2-shot-${s.tag}`, {
    path: best.path,
    theme: await page.evaluate(() => (window as unknown as { __m: M }).__m.root()),
  });
  await shot(page, `2-select-${s.tag}`);
  await ctx.close();
}

/* ---------- item 3: the template page ---------- */
function templateProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const card = document.querySelector('.ent-invite-card') as HTMLElement | null;
  const form = card?.querySelector('form') as HTMLElement | null;
  const input = form?.querySelector('input') as HTMLElement | null;
  const btn = form?.querySelector('button[type=submit]') as HTMLElement | null;
  if (!card || !form || !input || !btn) return null;
  M.center(btn);
  const cb = card.getBoundingClientRect();
  const inside = (e: Element) => {
    const b = e.getBoundingClientRect();
    return b.left >= cb.left - 0.5 && b.right <= cb.right + 0.5;
  };
  const ib = input.getBoundingClientRect();
  const bb = btn.getBoundingClientRect();
  const cut = Array.from(card.querySelectorAll<HTMLElement>('h1, h2, p, li, span, button, label'))
    .filter(
      (e) => e.scrollWidth > e.clientWidth + 1 || getComputedStyle(e).textOverflow === 'ellipsis',
    )
    .map((e) => ({
      el: M.d(e),
      sw: e.scrollWidth,
      cw: e.clientWidth,
      ellipsis: getComputedStyle(e).textOverflow,
    }));
  return {
    theme: M.root(),
    card: M.box(card),
    form: M.box(form),
    formCs: {
      display: getComputedStyle(form).display,
      flexDirection: getComputedStyle(form).flexDirection,
    },
    field: { box: M.box(input), inside: inside(input), hit: M.three(input) },
    button: {
      box: M.box(btn),
      inside: inside(btn),
      label: (btn.textContent ?? '').trim(),
      hit: M.three(btn),
    },
    sameRow: Math.abs(ib.top - bb.top) < 4,
    stack: { gapBetween: Math.round((bb.top - ib.bottom) * 10) / 10 },
    cut,
    scroll: M.scroll(),
    cardTextWidth: card.clientWidth,
  };
}

async function item3(browser: Browser, owner: BrowserContext) {
  const w = world();
  const s = SETTINGS.phone;
  await setMain(owner, s.theme);
  let code: string | null = null;
  try {
    const tpl = await apiPost<{ code: string }>(owner.request, `/guilds/${w.guildId}/templates`, {
      name: 'P90 measure template',
      description: 'Made by the wave 90 measure spec; deleted at its end.',
    });
    code = tpl.code;
    const { ctx, page } = await newPage(browser, s, owner);
    await go(page, `/template/${code}`);
    await page
      .locator('.ent-invite-card form button[type=submit]')
      .first()
      .waitFor({ timeout: 15_000 });
    await page.waitForTimeout(800);
    const m = await page.evaluate(templateProbe);
    if (!m) throw new Error('item3: template page not reached');
    log(`item3-template-${s.tag}`, m);
    await page.evaluate(() => window.scrollTo(0, 0));
    await shot(page, `3-template-${s.tag}`);
    await ctx.close();
  } finally {
    if (code) {
      const res = await owner.request.delete(`/api/v1/guilds/${w.guildId}/templates/${code}`, {
        headers: await csrfHeaders(owner),
      });
      console.log(`REPORT template deleted: ${res.status()}`);
    }
  }
}

async function csrfHeaders(ctx: BrowserContext): Promise<Record<string, string>> {
  const tok = (await (
    await ctx.request.get('/api/v1/sr/csrf', { headers: { Origin: ORIGIN } })
  ).json()) as { token: string };
  return { Origin: ORIGIN, 'X-SR-CSRF': tok.token };
}

/* ---------- item 4: Private content's segmented control ---------- */
function segProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const groups = Array.from(document.querySelectorAll<HTMLElement>('[role="radiogroup"]'));
  const out = groups.map((g) => {
    M.center(g);
    const opts = Array.from(g.querySelectorAll<HTMLElement>('label'));
    const tops = Array.from(new Set(opts.map((o) => Math.round(o.getBoundingClientRect().top))));
    return {
      label: g.getAttribute('aria-label'),
      box: M.box(g),
      rows: tops.length,
      gapY: getComputedStyle(g).rowGap,
      options: opts.map((o) => {
        const sp = o.querySelector('span')!;
        const lh = parseFloat(getComputedStyle(sp).lineHeight) || 20;
        return {
          text: (o.textContent ?? '').trim(),
          checked: o.hasAttribute('data-checked'),
          box: M.box(o),
          lines: Math.round(sp.getBoundingClientRect().height / lh),
          hit: M.three(o),
          bg: getComputedStyle(o).backgroundColor,
        };
      }),
    };
  });
  return { theme: M.root(), scroll: M.scroll(), groups: out };
}

async function item4(browser: Browser, owner: BrowserContext) {
  const s = SETTINGS.phone;
  await setMain(owner, s.theme);
  const { ctx, page } = await newPage(browser, s, owner);
  await go(page, '/admin/dashboard');
  const nav = '[role="dialog"] nav ul li > button';
  const labels = await page
    .locator(nav)
    .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
  const idx = labels.findIndex((l) => /^private content$/i.test(l));
  console.log(`REPORT item4 admin pages ${labels.length}, Private content at ${idx}`);
  if (idx < 0) throw new Error('item4: Private content not in the Admin navigation');
  await page
    .locator(nav)
    .nth(idx)
    .evaluate((el) => (el as HTMLElement).click());
  await idle(page, 2000);
  await page.locator('[role="radiogroup"]').first().waitFor({ timeout: 10_000 });
  const m = await page.evaluate(segProbe);
  log(`item4-private-content-${s.tag}`, m);
  await page.evaluate(() => {
    const g = document.querySelector('[role="radiogroup"]');
    (g as HTMLElement | null)?.scrollIntoView({ block: 'center' });
  });
  await page.waitForTimeout(400);
  await shot(page, `4-private-content-${s.tag}`);
  await ctx.close();
}

/* ---------- item 5: the Sounds switch stack ---------- */
function stacksProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const cards = Array.from(document.querySelectorAll<HTMLElement>('.us-card'));
  const kind = (e: Element) =>
    e.querySelector(':scope > input[role="switch"]')
      ? 'switch'
      : e.querySelector('input[type="range"]')
        ? 'slider'
        : e.tagName === 'BUTTON'
          ? 'button'
          : e.matches('div:has(> input[type="checkbox"])')
            ? 'checkbox'
            : 'other';
  const out = cards
    .map((c) => {
      const kids = Array.from(c.children).filter((k) => {
        const r = k.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && !k.classList.contains('us-card-head');
      });
      let prev: Element | null = null;
      const items = kids.map((k) => {
        const kb = k.getBoundingClientRect();
        const gap = prev
          ? Math.round((kb.top - prev.getBoundingClientRect().bottom) * 10) / 10
          : null;
        prev = k;
        return {
          kind: kind(k),
          el: M.d(k),
          box: M.box(k),
          gapToPrev: gap,
          marginTop: getComputedStyle(k).marginTop,
        };
      });
      return {
        card: c.id || null,
        title: (c.querySelector('.us-card-title')?.textContent ?? '').trim(),
        switches: items.filter((i) => i.kind === 'switch').length,
        items,
      };
    })
    .filter((c) => c.switches >= 2 || c.card === 'us-voice-sounds');
  return { theme: M.root(), scroll: M.scroll(), cards: out };
}

async function item5(browser: Browser, owner: BrowserContext) {
  const s = SETTINGS.phone;
  await setMain(owner, s.theme);
  const { ctx, page } = await newPage(browser, s, owner);
  for (const p of ['voice', 'notifications', 'streamer', 'accessibility']) {
    try {
      await go(page, `/settings/user/${p}`);
      log(`item5-${p}-${s.tag}`, await page.evaluate(stacksProbe));
      if (p === 'voice') {
        await page.evaluate(() => {
          document.getElementById('us-voice-sounds')?.scrollIntoView({ block: 'start' });
        });
        await page.waitForTimeout(500);
        await shot(page, `5-sounds-${s.tag}`);
      }
    } catch (e) {
      log(`item5-${p}-${s.tag}`, { notReached: String(e).slice(0, 160) });
    }
  }
  await ctx.close();
}

test('p90 measure: the five re-measure items', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  appendFileSync(resolve(OUT, 'run.log'), `run ${new Date().toISOString()}\n`);
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const errors: string[] = [];
  const run = async (id: string, fn: () => Promise<void>) => {
    if (!ONLY.includes(id)) return;
    try {
      await fn();
    } catch (e) {
      errors.push(`item ${id}: ${String(e).slice(0, 300)}`);
      console.log(`REPORT item ${id} FAILED ${String(e).slice(0, 300)}`);
    }
  };
  if (ONLY.includes('1')) {
    // Extra channels, so that the chips wrap to three rows at 375 (they stay in the scratch database).
    for (const name of [
      'p90-measure-announcements',
      'p90-measure-a-much-longer-channel-name-for-wrapping',
      'p90-m-three',
      'p90-measure-general-chat-and-more',
      'p90-measure-five',
    ])
      await apiPost(owner.request, `/guilds/${w.guildId}/channels`, { name, type: 0 }).catch(
        () => undefined,
      );
  }
  await run('1', () => item1(browser, owner, 'phone'));
  await run('1', () => item1(browser, owner, 'desk'));
  await run('2', () => item2(browser, owner, 'phone'));
  await run('2', () => item2(browser, owner, 'desk'));
  await run('3', () => item3(browser, owner));
  await run('4', () => item4(browser, owner));
  await run('5', () => item5(browser, owner));
  await owner.close();
  log('totals', { errors });
  // The only assertions: every item that was asked for was reached.
  test.info().annotations.push({ type: 'errors', description: errors.join(' | ') });
  if (errors.length) throw new Error(errors.join('\n'));
});
