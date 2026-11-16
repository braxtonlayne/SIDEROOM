import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-324 (wave 80; D-825, D-821), on the real stack: the Developers pages' bare selects take the kit field's look,
 * and the phone nav shows its current link and its hidden edge.
 * Probes: the Rich Presence "Type" select against a kit text field on the same page (background, colour, border colour,
 * radius, block size); at 375 the nav's `data-overflow` against scrollWidth/clientWidth/scrollLeft, the current link's box
 * and "Back to the lounge" against the nav's client box, the document's sideways scroll; at 1280 no attribute.
 * Rings are switched on through the Admin feature API (`lounge.rings`), then the Rings composer's file input is shot with
 * the real feed (no page.route).
 * Lounge Dark and Lounge Light (the account's main theme, set by PATCH and localStorage, read back), Editorial once.
 * Local only (SR_P80_DEVELOPERS=1); shots go to SR_P80_DEVELOPERS_OUT. SR_P80_BASE=1: the base case (the change held back
 * in a scratch copy): the faults are logged instead of asserted.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_DEVELOPERS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-developers';
const BASE = process.env.SR_P80_BASE === '1';
const SIZES = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
const THEMES = ['lounge-dark', 'lounge-light'] as const;

test.skip(process.env.SR_P80_DEVELOPERS !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number };
const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;

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

async function reach(name: string, run: () => Promise<void>) {
  try {
    await run();
  } catch (e) {
    log(`UNREACHED ${name}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
  }
}

async function shoot(page: Page, name: string) {
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${name}.png`) });
  shots++;
}

/** The select's computed look against the first kit text field on the page. */
async function selectLook(page: Page, tag: string) {
  const r = await page.evaluate(() => {
    const sel = document.querySelector<HTMLSelectElement>('.dev-main select');
    const field = Array.from(
      document.querySelectorAll<HTMLInputElement>(
        '.dev-main input:not([type=file]):not([type=checkbox]):not([type=radio]):not([type=hidden])',
      ),
    ).find((i) => i.offsetParent);
    if (!sel) return null;
    const pick = (el: Element | undefined) => {
      if (!el) return null;
      const c = getComputedStyle(el);
      return {
        bg: c.backgroundColor,
        color: c.color,
        border: c.borderTopColor,
        radius: c.borderTopLeftRadius,
        block: Math.round(el.getBoundingClientRect().height),
        font: c.fontFamily,
      };
    };
    return {
      select: pick(sel),
      field: pick(field),
      theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    };
  });
  log(`select ${tag} ${JSON.stringify(r)}`);
  if (r?.select && r.field) {
    const ok =
      r.select.bg === r.field.bg &&
      r.select.color === r.field.color &&
      r.select.border === r.field.border &&
      r.select.radius === r.field.radius &&
      r.select.block >= r.field.block;
    if (BASE) log(`select ${tag} matches kit field: ${ok} (base: expected false)`);
    else {
      expect.soft(r.select.bg, `${tag}: background`).toBe(r.field.bg);
      expect.soft(r.select.color, `${tag}: colour`).toBe(r.field.color);
      expect.soft(r.select.border, `${tag}: border`).toBe(r.field.border);
      expect.soft(r.select.radius, `${tag}: radius`).toBe(r.field.radius);
      expect.soft(r.select.block, `${tag}: block size`).toBeGreaterThanOrEqual(r.field.block);
    }
  }
  return r;
}

/** The nav's figures: overflow attribute, scroll position, link boxes against the nav's client box. */
async function navLook(page: Page, tag: string) {
  const r = await page.evaluate(() => {
    const nav = document.querySelector<HTMLElement>('.dev-nav');
    if (!nav) return null;
    const nb = nav.getBoundingClientRect();
    const cl = nb.left + nav.clientLeft;
    const cr = cl + nav.clientWidth;
    const box = (el: Element | null) => {
      if (!el) return null;
      const b = el.getBoundingClientRect();
      return { left: Math.round(b.left), right: Math.round(b.right) };
    };
    const cur = box(nav.querySelector("[aria-current='page']"));
    const exit = box(nav.querySelector('.dev-nav-exit'));
    return {
      attr: nav.getAttribute('data-overflow'),
      scrollWidth: nav.scrollWidth,
      clientWidth: nav.clientWidth,
      scrollLeft: Math.round(nav.scrollLeft),
      navLeft: Math.round(cl),
      navRight: Math.round(cr),
      current: cur,
      exit,
      docOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
      winScrollX: window.scrollX,
    };
  });
  log(`nav ${tag} ${JSON.stringify(r)}`);
  return r;
}

