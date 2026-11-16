import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-357 (wave 86, D-883): the Library defaults card (`#us-library-defaults`, the Activity & privacy tab of the
 * member's Settings) has a visible label over each of its four controls (Reading mode, Reading direction, Reader
 * background, the preload slider). NEEDS THE SWEEP'S SEEDED WORLD: the Owner of `e2e/.state/sweep/owner.json`, made on
 * the same database by `page-sweep.spec.ts -g "seed the sweep world"` (this spec never edits that spec).
 *
 * Per setting (375x812 touch in Lounge Light and Lounge Dark, 1280x800 in Editorial; the theme is set with the
 * account's settings (PATCH) and localStorage, `data-theme` is read back): the four controls' accessible names, the box
 * of each visible label and of its control, the card's text start, the label's contrast, the Cinema defaults card's
 * boxes (for "0 px moved"), one shot of the card. On the Lounge Light phone it also changes Reading mode, reloads,
 * reads it back and sets it back. One JSON line per setting in `<tag>-measure.json`.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (Vite on 5643, API on 8643):
 *   SR_P86_SETTINGS_BASE=1 SR_P86_SETTINGS_TAG=after SR_P86_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-settings SR_E2E_BASE_URL=http://localhost:5643 npx playwright test tests/p86-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (`SR_P86_SETTINGS_TAG` only names the files: `before` or `after`.)
 */
const ON = process.env.SR_P86_SETTINGS_BASE === '1';
const OUT =
  process.env.SR_P86_SETTINGS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-settings';
const TAG = process.env.SR_P86_SETTINGS_TAG ?? 'run';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

interface Setting {
  key: string;
  theme: string;
  size: { width: number; height: number };
  phone: boolean;
}
const SETTINGS: Setting[] = [
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
  {
    key: 'desk-editorial',
    theme: 'lounge-editorial',
    size: { width: 1280, height: 800 },
    phone: false,
  },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

/** In the page: the card's four controls, their names, labels, boxes and the label's contrast. */
function probeInPage(): Record<string, unknown> {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element | null | undefined) => {
    if (!e) return null;
    const b = e.getBoundingClientRect();
    return [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)];
  };
  /** The text's own box (a Range over the node's contents), not the element's. */
  const textBox = (e: Element | null | undefined) => {
    if (!e) return null;
    const rg = document.createRange();
    rg.selectNodeContents(e);
    const b = rg.getBoundingClientRect();
    return [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)];
  };
  const parse = (c: string) => {
    const m = /(rgba?|color)\(([^)]+)\)/.exec(c);
    if (!m) return null;
    const k = m[1] === 'color' ? 255 : 1;
    const v = m[2]!
      .replace('srgb', '')
      .split(/[ ,/]+/)
      .filter(Boolean)
      .map((x, i) => (i < 3 ? Number(x) * k : Number(x)));
    return {
      r: v[0]!,
      g: v[1]!,
      b: v[2]!,
      a: v[3] === undefined || Number.isNaN(v[3]) ? 1 : v[3]!,
    };
  };
  const lum = (c: { r: number; g: number; b: number }) => {
    const f = (x: number) => {
      const y = x / 255;
      return y <= 0.03928 ? y / 12.92 : Math.pow((y + 0.055) / 1.055, 2.4);
    };
    return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
  };
  const contrast = (e: HTMLElement) => {
    const cs = getComputedStyle(e);
    const fg = parse(cs.color);
    if (!fg) return null;
    const layers: { r: number; g: number; b: number; a: number }[] = [];
    for (let a: HTMLElement | null = e; a; a = a.parentElement) {
      const bg = parse(getComputedStyle(a).backgroundColor);
      if (bg && bg.a > 0) layers.push(bg);
      if (bg && bg.a >= 1) break;
    }
    let base = { r: 255, g: 255, b: 255 };
    for (const l of layers.reverse())
      base = {
        r: l.r * l.a + base.r * (1 - l.a),
        g: l.g * l.a + base.g * (1 - l.a),
        b: l.b * l.a + base.b * (1 - l.a),
      };
    const mix = {
      r: fg.r * fg.a + base.r * (1 - fg.a),
      g: fg.g * fg.a + base.g * (1 - fg.a),
      b: fg.b * fg.a + base.b * (1 - fg.a),
    };
    const l1 = lum(mix);
    const l2 = lum(base);
    return {
      ratio: Math.round(((Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)) * 100) / 100,
      fg: cs.color,
      bg: `rgb(${Math.round(base.r)},${Math.round(base.g)},${Math.round(base.b)})`,
      size: cs.fontSize,
      weight: cs.fontWeight,
    };
  };
  const text = (e: Element | null | undefined) =>
    (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
  /** The accessible name by the usual order: aria-labelledby, aria-label, a label for the id. */
  const nameOf = (e: Element) => {
    const lb = e.getAttribute('aria-labelledby');
    if (lb)
      return lb
        .split(/\s+/)
        .map((id) => text(document.getElementById(id)))
        .join(' ');
    const al = e.getAttribute('aria-label');
    if (al) return al;
    if (e.id) {
      const l = document.querySelector(`label[for="${e.id}"]`);
      if (l) return text(l);
    }
    return '';
  };
  const card = document.querySelector<HTMLElement>('#us-library-defaults');
  if (!card) return { card: null };
  const stack = card.querySelector('.us-stack');
  const controls: Record<string, unknown>[] = [];
  const els = Array.from(
    card.querySelectorAll<HTMLElement>('[role=radiogroup], input[type=range]'),
  );
  for (const c of els) {
    const slider = c.tagName === 'INPUT';
    // The visible label: the sibling `.us-seg-label` (segmented), or the slider's `label[for]`.
    const vis: HTMLElement | null = slider
      ? c.id
        ? document.querySelector<HTMLElement>(`label[for="${c.id}"]`)
        : null
      : (c.parentElement?.querySelector<HTMLElement>('.us-seg-label') ?? null);
    controls.push({
      kind: slider ? 'slider' : 'segmented',
      name: nameOf(c),
      labelledby: c.getAttribute('aria-labelledby'),
      visibleLabel: text(vis),
      visibleAriaHidden: vis?.getAttribute('aria-hidden') ?? null,
      labelTextBox: textBox(vis),
      labelBox: box(vis),
      controlBox: box(c),
      labelAboveControl: vis
        ? vis.getBoundingClientRect().bottom <= c.getBoundingClientRect().top + 1
        : false,
      contrast: vis ? contrast(vis) : null,
    });
  }
  const cin = document.querySelector<HTMLElement>('#us-cinema-defaults');
  return {
    card: box(card),
    stack: box(stack),
    cardTextStart: r1(stack?.getBoundingClientRect().left ?? 0),
    title: textBox(card.querySelector('h2, h3')),
    controls,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    cinema: cin
      ? [cin, ...Array.from(cin.querySelectorAll<HTMLElement>('*'))].map((e) => ({
          sel: e.tagName.toLowerCase() + (e.id ? `#${e.id}` : ''),
          b: box(e),
        }))
      : null,
    theme: (document.querySelector('[data-theme]') as HTMLElement | null)?.dataset.theme ?? null,
  };
}

