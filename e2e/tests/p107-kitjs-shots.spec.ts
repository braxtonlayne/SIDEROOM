import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { hitTestAll } from '../support/hit-test';
import { apiPatch, expect, test } from '../support/fixtures';

/**
 * D-1021 (a kit Modal's first focus) and D-1022 (the kit Select's rows are 44 px on a phone), wave 107, kitjs.
 * Figures: for Create Channel (by click and by Enter on the opener), Create Event, a confirm (Leave Server, opened
 * and closed with Escape) and Add a Server: the focused element at open, the first Tab's target, whether Escape
 * closes and focus returns to the opener. At 375 touch: three open Select lists (Admin Bans, Admin Accounts, server
 * settings Channels > Permissions "Add a role"): option heights, list height, containment in the viewport, hit-test
 * fails. At 1024: option heights and list height.
 *
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p107-integ/apps/web/dist --env SR_P107_KITJS_ON=1 --env SR_P107_KITJS_PHASE=after --env SR_P107_KITJS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-kitjs -- tests/p107-kitjs-shots.spec.ts
 */
const ON = process.env.SR_P107_KITJS_ON === '1';
const PHASE = process.env.SR_P107_KITJS_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P107_KITJS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p107-kitjs';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
type World = { guildId: string; channelId: string };
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 220);

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const figs: Record<string, any> = {};
const problems: string[] = [];

const settled = async (page: Page) => {
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.waitForTimeout(700);
};

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  if (sideways > 0) say(`${name}: the page scrolls sideways by ${sideways}px`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`), timeout: 10_000 });
}

async function newPage(
  browser: Browser,
  owner: BrowserContext,
  size: { width: number; height: number },
  touch: boolean,
) {
  await apiPatch(owner.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: {} } },
  });
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    hasTouch: touch,
    storageState: await owner.storageState(),
    bypassCSP: true,
  });
  await ctx.addInitScript(() => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({
          state: { prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: {} } },
          version: 2,
        }),
      );
    } catch {
      /* no storage */
    }
  });
  return { ctx, page: await ctx.newPage() };
}

const activeDesc = (page: Page) =>
  page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return 'none';
    const label =
      a.getAttribute('aria-label') ??
      (a.id ? document.querySelector(`label[for="${a.id}"]`)?.textContent : null) ??
      a.textContent ??
      '';
    return `${a.tagName.toLowerCase()} "${label.trim().slice(0, 40)}"`;
  });

async function goChannels(page: Page, w: World, phone: boolean) {
  await page.goto(phone ? `/channels/${w.guildId}` : `/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 30_000,
  });
  await settled(page);
}

async function guildMenuItem(page: Page, w: World, item: string | RegExp): Promise<Locator> {
  const guild = (await (await page.request.get(`/api/v1/guilds/${w.guildId}`)).json()) as {
    name: string;
  };
  const sidebar = page.getByTestId('channel-sidebar').first();
  await sidebar.getByRole('button', { name: guild.name }).first().click();
  return page.getByRole('menuitem', { name: item });
}

/** One dialog: open it, read focus, Tab, Escape, then whether focus is back on the opener. */
async function dialogFigures(
  page: Page,
  key: string,
  dlgName: string | RegExp,
  open: () => Promise<Locator | null>,
  shotFile?: string,
) {
  const out: Record<string, unknown> = {};
  try {
    const opener = await open();
    const dlg = page.getByRole('dialog', { name: dlgName });
    await dlg.waitFor({ timeout: 10_000 });
    await page.waitForTimeout(500);
    out.openFocus = await activeDesc(page);
    out.inDialog = await dlg.evaluate((d) => d.contains(document.activeElement));
    if (shotFile) await shot(page, shotFile);
    await page.keyboard.press('Tab');
    out.firstTab = await activeDesc(page);
    await page.keyboard.press('Escape');
    await dlg.waitFor({ state: 'hidden', timeout: 5000 });
    out.escapeCloses = true;
    out.focusBack = opener ? await opener.evaluate((el) => el === document.activeElement) : null;
    out.afterEscapeFocus = await activeDesc(page);
  } catch (e) {
    out.error = msg(e);
    problems.push(`${key}: ${msg(e)}`);
  }
  figs[key] = out;
  say(`${key}: ${JSON.stringify(out)}`);
}

interface ListFig {
  options: number;
  optionHeights: number[];
  listHeight: number;
  listTop: number;
  listBottom: number;
  viewportH: number;
  fails?: number;
}

async function openList(
  page: Page,
  combo: Locator,
  key: string,
  shotFile: string | null,
  hit: boolean,
) {
  try {
    await combo.scrollIntoViewIfNeeded();
    await combo.click({ timeout: 8000 });
    const list = page.getByRole('listbox').filter({ visible: true }).last();
    await list.waitFor({ timeout: 8000 });
    await page.waitForTimeout(400);
    const f: ListFig = await list.evaluate((l) => {
      const opts = Array.from(l.querySelectorAll('[role=option]')) as HTMLElement[];
      const r = l.getBoundingClientRect();
      return {
        options: opts.length,
        optionHeights: Array.from(
          new Set(opts.map((o) => Math.round(o.getBoundingClientRect().height))),
        ),
        listHeight: Math.round(r.height),
        listTop: Math.round(r.top),
        listBottom: Math.round(r.bottom),
        viewportH: window.innerHeight,
      };
    });
    if (hit) {
      const rows = await hitTestAll(page, list, { also: '[role=option]' });
      f.fails = rows.filter((r) => r.status === 'fail').length;
      for (const r of rows.filter((x) => x.status === 'fail' || x.status === 'not-reached')) {
        const bad = r.result?.points.find((p) => !p.ok);
        say(
          `  ${key} ${r.status} "${r.name}" taker ${bad?.taker ? `${bad.taker.tag} ${bad.taker.path}` : (r.reason ?? '')}`,
        );
      }
    }
    if (shotFile) await shot(page, shotFile);
    figs[key] = f;
    say(`${key}: ${JSON.stringify(f)}`);
    await page.keyboard.press('Escape');
  } catch (e) {
    figs[key] = { error: msg(e) };
    problems.push(`${key}: ${msg(e)}`);
  }
}

