import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-381 (wave 88, D-890 built): the Cinema defaults card's three labels (`#us-sub-lang-label`,
 * `#us-audio-lang-label`, `#us-quality-cap-label`, Activity & privacy tab of the member's Settings) take the
 * `us-seg-label` style, as the Library defaults card beside it. NEEDS THE SWEEP'S SEEDED WORLD: the Owner of
 * `e2e/.state/sweep/owner.json`, made on the same database by `page-sweep.spec.ts -g "seed the sweep world"` (this spec
 * never edits that spec).
 *
 * Per setting (1280x800 Editorial, 375x812 touch in Lounge Light and Lounge Dark; the theme is set with the account's
 * settings (PATCH) and localStorage, `data-theme` is read back): the BASE case puts the old `us-muted` class back on
 * the three Cinema labels in the page (the served tree has the change; the swap shows the base exactly) and probes
 * the three Cinema and three Library labels (colour, weight, size, margins, gap to the control, the control's
 * accessible name) and the cards' boxes; the AFTER case reloads and probes the served page. One JSON line per
 * setting and case in `<tag>-measure.json`. Shots: Editorial base (1), and after in all three settings (3), with the
 * viewport made tall for the shot only so both cards are whole.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (Vite on 5663, API on 8663):
 *   SR_P88_SETTINGS_ON=1 SR_P88_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-settings SR_E2E_BASE_URL=http://localhost:5663 npx playwright test tests/p88-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * Add `SR_P88_SETTINGS_BASE=1` to run the base case only (nothing after); `SR_P88_SETTINGS_TAG` only names the files.
 */
const ON = process.env.SR_P88_SETTINGS_ON === '1';
const BASE_ONLY = process.env.SR_P88_SETTINGS_BASE === '1';
const OUT =
  process.env.SR_P88_SETTINGS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-settings';
const TAG = process.env.SR_P88_SETTINGS_TAG ?? 'run';
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
  {
    key: 'desk-editorial',
    theme: 'lounge-editorial',
    size: { width: 1280, height: 800 },
    phone: false,
  },
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

/** In the page: put the base class back (when asked) and probe both cards' labels. */
function probeInPage(asBase: boolean): Record<string, unknown> {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const text = (e: Element | null | undefined) =>
    (e?.textContent ?? '').replace(/\s+/g, ' ').trim();
  const box = (e: Element | null | undefined) => {
    if (!e) return null;
    const b = e.getBoundingClientRect();
    return [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)];
  };
  const cin = document.querySelector<HTMLElement>('#us-cinema-defaults');
  const lib = document.querySelector<HTMLElement>('#us-library-defaults');
  if (!cin || !lib) return { cin: null, lib: null };
  const ids = ['us-sub-lang-label', 'us-audio-lang-label', 'us-quality-cap-label'];
  if (asBase)
    for (const id of ids) {
      const l = document.getElementById(id);
      l?.classList.remove('us-seg-label');
      l?.classList.add('us-muted');
    }
  const look = (e: HTMLElement, control: Element | null) => {
    const cs = getComputedStyle(e);
    const lb = e.getBoundingClientRect();
    const cb = control?.getBoundingClientRect();
    return {
      text: text(e),
      tag: e.tagName.toLowerCase(),
      cls: e.className,
      ariaHidden: e.getAttribute('aria-hidden'),
      color: cs.color,
      weight: cs.fontWeight,
      size: cs.fontSize,
      marginTop: cs.marginTop,
      marginBottom: cs.marginBottom,
      display: cs.display,
      gapToControl: cb ? r1(cb.top - lb.bottom) : null,
      box: box(e),
    };
  };
  const nameOf = (e: Element) => {
    const lb = e.getAttribute('aria-labelledby');
    if (lb)
      return lb
        .split(/\s+/)
        .map((id) => text(document.getElementById(id)))
        .join(' ');
    return e.getAttribute('aria-label') ?? '';
  };
  const cinema = ids.map((id) => {
    const l = document.getElementById(id) as HTMLElement;
    const c = document.querySelector(`#us-cinema-defaults [aria-labelledby="${id}"]`);
    return {
      ...look(l, c),
      controlName: c ? nameOf(c) : null,
      controlRole: c?.getAttribute('role') ?? c?.tagName.toLowerCase() ?? null,
    };
  });
  const library = Array.from(lib.querySelectorAll<HTMLElement>('.us-seg-label'))
    .filter((e) => e.getAttribute('aria-hidden') === 'true')
    .map((l) => look(l, l.parentElement?.querySelector('[role=radiogroup]') ?? null));
  return {
    cinema,
    library,
    cinemaCard: box(cin),
    libraryCard: box(lib),
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
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
      if (!localStorage.getItem('sr.p88.seeded')) {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
        );
        localStorage.setItem('sr.p88.seeded', '1');
      }
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

/** Open Settings and the tab that shows the Cinema defaults card; scroll the card to the top. */
async function openCard(page: Page, first: boolean) {
  await page.goto('/settings/user/account', { waitUntil: 'domcontentloaded', timeout: 20_000 });
  const btns = page.locator('[role="dialog"] nav ul li > button');
  await btns.first().waitFor({ state: 'attached', timeout: 15_000 });
  const n = await btns.count();
  for (let i = 0; i < n; i++) {
    await btns.nth(i).evaluate((el) => (el as HTMLElement).click());
    await page.waitForTimeout(600);
    if ((await page.locator('#us-cinema-defaults').count()) > 0) break;
  }
  await page.locator('#us-cinema-defaults').waitFor({ timeout: 10_000 });
  await page.locator('#us-library-defaults').waitFor({ timeout: 10_000 });
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.evaluate(() =>
    document.querySelector('#us-cinema-defaults')?.scrollIntoView({ block: 'start' }),
  );
  await page.waitForTimeout(first ? 600 : 300);
}

/** The viewport made tall for the shot only, so both cards are whole. */
async function shot(page: Page, s: Setting, file: string) {
  await page.setViewportSize({ width: s.size.width, height: s.phone ? 2400 : 1500 });
  await page.waitForTimeout(400);
  await page.evaluate(() =>
    document.querySelector('#us-cinema-defaults')?.scrollIntoView({ block: 'start' }),
  );
  await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
  await page.setViewportSize(s.size);
}

test('SEC-AUD-381: the Cinema defaults labels, measured before and after, and shot', async ({
  browser,
}) => {
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
    const before = await page.evaluate(probeInPage, true);
    const bf = `${TAG}-before-${s.key}-${String(before.theme)}.png`;
    if (s.key === 'desk-editorial') await shot(page, s, bf);
    appendFileSync(
      jsonl,
      `${JSON.stringify({ case: 'before', setting: s.key, want: s.theme, ...before })}\n`,
    );
    if (!BASE_ONLY) {
      await page.reload({ waitUntil: 'domcontentloaded' });
      await openCard(page, false);
      const after = await page.evaluate(probeInPage, false);
      const af = `${TAG}-after-${s.key}-${String(after.theme)}.png`;
      await shot(page, s, af);
      appendFileSync(
        jsonl,
        `${JSON.stringify({ case: 'after', setting: s.key, want: s.theme, file: af, ...after })}\n`,
      );
      const names = (after.cinema as { text: string; controlName: string }[]).map(
        (c) => c.text === c.controlName,
      );
      console.log(
        `REPORT ${s.key} theme ${String(after.theme)} names match label: ${names.join(',')}`,
      );
    }
    await ctx.close();
  }
  await ownerCtx.close();
});
