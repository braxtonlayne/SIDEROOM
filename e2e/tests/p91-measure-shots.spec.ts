import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * Wave 91, measure builder: SEC-AUD-399 (D-911): the two cases wave 90 left unmeasured, measured on the merged
 * tree at 375x812 with touch in Lounge Light. Nothing is built and nothing is asserted but "the page was reached
 * and the probe returned boxes": the verdicts are in the handoff. NEEDS THE SWEEP'S SEEDED WORLD
 * (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited). The Owner's account. Local only.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8694, Vite 5694):
 *   SR_E2E_BASE_URL=http://localhost:5694 SR_P91_MEASURE_ON=1 \
 *   SR_P91_MEASURE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-measure \
 *   npx playwright test tests/p91-measure-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (SR_P91_MEASURE_ONLY=1 or =2 runs one case.)
 *
 * Case 1: Server settings > Community > Onboarding "Default channels". Channels p91-r1 to p91-r9 are made through
 * the API (D-863); every other chip is removed and the nine are chosen, so the chips are equal in width and the
 * remove buttons of adjacent rows can sit in one column. Per chip: the chip's box, the remove button's drawn box,
 * its target's box (the ::after rule's computed box), the hit test (centre, 21 px above, 21 px below, with what each
 * returns), a 1 px walk of elementFromPoint up and down from the centre (where the button's own run starts and ends),
 * whether any button's centre lies inside another chip's target, the overlap of adjacent targets and who wins it, the
 * row pitch and gap. Then a touch tap on the middle button of the middle row, and which chip went.
 * Case 2: every user Settings page that has a stack of two or more switch rows: per stack the parent's display and
 * gap, each row's box and the gap to the previous row, and whether the row is the previous one's next sibling.
 * Shots (3): 1-chips, 1-chips-outlined (the targets outlined by the spec), 2-switch-stack. `data-theme` is read back
 * before each shot. Shots and measure.json go to SR_P91_MEASURE_OUT.
 */
const ON = process.env.SR_P91_MEASURE_ON === '1';
const OUT =
  process.env.SR_P91_MEASURE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p91-measure';
const ONLY = (process.env.SR_P91_MEASURE_ONLY ?? '1,2').split(',').map((s) => s.trim());
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const THEME = 'lounge-light';
const TAG = '375-lounge-light';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const rows: unknown[] = [];
const log = (id: string, data: unknown) => {
  mkdirSync(OUT, { recursive: true });
  rows.push({ id, ...(data as object) });
  writeFileSync(resolve(OUT, 'measure.json'), JSON.stringify(rows, null, 1));
  console.log(`REPORT ${id} ${JSON.stringify(data).slice(0, 900)}`);
};

async function setMain(ownerCtx: BrowserContext) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: THEME, zoom: 100 } },
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

async function newPage(browser: Browser, ownerCtx: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: PHONE,
    storageState: await ownerCtx.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
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
  }, THEME);
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

