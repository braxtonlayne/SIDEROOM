import { appendFileSync, mkdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * Wave 86, kit builder: SEC-AUD-373 (D-879): the kit's `FileDrop` button and a multi-select chip's remove button are
 * 44 px targets on a phone. NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 * `SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium
 * --workers=1 --retries=0 --reporter=line`; never edit that spec). The Owner's account. Local only.
 *
 * The commander's command, from e2e/, on a stack with the sweep's world (API 8641, Vite 5641):
 *   SR_E2E_BASE_URL=http://localhost:5641 SR_P86_KIT_ON=1 SR_P86_KIT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-kit
 *   npx playwright test tests/p86-kit-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (SR_P86_KIT_BASE=1 prefixes every file and line "base-": run it with the two sheets held back.)
 *
 * Per theme (375x812 touch: Lounge Light, Lounge Dark; 1280x800: Editorial): the Community tab of a server's settings
 * (the "Default channels" multi-select, two chips chosen) and the Emoji & stickers tab (a FileDrop). Measured: the
 * control's box, each chip's remove button's box, the hit test (above and below the centre by 21 px; left and right
 * too, for the chip's button, which is narrower than 44), the FileDrop button and its card, the sideways scroll. Then,
 * at 375 and not in the base case: a touch on the second chip's remove button 20 px above its centre removes that chip
 * only, and a touch on the chevron opens the list. Shots and measure.jsonl go to SR_P86_KIT_OUT. No assertions.
 */
const ON = process.env.SR_P86_KIT_ON === '1';
const BASE = process.env.SR_P86_KIT_BASE === '1';
const PRE = BASE ? 'base-' : '';
const OUT =
  process.env.SR_P86_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p86-kit';
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

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };

const log = (id: string, line: Record<string, unknown>) => {
  mkdirSync(OUT, { recursive: true });
  const row = { id: `${PRE}${id}`, ...line };
  appendFileSync(resolve(OUT, `${PRE}measure.jsonl`), `${JSON.stringify(row)}\n`);
  console.log(`REPORT ${JSON.stringify(row).slice(0, 900)}`);
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

async function newPage(browser: Browser, size: Size, theme: string) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: OWNER_STATE,
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
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

const idle = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
  await page.waitForTimeout(600);
};
const shot = (page: Page, name: string) =>
  page.screenshot({ path: resolve(OUT, `${PRE}${name}.png`), timeout: 10_000 });

/** Boxes and hit tests, run in the page. */
function measure(kind: string) {
  const r2 = (r: DOMRect) => [
    Math.round(r.left * 10) / 10,
    Math.round(r.top * 10) / 10,
    Math.round(r.width * 10) / 10,
    Math.round(r.height * 10) / 10,
  ];
  const hit = (el: Element, dx: number, dy: number) => {
    const b = el.getBoundingClientRect();
    const t = document.elementFromPoint(b.left + b.width / 2 + dx, b.top + b.height / 2 + dy);
    return (
      !!t &&
      (t === el ||
        el.contains(t) ||
        (!!el.closest('label') && t.closest('label') === el.closest('label')))
    );
  };
  const four = (el: Element) => ({
    up: hit(el, 0, -21),
    down: hit(el, 0, 21),
    left: hit(el, -21, 0),
    right: hit(el, 21, 0),
  });
  const scroll = [document.documentElement.scrollWidth, document.documentElement.clientWidth];
  const theme = document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null;
  if (kind === 'multi') {
    const first = document.querySelector('button[aria-label^="Remove "]');
    const control = first?.closest('[data-open], [class*="control"]') as HTMLElement | null;
    const chips = Array.from(
      control?.querySelectorAll<HTMLElement>('button[aria-label^="Remove "]') ?? [],
    );
    const chevron = control?.querySelector('button[class*="chevron"]') as HTMLElement | null;
    return {
      theme,
      control: control ? r2(control.getBoundingClientRect()) : null,
      chips: chips.map((c) => ({
        label: c.getAttribute('aria-label'),
        chip: r2(c.parentElement!.getBoundingClientRect()),
        box: r2(c.getBoundingClientRect()),
        hit: four(c),
      })),
      chevron: chevron ? { box: r2(chevron.getBoundingClientRect()), hit: four(chevron) } : null,
      scroll,
    };
  }
  const btn = Array.from(document.querySelectorAll<HTMLElement>('button')).find(
    (b) => b.nextElementSibling?.getAttribute('aria-hidden') === 'true',
  );
  const drop = btn?.parentElement;
  return {
    theme,
    button: btn ? { box: r2(btn.getBoundingClientRect()), hit: four(btn) } : null,
    drop: drop ? r2(drop.getBoundingClientRect()) : null,
    scroll,
  };
}

test('p86 kit: FileDrop button and multi-select chip remove', async ({ browser }) => {
  const w = world();
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const cases: { size: Size; theme: string; tag: string }[] = [
    { size: 'phone', theme: 'lounge-light', tag: '375-light' },
    { size: 'phone', theme: 'lounge-dark', tag: '375-dark' },
    { size: 'desktop', theme: 'lounge-editorial', tag: '1280-editorial' },
  ];
  for (const c of cases) {
    await setMain(owner, c.theme);
    const { ctx, page } = await newPage(browser, c.size, c.theme);
    const phone = c.size === 'phone';
    // The multi-select.
    await page.goto(`/settings/server/${w.guildId}/community`, { waitUntil: 'domcontentloaded' });
    await idle(page);
    await page.getByRole('tab', { name: 'Onboarding' }).click();
    const field = page.locator('[aria-label="Default channels"]').first();
    await field.scrollIntoViewIfNeeded();
    await field.click();
    const opts = page.getByRole('option');
    console.log(`REPORT options: ${await opts.count()}`);
    await opts.nth(0).click();
    await opts.nth(1).click();
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);
    const m = (await page.evaluate(measure, 'multi')) as { chips: unknown[] };
    log(`${c.tag}-multi`, m);
    await shot(page, `multi-${c.tag}`);
    if (phone && !BASE && m.chips.length >= 2) {
      const second = page.locator('button[aria-label^="Remove "]').nth(1);
      const bb = (await second.boundingBox())!;
      const label = await second.getAttribute('aria-label');
      await page.touchscreen.tap(bb.x + bb.width / 2, bb.y + bb.height / 2 - 20);
      await page.waitForTimeout(400);
      const left = await page
        .locator('button[aria-label^="Remove "]')
        .evaluateAll((els) => els.map((e) => e.getAttribute('aria-label')));
      log(`${c.tag}-tap-second`, { tapped: label, remaining: left });
      const chev = page.locator('button[class*="chevron"]').first();
      const cb = (await chev.boundingBox())!;
      await page.touchscreen.tap(cb.x + cb.width / 2, cb.y + cb.height / 2);
      await page.waitForTimeout(400);
      log(`${c.tag}-tap-chevron`, { listOpen: await page.getByRole('listbox').count() });
      await shot(page, `multi-${c.tag}-after-taps`);
    }
    // The FileDrop.
    await page.goto(`/settings/server/${w.guildId}/emoji`, { waitUntil: 'domcontentloaded' });
    await idle(page);
    log(`${c.tag}-filedrop`, await page.evaluate(measure, 'drop'));
    await page
      .locator('button', { hasText: /upload/i })
      .first()
      .scrollIntoViewIfNeeded()
      .catch(() => undefined);
    await shot(page, `filedrop-${c.tag}`);
    await ctx.close();
  }
  await owner.close();
});
