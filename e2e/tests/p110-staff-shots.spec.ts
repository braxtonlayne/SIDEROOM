import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * Wave 110, staff (SEC-AUD-555, 556). Measures: (1) the Audit log's filter fields (admin and server settings) at 1024,
 * Lounge Light and Dark: tag, class, computed ground, border, text colour; (2) the tool drawer at 375 touch (812 and 667
 * high) with Dice after a roll and with the Wheel: the drawer's box, every scrolling ancestor of the tool body
 * (overflow-y, scrollHeight, clientHeight), and whether a member can scroll the roll detail and Spin into view by a
 * touch swipe (CDP touch gesture) and by scrollTop. Makes nothing, submits nothing, imports no /src module.
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p110-integ/apps/web/dist --env SR_P110_STAFF_ON=1 --env SR_P110_STAFF_PHASE=after --env SR_P110_STAFF_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-staff -- tests/p110-staff-shots.spec.ts
 */
const ON = process.env.SR_P110_STAFF_ON === '1';
const PHASE = process.env.SR_P110_STAFF_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P110_STAFF_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-staff';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(!ON, 'local visual check only');
test.setTimeout(300_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 220);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const figs: Record<string, unknown> = {};
const problems: string[] = [];
const notCovered: string[] = [];
const pre = PHASE === 'base' ? 'base-' : '';

async function newPage(
  browser: Browser,
  owner: BrowserContext,
  w: number,
  h: number,
  touch: boolean,
  theme: string,
) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: w, height: h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme, zoom: 100 },
  );
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  page.on('pageerror', (e) => problems.push(`pageerror: ${msg(e)}`));
  return { ctx, page };
}

async function settle(page: Page, ms = 5000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(600);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const s = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  if (s[0]! > s[1]!) problems.push(`${name}: page scrolls sideways (${s[0]} > ${s[1]})`);
  await page.screenshot({ path: resolve(OUT, `${pre}${name}.png`), timeout: 15_000 });
}

// ---- 1: the Audit log's filter fields -------------------------------------------------------------------------------

async function readFields(page: Page, rootSel: string) {
  return page.evaluate((sel) => {
    const root = document.querySelector(sel);
    if (!root) return { missing: true };
    const rd = (n: number) => Math.round(n * 10) / 10;
    const grounds = (el: Element) => {
      const chain: string[] = [];
      let e: Element | null = el;
      for (let i = 0; e && i < 4; i++, e = e.parentElement) {
        const cs = getComputedStyle(e);
        chain.push(
          `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)} bg=${cs.backgroundColor}`,
        );
      }
      return chain;
    };
    const fields = Array.from(root.querySelectorAll('input, select, textarea')).map((el) => {
      const cs = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      const lab =
        (el.id && document.querySelector(`label[for="${el.id}"]`)?.textContent) ||
        el.getAttribute('aria-label') ||
        el.closest('label')?.textContent ||
        '';
      return {
        name: String(lab).trim().slice(0, 30),
        tag: el.tagName.toLowerCase(),
        type: el.getAttribute('type'),
        cls: String(el.className).slice(0, 80),
        bg: cs.backgroundColor,
        border: `${cs.borderTopWidth} ${cs.borderTopColor}`,
        color: cs.color,
        box: { w: rd(r.width), h: rd(r.height) },
        parents: grounds(el),
      };
    });
    const tok = getComputedStyle(root);
    return {
      fields,
      tokens: {
        bgRaised: tok.getPropertyValue('--ui-bg-raised').trim(),
        bg: tok.getPropertyValue('--ui-bg').trim(),
        border: tok.getPropertyValue('--ui-border').trim(),
      },
    };
  }, rootSel);
}

async function auditPart(browser: Browser, owner: BrowserContext) {
  const w = world();
  for (const mode of ['light', 'dark'] as const) {
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: `lounge-${mode}`, zoom: 100 } },
    });
    const { ctx, page } = await newPage(browser, owner, 1024, 768, false, `lounge-${mode}`);
    try {
      for (const [id, url, root] of [
        ['admin', '/admin/audit-log', 'main'],
        ['server', `/settings/server/${w.guildId}/audit-log`, 'main'],
      ] as const) {
        try {
          await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 45_000 });
          await page.getByRole('main').first().waitFor({ timeout: 30_000 });
          await settle(page);
          if (id === 'admin') await shot(page, `audit-filters-1024-${mode}`);
          else await shot(page, `audit-server-1024-${mode}`);
          figs[`audit-${id}-${mode}`] = await readFields(page, root);
          say(`AUDIT ${id} ${mode}: ${JSON.stringify(figs[`audit-${id}-${mode}`])}`);
        } catch (e) {
          notCovered.push(`audit ${id} ${mode}: ${msg(e)}`);
        }
      }
    } finally {
      await ctx.close();
    }
  }
}