/* ---------- case 1: equal-width chips in one column ---------- */
function chipsProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const first = document.querySelector('button[aria-label^="Remove "]');
  const control = first?.closest('div[class*="control"]') as HTMLElement | null;
  if (!control) return null;
  M.center(control);
  const btns = Array.from(control.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'));
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const areaOf = (b: HTMLElement) => {
    const bb = b.getBoundingClientRect();
    const cs = getComputedStyle(b, '::after');
    const left = bb.left + (parseFloat(cs.left) || 0) + (parseFloat(cs.marginLeft) || 0);
    const top = bb.top + (parseFloat(cs.top) || 0) + (parseFloat(cs.marginTop) || 0);
    return {
      left,
      top,
      w: parseFloat(cs.width) || 0,
      h: parseFloat(cs.height) || 0,
      has: cs.content !== 'none' && cs.content !== 'normal',
      raw: {
        content: cs.content,
        position: cs.position,
        top: cs.top,
        left: cs.left,
        width: cs.width,
        height: cs.height,
        marginTop: cs.marginTop,
        marginLeft: cs.marginLeft,
      },
    };
  };
  const areas = btns.map(areaOf);
  const owner = (t: Element | null) => {
    if (!t) return 'none';
    const i = btns.findIndex((b) => b === t || b.contains(t));
    return i >= 0 ? `btn${i}` : `other:${M.d(t)}`;
  };
  const chips = btns.map((b, i) => {
    const a = areas[i]!;
    const bb = b.getBoundingClientRect();
    const cx = bb.left + bb.width / 2;
    const cy = bb.top + bb.height / 2;
    // A walk of elementFromPoint up and down in 1 px steps from the centre: how far up and down the button's own run goes.
    const run = (sign: number) => {
      let last = 0;
      for (let s = 0; s <= 60; s++) {
        if (owner(document.elementFromPoint(cx, cy + sign * s)) === `btn${i}`) last = s;
        else break;
      }
      const next = owner(document.elementFromPoint(cx, cy + sign * (last + 1)));
      return { ownRun: last, thenReturns: next };
    };
    const coveredBy = areas
      .map((o, j) => ({ j, o }))
      .filter(
        ({ j, o }) =>
          j !== i && cx >= o.left && cx <= o.left + o.w && cy >= o.top && cy <= o.top + o.h,
      )
      .map(({ j }) => j);
    const h = M.three(b);
    return {
      i,
      label: b.getAttribute('aria-label'),
      chip: M.box(b.parentElement!),
      button: M.box(b),
      target: [r1(a.left), r1(a.top), r1(a.w), r1(a.h)],
      targetRaw: a.raw,
      hasTarget: a.has,
      hit: h,
      returnsOwn: {
        up: owner(document.elementFromPoint(cx, cy - 21)),
        centre: owner(document.elementFromPoint(cx, cy)),
        down: owner(document.elementFromPoint(cx, cy + 21)),
      },
      walkUp: run(-1),
      walkDown: run(1),
      centreInsideOthersTarget: coveredBy,
    };
  });
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
  const rowOf = (c: { chip: number[] }) => tops.indexOf(c.chip[1]!);
  // Columns: the buttons' x (left edge) per row; do adjacent rows line up within 1 px?
  const xsByRow = tops.map((_, k) => chips.filter((c) => rowOf(c) === k).map((c) => c.button[0]!));
  const aligned: unknown[] = [];
  for (let k = 1; k < xsByRow.length; k++)
    aligned.push({
      rows: [k - 1, k],
      sharedColumns: xsByRow[k - 1]!.filter((x) => xsByRow[k]!.some((y) => Math.abs(x - y) <= 1)),
    });
  const chev = control.querySelector('button[class*="chevron"]');
  return {
    theme: M.root(),
    control: M.box(control),
    rows: tops.length,
    rowTops: tops,
    rowPitch: tops.slice(1).map((tp, k) => r1(tp - tops[k]!)),
    rowGaps: rowsB.slice(1).map((rw, k) => r1(rw.top - rowsB[k]!.bottom)),
    xsByRow,
    aligned,
    chips,
    overlaps,
    chevron: chev ? { box: M.box(chev), hit: M.three(chev) } : null,
    scroll: M.scroll(),
  };
}

/** The targets outlined (fixed, no pointer events) over the page, for the second shot. */
function outlineTargets() {
  const control = document
    .querySelector('button[aria-label^="Remove "]')
    ?.closest('div[class*="control"]');
  if (!control) return 0;
  let n = 0;
  control.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]').forEach((b, i) => {
    const bb = b.getBoundingClientRect();
    const cs = getComputedStyle(b, '::after');
    const o = document.createElement('div');
    const colour = i % 2 ? 'rgba(220,0,0,0.9)' : 'rgba(0,90,220,0.9)';
    o.style.cssText = `position:fixed;pointer-events:none;z-index:99999;box-sizing:border-box;border:1px solid ${colour};background:${i % 2 ? 'rgba(220,0,0,0.12)' : 'rgba(0,90,220,0.12)'};left:${bb.left + (parseFloat(cs.left) || 0) + (parseFloat(cs.marginLeft) || 0)}px;top:${bb.top + (parseFloat(cs.top) || 0) + (parseFloat(cs.marginTop) || 0)}px;width:${parseFloat(cs.width) || 0}px;height:${parseFloat(cs.height) || 0}px`;
    document.body.appendChild(o);
    n++;
  });
  return n;
}

