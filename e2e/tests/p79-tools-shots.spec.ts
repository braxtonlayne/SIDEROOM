import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-312 (wave 79; D-814), on the real stack: the tools drawer's own parts (`utl-dev.css`) follow the theme.
 * For every tool that draws `.ud-*` parts, under Lounge Light and Lounge Dark (desktop 1280x800 and phone
 * 375x812; the account's main theme set by the settings PATCH and localStorage, read back from `data-theme`),
 * and Editorial (desktop) for three of them, the spec reads the computed look of the tool's fields and output
 * against a probe carrying the kit's tokens (`--ui-bg-raised`, `--ui-border`, `--ui-text`), the text-on-field
 * contrast, and takes a shot of the drawer. A value is typed or a timer started in some so result fields show.
 * Local only (SR_P79_TOOLS=1); shots to SR_P79_TOOLS_OUT. SR_P79_BASE=1: the base case (log, shots, no assertion
 * on the look; run it with the change held back).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_TOOLS_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-tools';
const BASE = process.env.SR_P79_BASE === '1';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };

test.skip(process.env.SR_P79_TOOLS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };
const log = (line: string) => console.log(`REPORT ${line}`);

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

async function openTool(page: Page, query: string, ready: string) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.keyboard.press('Enter');
  await expect(page.locator(ready).first()).toBeVisible({ timeout: 15_000 });
}