// ---- 2: the drawer ---------------------------------------------------------------------------------------------------

const drawerOf = (page: Page) => page.getByRole('dialog', { name: 'Drawer', exact: true });

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawerOf(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click();
  await page.waitForTimeout(900);
}

/** The scrolling chain from the tool root up to the dialog, and where Spin / the roll detail sit in the viewport. */
async function measureDrawer(page: Page, vh: number) {
  return page.evaluate((viewH) => {
    const rd = (n: number) => Math.round(n * 10) / 10;
    const dlg = document.querySelector(
      '[role=dialog][aria-label="Drawer"], .sr-tl-drawer',
    ) as HTMLElement | null;
    const dialog = (document.querySelector('.sr-tl-drawer') as HTMLElement | null) ?? dlg;
    if (!dialog) return { missing: true };
    const rb = dialog.getBoundingClientRect();
    const start = dialog.querySelector('.ud-root, .sr-tl-view-body') ?? dialog;
    const chain: Record<string, unknown>[] = [];
    for (let e: Element | null = start; e; e = e.parentElement) {
      const cs = getComputedStyle(e);
      const r = e.getBoundingClientRect();
      chain.push({
        el: `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`,
        overflowY: cs.overflowY,
        h: rd(r.height),
        top: rd(r.top),
        scrollHeight: (e as HTMLElement).scrollHeight,
        clientHeight: (e as HTMLElement).clientHeight,
        maxBlock: cs.maxBlockSize,
      });
      if (e === dialog || e === document.body) break;
    }
    const pos = (sel: string) => {
      const el = dialog.querySelector(sel);
      if (!el || !el.getClientRects().length) return null;
      const r = el.getBoundingClientRect();
      return { top: rd(r.top), bottom: rd(r.bottom), inView: r.top >= 0 && r.bottom <= viewH };
    };
    return {
      dialogBox: {
        x: rd(rb.left),
        y: rd(rb.top),
        w: rd(rb.width),
        h: rd(rb.height),
        bottom: rd(rb.bottom),
      },
      chain,
      primary: pos('.ud-btn-primary'),
      outs: Array.from(dialog.querySelectorAll('.ud-out, [role=status], output, .ud-result'))
        .filter((e) => e.getClientRects().length)
        .map((e) => {
          const r = e.getBoundingClientRect();
          return {
            cls: String(e.className).slice(0, 30),
            top: rd(r.top),
            bottom: rd(r.bottom),
            inView: r.bottom <= viewH,
          };
        }),
    };
  }, vh);
}

/** Scroll by a real touch swipe (CDP) over the dialog; returns nothing. */
async function swipeUp(page: Page, x: number, y0: number, y1: number) {
  const cdp = await page.context().newCDPSession(page);
  try {
    const pt = (y: number) => [{ x, y, id: 1 }];
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: pt(y0) });
    for (let i = 1; i <= 10; i++) {
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: pt(y0 + ((y1 - y0) * i) / 10),
      });
      await page.waitForTimeout(16);
    }
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
  } finally {
    await cdp.detach().catch(() => undefined);
  }
  await page.waitForTimeout(500);
}

/** Can the point at the centre of `sel` be hit (elementFromPoint lands in it) once on screen? */
async function reachable(page: Page, sel: string) {
  return page.evaluate((s) => {
    const el = document.querySelector('.sr-tl-drawer')?.querySelector(s);
    if (!el) return null;
    const r = el.getBoundingClientRect();
    const x = r.left + r.width / 2;
    const y = r.top + r.height / 2;
    if (y < 0 || y > innerHeight) return { onScreen: false, y: Math.round(y) };
    const hit = document.elementFromPoint(x, y);
    return { onScreen: true, y: Math.round(y), hit: !!hit && (el === hit || el.contains(hit)) };
  }, sel);
}

