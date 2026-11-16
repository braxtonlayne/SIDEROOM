import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPut, test } from '../support/fixtures';

/**
 * SEC-AUD-364 (wave 87, D-886): a profile page's block headings (`h3.pp-title`) read at least 4.5:1. NEEDS THE SWEEP'S
 * SEEDED WORLD: the Owner of `e2e/.state/sweep/owner.json` and `world.json`, made on the same database by
 * `page-sweep.spec.ts -g "seed the sweep world"` (this spec never edits that spec). The Owner's page there has blocks
 * with headings (About, Links...); if it has none this spec puts an About block on it through the API (D-863).
 *
 * Per setting (1280x800 Editorial, 375x812 touch Lounge Dark, 375x812 touch Lounge Light; the theme is set with the
 * account's settings (PATCH) and localStorage, `data-theme` is read back): the page `/u/owner` (every `.pp-title`: its
 * computed colour, the ground behind it, the ratio, the rule that sets the colour, its box; every `.pp-block` box for
 * "0 px moved"), the Settings preview (Profile tab) with the headings it has, and at 1280 the profile pop-up from the
 * Member List (the headings it shows, if any). With `SR_P87_PUBLIC_ACCENT=1` the Owner's document first gets another
 * accent (`#e91e63`) and gets the default back at the end. One shot per surface. One JSON line per setting in
 * `<tag>-measure.json`. No assertions.
 *
 * Local only. The commander's command, from `e2e/`, on a stack with the seeded world (Vite on 5651, API on 8651):
 *   SR_P87_PUBLIC_ON=1 SR_P87_PUBLIC_TAG=after SR_P87_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-public SR_E2E_BASE_URL=http://localhost:5651 npx playwright test tests/p87-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * (`SR_P87_PUBLIC_BASE=1` instead of `_ON=1` is the base case: the same run on the code before the change;
 * `SR_P87_PUBLIC_TAG` only names the files: `before` or `after`; add `SR_P87_PUBLIC_ACCENT=1` for the other accent.)
 */
const ON = process.env.SR_P87_PUBLIC_ON === '1' || process.env.SR_P87_PUBLIC_BASE === '1';
const OUT =
  process.env.SR_P87_PUBLIC_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-public';
const TAG =
  process.env.SR_P87_PUBLIC_TAG ?? (process.env.SR_P87_PUBLIC_BASE === '1' ? 'before' : 'run');
/** The Settings preview of the page (Profile tab): the region that shows the page as a chosen viewer sees it. */
const PREVIEW = '[role="region"][aria-label="Your page as that viewer sees it"]';
const OTHER_ACCENT = process.env.SR_P87_PUBLIC_ACCENT === '1';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const WORLD = resolve(repoRoot, 'e2e', '.state', 'sweep', 'world.json');
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
  { key: 'phone-dark', theme: 'lounge-dark', size: { width: 375, height: 812 }, phone: true },
  { key: 'phone-light', theme: 'lounge-light', size: { width: 375, height: 812 }, phone: true },
];

test.skip(!ON, 'local visual check only');
test.setTimeout(600_000);