/** Colours of the tool's own parts, the probe's tokens, and the drawer's background. */
async function look(page: Page) {
  return page.evaluate(() => {
    const root = document.querySelector('.ud-root, .ud-timer') as HTMLElement;
    const field = root.querySelector(
      'input:not([type=file]):not([type=range]), textarea, select',
    ) as HTMLElement | null;
    const out = root.querySelector('.ud-out, .ud-history') as HTMLElement | null;
    const btn = root.querySelector('.ud-btn') as HTMLElement | null;
    const probe = document.createElement('span');
    probe.style.cssText =
      'position:absolute;visibility:hidden;background:var(--ui-bg-raised);color:var(--ui-text);border:1px solid var(--ui-border);border-radius:var(--ui-radius)';
    root.appendChild(probe);
    const want = getComputedStyle(probe);
    const wantBg = want.backgroundColor;
    const wantText = want.color;
    const wantBorder = want.borderTopColor;
    const wantRadius = want.borderTopLeftRadius;
    probe.style.background = 'var(--ui-bg-hover)';
    const wantBtn = getComputedStyle(probe).backgroundColor;
    probe.remove();
    // The nearest opaque background above the tool.
    let bgEl: HTMLElement | null = root;
    let drawerBg = 'rgba(0, 0, 0, 0)';
    while (bgEl) {
      const c = getComputedStyle(bgEl).backgroundColor;
      if (!/rgba\(.*, 0\)$|transparent/.test(c)) {
        drawerBg = c;
        break;
      }
      bgEl = bgEl.parentElement;
    }
    const cs = (e: HTMLElement | null) => (e ? getComputedStyle(e) : null);
    return {
      theme: document.querySelector('.lng-root')?.getAttribute('data-theme') ?? null,
      drawerBg,
      rootText: getComputedStyle(root).color,
      fieldBg: cs(field)?.backgroundColor ?? null,
      fieldText: cs(field)?.color ?? null,
      fieldBorder: cs(field)?.borderTopColor ?? null,
      fieldRadius: cs(field)?.borderTopLeftRadius ?? null,
      outBg: cs(out)?.backgroundColor ?? null,
      btnBg: cs(btn)?.backgroundColor ?? null,
      wantBg,
      wantText,
      wantBorder,
      wantRadius,
      wantBtn,
      scrollOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  });
}

function rgb(c: string): [number, number, number] {
  const m = c.match(/[\d.]+/g)!.map(Number);
  return [m[0] ?? 0, m[1] ?? 0, m[2] ?? 0];
}
function lum([r, g, b]: [number, number, number]) {
  const f = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}
function contrast(a: string, b: string) {
  const [x = 0, y = 0] = [lum(rgb(a)), lum(rgb(b))].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

type Tool = {
  slug: string;
  query: string;
  ready?: string;
  act?: (page: Page) => Promise<void>;
  /** also shot in Editorial (desktop) */
  editorial?: boolean;
};

const fillFirst = (text: string) => async (page: Page) => {
  const ta = page
    .locator('.ud-root textarea, .ud-root input[type=text], .ud-root input:not([type])')
    .first();
  if (await ta.count()) await ta.fill(text);
};

const TOOLS: Tool[] = [
  { slug: 'json', query: 'json', act: fillFirst('{"a":1,"b":[1,2,3]}') },
  { slug: 'regex', query: 'regex tester', act: fillFirst('a+b') },
  { slug: 'base64', query: 'base64', act: fillFirst('hello world') },
  { slug: 'url', query: 'url encode', act: fillFirst('a b&c=d/é') },
  { slug: 'jwt', query: 'jwt decoder', act: fillFirst('eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig') },
  { slug: 'mdtable', query: 'markdown table', act: fillFirst('a,b\n1,2') },
  { slug: 'counter', query: 'word counter', act: fillFirst('one two three four') },
  {
    slug: 'hash',
    query: 'hashes',
    editorial: true,
    act: async (page) => {
      await page
        .locator('input[type=file]')
        .first()
        .setInputFiles({ name: 'a.txt', mimeType: 'text/plain', buffer: Buffer.from('hello') });
      await page.waitForTimeout(600);
    },
  },
  { slug: 'diff', query: 'diff', act: fillFirst('one\ntwo') },
  {
    slug: 'dice',
    query: 'dice',
    act: async (p) => {
      await p
        .locator('.ud-root button.ud-btn-primary')
        .first()
        .click()
        .catch(() => undefined);
    },
  },
  {
    slug: 'coin',
    query: 'coin flip',
    act: async (p) => {
      await p
        .locator('.ud-root button')
        .first()
        .click()
        .catch(() => undefined);
    },
  },
  { slug: 'wheel', query: 'wheel' },
  { slug: 'loot', query: 'loot tables' },
  { slug: 'notes', query: 'private notes', editorial: true },
  {
    slug: 'pomodoro',
    query: 'pomodoro',
    ready: '.ud-timer',
    editorial: true,
    act: async (p) => {
      await p.locator('.ud-timer button.ud-btn-primary').first().click();
      await p.waitForTimeout(1500);
    },
  },
  { slug: 'stopwatch', query: 'stopwatch', ready: '.ud-timer' },
  { slug: 'countdown', query: 'countdown', ready: '.ud-timer' },
];

test('the tools drawer follows the theme', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  let shots = 0;
  let probed = 0;

  const variants: Array<[string, string, { width: number; height: number }]> = [
    ['lounge-light', 'desktop', DESKTOP],
    ['lounge-dark', 'desktop', DESKTOP],
    ['lounge-light', 'phone', PHONE],
    ['lounge-dark', 'phone', PHONE],
    ['lounge-editorial', 'desktop', DESKTOP],
  ];
  for (const [theme, vp, size] of variants) {
    const prefs: Prefs = { theme, zoom: 100 };
    await apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: prefs } });
    const c = await context(browser, octx, size, prefs);
    const page = await c.newPage();
    page.setDefaultTimeout(20_000);
    for (const tool of TOOLS) {
      if (theme === 'lounge-editorial' && !tool.editorial) continue;
      const tag = `${tool.slug}-${theme}-${vp}`;
      try {
        await page.goto('/channels/@me');
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        const root = await page.locator('.lng-root').first().getAttribute('data-theme');
        expect(root, 'the room wears the chosen theme').toBe(theme);
        await openTool(page, tool.query, tool.ready ?? '.ud-root, .ud-timer');
        if (tool.act) await tool.act(page);
        await page.waitForTimeout(300);
        const l = await look(page);
        probed++;
        log(`${tag} ${JSON.stringify(l)}`);
        if (l.fieldBg && l.fieldText) {
          const ratio = contrast(l.fieldText, l.fieldBg);
          log(`${tag} text-on-field ${ratio.toFixed(2)}`);
          if (!BASE) {
            expect.soft(l.fieldBg, `${tag}: field background`).toBe(l.wantBg);
            expect.soft(l.fieldBorder, `${tag}: field border`).toBe(l.wantBorder);
            expect.soft(l.fieldRadius, `${tag}: field radius`).toBe(l.wantRadius);
            expect.soft(ratio, `${tag}: text on field`).toBeGreaterThanOrEqual(4.5);
          }
        }
        if (l.btnBg && !BASE) expect.soft(l.btnBg, `${tag}: button background`).toBe(l.wantBtn);
        if (!BASE) {
          expect
            .soft(contrast(l.rootText, l.drawerBg), `${tag}: tool text on the drawer`)
            .toBeGreaterThanOrEqual(4.5);
        }
        expect.soft(l.scrollOver, `${tag}: no sideways scroll`).toBeLessThanOrEqual(0);
        await page.screenshot({ path: resolve(OUT, `${tag}.png`) });
        shots++;
      } catch (e) {
        log(`UNREACHED ${tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      }
    }
    // Click through: switch between three tools in one open drawer.
    if (vp === 'desktop' && theme !== 'lounge-editorial') {
      try {
        await page.goto('/channels/@me');
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        await openTool(page, 'json', '.ud-root');
        for (const q of ['hashes', 'dice', 'stopwatch']) {
          await page.keyboard.press('ControlOrMeta+k');
          await page.keyboard.type(`>${q}`);
          await page.waitForTimeout(600);
          await page.keyboard.press('Enter');
          await expect(page.locator('.ud-root, .ud-timer').first()).toBeVisible();
        }
        const start = page.locator('.ud-timer button.ud-btn-primary').first();
        await start.click();
        await page.waitForTimeout(1200);
        await start.click();
        log(`switch-${theme} ok`);
      } catch (e) {
        log(
          `UNREACHED switch-${theme}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`,
        );
      }
    }
    await c.close();
  }
  log(`done: ${probed} probed, ${shots} shots`);
  expect(shots).toBeGreaterThan(0);
});