async function drawerPart(browser: Browser, owner: BrowserContext) {
  const w = world();
  for (const vh of [812, 667]) {
    for (const tool of [
      { id: 'dice', query: 'dice', pick: /^dice/i, wait: 900 },
      { id: 'wheel', query: 'wheel', pick: /^wheel/i, wait: 3500 },
    ]) {
      const { ctx, page } = await newPage(browser, owner, 375, vh, true, 'lounge-light');
      const tag = `${tool.id}-${vh}`;
      try {
        await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
          waitUntil: 'domcontentloaded',
          timeout: 45_000,
        });
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        await settle(page, 4000);
        await openTool(page, tool.query, tool.pick);
        await expect(drawerOf(page)).toBeVisible();
        await settle(page, 3000);
        const prim = drawerOf(page).locator('.ud-btn-primary').first();
        if (tool.id === 'dice' && (await prim.count())) {
          await prim.evaluate((el) => (el as HTMLElement).click());
          await page.waitForTimeout(tool.wait);
        }
        const before = await measureDrawer(page, vh);
        const reachBefore = await reachable(page, '.ud-btn-primary');
        if (vh === 812) await shot(page, tool.id === 'dice' ? 'dice-rolled-375' : 'wheel-375');
        say(
          `DRAWER ${tag} first: ${JSON.stringify(before)} primary-reach ${JSON.stringify(reachBefore)}`,
        );

        // Swipe up by touch over the tool body; then measure again.
        const box = await drawerOf(page).locator('.sr-tl-view-body').boundingBox();
        const x = (box?.x ?? 0) + (box?.width ?? 300) / 2;
        const y0 = Math.min(vh - 30, (box?.y ?? 0) + (box?.height ?? 300) - 15);
        await swipeUp(page, x, y0, Math.max(40, y0 - 250));
        const afterSwipe = await measureDrawer(page, vh);
        const reachSwipe = await reachable(page, '.ud-btn-primary');
        say(
          `DRAWER ${tag} after swipe: ${JSON.stringify(afterSwipe)} primary-reach ${JSON.stringify(reachSwipe)}`,
        );

        // scrollTop on every scrollable ancestor, read back.
        const st = await page.evaluate(() => {
          const d = document.querySelector('.sr-tl-drawer');
          const out: Record<string, unknown>[] = [];
          const start = d?.querySelector('.ud-root, .sr-tl-view-body') ?? d;
          for (let e: Element | null = start ?? null; e; e = e.parentElement) {
            const h = e as HTMLElement;
            if (h.scrollHeight > h.clientHeight + 1) {
              const was = h.scrollTop;
              h.scrollTop = 9999;
              out.push({
                el: `${e.tagName.toLowerCase()}.${String(e.className).slice(0, 40)}`,
                overflowY: getComputedStyle(e).overflowY,
                was,
                now: h.scrollTop,
                max: h.scrollHeight - h.clientHeight,
              });
            }
            if (e === d || e === document.body) break;
          }
          return out;
        });
        await page.waitForTimeout(300);
        const afterTop = await measureDrawer(page, vh);
        const reachTop = await reachable(page, '.ud-btn-primary');
        say(
          `DRAWER ${tag} scrollTop: ${JSON.stringify(st)} then ${JSON.stringify(afterTop)} primary-reach ${JSON.stringify(reachTop)}`,
        );
        if (vh === 812)
          await shot(page, tool.id === 'dice' ? 'dice-rolled-scrolled-375' : 'wheel-scrolled-375');
        figs[`drawer-${tag}`] = {
          before,
          reachBefore,
          afterSwipe,
          reachSwipe,
          scrollTop: st,
          afterTop,
          reachTop,
        };
      } catch (e) {
        notCovered.push(`drawer ${tag}: ${msg(e)}`);
        say(`NOT COVERED drawer ${tag}: ${msg(e)}`);
      } finally {
        await ctx.close();
      }
    }
  }
}

test(`staff ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(owner.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    await auditPart(browser, owner);
    await drawerPart(browser, owner);
  } finally {
    await owner.close();
  }
  const file = resolve(OUT, `figures-${PHASE}.json`);
  writeFileSync(file, JSON.stringify({ figs, problems, notCovered }, null, 1));
  // In the after phase, compare with the base: the drawer's box must not change.
  if (PHASE === 'after' && existsSync(resolve(OUT, 'figures-base.json'))) {
    const base = JSON.parse(readFileSync(resolve(OUT, 'figures-base.json'), 'utf8')) as {
      figs: Record<string, { before?: { dialogBox?: unknown } }>;
    };
    for (const [k, v] of Object.entries(figs)) {
      if (!k.startsWith('drawer-')) continue;
      const b = base.figs[k]?.before?.dialogBox;
      const a = (v as { before?: { dialogBox?: unknown } }).before?.dialogBox;
      if (b && JSON.stringify(a) !== JSON.stringify(b))
        problems.push(
          `${k}: the drawer's box changed ${JSON.stringify(b)} -> ${JSON.stringify(a)}`,
        );
    }
    writeFileSync(file, JSON.stringify({ figs, problems, notCovered }, null, 1));
  }
  for (const p of problems) say(`problem: ${p}`);
  for (const n of notCovered) say(`not covered: ${n}`);
  expect(notCovered, 'every page covered').toEqual([]);
});
