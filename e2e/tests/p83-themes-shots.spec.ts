import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * SEC-AUD-345 (wave 83; D-852): Support, the Leaderboards and the invite and template pages opened inside the app
 * follow the member's main theme. The Owner on the visual sweep's seeded world (`e2e/.state/sweep/world.json`,
 * made by `page-sweep.spec.ts`'s "seed the sweep world"). For each main theme (Editorial, Lounge Light, Lounge
 * Dark; set with PATCH /users/@me/settings and localStorage, read back) and each size (1280x800, 375x812) and each
 * of the four pages one JSON line goes to `measure.jsonl`: the page root's `data-theme`, the root's and the main
 * blocks' boxes, the root's computed background, whether the ground reaches the bottom of the viewport, the
 * document's `scrollWidth - clientWidth`, and the colour of a few texts against the ground behind them; plus a shot
 * of the viewport. SR_P83_BASE=1 records the base (no assertion, out folder `<OUT>-base`); otherwise the data-theme
 * must equal the main theme, and the Editorial boxes are compared with the base's file (at most 2 px apart except
 * the root's height). A last test clicks through in Lounge Light: Lounge, Support, Leaderboards, back, an invite.
 *
 * Local only (SR_P83_THEMES=1); shots go to SR_P83_THEMES_OUT.
 */
const ON = process.env.SR_P83_THEMES === '1';
const BASE = process.env.SR_P83_BASE === '1';
const OUT_AFTER =
  process.env.SR_P83_THEMES_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-themes';
const OUT = BASE ? `${OUT_AFTER}-base` : OUT_AFTER;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  inviteCode: string | null;
}
const loadWorld = (): World => {
  if (!existsSync(WORLD_FILE)) throw new Error('no sweep world: run the sweep seed first');
  return JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
};

const PAGES: { id: string; path: (w: World) => string | null; root: string }[] = [
  { id: 'support', path: () => '/support', root: 'main.sup-page' },
  { id: 'leaderboards', path: () => '/support/leaderboards', root: 'main.sup-page' },
  {
    id: 'invite-app',
    path: (w) => (w.inviteCode ? `/invite/${w.inviteCode}` : null),
    root: '.ent-invite-page',
  },
  { id: 'template-app', path: () => '/template/abcd1234', root: '.ent-invite-page' },
];