/** In the page: every heading under `root` with colour, ground, ratio, the winning rule, and the boxes. */
function probeInPage(rootSel: string): Record<string, unknown> {
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const box = (e: Element) => {
    const b = e.getBoundingClientRect();
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
  /** The colour a rule gives: the matching selector with the highest specificity that sets `color` (best effort). */
  const source = (e: Element) => {
    const hits: string[] = [];
    for (const sh of Array.from(document.styleSheets)) {
      let rules: CSSRuleList;
      try {
        rules = sh.cssRules;
      } catch {
        continue;
      }
      const walk = (list: CSSRuleList) => {
        for (const r of Array.from(list)) {
          const st = r as CSSStyleRule;
          if (st.selectorText && st.style?.color) {
            try {
              if (e.matches(st.selectorText))
                hits.push(`${st.selectorText} { color: ${st.style.color} }`);
            } catch {
              /* a selector the browser cannot match */
            }
          }
          const g = r as CSSGroupingRule;
          if (g.cssRules) walk(g.cssRules);
        }
      };
      walk(rules);
    }
    const inline = (e as HTMLElement).style?.color;
    return { inline: inline || null, rules: hits.slice(-3) };
  };
  const heading = (e: HTMLElement) => {
    const cs = getComputedStyle(e);
    const fg = parse(cs.color);
    const layers: { r: number; g: number; b: number; a: number }[] = [];
    let groundEl = '';
    for (let a: HTMLElement | null = e; a; a = a.parentElement) {
      const bg = parse(getComputedStyle(a).backgroundColor);
      if (bg && bg.a > 0) {
        if (!layers.length) groundEl = `${a.tagName.toLowerCase()}.${a.className}`;
        layers.push(bg);
      }
      if (bg && bg.a >= 1) break;
    }
    let base = { r: 255, g: 255, b: 255 };
    for (const l of layers.reverse())
      base = {
        r: l.r * l.a + base.r * (1 - l.a),
        g: l.g * l.a + base.g * (1 - l.a),
        b: l.b * l.a + base.b * (1 - l.a),
      };
    const mix = fg
      ? {
          r: fg.r * fg.a + base.r * (1 - fg.a),
          g: fg.g * fg.a + base.g * (1 - fg.a),
          b: fg.b * fg.a + base.b * (1 - fg.a),
        }
      : base;
    const l1 = lum(mix);
    const l2 = lum(base);
    return {
      text: (e.textContent ?? '').trim(),
      color: cs.color,
      ground: `rgb(${Math.round(base.r)},${Math.round(base.g)},${Math.round(base.b)})`,
      groundEl,
      ratio: Math.round(((Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05)) * 100) / 100,
      size: cs.fontSize,
      box: box(e),
      source: source(e),
    };
  };
  const root = document.querySelector(rootSel);
  if (!root) return { found: false };
  const heads = Array.from(root.querySelectorAll<HTMLElement>('.pp-title, h3')).filter(
    (h) => h.matches('.pp-title') || false,
  );
  return {
    found: true,
    headings: heads.map(heading),
    blocks: Array.from(root.querySelectorAll('.pp-block')).map(box),
    doc: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    theme: (document.querySelector('[data-theme]') as HTMLElement | null)?.dataset.theme ?? null,
    accentVar:
      (root.closest('.pp-root') ?? root.querySelector('.pp-root'))
        ? getComputedStyle(
            (root.closest('.pp-root') ?? root.querySelector('.pp-root')) as Element,
          ).getPropertyValue('--pp-accent')
        : null,
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
      if (!localStorage.getItem('sr.p87.seeded')) {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
        );
        localStorage.setItem('sr.p87.seeded', '1');
      }
    } catch {
      /* no storage */
    }
  }, s.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(10_000);
  return { ctx, page };
}