test(`p107 kitjs: modal first focus and select rows, ${PHASE}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    {
      const { ctx, page } = await newPage(browser, owner, { width: 1024, height: 768 }, false);
      try {
        await goChannels(page, w, false);
        const createCh = () => page.getByRole('button', { name: /^Create Channel in / }).first();
        await dialogFigures(
          page,
          'createChannel:click',
          'Create Channel',
          async () => {
            const b = createCh();
            await b.click({ timeout: 10_000 });
            return b;
          },
          'create-channel-open-1024',
        );
        await dialogFigures(page, 'createChannel:keyboard', 'Create Channel', async () => {
          const b = createCh();
          await b.focus();
          await page.keyboard.press('Enter');
          return b;
        });
        await dialogFigures(page, 'createEvent:click', /Create Event/, async () => {
          await (await guildMenuItem(page, w, 'Create Event')).click();
          return null;
        });
        await goChannels(page, w, false);
        await dialogFigures(
          page,
          'confirm:leaveServer',
          /Leave/,
          async () => {
            await (await guildMenuItem(page, w, /^Leave/)).click();
            return null;
          },
          'confirm-open-1024',
        );
        await goChannels(page, w, false);
        await dialogFigures(page, 'addServer', /Add a Server/, async () => {
          const b = page.getByRole('button', { name: 'Add a Server' });
          await b.click({ timeout: 10_000 });
          return b;
        });
      } catch (e) {
        problems.push(`dialogs 1024: ${msg(e)}`);
      } finally {
        await ctx.close();
      }
    }
    {
      const { ctx, page } = await newPage(browser, owner, { width: 375, height: 812 }, true);
      try {
        await goChannels(page, w, true);
        await dialogFigures(
          page,
          'createEvent:375',
          /Create Event/,
          async () => {
            await (await guildMenuItem(page, w, 'Create Event')).click();
            return null;
          },
          'create-event-open-375',
        );
      } catch (e) {
        problems.push(`createEvent 375: ${msg(e)}`);
      } finally {
        await ctx.close();
      }
    }
    for (const phone of [true, false]) {
      const size = phone ? { width: 375, height: 812 } : { width: 1024, height: 768 };
      const sz = phone ? '375' : '1024';
      const { ctx, page } = await newPage(browser, owner, size, phone);
      try {
        const lists: [string, string, (p: Page) => Locator][] = [
          ['bans', '/admin/bans', (p) => p.getByRole('main').getByRole('combobox').first()],
          ['accounts', '/admin/accounts', (p) => p.getByRole('main').getByRole('combobox').first()],
          [
            'permissions',
            `/settings/server/${w.guildId}/channels?channel=${w.channelId}&tab=permissions`,
            (p) => p.getByRole('combobox', { name: /Add a role/ }).first(),
          ],
        ];
        for (const [name, path, pick] of lists) {
          try {
            await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
            await settled(page);
            if (name === 'permissions') {
              const trigger = page.getByRole('button', { name: /Add a role/ }).first();
              if (await trigger.count()) await trigger.click().catch(() => undefined);
              await page.waitForTimeout(400);
            }
            await openList(
              page,
              pick(page),
              `select:${name}:${sz}`,
              name === 'bans' ? `select-open-${sz}` : null,
              phone,
            );
          } catch (e) {
            problems.push(`select ${name} ${sz}: ${msg(e)}`);
          }
        }
      } finally {
        await ctx.close();
      }
    }
  } finally {
    await owner.close();
    writeFileSync(
      resolve(OUT, `figures-${PHASE}.json`),
      JSON.stringify({ figs, problems }, null, 2),
    );
  }

  if (PHASE === 'after' && existsSync(resolve(OUT, 'figures-base.json'))) {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const base = JSON.parse(readFileSync(resolve(OUT, 'figures-base.json'), 'utf8')).figs as any;
    for (const key of Object.keys(figs)) {
      const b = base[key];
      const a = figs[key];
      if (!b || a.error || b.error) continue;
      if (key.startsWith('select:') && key.endsWith(':1024')) {
        if (JSON.stringify(a) !== JSON.stringify(b))
          problems.push(
            `${key}: pointer-width list changed: ${JSON.stringify(b)} -> ${JSON.stringify(a)}`,
          );
      }
      if (key.startsWith('select:') && key.endsWith(':375')) {
        if (a.optionHeights?.some((h: number) => h < 44)) problems.push(`${key}: option under 44`);
        if (a.fails) problems.push(`${key}: ${a.fails} hit-test fails`);
        if (a.listTop < 0 || a.listBottom > a.viewportH)
          problems.push(`${key}: list outside the viewport`);
      }
      if (key === 'confirm:leaveServer' && a.openFocus !== b.openFocus)
        problems.push(`${key}: starting button changed: ${b.openFocus} -> ${a.openFocus}`);
      if (a.escapeCloses !== undefined && (a.escapeCloses !== true || a.focusBack === false))
        problems.push(`${key}: Escape or return of focus failed`);
    }
  }
  say(`problems: ${problems.length}`);
  for (const p of problems) say(`PROBLEM ${p}`);
  expect(figs).toBeTruthy();
});
