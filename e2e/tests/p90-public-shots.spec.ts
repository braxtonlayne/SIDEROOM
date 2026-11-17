import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { expect } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';
import { over, parseColor, ratio, textRatio, type Rgba } from '../support/contrast';

/**
 * SEC-AUD-384 (wave 90, D-897): on a profile's guestbook form the ENABLED "Sign" button takes the profile's own colours.
 * NEEDS THE SWEEP'S SEEDED WORLD: the Owner of `e2e/.state/sweep/owner.json` and `world.json`, made on the same
 * database by `page-sweep.spec.ts -g "seed the sweep world"` (this spec never edits that spec).
 *
 * It begins with a self-check of `e2e/support/contrast.ts` on wave 89's figures (7.70 and 7.72; more than 0.05 off
 * fails the spec before anything is measured). Per setting (375x812 touch Lounge Light, 375x812 touch Lounge Dark,
 * 1280x800 Editorial; theme set by the account's settings (PATCH) and localStorage, `data-theme` read back):
 * `/u/owner`, the disabled button, a note typed (not sent): the enabled button at rest, hovered (1280 only, a mouse),
 * with keyboard focus (Tab from the textarea): computed colour, ground, border, opacity, classes, outline, and every
 * ratio from the helper. At 375 Lounge Light also the signed-out `/~owner` form's computed styles. One JSON line per
 * setting in `<tag>-measure.json`. No assertions but the self-check.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (Vite 5681, API 8681, metrics 9381):
 *   SR_P90_PUBLIC_ON=1 SR_P90_PUBLIC_TAG=after SR_P90_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-public SR_E2E_BASE_URL=http://localhost:5681 npx playwright test tests/p90-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (`SR_P90_PUBLIC_BASE=1` instead of `_ON=1` is the base case: the same run on the code before the change;
 * `SR_P90_PUBLIC_TAG` only names the files: `before` or `after`.)
 */
const ON = process.env.SR_P90_PUBLIC_ON === '1' || process.env.SR_P90_PUBLIC_BASE === '1';
const OUT =
  process.env.SR_P90_PUBLIC_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p90-public';
const TAG =
  process.env.SR_P90_PUBLIC_TAG ?? (process.env.SR_P90_PUBLIC_BASE === '1' ? 'before' : 'run');
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
test.setTimeout(300_000);

interface Raw {
  text: string;
  classes: string;
  color: string;
  /** background colours from the element up to the first opaque one, innermost first */
  layers: string[];
  opacity: number;
  border: string;
  borderWidth: string;
  outlineColor: string;
  outlineWidth: string;
  outlineStyle: string;
  outlineOffset: string;
  box: number[];
  disabled: boolean;
}

/** In the page: raw computed values of the guestbook form's parts (the ratios are taken in Node, by the helper). */
function probeRaw(root: string): Record<string, unknown> {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const b = e.getBoundingClientRect();
    return [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)];
  };
  const one = (e: Element | null) => {
    if (!e) return null;
    const cs = getComputedStyle(e);
    let op = 1;
    for (let a: Element | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    const layers: string[] = [];
    for (let a: Element | null = e; a; a = a.parentElement) {
      const bg = getComputedStyle(a).backgroundColor;
      if (bg !== 'rgba(0, 0, 0, 0)' && bg !== 'transparent') {
        layers.push(bg);
        const m = bg.match(/\/\s*([\d.]+)\s*\)|,\s*([\d.]+)\s*\)$/);
        const al = m ? Number(m[1] ?? m[2]) : 1;
        if (al >= 1) break;
      }
    }
    return {
      text: (e.textContent ?? '').trim(),
      classes: (e as HTMLElement).className,
      color: cs.color,
      layers,
      opacity: op,
      border: cs.borderTopColor,
      borderWidth: cs.borderTopWidth,
      outlineColor: cs.outlineColor,
      outlineWidth: cs.outlineWidth,
      outlineStyle: cs.outlineStyle,
      outlineOffset: cs.outlineOffset,
      box: box(e),
      disabled: (e as HTMLButtonElement).disabled,
    } satisfies Raw;
  };
  const form = document.querySelector(`${root} form.pp-sign`);
  if (!form) return { found: false };
  return {
    found: true,
    label: one(form.querySelector('label')),
    span: one(form.querySelector('label > span')),
    button: one(form.querySelector('button[type="submit"]')),
    form: box(form),
    // The panel the form sits on: the first opaque ancestor of the form.
    panel: one(form.parentElement),
    theme: (document.querySelector('[data-theme]') as HTMLElement | null)?.dataset.theme ?? null,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
  };
}

function ground(layers: string[]): Rgba {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const l of [...layers].reverse()) g = over(parseColor(l), g);
  return g;
}

