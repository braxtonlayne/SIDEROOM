import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';

/**
 * SEC-AUD-382 (wave 89, D-895): on a profile's guestbook form the label and the disabled "Sign" button take the
 * profile's own colours. NEEDS THE SWEEP'S SEEDED WORLD: the Owner of `e2e/.state/sweep/owner.json` and
 * `world.json`, made on the same database by `page-sweep.spec.ts -g "seed the sweep world"` (this spec never edits
 * that spec). The Owner's page there has a guestbook block.
 *
 * Per setting (375x812 touch Lounge Light, 375x812 touch Lounge Dark, 1280x800 Editorial; the theme is set with the
 * account's settings (PATCH) and localStorage, `data-theme` is read back): `/u/owner`, the guestbook form's label and
 * disabled button (ratio of the text on its drawn ground), the DOM path from `form.pp-sign`, the form's boxes, then a
 * note typed (not sent) and the enabled button's computed colours. At 375 Lounge Light also the signed-out `/~owner`
 * form's computed styles. One JSON line per setting in `<tag>-measure.json`. No assertions.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (Vite 5671, API 8671, metrics 9371):
 *   SR_P89_PUBLIC_ON=1 SR_P89_PUBLIC_TAG=after SR_P89_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p89-public SR_E2E_BASE_URL=http://localhost:5671 npx playwright test tests/p89-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (`SR_P89_PUBLIC_BASE=1` instead of `_ON=1` is the base case: the same run on the code before the change;
 * `SR_P89_PUBLIC_TAG` only names the files: `before` or `after`.)
 */
const ON = process.env.SR_P89_PUBLIC_ON === '1' || process.env.SR_P89_PUBLIC_BASE === '1';
const OUT =
  process.env.SR_P89_PUBLIC_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p89-public';
const TAG =
  process.env.SR_P89_PUBLIC_TAG ?? (process.env.SR_P89_PUBLIC_BASE === '1' ? 'before' : 'run');
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

/** In the page: the guestbook form's parts with colours, ratios, boxes, and the DOM path to label and button. */
function probeGuestbook(root: string): Record<string, unknown> {
  const nums = (c: string) => (c.match(/[\d.]+/g) ?? []).map(Number);
  const f = (x: number) => {
    const y = x / 255;
    return y <= 0.03928 ? y / 12.92 : Math.pow((y + 0.055) / 1.055, 2.4);
  };
  const lum = (v: number[]) => 0.2126 * f(v[0]!) + 0.7152 * f(v[1]!) + 0.0722 * f(v[2]!);
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const b = e.getBoundingClientRect();
    return [r1(b.left), r1(b.top + window.scrollY), r1(b.width), r1(b.height)];
  };
  const path = (form: Element, e: Element) => {
    const parts: string[] = [];
    for (let a: Element | null = e; a && a !== form; a = a.parentElement)
      parts.unshift(`${a.tagName.toLowerCase()}.${a.className}`);
    return parts.join(' > ');
  };
  /** Ratio of the element's text over the ground drawn behind it (layers composited, opacity chain applied). */
  const one = (form: Element, e: Element | null) => {
    if (!e) return null;
    const cs = getComputedStyle(e);
    const fg = nums(cs.color);
    let op = 1;
    for (let a: Element | null = e; a; a = a.parentElement)
      op *= Number(getComputedStyle(a).opacity);
    let base = [255, 255, 255];
    const layers: number[][] = [];
    let first = true;
    let ownBg: number[] | null = null;
    for (let a: Element | null = e; a; a = a.parentElement) {
      const bg = nums(getComputedStyle(a).backgroundColor);
      const al = bg.length > 3 ? bg[3]! : 1;
      if (bg.length >= 3 && al > 0) {
        layers.push([bg[0]!, bg[1]!, bg[2]!, al]);
        if (first) ownBg = [bg[0]!, bg[1]!, bg[2]!, al];
        if (al >= 1) break;
      }
      first = false;
    }
    for (const l of layers.reverse())
      base = [0, 1, 2].map((i) => l[i]! * l[3]! + base[i]! * (1 - l[3]!));
    // Text over the ground, with the opacity chain applied to both (the disabled button's own opacity).
    const fa = (fg.length > 3 ? fg[3]! : 1) * op;
    const page = [255, 255, 255];
    const eff = [0, 1, 2].map((i) => base[i]! * op + page[i]! * (1 - op));
    void ownBg;
    const mix = [0, 1, 2].map((i) => fg[i]! * fa + eff[i]! * (1 - fa));
    const l1 = lum(mix);
    const l2 = lum(eff);
    return {
      text: (e.textContent ?? '').trim(),
      path: path(form, e),
      color: cs.color,
      background: cs.backgroundColor,
      opacity: op,
      border: cs.borderTopColor,
      ground: base.map(Math.round),
      ratio: Math.round(((Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)) * 100) / 100,
      font: cs.fontSize,
      box: box(e),
    };
  };
  const form = document.querySelector(`${root} form.pp-sign`);
  if (!form) return { found: false };
  const btn = form.querySelector('button[type="submit"]') as HTMLButtonElement | null;
  return {
    found: true,
    label: one(form, form.querySelector('label')),
    span: one(form, form.querySelector('label > span')),
    textarea: one(form, form.querySelector('textarea')),
    button: one(form, btn),
    disabled: btn?.disabled,
    form: box(form),
    theme: (document.querySelector('[data-theme]') as HTMLElement | null)?.dataset.theme ?? null,
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
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
      if (!localStorage.getItem('sr.p89.seeded')) {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
        );
        localStorage.setItem('sr.p89.seeded', '1');
      }
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

test('SEC-AUD-382: the guestbook form on the profile, measured and shot', async ({ browser }) => {
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
      line.disabled = await page.evaluate(probeGuestbook, '.pp-root');
      await form.screenshot({ path: resolve(OUT, `${TAG}-form-disabled-${s.key}.png`) });
      // A note typed, not sent: the enabled button.
      await form.locator('textarea').fill('A note, not sent.');
      await page.waitForTimeout(300);
      line.enabled = await page.evaluate(probeGuestbook, '.pp-root');
      if (s.key === 'phone-light') {
        await form.screenshot({ path: resolve(OUT, `${TAG}-form-enabled-${s.key}.png`) });
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
          line.signedOut = await ap.evaluate(probeGuestbook, 'body');
          await af.screenshot({ path: resolve(OUT, `${TAG}-signedout-form-${s.key}.png`) });
          await anon.close();
        } catch (e) {
          line.signedOutError = String(e).slice(0, 200);
        }
      }
      appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
      const d = line.disabled as { label?: { ratio: number }; button?: { ratio: number } };
      console.log(
        `REPORT ${s.key} label ${d.label?.ratio} button(disabled) ${d.button?.ratio} ${String(line.signedOutError ?? '')}`,
      );
      await ctx.close();
    }
  } finally {
    await ownerCtx.close();
  }
});
