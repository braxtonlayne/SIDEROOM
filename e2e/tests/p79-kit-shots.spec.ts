import { mkdirSync, appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-310 (wave 79; D-812), on the real stack: the kit's Tabs fade at an edge that hides tabs and scroll the
 * selected tab into view. Saved items on a phone (Bookmarks, Reminders, Scheduled): on open `data-overflow="end"`;
 * after "Scheduled" the tab's right edge is inside the list's client box and the value is `start`. At 1280 no
 * attribute. SR_P79_BASE=1 measures the fault only (run with Tabs.tsx and Tabs.module.css held back). Local only
 * (SR_P79_KIT=1); shots go to SR_P79_KIT_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P79_KIT_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p79-kit';
const BASE = process.env.SR_P79_BASE === '1';
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;

test.skip(process.env.SR_P79_KIT !== '1', 'local visual check only');
test.setTimeout(1_200_000);

const note = (text: string) => {
  appendFileSync(resolve(OUT, 'measurements.txt'), `${text}\n`);
  console.log(text);
};

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

interface Strip {
  overflow: string | null;
  selected: string;
  clientW: number;
  scrollW: number;
  scrollLeft: number;
  listLeft: number;
  listRight: number;
  tabs: { name: string; left: number; right: number; selected: boolean }[];
  mask: string;
  docOver: number;
}

/** Read the tab list named `label` (or the last tab list on the page). */
async function strip(page: Page, label?: string): Promise<Strip> {
  return page.evaluate((lbl) => {
    const lists = Array.from(document.querySelectorAll<HTMLElement>('[role="tablist"]'));
    const el = (lbl ? lists.find((l) => l.getAttribute('aria-label') === lbl) : lists.at(-1))!;
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    const tabs = Array.from(el.querySelectorAll<HTMLElement>('[role="tab"]')).map((t) => {
      const b = t.getBoundingClientRect();
      return {
        name: (t.textContent ?? '').trim(),
        left: b.left,
        right: b.right,
        selected: t.getAttribute('aria-selected') === 'true',
      };
    });
    return {
      overflow: el.getAttribute('data-overflow'),
      selected: tabs.find((t) => t.selected)?.name ?? '',
      clientW: el.clientWidth,
      scrollW: el.scrollWidth,
      scrollLeft: el.scrollLeft,
      listLeft: r.left + el.clientLeft,
      listRight: r.left + el.clientLeft + el.clientWidth,
      tabs,
      mask: cs.maskImage || 'none',
      docOver: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    };
  }, label);
}

const fmt = (where: string, s: Strip) =>
  `${where}: overflow=${s.overflow} selected=${s.selected} scroll ${s.scrollLeft.toFixed(0)}/${s.scrollW}/${s.clientW} ` +
  `list ${s.listLeft.toFixed(0)}..${s.listRight.toFixed(0)} tabs ${s.tabs.map((t) => `${t.name}[${t.left.toFixed(0)}..${t.right.toFixed(0)}]`).join(' ')} ` +
  `mask ${s.mask === 'none' ? 'none' : 'yes'} docOver ${s.docOver}`;

async function world(browser: Browser) {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const a = octx.request;
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    a,
    '/guilds',
    { name: 'Kit Room' },
  );
  const channel = guild.channels.find((c) => c.type === 0)!.id;
  await apiPost(a, `/channels/${channel}/messages`, { content: 'Something to save' });
  return { octx, a, guild, channel };
}