async function newPage(browser: Browser, s: Setting, base: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: s.size,
    storageState: await base.storageState(),
    bypassCSP: true,
    ...(s.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((theme) => {
    try {
      if (!localStorage.getItem('sr.p86.seeded')) {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
        );
        localStorage.setItem('sr.p86.seeded', '1');
      }
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

/** Open Settings and the tab that shows the Library defaults card; scroll the card to the top. */
async function openCard(page: Page, first: boolean) {
  await page.goto('/settings/user/account', { waitUntil: 'domcontentloaded', timeout: 20_000 });
  const btns = page.locator('[role="dialog"] nav ul li > button');
  await btns.first().waitFor({ timeout: 15_000 });
  const n = await btns.count();
  for (let i = 0; i < n; i++) {
    await btns.nth(i).evaluate((el) => (el as HTMLElement).click());
    await page.waitForTimeout(600);
    if ((await page.locator('#us-library-defaults').count()) > 0) break;
  }
  await page.locator('#us-library-defaults').waitFor({ timeout: 10_000 });
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.evaluate(() =>
    document.querySelector('#us-library-defaults')?.scrollIntoView({ block: 'start' }),
  );
  await page.waitForTimeout(first ? 600 : 300);
}

const modeChecked = (page: Page) =>
  page.evaluate(() => {
    const g = document.querySelector('#us-library-defaults [role=radiogroup]');
    return g?.querySelector<HTMLInputElement>('input:checked')?.value ?? null;
  });

test('SEC-AUD-357: the Library defaults card labels, shot and measured', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}-measure.json`);
  writeFileSync(jsonl, '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  for (const s of SETTINGS) {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: s.theme, zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(`REPORT ${s.key} account theme read back: ${back.settings?.user_prefs?.theme}`);
    const { ctx, page } = await newPage(browser, s, ownerCtx);
    await openCard(page, true);
    const m = await page.evaluate(probeInPage);
    const file = `${TAG}-library-defaults-${s.key}-${String(m.theme)}.png`;
    await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
    const line: Record<string, unknown> = { setting: s.key, want: s.theme, file, ...m };
    if (s.key === 'phone-light') {
      const group = () => page.locator('#us-library-defaults [role=radiogroup]').first();
      const before = await modeChecked(page);
      const other = before === 'strip' ? 'paged' : 'strip';
      await group().locator(`label:has(input[value="${other}"])`).click();
      await page.waitForTimeout(400);
      await page.reload({ waitUntil: 'domcontentloaded' });
      await openCard(page, false);
      const kept = await modeChecked(page);
      await group().locator(`label:has(input[value="${before}"])`).click();
      await page.waitForTimeout(400);
      await page.reload({ waitUntil: 'domcontentloaded' });
      await openCard(page, false);
      line.persist = { before, other, keptAfterReload: kept, restored: await modeChecked(page) };
    }
    appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
    console.log(
      `REPORT ${s.key} theme ${String(m.theme)} controls ${(m.controls as unknown[] | undefined)?.length} ${JSON.stringify(line.persist ?? '')}`,
    );
    await ctx.close();
  }
  await ownerCtx.close();
});