interface Box {
  k: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Line {
  id: string;
  theme: string;
  size: string;
  main: string | null;
  rootTheme: string | null;
  rootBox: Box | null;
  rootBg: string | null;
  bottomReaches: boolean;
  bottomGround: string;
  display: string | null;
  over: number;
  boxes: Box[];
  texts: { k: string; color: string; bg: string; ratio: number }[];
}

/** In the page: everything the shots table needs. */
function measureInPage(rootSel: string): Omit<Line, 'id' | 'theme' | 'size' | 'main'> {
  const all = Array.from(document.querySelectorAll<HTMLElement>(rootSel)).map(
    (e) => e.closest<HTMLElement>('[data-theme]') ?? e,
  );
  const root =
    all.find((e) => e.hasAttribute('data-theme') && e.getBoundingClientRect().width > 0) ??
    all.find((e) => e.getBoundingClientRect().width > 0) ??
    all[0] ??
    null;
  const de = document.documentElement;
  const rgb = (s: string) => (s.match(/[\d.]+/g) ?? []).map(Number);
  const lum = (c: number[]) => {
    const f = (v: number) => {
      const s = v / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    };
    return 0.2126 * f(c[0]!) + 0.7152 * f(c[1]!) + 0.0722 * f(c[2]!);
  };
  const groundOf = (e: Element) => {
    for (let a: Element | null = e; a; a = a.parentElement) {
      const c = rgb(getComputedStyle(a).backgroundColor);
      if (c.length >= 3 && (c[3] ?? 1) > 0.5) return c;
    }
    return [255, 255, 255];
  };
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 2) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const box = (e: Element, k: string): Box => {
    const r = e.getBoundingClientRect();
    return {
      k,
      x: Math.round(r.x),
      y: Math.round(r.y + scrollY),
      w: Math.round(r.width),
      h: Math.round(r.height),
    };
  };
  const boxes: Box[] = [];
  const texts: Line['texts'] = [];
  if (root) {
    const seen = new Map<string, number>();
    const walk = (e: Element, depth: number) => {
      if (depth > 4 || boxes.length >= 70) return;
      for (const c of Array.from(e.children)) {
        const r = c.getBoundingClientRect();
        if (r.width === 0 || r.height === 0 || getComputedStyle(c).display === 'none') continue;
        const s = sel(c);
        const n = seen.get(s) ?? 0;
        seen.set(s, n + 1);
        boxes.push(box(c, `${s}#${n}`));
        walk(c, depth + 1);
      }
    };
    walk(root, 1);
    const ownText = (e: Element) =>
      Array.from(e.childNodes).some((n) => n.nodeType === 3 && (n.textContent ?? '').trim());
    const els = Array.from(root.querySelectorAll('h1,h2,h3,p,a,button,li,td,th,span,small,label'))
      .filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.height > 0 && ownText(e);
      })
      .slice(0, 40);
    for (const e of els) {
      const fg = rgb(getComputedStyle(e).color);
      const bg = groundOf(e);
      const l1 = lum(fg);
      const l2 = lum(bg);
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      texts.push({
        k: `${sel(e)} "${(e.textContent ?? '').trim().slice(0, 24)}"`,
        color: getComputedStyle(e).color,
        bg: `rgb(${bg.slice(0, 3).join(',')})`,
        ratio: Math.round(ratio * 100) / 100,
      });
    }
  }
  const rr = root?.getBoundingClientRect();
  return {
    rootTheme:
      root?.getAttribute('data-theme') ??
      root?.closest('[data-theme]')?.getAttribute('data-theme') ??
      null,
    rootBox: root ? box(root, rootSel) : null,
    rootBg: root ? getComputedStyle(root).backgroundColor : null,
    bottomGround: groundOf(
      document.elementFromPoint(innerWidth / 2, innerHeight - 2) ?? document.body,
    ).join(','),
    display: root ? getComputedStyle(root).display : null,
    bottomReaches: rr ? rr.bottom + scrollY >= innerHeight - 1 : false,
    over: de.scrollWidth - de.clientWidth,
    boxes,
    texts,
  };
}