/** Add the helper's figures to a raw probe: text on its own ground, and the focus ring against the panel. */
function figures(raw: Record<string, unknown>) {
  const out: Record<string, unknown> = { ...raw };
  const panel = raw.panel as Raw | null;
  const panelGround = panel ? ground(panel.layers) : null;
  for (const k of ['label', 'span', 'button'] as const) {
    const e = raw[k] as Raw | null;
    if (!e) continue;
    const g = ground(e.layers);
    // The element's own opacity (the disabled kit button is 0.5) lays text and ground over the page's white.
    const eff = e.opacity < 1 ? over({ ...g, a: e.opacity }, { r: 255, g: 255, b: 255, a: 1 }) : g;
    const text = parseColor(e.color);
    const t = over({ ...text, a: text.a * e.opacity }, eff);
    const fig: Record<string, unknown> = {
      ...e,
      ground: [g.r, g.g, g.b].map(Math.round),
      ratio: Math.round(ratio(t, eff) * 100) / 100,
    };
    if (k === 'button' && e.outlineStyle !== 'none' && panelGround) {
      const ring = over(parseColor(e.outlineColor), panelGround);
      fig.ringOnPanel = Math.round(ratio(ring, panelGround) * 100) / 100;
      fig.ringOnButton = Math.round(ratio(ring, eff) * 100) / 100;
    }
    if (k === 'button' && panelGround)
      fig.groundOnPanel = Math.round(ratio(eff, panelGround) * 100) / 100;
    out[k] = fig;
  }
  return out;
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
      if (!localStorage.getItem('sr.p90.seeded')) {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
        );
        localStorage.setItem('sr.p90.seeded', '1');
      }
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

async function probe(page: Page, root = '.pp-root') {
  return figures((await page.evaluate(probeRaw, root)) as Record<string, unknown>);
}

test('the contrast helper reads wave 89 figures', () => {
  // 75% of rgb(242,243,245) over rgb(43,45,49), against rgb(43,45,49): 7.70.
  const label = textRatio('color(srgb 0.949 0.953 0.961 / 0.75)', 'rgb(43, 45, 49)');
  // rgb(242,243,245) on 45% of rgb(88,101,242) over rgb(43,45,49): 7.72.
  const g = over(parseColor('color(srgb 0.345 0.396 0.949 / 0.45)'), parseColor('rgb(43,45,49)'));
  const btn = ratio(parseColor('rgb(242, 243, 245)'), g);
  console.log(
    `REPORT helper self-check: label ${label.toFixed(2)} (7.70), button ${btn.toFixed(2)} (7.72)`,
  );
  expect(Math.abs(label - 7.7)).toBeLessThanOrEqual(0.05);
  expect(Math.abs(btn - 7.72)).toBeLessThanOrEqual(0.05);
  expect(() => parseColor('hsl(10 10% 10%)')).toThrow();
});

test('SEC-AUD-384: the guestbook form on the profile, measured and shot', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}-measure.json`);
  writeFileSync(jsonl, '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  try {
    for (const s of SETTINGS) {
      await apiPatch(o, '/users/@me/settings', {
        settings: { user_prefs: { theme: s.theme, zoom: 100 } },
      });
      const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
        settings?: { user_prefs?: { theme?: string } };
      };
      console.log(`REPORT ${s.key} account theme read back: ${back.settings?.user_prefs?.theme}`);
      const { ctx, page } = await newPage(browser, s, ownerCtx);
      const line: Record<string, unknown> = { setting: s.key, want: s.theme };
      await page.goto('/u/owner', { waitUntil: 'domcontentloaded', timeout: 20_000 });
      const form = page.locator('.pp-root form.pp-sign').first();
      await form.waitFor({ timeout: 15_000 });
      await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
      await form.scrollIntoViewIfNeeded();
      await page.waitForTimeout(500);
      line.disabled = await probe(page);
      if (s.key === 'desk-editorial')
        await form.screenshot({ path: resolve(OUT, `${TAG}-form-disabled-${s.key}.png`) });
      // A note typed, not sent: the enabled button at rest.
      await form.locator('textarea').fill('A note, not sent.');
      await page.waitForTimeout(300);
      line.enabled = await probe(page);
      await form.screenshot({ path: resolve(OUT, `${TAG}-form-enabled-${s.key}.png`) });
      const btn = form.locator('button[type="submit"]');
      if (!s.phone) {
        await btn.hover();
        await page.waitForTimeout(400);
        line.hover = await probe(page);
        await page.mouse.move(0, 0);
      }
      // Keyboard focus: Tab from the textarea.
      await form.locator('textarea').focus();
      await page.keyboard.press('Tab');
      await page.waitForTimeout(300);
      line.focus = await probe(page);
      line.focusIsButton = await page.evaluate(
        () => document.activeElement?.matches('form.pp-sign button[type="submit"]') ?? false,
      );
      if (!s.phone) await form.screenshot({ path: resolve(OUT, `${TAG}-form-focus-${s.key}.png`) });
      if (s.key === 'phone-light') {
        try {
          const anon = await browser.newContext({
            baseURL: ORIGIN,
            viewport: s.size,
            userAgent: PHONE_UA,
            isMobile: true,
            hasTouch: true,
          });
          const ap = await anon.newPage();
          ap.setDefaultTimeout(10_000);
          await ap.goto('/~owner', { waitUntil: 'domcontentloaded', timeout: 20_000 });
          const af = ap.locator('form.pp-sign').first();
          await af.waitFor({ timeout: 15_000 });
          await af.scrollIntoViewIfNeeded();
          await ap.waitForTimeout(600);
          line.signedOut = await probe(ap, 'body');
          await af.screenshot({ path: resolve(OUT, `${TAG}-signedout-form-${s.key}.png`) });
          await anon.close();
        } catch (e) {
          line.signedOutError = String(e).slice(0, 200);
        }
      }
      appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
      const rr = (k: string) =>
        (line[k] as { button?: { ratio: number } } | undefined)?.button?.ratio;
      console.log(
        `REPORT ${s.key} button ratio: disabled ${rr('disabled')} enabled ${rr('enabled')} hover ${rr('hover')} focus ${rr('focus')} ${String(line.signedOutError ?? '')}`,
      );
      await ctx.close();
    }
  } finally {
    await ownerCtx.close();
  }
});