async function setTheme(a: APIRequestContext, theme: string) {
  await apiPatch(a, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
}

async function openSaved(page: Page, guildId: string, channel: string) {
  await page.goto(`/channels/${guildId}/${channel}`);
  await expect(page.locator('.lng-composer-wrap').last().locator('textarea')).toBeVisible();
  await page.waitForTimeout(600);
  await page.getByRole('button', { name: 'Saved items', exact: true }).first().click();
  await expect(page.getByRole('tab', { name: 'Bookmarks' })).toBeVisible();
  await page.waitForTimeout(500);
}

test('the base: Saved items on a phone has tabs past the edge and no cue', async ({ browser }) => {
  test.skip(!BASE, 'base measurement only (SR_P79_BASE=1)');
  const { octx, a, guild, channel } = await world(browser);
  await setTheme(a, 'lounge-dark');
  const c = await context(browser, octx, VIEWPORTS.phone, 'lounge-dark');
  const page = await c.newPage();
  await openSaved(page, guild.id, channel);
  const s = await strip(page, 'Saved items');
  note(fmt('BASE phone saved', s));
  await page.screenshot({ path: resolve(OUT, 'base-saved-phone.png') });
  expect(s.tabs.at(-1)!.right, 'base: the last tab ends past the list').toBeGreaterThan(
    s.listRight + 0.5,
  );
  expect(s.overflow, 'base: no cue').toBeNull();
  await octx.close();
});

test('Tabs show an overflow cue and keep the selected tab in view', async ({ browser }) => {
  test.skip(BASE, 'not in the base run');
  const { octx, a, guild, channel } = await world(browser);
  for (const theme of THEMES) {
    await setTheme(a, theme);
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      if (theme === 'lounge-editorial' && vp === 'desktop') continue;
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(15_000);
      await openSaved(page, guild.id, channel);
      const root = await page.evaluate(() => document.documentElement.dataset.theme ?? '');
      note(`${tag}: data-theme=${root}`);
      const phone = vp === 'phone';

      // 1. As opened.
      let s = await strip(page, 'Saved items');
      note(fmt(`${tag} saved open`, s));
      await page.screenshot({ path: resolve(OUT, `saved-open-${tag}.png`) });
      expect(s.docOver, `${tag} page does not scroll sideways`).toBeLessThanOrEqual(0);
      if (phone) {
        expect(s.overflow, `${tag} open: the end fades`).toBe('end');
        expect(s.mask, `${tag} open: mask`).not.toBe('none');
      } else {
        // The popover is only 318 px wide at 1280 too, so this strip really overflows: the cue follows the data.
        expect(s.overflow, `${tag}: cue matches overflow`).toBe(
          s.scrollW > s.clientW ? 'end' : null,
        );
      }

      // 2. Scheduled.
      await page.getByRole('tab', { name: /^Scheduled/ }).click();
      await page.waitForTimeout(700);
      s = await strip(page, 'Saved items');
      note(fmt(`${tag} saved scheduled`, s));
      await page.screenshot({ path: resolve(OUT, `saved-scheduled-${tag}.png`) });
      const sel = s.tabs.find((t) => t.selected)!;
      expect(sel.name).toMatch(/^Scheduled/);
      expect(sel.right, `${tag}: Scheduled inside the list`).toBeLessThanOrEqual(s.listRight + 1);
      expect(sel.left).toBeGreaterThanOrEqual(s.listLeft - 1);
      if (phone) expect(s.overflow, `${tag} scheduled: the start fades`).toBe('start');
      else expect(s.overflow).toBe('start');

      // 3. Back to Bookmarks.
      await page.getByRole('tab', { name: 'Bookmarks' }).click();
      await page.waitForTimeout(700);
      s = await strip(page, 'Saved items');
      note(fmt(`${tag} saved bookmarks`, s));
      await page.screenshot({ path: resolve(OUT, `saved-bookmarks-${tag}.png`) });
      if (phone) {
        expect(s.overflow).toBe('end');
        // Click through: every tab in turn is selected and whole.
        for (const name of [/^Reminders/, /^Scheduled/, 'Bookmarks']) {
          await page.getByRole('tab', { name }).click();
          await page.waitForTimeout(400);
          const t = await strip(page, 'Saved items');
          const cur = t.tabs.find((x) => x.selected)!;
          expect(cur.right).toBeLessThanOrEqual(t.listRight + 1);
          expect(cur.left).toBeGreaterThanOrEqual(t.listLeft - 1);
        }
      } else {
        // Arrow keys across the strip.
        await page.getByRole('tab', { name: 'Bookmarks' }).focus();
        await page.keyboard.press('ArrowRight');
        await expect(page.getByRole('tab', { name: /^Reminders/ })).toHaveAttribute(
          'aria-selected',
          'true',
        );
        await page.keyboard.press('End');
        await expect(page.getByRole('tab', { name: /^Scheduled/ })).toHaveAttribute(
          'aria-selected',
          'true',
        );
        await page.keyboard.press('Home');
        await expect(page.getByRole('tab', { name: 'Bookmarks' })).toHaveAttribute(
          'aria-selected',
          'true',
        );
      }
      await page.keyboard.press('Escape');

      if (theme !== 'lounge-editorial') {
        // Two other Tabs users: Cinema's My list and the Friends page.
        for (const [name, url] of [
          ['mylist', '/cinema/my-list'],
          ['friends', '/channels/@me'],
        ] as const) {
          await page.goto(url);
          await page.waitForTimeout(1200);
          const n = await page.locator('[role="tablist"]').count();
          if (n === 0) {
            note(`${tag} ${name}: NOT REACHED (no tab list at ${url})`);
            continue;
          }
          const o = await strip(page);
          note(fmt(`${tag} ${name}`, o));
          await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
        }
      }
      await c.close();
    }
  }
  await octx.close();
});
