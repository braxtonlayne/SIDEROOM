import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-350 (wave 83; D-854): a look pass over every tab of a server's settings, plus a text channel's and a voice
 * channel's settings tabs. The Owner on the visual sweep's seeded world (`e2e/.state/sweep/world.json`, made by
 * `page-sweep.spec.ts`'s "seed the sweep world"; that spec is never edited). Three settings: 375x812 (touch, a mobile
 * user agent) in Lounge Light and in Lounge Dark, and 1280x800 in Editorial (the account's main theme by PATCH and
 * localStorage, `data-theme` read back before each shot and written into the file name).
 *
 * Per tab and setting: the viewport shot (one more scrolled to the middle and one to the end where the tab's scroller
 * is taller than two viewports) and one JSON line in `measure.json`: the sideways scroll of the document and of the
 * tab's scroller; at 375 every visible control under 44 px tall; every element whose own text is wider than its box
 * and has no ellipsis; the boxes of the tab's cards and their direct children (for the "nothing else moved" check).
 * The shared sheet's three Admin pages (Accounts, one form, the platform audit log) at 375 and 1280 in Lounge Dark
 * are logged and shot the same way (`admin-*`).
 *
 * Local only (SR_P83_GUILD=1); shots and measure.json go to SR_P83_GUILD_OUT; SR_P83_ONLY is a regex over tab ids.
 * SR_P83_TAG (default empty) is put in front of the ids of the file names, so an "after" run does not overwrite a
 * "before" one; the JSON lines carry it too.
 */
const ON = process.env.SR_P83_GUILD === '1';
const OUT =
  process.env.SR_P83_GUILD_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p83-guild';
const ONLY = process.env.SR_P83_ONLY ? new RegExp(process.env.SR_P83_ONLY) : null;
const TAG = process.env.SR_P83_TAG ?? '';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;
const SETTINGS: { size: Size; theme: string }[] = [
  { size: 'phone', theme: 'lounge-light' },
  { size: 'phone', theme: 'lounge-dark' },
  { size: 'desktop', theme: 'lounge-editorial' },
];
const ADMIN_SETTINGS: { size: Size; theme: string }[] = [
  { size: 'phone', theme: 'lounge-dark' },
  { size: 'desktop', theme: 'lounge-dark' },
];
/** The limit for one tab. */
const LIMIT_MS = 25_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Box {
  sel: string;
  x: number;
  y: number;
  w: number;
  h: number;
}
interface Measured {
  vw: number;
  vh: number;
  theme: string | null;
  doc: number;
  scroller: string | null;
  scrollerOver: number;
  scrollerHeight: number;
  scrollerClient: number;
  under44: { tag: string; cls: string; text: string; box: number[] }[];
  under44Count: number;
  wide: { tag: string; cls: string; text: string; sw: number; cw: number; ox: string }[];
  boxes: Box[];
}