test('SEC-AUD-364: the profile page headings, measured and shot', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, `${TAG}${OTHER_ACCENT ? '-accent' : ''}-measure.json`);
  writeFileSync(jsonl, '');
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const w = JSON.parse(readFileSync(WORLD, 'utf8')) as { guildId: string; channelId: string };
  type Doc = { version: 1; style: { palette: Record<string, string> }; blocks: { type: string }[] };
  const raw = await o.get('/api/v1/users/@me/profile-doc');
  const doc = (await raw.json()) as Doc;
  const original = JSON.parse(JSON.stringify(doc)) as Doc;
  if (!doc.blocks.some((b) => b.type !== 'header' && b.type !== 'guestbook')) {
    (doc.blocks as unknown[]).push({
      id: 'p87-about',
      type: 'about',
      x: 0,
      y: 20,
      w: 12,
      h: 4,
      config: {},
    });
  }
  if (OTHER_ACCENT) doc.style.palette.accent = '#e91e63';
  await apiPut(o, '/users/@me/profile-doc', doc);
  console.log(
    `REPORT blocks: ${doc.blocks.map((b) => b.type).join(',')} accent ${doc.style.palette.accent}`,
  );
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
      const shot = async (name: string) => {
        const file = `${TAG}${OTHER_ACCENT ? '-accent' : ''}-${name}-${s.key}.png`;
        await page.screenshot({ path: resolve(OUT, file), timeout: 10_000 });
        return file;
      };
      // 1. The page.
      await page.goto('/u/owner', { waitUntil: 'domcontentloaded', timeout: 20_000 });
      await page.locator('.pp-root .pp-title').first().waitFor({ timeout: 15_000 });
      await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
      await page.waitForTimeout(500);
      line.page = await page.evaluate(probeInPage, '.pp-root');
      line.pageShot = await shot('page');
      if (!OTHER_ACCENT) {
        // 2. The Settings preview (Profile tab).
        try {
          await page.goto('/settings/user/account', {
            waitUntil: 'domcontentloaded',
            timeout: 20_000,
          });
          const btns = page.locator('[role="dialog"] nav ul li > button');
          await btns.first().waitFor({ state: 'attached', timeout: 15_000 });
          const n = await btns.count();
          for (let i = 0; i < n; i++) {
            const label = ((await btns.nth(i).textContent()) ?? '').trim();
            if (/^profile/i.test(label)) {
              await btns.nth(i).evaluate((el) => (el as HTMLElement).click());
              break;
            }
          }
          await page.locator(PREVIEW).first().waitFor({ timeout: 10_000 });
          await page.waitForTimeout(800);
          line.preview = await page.evaluate(probeInPage, PREVIEW);
          await page.locator(PREVIEW).first().scrollIntoViewIfNeeded();
          line.previewShot = await shot('preview');
        } catch (e) {
          line.previewError = String(e).slice(0, 200);
        }
        // 3. The pop-up from the Member List (desktop only), until one shows a page block heading.
        if (!s.phone) {
          try {
            await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
              waitUntil: 'domcontentloaded',
              timeout: 20_000,
            });
            const b = page.getByRole('button', { name: /Member List/ }).first();
            await b.waitFor({ timeout: 15_000 });
            if ((await b.getAttribute('aria-pressed')) !== 'true') await b.click();
            const rows = page.locator('.lng-right .lng-member');
            await rows.first().waitFor({ timeout: 10_000 });
            const cnt = Math.min(await rows.count(), 6);
            const pops: unknown[] = [];
            for (let i = 0; i < cnt; i++) {
              await rows.nth(i).click({ timeout: 5000, force: true });
              await page.waitForTimeout(900);
              const has = (await page.locator('.lng-pop .pp-title').count()) > 0;
              pops.push({ row: i, hasHeading: has });
              if (has) {
                line.popup = await page.evaluate(probeInPage, '.lng-pop');
                line.popupShot = await shot('popup');
                break;
              }
              await page.keyboard.press('Escape');
              await page.waitForTimeout(300);
            }
            line.popTried = pops;
          } catch (e) {
            line.popupError = String(e).slice(0, 200);
          }
        }
      }
      appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
      const hs = (x: unknown) =>
        (
          (x as { headings?: { text: string; ratio: number; color: string }[] } | undefined)
            ?.headings ?? []
        )
          .map((h) => `${h.text}=${h.ratio}(${h.color})`)
          .join(' ');
      console.log(`REPORT ${s.key} page: ${hs(line.page)}`);
      console.log(
        `REPORT ${s.key} preview: ${hs(line.preview)} ${String(line.previewError ?? '')}`,
      );
      console.log(
        `REPORT ${s.key} popup: ${hs(line.popup)} ${JSON.stringify(line.popTried ?? '')} ${String(line.popupError ?? '')}`,
      );
      await ctx.close();
    }
  } finally {
    await apiPut(o, '/users/@me/profile-doc', original);
    await ownerCtx.close();
  }
});
