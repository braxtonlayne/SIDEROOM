import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { CONTROL_SELECTOR, hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * D-1016 (wave 106, solo builder tier): at 720 px and under every control a member taps in a tier list is a 44 px tall
 * target: the tier's remove button, its name field, and the controls under the board.
 *
 * Runs on the seeded sweep world (owner.json). Data (D-942): one guild `P106 tier shots` (found by that name, else made),
 * per configuration one text channel `p106-<phase>-<run>-<tag>` (deleted in a finally), four items `P106 tier a..d`.
 * Per configuration (375 Light touch, 375 Dark touch, 1024 Light) it records: every control under the tool (tag, class,
 * name, box, hit status, takers), the tier head and row boxes, the board's height, the chips' boxes, the scroll widths.
 * Phases: `SR_P106_TIER_PHASE=base|after`; `figures-<phase>.json`; the after phase compares with `figures-base.json`
 * when it is in the folder (`report-after.json`).
 *
 * Command: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_TIER_ON=1 --env SR_P106_TIER_PHASE=after --env SR_P106_TIER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-tier -- tests/p106-tier-shots.spec.ts
 */
const ON = process.env.SR_P106_TIER_ON === '1';
const PHASE = process.env.SR_P106_TIER_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P106_TIER_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-tier';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const GUILD_NAME = 'P106 tier shots';
const RUN = Date.now().toString(36);

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

type Theme = 'lounge-light' | 'lounge-dark';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: Theme;
  shot: string;
}
const CONFIGS: Cfg[] = [
  {
    tag: '375-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shot: 'tier-375-light',
  },
  {
    tag: '375-dark',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-dark',
    shot: 'tier-375-dark',
  },
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shot: 'tier-1024-light',
  },
];

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface CtlFig {
  key: string;
  tag: string;
  cls: string;
  name: string;
  box: Box | null;
  status: string;
  takers: string[];
}
const say = (s: string) => console.log(`REPORT ${s}`);
const r1 = (n: number) => Math.round(n * 10) / 10;

async function setMain(ownerCtx: BrowserContext, theme: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(theme);
}

async function newPage(browser: Browser, owner: BrowserContext, c: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: c.width, height: c.height },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(c.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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
  }, c.theme);
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

const sideways = (page: Page) =>
  page.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await sideways(page);
  if (sw! > cw!) say(`sideways scroll ${sw} > ${cw} before ${name}`);
  await page.waitForTimeout(350);
  await page.screenshot({
    path: resolve(OUT, `${PHASE === 'base' ? 'base-' : ''}${name}.png`),
    timeout: 10_000,
  });
}

async function openTool(page: Page, query: string, pick: RegExp) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(page.getByRole('dialog', { name: 'Drawer', exact: true })).toBeVisible({
      timeout: 1000,
    });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type(`>${query}`);
  await page.waitForTimeout(600);
  await page.getByRole('option', { name: pick }).first().click();
  await page.waitForTimeout(900);
}

async function closeTool(page: Page) {
  const dlg = page.getByRole('dialog', { name: 'Drawer', exact: true });
  for (let i = 0; i < 3 && (await dlg.isVisible().catch(() => false)); i++) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(250);
  }
}