/** In the page: everything the look pass logs for one tab at one setting (the scroller is left at the top). */
function measureInPage(phone: boolean): Measured {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const vh = de.clientHeight;
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 3) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const visible = (e: Element) => {
    const r = e.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) return false;
    const cs = getComputedStyle(e);
    return cs.display !== 'none' && cs.visibility !== 'hidden';
  };
  // The tab's scroller: the vertical scroller with the most to scroll among the big ones; else the document.
  const vp = vw * vh;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? null;
  if (sc) sc.scrollTop = 0;
  const scEl: HTMLElement = sc ?? (document.scrollingElement as HTMLElement);
  const themed = document.querySelector('[data-theme]');
  const root: ParentNode = sc ?? document.body;

  // Controls under 44 px tall on a phone.
  const under44: Measured['under44'] = [];
  let under44Count = 0;
  if (phone) {
    const ctl = document.querySelectorAll<HTMLElement>(
      'button, a[href], input, select, textarea, [role=switch], [role=tab], [role=checkbox], [role=radio]',
    );
    for (const e of Array.from(ctl)) {
      if (!visible(e)) continue;
      const r = e.getBoundingClientRect();
      if (r.right <= 0 || r.left >= vw) continue; // an off-screen drawer
      if (r.height >= 43.5) continue;
      if (e.closest('.ss-sr, .ss-bridge-sr')) continue;
      if (e.tagName === 'A') {
        const p = e.closest('p');
        if (p && (p.textContent ?? '').trim() !== (e.textContent ?? '').trim()) continue;
      }
      if (e instanceof HTMLInputElement && (e.type === 'checkbox' || e.type === 'radio')) {
        const lab = e.closest('label');
        if (lab && lab.getBoundingClientRect().height >= 43.5) continue;
      }
      if (e instanceof HTMLInputElement && e.type === 'hidden') continue;
      under44Count++;
      const lab =
        e.getAttribute('aria-label') ||
        (e.textContent ?? '').trim() ||
        (e as HTMLInputElement).placeholder ||
        (e as HTMLInputElement).name ||
        (e.id ? (document.querySelector(`label[for="${e.id}"]`)?.textContent ?? '') : '') ||
        '';
      if (under44.length < 60)
        under44.push({
          tag: e.tagName.toLowerCase(),
          cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
          text: lab.trim().slice(0, 40),
          box: [
            Math.round(r.left),
            Math.round(r.top + scEl.scrollTop),
            Math.round(r.width),
            Math.round(r.height * 10) / 10,
          ],
        });
    }
  }

  // Own text wider than its box, with no ellipsis.
  const wide: Measured['wide'] = [];
  for (const e of Array.from(document.querySelectorAll<HTMLElement>('body *'))) {
    if (e.clientWidth < 2 || e.scrollWidth <= e.clientWidth + 1) continue;
    if (e.closest('.ss-sr, .ss-bridge-sr')) continue;
    if (!visible(e)) continue;
    const own = Array.from(e.childNodes).some(
      (n) => n.nodeType === 3 && (n.textContent ?? '').trim() !== '',
    );
    const isField = e instanceof HTMLInputElement || e instanceof HTMLTextAreaElement;
    if (!own && !isField) continue;
    const cs = getComputedStyle(e);
    if (cs.textOverflow === 'ellipsis') continue;
    if (wide.length < 40)
      wide.push({
        tag: e.tagName.toLowerCase(),
        cls: typeof e.className === 'string' ? e.className.slice(0, 60) : '',
        text: (e.textContent ?? '').trim().slice(0, 40),
        sw: e.scrollWidth,
        cw: e.clientWidth,
        ox: cs.overflowX,
      });
  }

  // Boxes of the tab's cards and their direct children (page coordinates inside the scroller).
  const top0 = sc ? sc.getBoundingClientRect().top : 0;
  const left0 = sc ? sc.getBoundingClientRect().left : 0;
  const boxes: Box[] = [];
  const cards = Array.from(
    root.querySelectorAll<HTMLElement>(
      '[class*="card"], .ss-danger-zone, .ss-form, .ss-table, .ss-toolbar, .ss-group, .ss-bridge, .ss-split, [role=tabpanel]',
    ),
  );
  const put = (e: Element, i: number) => {
    if (boxes.length >= 220) return;
    const r = e.getBoundingClientRect();
    if (r.width === 0 && r.height === 0) return;
    boxes.push({
      sel: `${i}:${sel(e)}`,
      x: Math.round((r.left - left0) * 10) / 10,
      y: Math.round((r.top - top0 + (sc ? sc.scrollTop : 0)) * 10) / 10,
      w: Math.round(r.width * 10) / 10,
      h: Math.round(r.height * 10) / 10,
    });
  };
  cards.forEach((c, i) => {
    put(c, i);
    Array.from(c.children).forEach((k, j) => put(k, i * 100 + j + 1));
  });

  return {
    vw,
    vh,
    theme: themed?.getAttribute('data-theme') ?? null,
    doc: de.scrollWidth - vw,
    scroller: sc ? sel(sc) : null,
    scrollerOver: sc ? sc.scrollWidth - sc.clientWidth : 0,
    scrollerHeight: scEl.scrollHeight,
    scrollerClient: scEl.clientHeight,
    under44,
    under44Count,
    wide,
    boxes,
  };
}

/** Scroll the tab's scroller to a fraction of its way down (0 to 1); the same pick as `measureInPage`. */
function scrollScroller(frac: number) {
  const de = document.documentElement;
  const vp = de.clientWidth * de.clientHeight;
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    return /(auto|scroll|overlay)/.test(cs.overflowY) && e.clientWidth * e.clientHeight >= vp * 0.2;
  });
  cands.sort((a, b) => b.scrollHeight - a.scrollHeight);
  const sc = cands[0] ?? (document.scrollingElement as HTMLElement);
  sc.scrollTop = (sc.scrollHeight - sc.clientHeight) * frac;
}

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 36);

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};