async function newPage(browser: Browser, size: keyof typeof SIZES, theme: string) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: OWNER_STATE,
    bypassCSP: true,
    ...(size === 'phone' ? { isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((t) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: t, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  const got = back.settings?.user_prefs?.theme ?? null;
  console.log(`REPORT account theme read back: ${got}`);
  expect(got).toBe(theme);
}

const settle = async (page: Page, ms = 2500) => {
  await page.waitForLoadState('networkidle', { timeout: ms + 3000 }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(500);
};

test('SEC-AUD-345: the four pages in the three main themes', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.jsonl');
  writeFileSync(jsonl, '');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const lines: Line[] = [];
  for (const theme of THEMES) {
    await setMain(ownerCtx, theme);
    for (const size of ['desktop', 'phone'] as const) {
      const { ctx, page } = await newPage(browser, size, theme);
      for (const p of PAGES) {
        const path = p.path(world);
        if (!path) {
          console.log(`REPORT ${p.id}: no data in the world`);
          continue;
        }
        await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
        await page.locator(p.root).first().waitFor({ timeout: 15_000 });
        await settle(page);
        const m = await page.evaluate(measureInPage, p.root);
        const mainTheme = await page.evaluate(
          () =>
            (
              JSON.parse(localStorage.getItem('sr.user.prefs') ?? '{}') as {
                state?: { prefs?: { theme?: string } };
              }
            ).state?.prefs?.theme ?? null,
        );
        const line: Line = { id: p.id, theme, size, main: mainTheme, ...m };
        lines.push(line);
        appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
        const tag = `${p.id}-${size}-${theme}`;
        await page.screenshot({ path: resolve(OUT, `${tag}.png`), timeout: 10_000 });
        console.log(
          `REPORT ${tag}: data-theme ${m.rootTheme} main ${mainTheme} bg ${m.rootBg} reaches ${m.bottomReaches} display ${m.display} bottom-ground ${m.bottomGround} over ${m.over} root ${JSON.stringify(m.rootBox)} low-contrast ${m.texts.filter((t) => t.ratio < 4.5).length}/${m.texts.length}`,
        );
        if (!BASE) {
          expect.soft(m.rootTheme, `${tag} data-theme`).toBe(theme);
          expect.soft(m.over, `${tag} sideways scroll`).toBeLessThanOrEqual(0);
          expect.soft(m.bottomReaches, `${tag} ground reaches the bottom`).toBe(true);
        }
      }
      await ctx.close();
    }
  }
  await ownerCtx.close();

  // Editorial against the base: no box moved by more than 2 px except the root's height.
  const baseFile = resolve(`${OUT_AFTER}-base`, 'measure.jsonl');
  if (!BASE && existsSync(baseFile)) {
    const base = readFileSync(baseFile, 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((l) => JSON.parse(l) as Line);
    let moved = 0;
    for (const l of lines.filter((x) => x.theme === 'lounge-editorial')) {
      const b = base.find((x) => x.id === l.id && x.theme === l.theme && x.size === l.size);
      if (!b) continue;
      const pairs: [Box | null, Box | null][] = b.rootBox?.w ? [[b.rootBox, l.rootBox]] : []; // a display: contents root has no box on the base
      for (const bb of b.boxes) pairs.push([bb, l.boxes.find((x) => x.k === bb.k) ?? null]);
      for (const [x, y] of pairs) {
        if (!x || !y) {
          console.log(`REPORT MOVED ${l.id} ${l.size}: ${x?.k} missing after`);
          moved++;
          continue;
        }
        const isRoot = x === b.rootBox;
        const d = Math.max(
          Math.abs(x.x - y.x),
          Math.abs(x.y - y.y),
          Math.abs(x.w - y.w),
          isRoot ? 0 : Math.abs(x.h - y.h),
        );
        if (d > 2) {
          console.log(
            `REPORT MOVED ${l.id} ${l.size} ${x.k}: ${JSON.stringify(x)} -> ${JSON.stringify(y)}`,
          );
          moved++;
        }
      }
    }
    console.log(`REPORT editorial boxes moved by more than 2 px: ${moved}`);
    expect(moved).toBe(0);
  }
});

test('SEC-AUD-345: click-through in Lounge Light', async ({ browser }) => {
  test.skip(BASE, 'the click-through is for the changed tree');
  mkdirSync(OUT, { recursive: true });
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  await setMain(ownerCtx, 'lounge-light');
  const themeOf = (page: Page) =>
    page.evaluate(() => document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null);
  for (const size of ['phone', 'desktop'] as const) {
    const { ctx, page } = await newPage(browser, size, 'lounge-light');
    const step = async (name: string, want = 'lounge-light') => {
      await settle(page);
      const t = await themeOf(page);
      console.log(
        `REPORT click ${size} ${name}: ${page.url().replace(ORIGIN, '')} data-theme ${t}`,
      );
      await page.screenshot({ path: resolve(OUT, `click-${size}-${name}.png`), timeout: 10_000 });
      expect(t, `${size} ${name}`).toBe(want);
    };
    await page.goto(`/channels/${world.guildId}/${world.channelId}`, {
      waitUntil: 'domcontentloaded',
    });
    await step('1-lounge');
    await page.goto('/support', { waitUntil: 'domcontentloaded' });
    await page.locator('main.sup-page').first().waitFor();
    await step('2-support');
    const lb = page.locator('a[href="/support/leaderboards"]').first();
    await lb.scrollIntoViewIfNeeded();
    await lb.click();
    await page.waitForURL(/\/support\/leaderboards/);
    await step('3-leaderboards');
    await page.goBack();
    await page.waitForURL(/\/support$/);
    await step('4-back-on-support');
    if (world.inviteCode) {
      await page.goto(`/invite/${world.inviteCode}`, { waitUntil: 'domcontentloaded' });
      await page.locator('.ent-invite-page').first().waitFor();
      await step('5-invite');
      const btn = page.locator('.ent-invite-page main button, .ent-invite-page main a').first();
      await btn.click();
      await page.waitForURL((u) => !/\/invite\//.test(u.pathname), { timeout: 15_000 });
      await step('6-after-invite-button');
    }
    await ctx.close();
  }
  await ownerCtx.close();
});