async function case1(browser: Browser, owner: BrowserContext) {
  const w = world();
  for (let i = 1; i <= 9; i++)
    await apiPost(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p91-r${i}`,
      type: 0,
    }).catch(() => undefined);
  await setMain(owner);
  const { ctx, page } = await newPage(browser, owner);
  await go(page, `/settings/server/${w.guildId}/community`);
  await page.getByRole('tab', { name: 'Onboarding' }).click();
  const field = page.locator('[aria-label="Default channels"]').first();
  await field.scrollIntoViewIfNeeded();
  // Remove every chip that is not one of the nine (a chip that is not equal in width would break the columns).
  for (let n = 0; n < 40; n++) {
    const gone = await page.evaluate(() => {
      const b = Array.from(
        document.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'),
      ).find((x) => !/p91-r\d/.test(x.getAttribute('aria-label') ?? ''));
      if (!b) return false;
      b.click();
      return true;
    });
    if (!gone) break;
    await page.waitForTimeout(150);
  }
  await field.click();
  console.log(`REPORT case1 options: ${await page.getByRole('option').count()}`);
  for (let n = 0; n < 12; n++) {
    const free = page.locator('[role="option"]:not([aria-selected="true"])', {
      hasText: /p91-r\d/,
    });
    if ((await page.getByRole('listbox').count()) === 0) await field.click();
    if ((await free.count()) === 0) break;
    await free.first().click();
    await page.waitForTimeout(150);
  }
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);
  const m = await page.evaluate(chipsProbe);
  if (!m) throw new Error('case1: no chips reached');
  log(`case1-chips-${TAG}`, m);
  await shot(page, `1-chips-${TAG}`);
  const nOut = await page.evaluate(outlineTargets);
  console.log(`REPORT case1 outlines drawn: ${nOut}; data-theme ${JSON.stringify(m.theme)}`);
  await shot(page, `1-chips-outlined-${TAG}`);
  await page.evaluate(() => {
    document.querySelectorAll('body > div[style*="z-index: 99999"]').forEach((e) => e.remove());
  });
  // A tap at the drawn box of the middle button of the middle row: which chip went?
  const labelsBefore = await page.evaluate(() =>
    Array.from(document.querySelectorAll('button[aria-label^="Remove "]')).map(
      (b) => b.getAttribute('aria-label') ?? '',
    ),
  );
  const tapAt = await page.evaluate(() => {
    const M = (window as unknown as { __m: M }).__m;
    const btns = Array.from(
      document.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'),
    );
    const tops = Array.from(
      new Set(btns.map((b) => Math.round(b.parentElement!.getBoundingClientRect().top))),
    ).sort((a, b) => a - b);
    const midRow = tops[Math.floor(tops.length / 2)]!;
    const inRow = btns.filter(
      (b) => Math.round(b.parentElement!.getBoundingClientRect().top) === midRow,
    );
    const b = inRow[Math.floor(inRow.length / 2)]!;
    M.center(b);
    const r = b.getBoundingClientRect();
    return {
      label: b.getAttribute('aria-label'),
      x: r.left + r.width / 2,
      y: r.top + r.height / 2,
    };
  });
  await page.waitForTimeout(300);
  const tapAt2 = await page.evaluate((label) => {
    const b = Array.from(
      document.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]'),
    ).find((x) => x.getAttribute('aria-label') === label)!;
    const r = b.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  }, tapAt.label);
  await page.touchscreen.tap(tapAt2.x, tapAt2.y);
  await page.waitForTimeout(500);
  const labelsAfter = await page.evaluate(() =>
    Array.from(document.querySelectorAll('button[aria-label^="Remove "]')).map(
      (b) => b.getAttribute('aria-label') ?? '',
    ),
  );
  log('case1-tap-middle', {
    tapped: tapAt.label,
    at: tapAt2,
    removed: labelsBefore.filter((l) => !labelsAfter.includes(l)),
    before: labelsBefore.length,
    after: labelsAfter.length,
  });
  await ctx.close();
}

/* ---------- case 2: switch stacks of the user Settings ---------- */
function stacksProbe() {
  const M = (window as unknown as { __m: M }).__m;
  const rowsAll = Array.from(document.querySelectorAll<HTMLElement>('div')).filter((e) =>
    e.querySelector(':scope > input[role="switch"]'),
  );
  const parents = Array.from(new Set(rowsAll.map((e) => e.parentElement!)));
  const stacks = parents
    .map((par) => ({ par, rs: rowsAll.filter((e) => e.parentElement === par) }))
    .filter((g) => g.rs.length >= 2)
    .map(({ par, rs }) => {
      const cs = getComputedStyle(par);
      const kids = Array.from(par.children).filter((k) => {
        const r = k.getBoundingClientRect();
        return r.width > 0 && r.height > 0;
      });
      return {
        parent: M.d(par),
        parentClass: par.className,
        card: par.closest('.us-card')?.id || null,
        title: (par.closest('.us-card')?.querySelector('.us-card-title')?.textContent ?? '').trim(),
        parentDisplay: cs.display,
        parentDirection: cs.flexDirection,
        parentRowGap: cs.rowGap,
        children: kids.length,
        switches: rs.length,
        rows: rs.map((r, i) => {
          const rb = r.getBoundingClientRect();
          const pb = i ? rs[i - 1]!.getBoundingClientRect() : null;
          return {
            label: (r.textContent ?? '').trim().slice(0, 24),
            box: M.box(r),
            gapToPrevSwitch: pb ? Math.round((rb.top - pb.bottom) * 10) / 10 : null,
            marginTop: getComputedStyle(r).marginTop,
            adjacentSibling: i ? r.previousElementSibling === rs[i - 1] : null,
          };
        }),
      };
    });
  return { theme: M.root(), scroll: M.scroll(), stacks };
}

async function case2(browser: Browser, owner: BrowserContext) {
  await setMain(owner);
  const { ctx, page } = await newPage(browser, owner);
  let best: { path: string; card: string | null; n: number } | null = null;
  for (const p of [
    'voice',
    'notifications',
    'activity',
    'text',
    'accessibility',
    'streamer',
    'privacy',
    'appearance',
    'app',
    'support',
    'connections',
  ]) {
    try {
      await go(page, `/settings/user/${p}`);
      const m = await page.evaluate(stacksProbe);
      log(`case2-${p}-${TAG}`, m);
      for (const s of m.stacks) {
        // Sounds is the reference, not a candidate.
        if (p === 'voice' && s.card === 'us-voice-sounds') continue;
        if (!best || s.switches > best.n)
          best = { path: `/settings/user/${p}`, card: s.card, n: s.switches };
      }
    } catch (e) {
      log(`case2-${p}-${TAG}`, { notReached: String(e).slice(0, 160) });
    }
  }
  log('case2-best', { best });
  if (!best) throw new Error('case2: no stack found');
  await go(page, best.path);
  await page.evaluate((id) => {
    const el = id ? document.getElementById(id) : null;
    (el ?? document.querySelector('input[role="switch"]'))?.scrollIntoView({ block: 'start' });
  }, best.card);
  await page.waitForTimeout(500);
  log('case2-shot', {
    path: best.path,
    card: best.card,
    theme: await page.evaluate(() => (window as unknown as { __m: M }).__m.root()),
  });
  await shot(page, `2-switch-stack-${TAG}`);
  await ctx.close();
}

test('p91 measure: equal-width chips and a second switch stack', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
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
  await run('1', () => case1(browser, owner));
  await run('2', () => case2(browser, owner));
  await owner.close();
  log('totals', { errors });
  test.info().annotations.push({ type: 'errors', description: errors.join(' | ') });
  if (errors.length) throw new Error(errors.join('\n'));
});