test('Tier list: every control a 44 px target at 375', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const problems: string[] = [];
  const all: Record<string, Record<string, unknown>> = {};
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const made: string[] = [];
  try {
    const mine = (await (await owner.request.get('/api/v1/users/@me/guilds')).json()) as {
      id: string;
      name: string;
    }[];
    let guildId = mine.find((g) => g.name === GUILD_NAME)?.id;
    if (!guildId)
      guildId = (await apiPost<{ id: string }>(owner.request, '/guilds', { name: GUILD_NAME })).id;
    const baseFile = resolve(OUT, 'figures-base.json');
    const base =
      PHASE === 'after' && existsSync(baseFile)
        ? (JSON.parse(readFileSync(baseFile, 'utf8')) as typeof all)
        : null;
    if (PHASE === 'after' && !base) say('no figures-base.json: nothing compared');
    const flush = () => {
      writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(all, null, 1));
      writeFileSync(resolve(OUT, `report-${PHASE}.json`), JSON.stringify({ problems }, null, 1));
    };

    for (const cfg of CONFIGS) {
      await setMain(owner, cfg.theme);
      const room = await apiPost<{ id: string }>(owner.request, `/guilds/${guildId}/channels`, {
        name: `p106-${PHASE}-${RUN}-${cfg.tag}`.toLowerCase(),
        type: 0,
      });
      made.push(room.id);
      const { ctx, page } = await newPage(browser, owner, cfg);
      const fig: Record<string, unknown> = {};
      all[cfg.tag] = fig;
      try {
        await page.goto(`/channels/${guildId}/${room.id}`, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        });
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        fig['theme'] = await page.evaluate(() => [
          document.documentElement.getAttribute('data-theme'),
          document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
        ]);
        if (!(fig['theme'] as (string | null)[]).includes(cfg.theme))
          problems.push(`${cfg.tag}: data-theme is ${JSON.stringify(fig['theme'])}`);

        await openTool(page, 'tier list', /tier list/i);
        const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await expect(drawer).toBeVisible();
        await drawer.getByRole('button', { name: 'Start a new one' }).click();
        const rowsBtn = drawer.getByRole('button', { name: 'Add the rows S to D' });
        if (await rowsBtn.isVisible().catch(() => false)) await rowsBtn.click();
        for (const s of ['a', 'b', 'c', 'd']) {
          const n = `P106 tier ${s}`;
          await drawer.getByLabel('Item', { exact: true }).fill(n);
          await drawer.getByRole('button', { name: 'Add item' }).click();
          await expect(drawer.locator('li.sr-room-chip').filter({ hasText: n })).toHaveCount(1);
        }
        await page.waitForTimeout(400);
        const tool = drawer.locator('.sr-room').first();

        const geo = await tool.evaluate((root) => {
          const rnd = (n: number) => Math.round(n * 10) / 10;
          const bx = (e: Element) => {
            const b = e.getBoundingClientRect();
            return { x: rnd(b.x), y: rnd(b.y), w: rnd(b.width), h: rnd(b.height) };
          };
          const rows = Array.from(root.querySelectorAll('section.sr-tier-row'));
          return {
            rows: rows.map(bx),
            heads: rows.map((r) => bx(r.querySelector('.sr-tier-head')!)),
            names: rows.map((r) => bx(r.querySelector('.sr-tier-name')!)),
            heads_x: rows.map((r) => bx(r.querySelector('.sr-tier-head .sr-room-x')!)),
            board: bx(root.querySelector('.sr-tier-board')!),
            chips: Array.from(root.querySelectorAll('li.sr-room-chip')).map((c) => ({
              chip: bx(c),
              x: bx(c.querySelector('.sr-room-x')!),
              move: bx(c.querySelector('.sr-room-move')!),
            })),
          };
        });
        Object.assign(fig, geo);
        fig['scroll'] = await sideways(page);

        await openCollapsed(page, tool);
        const hit = await hitTestAll(page, tool, cfg.touch ? {} : { reach: 'box' });
        const meta = await tool
          .locator(CONTROL_SELECTOR)
          .evaluateAll((els) =>
            els.map((e) => ({ cls: e.getAttribute('class') ?? '', tag: e.tagName.toLowerCase() })),
          );
        const ctl: CtlFig[] = [];
        for (const r of hit) {
          const loc = tool.locator(CONTROL_SELECTOR).nth(r.index);
          const b = r.status === 'skipped' ? null : await loc.boundingBox();
          ctl.push({
            key: `${r.index}:${r.tag}:${r.name}`,
            tag: r.tag,
            cls: meta[r.index]?.cls ?? '',
            name: r.name,
            box: b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null,
            status: r.status,
            takers: (r.result?.points ?? []).filter((p) => !p.ok).map((p) => p.taker?.path ?? '?'),
          });
        }
        fig['controls'] = ctl;
        fig['hitFails'] = ctl.filter(
          (c) => c.status === 'fail' || c.status === 'not-reached',
        ).length;
        say(
          `${cfg.tag}: fails ${fig['hitFails']} ${JSON.stringify(ctl.filter((c) => c.status !== 'pass' && c.status !== 'skipped').map((c) => [c.cls || c.tag, c.name, c.box?.w, c.box?.h, c.takers[0]]))}`,
        );
        say(
          `${cfg.tag}: rows ${JSON.stringify((geo.rows as Box[]).map((b) => b.h))} board ${(geo.board as Box).h}`,
        );

        await tool.locator('.sr-tier-board').scrollIntoViewIfNeeded();
        await shot(page, cfg.shot);
        await closeTool(page);
      } catch (e) {
        problems.push(`${cfg.tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      } finally {
        await ctx.close();
        flush();
      }
    }

    /* ---- compare with the base ---- */
    if (base) {
      for (const cfg of CONFIGS) {
        /* eslint-disable @typescript-eslint/no-explicit-any */
        const b = base[cfg.tag] as Record<string, any> | undefined;
        const a = all[cfg.tag] as Record<string, any> | undefined;
        if (!a || !b) continue;
        if (a['scroll'][0] > a['scroll'][1]) problems.push(`${cfg.tag}: sideways scroll`);
        const bc: CtlFig[] = b['controls'] ?? [];
        const ac: CtlFig[] = a['controls'] ?? [];
        for (const c of ac) {
          const o = bc.find((x) => x.key === c.key);
          if (!o?.box || !c.box) continue;
          if (Math.abs(o.box.w - c.box.w) > 0.5)
            problems.push(`${cfg.tag}: ${c.key} width ${o.box.w} -> ${c.box.w}`);
          if (!cfg.touch && Math.abs(o.box.h - c.box.h) > 0.5)
            problems.push(`${cfg.tag}: ${c.key} height moved at ${cfg.width}`);
        }
        if (cfg.touch) {
          /* the room bar (All boards, Title, Delete) is RoomShell's, shared with the poll builder: named, not failed */
          const shell = /^(All boards|Title|Delete)$/;
          const bad = ac.filter(
            (c) => (c.status === 'fail' || c.status === 'not-reached') && !shell.test(c.name),
          );
          say(
            `${cfg.tag}: room bar fails left (RoomShell): ${ac
              .filter((c) => c.status === 'fail' && shell.test(c.name))
              .map((c) => c.name)
              .join(', ')}`,
          );
          if (bad.length > 0)
            problems.push(
              `${cfg.tag}: hit fails ${bad.length}: ${JSON.stringify(bad.map((c) => [c.key, c.takers[0]]))}`,
            );
          for (const h of a['heads_x'] as Box[])
            if (h.h < 43.5) problems.push(`${cfg.tag}: head remove ${h.h} tall, wanted 44`);
          for (const h of a['names'] as Box[])
            if (h.h < 43.5 || h.h > 44.5)
              problems.push(`${cfg.tag}: tier name ${h.h} tall, wanted 44`);
          (b['chips'] as any[]).forEach((c, i) => {
            const n = a['chips'][i];
            if (n && (Math.abs(c.chip.h - n.chip.h) > 0.5 || Math.abs(c.chip.w - n.chip.w) > 0.5))
              problems.push(`${cfg.tag}: chip ${i} box changed`);
          });
        } else {
          if (JSON.stringify(b['rows']) !== JSON.stringify(a['rows']))
            problems.push(`${cfg.tag}: 1024 row boxes changed`);
          if (b['board'].h !== a['board'].h) problems.push(`${cfg.tag}: 1024 board height changed`);
        }
        /* eslint-enable @typescript-eslint/no-explicit-any */
      }
    }
    flush();
    for (const p of problems) say(`PROBLEM ${p}`);
    say(`${PHASE}: problems ${problems.length}`);
    if (PHASE === 'after') expect(problems, 'no problems').toEqual([]);
  } finally {
    for (const id of made)
      await owner.request.delete(`/api/v1/channels/${id}`).catch(() => undefined);
    await owner.close();
  }
});