function checkNav(
  r: NonNullable<Awaited<ReturnType<typeof navLook>>>,
  phone: boolean,
  tag: string,
) {
  expect.soft(r.docOver, `${tag}: document scrolls sideways`).toBeLessThanOrEqual(0);
  if (!phone) {
    expect.soft(r.attr, `${tag}: no attribute at 1280`).toBeNull();
    return;
  }
  const max = r.scrollWidth - r.clientWidth;
  const want =
    max <= 1
      ? null
      : r.scrollLeft > 1 && r.scrollLeft < max - 1
        ? 'both'
        : r.scrollLeft > 1
          ? 'start'
          : 'end';
  expect.soft(r.attr, `${tag}: data-overflow matches the scroll position`).toBe(want);
  if (BASE) return;
  if (r.current) {
    expect
      .soft(r.current.left, `${tag}: current link inside (left)`)
      .toBeGreaterThanOrEqual(r.navLeft - 1);
    expect
      .soft(r.current.right, `${tag}: current link inside (right)`)
      .toBeLessThanOrEqual(r.navRight + 1);
  }
}

test('Developers selects and the phone nav', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });

  let ringsOn = false;
  try {
    await apiPatch(o, '/admin/features/lounge.rings', {
      enabled: true,
      reason: 'e2e: p80 developers shots (rings composer, real feed)',
    });
    ringsOn = true;
  } catch (e) {
    log(`RINGS switch failed: ${String(e).slice(0, 200)}`);
  }

  const stamp = Date.now().toString(36);
  const appId = (
    await apiPost<{ application: { id: string } }>(o, '/applications', {
      name: `Select app ${stamp}`,
    })
  ).application.id;
  log(`world app=${appId} rings=${ringsOn}`);

  const prefsList: Prefs[] = [
    ...THEMES.map((theme) => ({ theme, zoom: 100 })),
    { theme: 'lounge-editorial', zoom: 100 },
  ];
  for (const prefs of BASE ? prefsList.slice(0, 1) : prefsList) {
    const editorial = prefs.theme === 'lounge-editorial';
    const short = prefs.theme.replace('lounge-', '');
    await setPrefs(prefs);
    for (const [sname, size] of Object.entries(SIZES)) {
      if (editorial && sname !== 'desktop') continue;
      const phone = sname === 'phone';
      const tag = `${short}-${sname}`;
      const c = await context(browser, octx, size, prefs);
      const page = await c.newPage();
      page.setDefaultTimeout(20_000);

      // Rich Presence: the Type select, closed, then focused.
      await reach(`richpresence-${tag}`, async () => {
        await page.goto(`/developers/applications/${appId}?tab=richpresence`);
        const sel = page.locator('.dev-main select').first();
        await expect(sel).toBeVisible();
        await selectLook(page, `richpresence-${tag}`);
        if (!editorial) await shoot(page, `select-closed-${tag}`);
        if (!editorial) {
          await sel.focus();
          await page.keyboard.press('Shift+Tab');
          await page.keyboard.press('Tab');
          await shoot(page, `select-focused-${tag}`);
        }
        if (editorial) await shoot(page, `select-closed-${tag}`);
        const nav = await navLook(page, `app-${tag}`);
        if (nav) checkNav(nav, phone, `app-${tag}`);
        if (phone && !editorial) await shoot(page, `nav-app-${tag}`);
      });

      // Developers home and Documentation: nav figures and shots.
      for (const [name, url] of [
        ['home', '/developers'],
        ['docs', '/developers/docs/getting-started'],
      ] as const) {
        await reach(`${name}-${tag}`, async () => {
          await page.goto(url);
          await page.waitForTimeout(700);
          const nav = await navLook(page, `${name}-${tag}`);
          if (nav) checkNav(nav, phone, `${name}-${tag}`);
          if (phone || (name === 'home' && !editorial)) await shoot(page, `nav-${name}-${tag}`);
          if (phone && !editorial) {
            await page.locator('.dev-nav').evaluate((el) => {
              el.scrollLeft = el.scrollWidth;
            });
            await page.waitForTimeout(250);
            const end = await navLook(page, `${name}-${tag}-scrolled-end`);
            if (end) checkNav(end, phone, `${name}-${tag}-scrolled-end`);
            await shoot(page, `nav-${name}-end-${tag}`);
          }
        });
      }

      // Every other page with a select (desktop, Dark: one each).
      if (!BASE && !phone && prefs.theme === 'lounge-dark') {
        const others: [string, string][] = [
          ['commands', `/developers/applications/${appId}?tab=commands`],
          ['oauth2', `/developers/applications/${appId}?tab=oauth2`],
          ['install', `/developers/applications/${appId}?tab=installation`],
          ['docs-api-reference', '/developers/docs/api-reference'],
          ['docs-gateway-events', '/developers/docs/gateway-events'],
        ];
        for (const [name, url] of others) {
          await reach(`select-${name}`, async () => {
            await page.goto(url);
            if (name === 'commands') {
              // The command builder: a slash command with one option shows both selects.
              await page.getByRole('button', { name: 'Add option' }).click();
            }
            await expect(page.locator('.dev-main select').first()).toBeVisible();
            const n = await page.locator('.dev-main select').count();
            log(`selects on ${name}: ${n}`);
            await selectLook(page, `page-${name}`);
            await shoot(page, `select-page-${name}-${tag}`);
          });
        }
      }

      // Click-through (phone, Dark): choose a value, press each nav link, leave.
      if (!BASE && phone && prefs.theme === 'lounge-dark') {
        await reach('clickthrough', async () => {
          await page.goto(`/developers/applications/${appId}?tab=richpresence`);
          const sel = page.locator('.dev-main select').first();
          await sel.selectOption('2');
          expect(await sel.inputValue()).toBe('2');
          await shoot(page, `select-chosen-${tag}`);
          for (const name of ['Documentation', 'Packages', 'Applications', 'Overview']) {
            await page.locator('.dev-nav').getByRole('link', { name }).click();
            await page.waitForTimeout(500);
            const r = await navLook(page, `pressed-${name}`);
            if (r?.current) {
              expect
                .soft(r.current.left, `pressed ${name}: inside (left)`)
                .toBeGreaterThanOrEqual(r.navLeft - 1);
              expect
                .soft(r.current.right, `pressed ${name}: inside (right)`)
                .toBeLessThanOrEqual(r.navRight + 1);
            }
            await shoot(page, `nav-pressed-${name.toLowerCase()}-${tag}`);
          }
          await page.locator('.dev-nav').evaluate((el) => {
            el.scrollLeft = el.scrollWidth;
          });
          await page.locator('.dev-nav-exit').click();
          await page.waitForURL(/\/channels/);
          expect(page.url()).not.toContain('/developers');
          log(`exit left the Developers pages: ${page.url()}`);
        });
      }

      // Rings composer with the real feed.
      if (!BASE && ringsOn && !editorial) {
        await reach(`ring-composer-${tag}`, async () => {
          await page.goto('/channels/@me');
          await page.getByRole('button', { name: 'Add a ring' }).click();
          await page.getByRole('radio', { name: 'Photo' }).check();
          const input = page.locator('[role=dialog] input[type=file]');
          await expect(input).toBeVisible();
          const look = await input.evaluate((el) => {
            const p = getComputedStyle(el, '::file-selector-button');
            return {
              bg: p.backgroundColor,
              color: p.color,
              radius: p.borderTopLeftRadius,
              right: Math.round(el.getBoundingClientRect().right),
              panelRight: Math.round(
                (el.closest('[role=dialog]') ?? el).getBoundingClientRect().right,
              ),
            };
          });
          log(`ring composer ${tag} ${JSON.stringify(look)}`);
          await shoot(page, `ring-composer-${tag}`);
        });
      }

      await c.close();
    }
  }

  log(`SUMMARY shots=${shots}`);
  await octx.close();
});