async function withLimit<T>(what: string, fn: () => Promise<T>): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      fn(),
      new Promise<never>((_, rej) => {
        timer = setTimeout(() => rej(new Error(`${what}: over ${LIMIT_MS} ms`)), LIMIT_MS);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function newPage(browser: Browser, size: Size, theme: string, base: BrowserContext) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: await base.storageState(),
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

const NAV = '[role="dialog"] nav ul li > button';

test("SEC-AUD-350: every tab of a server's settings, looked at", async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  if (!ONLY && !TAG) writeFileSync(jsonl, '');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const totals = {
    tabs: 0,
    shots: 0,
    lines: 0,
    failed: [] as string[],
    lists: {} as Record<string, string[]>,
  };

  const setTheme = async (theme: string) => {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    console.log(
      `REPORT account theme set ${theme}, read back: ${back.settings?.user_prefs?.theme}`,
    );
  };

  /** Measure the tab shown now and shoot it (viewport, and the middle and the end when it is taller than two viewports). */
  const look = async (page: Page, id: string, size: Size, extra: Record<string, unknown>) => {
    await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
    const m = await page.evaluate(measureInPage, size === 'phone');
    const stem = `${TAG ? `${TAG}-` : ''}${id}-${size === 'phone' ? '375' : '1280'}-${m.theme ?? 'no-theme'}`;
    const shots: string[] = [];
    const shoot = async (suffix: string) => {
      const f = `${stem}${suffix}.png`;
      await page.screenshot({ path: resolve(OUT, f), timeout: 10_000 });
      shots.push(f);
      totals.shots++;
    };
    await shoot('');
    if (m.scrollerHeight > 2 * m.scrollerClient) {
      await page.evaluate(scrollScroller, 0.5);
      await page.waitForTimeout(250);
      await shoot('-mid');
      await page.evaluate(scrollScroller, 1);
      await page.waitForTimeout(250);
      await shoot('-end');
      await page.evaluate(scrollScroller, 0);
    }
    appendFileSync(
      jsonl,
      `${JSON.stringify({ id, tag: TAG || undefined, size, url: page.url().replace(ORIGIN, ''), ...extra, ...m, shots })}\n`,
    );
    totals.lines++;
    console.log(
      `REPORT ${size} ${m.theme} ${id}: doc ${m.doc} scroller ${m.scrollerOver} height ${m.scrollerHeight}/${m.scrollerClient} under44 ${m.under44Count} wide ${m.wide.length} shots ${shots.length}`,
    );
  };

  /** Walk the shell's section list (read from the page), clicking each; `pick` narrows to some labels. */
  const walkSettings = async (
    page: Page,
    size: Size,
    kind: string,
    path: string,
    pick?: (label: string) => boolean,
  ) => {
    let labels: string[] = [];
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    labels = await page
      .locator(NAV)
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    console.log(`REPORT ${size} ${kind}: ${labels.length} tabs: ${labels.join(' | ')}`);
    totals.lists[`${kind}-${size}`] = labels;
    for (let i = 0; i < labels.length; i++) {
      const label = labels[i] ?? '';
      if (pick && !pick(label)) continue;
      const id = `${kind}-${String(i + 1).padStart(2, '0')}-${slug(label)}`;
      if (ONLY && !ONLY.test(id)) continue;
      try {
        await withLimit(id, async () => {
          const btns = page.locator(NAV);
          if ((await btns.count()) !== labels.length) {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 3000);
          }
          await page
            .locator(NAV)
            .nth(i)
            .evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2500);
          await look(page, id, size, { tab: label });
        });
        totals.tabs++;
      } catch (e) {
        totals.failed.push(`${size} ${id}`);
        appendFileSync(
          jsonl,
          `${JSON.stringify({ id, size, ok: false, error: String(e).slice(0, 200) })}\n`,
        );
        console.log(`REPORT ${size} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
        await page
          .goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 })
          .catch(() => undefined);
      }
    }
  };

  /** A channel's own settings tabs: the tab strip of the editor, read from the page. */
  const walkChannel = async (page: Page, size: Size, kind: string, channelId: string) => {
    const path = `/settings/server/${world.guildId}/channels?channel=${channelId}`;
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    const labels = await page
      .locator('.ss-tabs [role="tab"]')
      .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    console.log(`REPORT ${size} ${kind}: ${labels.length} tabs: ${labels.join(' | ')}`);
    totals.lists[`${kind}-${size}`] = labels;
    for (let i = 0; i < labels.length; i++) {
      const label = labels[i] ?? '';
      const id = `${kind}-${String(i + 1).padStart(2, '0')}-${slug(label)}`;
      if (ONLY && !ONLY.test(id)) continue;
      try {
        await withLimit(id, async () => {
          if ((await page.locator('.ss-tabs [role="tab"]').count()) !== labels.length) {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
            await idle(page, 3000);
          }
          await page
            .locator('.ss-tabs [role="tab"]')
            .nth(i)
            .evaluate((el) => (el as HTMLElement).click());
          await idle(page, 2000);
          await look(page, id, size, { tab: label });
        });
        totals.tabs++;
      } catch (e) {
        totals.failed.push(`${size} ${id}`);
        appendFileSync(
          jsonl,
          `${JSON.stringify({ id, size, ok: false, error: String(e).slice(0, 200) })}\n`,
        );
        console.log(`REPORT ${size} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
      }
    }
  };

  /** A role with a long name, once; its editor's tabs (Display, Permissions, ...) are shot like the others. */
  const LONG_ROLE = 'A very long role name that keeps going past the end of one line on a phone';
  {
    const have = await o.get(`/api/v1/guilds/${world.guildId}/roles`);
    if (!have.ok() || !(await have.text()).includes(LONG_ROLE))
      await apiPost(o, `/guilds/${world.guildId}/roles`, {
        name: LONG_ROLE,
        color: 0x2ecc71,
      }).catch((e) => console.log(`REPORT long role not made: ${String(e).slice(0, 120)}`));
  }
  const walkRole = async (page: Page, size: Size) => {
    const path = `/settings/server/${world.guildId}/roles`;
    await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await idle(page, 4000);
    await page
      .locator('button.ss-name', { hasText: 'A very long role' })
      .first()
      .click({ timeout: 6000 });
    await idle(page, 2000);
    const tabs = page.locator('[role="tab"]');
    const labels = await tabs.evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
    console.log(`REPORT ${size} roles-long: ${labels.length} tabs: ${labels.join(' | ')}`);
    for (let i = 0; i < labels.length; i++) {
      const id = `roles-long-${String(i + 1).padStart(2, '0')}-${slug(labels[i] ?? '')}`;
      if (ONLY && !ONLY.test(id)) continue;
      try {
        await withLimit(id, async () => {
          await tabs.nth(i).evaluate((el) => (el as HTMLElement).click());
          await idle(page, 1500);
          await look(page, id, size, { tab: labels[i] });
        });
        totals.tabs++;
      } catch (e) {
        totals.failed.push(`${size} ${id}`);
        console.log(`REPORT ${size} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
      }
    }
  };

  for (const { size, theme } of SETTINGS) {
    await setTheme(theme);
    const { ctx, page } = await newPage(browser, size, theme, ownerCtx);
    await walkSettings(page, size, 'ss', `/settings/server/${world.guildId}`).catch((e) => {
      totals.failed.push(`${size} ${theme} ss: ${String(e).slice(0, 100)}`);
    });
    if (!ONLY || ONLY.test('roles'))
      await walkRole(page, size).catch((e) =>
        totals.failed.push(`${size} roles-long: ${String(e).slice(0, 100)}`),
      );
    if (!ONLY || ONLY.test('chan')) {
      await walkChannel(page, size, 'chan-text', world.channelId).catch((e) => {
        totals.failed.push(`${size} ${theme} chan-text: ${String(e).slice(0, 100)}`);
      });
      if (world.voiceChannelId)
        await walkChannel(page, size, 'chan-voice', world.voiceChannelId).catch((e) => {
          totals.failed.push(`${size} ${theme} chan-voice: ${String(e).slice(0, 100)}`);
        });
    }
    await ctx.close().catch(() => undefined);
  }

  // The shared sheet's three Admin pages in Lounge Dark at both sizes (the base for round 2).
  if (!ONLY || ONLY.test('admin')) {
    for (const { size, theme } of ADMIN_SETTINGS) {
      await setTheme(theme);
      const { ctx, page } = await newPage(browser, size, theme, ownerCtx);
      const picks = [/^Accounts$/i, /Platform audit log/i, /^Limits/i];
      await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 15_000 });
      await idle(page, 4000);
      const labels = await page
        .locator(NAV)
        .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
      console.log(`REPORT ${size} admin: ${labels.length} pages: ${labels.join(' | ')}`);
      totals.lists[`admin-${size}`] = labels;
      await walkSettings(page, size, 'admin', '/admin/dashboard', (l) =>
        picks.some((p) => p.test(l)),
      ).catch((e) => {
        totals.failed.push(`${size} admin: ${String(e).slice(0, 100)}`);
      });
      await ctx.close().catch(() => undefined);
    }
  }

  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(totals)}`);
  writeFileSync(
    resolve(OUT, `totals${TAG ? `-${TAG}` : ''}.json`),
    JSON.stringify(totals, null, 1),
  );
});
