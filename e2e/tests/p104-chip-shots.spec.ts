import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { parseColor, ratio, over, textRatio } from '../support/contrast';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { MENU_ITEM_SELECTOR, hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-480, 483; D-993, D-994 (wave 104, solo builder chip): the tier list's Move control is a menu button on the
 * chip (it was a raw select, 80 x 21, "Unranked" cut by its chevron at 375), and a tool card's own inputs and textareas
 * draw the kit's field ground `--ui-bg-raised`.
 *
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited). Data (D-942): one guild named `P104 chip shots` (found again by that whole name, else made) and,
 * per run and configuration, one text channel with its own name; the items are named `P104 <run> a|b|c`. Nothing else is
 * touched.
 *
 * Per configuration (1024 Lounge Light / Dark / Editorial, 1280 Light, 375 Light touch) it opens the tier list (Ctrl+K,
 * `>tier list`), starts a room, makes the rows S to D and three items (a to S, b to A by the control itself, c stays
 * Unranked), and records: per chip the chip's, the Move control's and the x's boxes, the tier rows' heights, the label
 * column's width, the hit test over the drawer (`reach: 'box'` at pointer widths, the default at 375), the open menu's
 * rows hit-tested (375 and 1024), the sideways scroll; at 1024 Light the keyboard walk and the drag check (after phase).
 * Then the Invite card: computed ground of an input, the textarea and a kit select, text and placeholder ratios on the
 * ground, the border against the card, every box. And the tier list's room-shell input beside the room's kit selects.
 *
 * Phases: `SR_P104_CHIP_PHASE=base` (the control is a select there; moves use selectOption; shots `base-` for the two
 * base shots only) or the default `after`. `figures-<phase>.json`; `figures-after.json` is compared with
 * `figures-base.json` when that is in the folder (`report-after.json`: problems, notes).
 *
 * Local only (SR_P104_CHIP_ON=1). SR_P104_CHIP_OUT is the folder. From e2e/, on a stack with the sweep's world (API 8802,
 * Vite 5802, metrics 9502):
 *   SR_E2E_BASE_URL=http://localhost:5802 SR_P104_CHIP_ON=1 SR_P104_CHIP_PHASE=base SR_P104_CHIP_OUT=<folder> \
 *   npx playwright test tests/p104-chip-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 */
const ON = process.env.SR_P104_CHIP_ON === '1';
const PHASE = process.env.SR_P104_CHIP_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P104_CHIP_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-chip';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const GUILD_NAME = 'P104 chip shots';
const RUN = Date.now().toString(36);
const BASE_SHOTS = new Set(['tier-1024-lounge-light', 'invite-1024-lounge-light']);

test.skip(!ON, 'local visual check only');
test.setTimeout(900_000);

type Theme = 'lounge-editorial' | 'lounge-light' | 'lounge-dark';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: Theme;
  shots: boolean;
  keyboard: boolean;
  menu: boolean;
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-lounge-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: true,
    keyboard: true,
    menu: true,
  },
  {
    tag: '1024-lounge-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: true,
    keyboard: false,
    menu: false,
  },
  {
    tag: '1024-lounge-editorial',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-editorial',
    shots: false,
    keyboard: false,
    menu: false,
  },
  {
    tag: '1280-lounge-light',
    width: 1280,
    height: 800,
    touch: false,
    theme: 'lounge-light',
    shots: false,
    keyboard: false,
    menu: false,
  },
  {
    tag: '375-lounge-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: true,
    keyboard: false,
    menu: true,
  },
];

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
const say = (s: string) => console.log(`REPORT ${s}`);

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

async function shot(page: Page, cfg: Cfg, name: string) {
  const file = `${name}-${cfg.tag}`;
  if (PHASE === 'base' && !BASE_SHOTS.has(file)) return;
  mkdirSync(OUT, { recursive: true });
  await page.waitForTimeout(350);
  await page.screenshot({
    path: resolve(OUT, `${PHASE === 'base' ? 'base-' : ''}${file}.png`),
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

const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};
const r1 = (n: number) => Math.round(n * 10) / 10;
const r2 = (n: number) => Math.round(n * 100) / 100;

const chipOf = (drawer: Locator, name: string) =>
  drawer.locator('li.sr-room-chip').filter({ hasText: name });

/** Moves a chip to a tier by the control: a select in the base, the menu after. */
async function moveBy(page: Page, drawer: Locator, name: string, tier: string) {
  const chip = chipOf(drawer, name);
  if (PHASE === 'base') {
    await chip.locator('select.sr-room-move').selectOption({ label: tier });
  } else {
    await chip.locator('.sr-room-move').click();
    await page.getByRole('menuitemradio', { name: tier, exact: true }).click();
  }
  await expect(
    drawer.getByRole('list', { name: tier, exact: true }).locator('li.sr-room-chip', {
      hasText: name,
    }),
  ).toHaveCount(1);
}

interface TierFig {
  chips: { name: string; zone: string; chip: Box | null; move: Box | null; x: Box | null }[];
  rows: { name: string; h: number; head: number }[];
  unrankedZone: Box | null;
  scroll: number[];
}
async function measureTier(drawer: Locator): Promise<TierFig> {
  const chips: TierFig['chips'] = [];
  const all = drawer.locator('li.sr-room-chip');
  for (let i = 0; i < (await all.count()); i++) {
    const c = all.nth(i);
    chips.push({
      name: ((await c.locator('.sr-room-chip-text').textContent()) ?? '').trim(),
      zone: (await c.locator('xpath=ancestor::ul[1]').getAttribute('aria-label')) ?? '',
      chip: await boxOf(c),
      move: await boxOf(c.locator('.sr-room-move')),
      x: await boxOf(c.locator('.sr-room-x')),
    });
  }
  const rows: TierFig['rows'] = [];
  const rl = drawer.locator('section.sr-tier-row');
  for (let i = 0; i < (await rl.count()); i++) {
    const row = rl.nth(i);
    const b = await boxOf(row);
    const hd = await boxOf(row.locator('.sr-tier-head'));
    rows.push({
      name: (await row.locator('input.sr-tier-name').inputValue()) || String(i),
      h: b?.h ?? 0,
      head: hd?.w ?? 0,
    });
  }
  const unrankedZone = await boxOf(drawer.getByRole('list', { name: 'Unranked', exact: true }));
  const scroll = await drawer.evaluate((el) => {
    const de = document.documentElement;
    return [de.scrollWidth, de.clientWidth, el.scrollWidth, el.clientWidth];
  });
  return { chips, rows, unrankedZone, scroll };
}

const countRows = (rows: Awaited<ReturnType<typeof hitTestAll>>) => {
  const o = { pass: 0, fail: 0, notReached: 0, skipped: 0 };
  for (const r of rows) {
    if (r.status === 'pass') o.pass++;
    else if (r.status === 'fail') o.fail++;
    else if (r.status === 'not-reached') o.notReached++;
    else o.skipped++;
  }
  return o;
};
const fails = (rows: Awaited<ReturnType<typeof hitTestAll>>) =>
  rows
    .filter((r) => r.status === 'fail' || r.status === 'not-reached')
    .map((r) => ({
      status: r.status,
      control: `${r.tag} "${r.name}"`,
      points: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map((p) => ({ at: p.at, taker: p.taker })),
    }));

/** Computed grounds and ratios of one field in the Invite card or the room shell. */
async function fieldFig(l: Locator, cardGroundOf: Locator) {
  const f = await l.evaluate((el) => {
    const cs = getComputedStyle(el);
    const ph = getComputedStyle(el, '::placeholder');
    return {
      bg: cs.backgroundColor,
      color: cs.color,
      border: cs.borderTopColor,
      placeholder: ph.color,
      hasPlaceholder: 'placeholder' in el && !!(el as HTMLInputElement).placeholder,
      tag: el.tagName.toLowerCase(),
      cls: el.className,
    };
  });
  const ground = await cardGroundOf.evaluate((el) => {
    for (let n: Element | null = el; n; n = n.parentElement) {
      const c = getComputedStyle(n).backgroundColor;
      if (c !== 'rgba(0, 0, 0, 0)' && c !== 'transparent') return c;
    }
    return 'rgb(255, 255, 255)';
  });
  const gr = parseColor(ground);
  return {
    ...f,
    cardGround: ground,
    text: r2(textRatio(f.color, f.bg)),
    placeholderRatio: f.hasPlaceholder ? r2(textRatio(f.placeholder, f.bg)) : null,
    borderOnCard: r2(ratio(over(parseColor(f.border), gr), gr)),
  };
}

test('Tier list Move menu button and the card field ground', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const problems: string[] = [];
  const notes: string[] = [];
  const all: Record<string, Record<string, unknown>> = {};
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    const mine = (await (await owner.request.get('/api/v1/users/@me/guilds')).json()) as {
      id: string;
      name: string;
    }[];
    let guildId = mine.find((g) => g.name === GUILD_NAME)?.id;
    if (!guildId) {
      const g = await apiPost<{ id: string }>(owner.request, '/guilds', { name: GUILD_NAME });
      guildId = g.id;
    }
    const baseFile = resolve(OUT, 'figures-base.json');
    const base =
      PHASE === 'after' && existsSync(baseFile)
        ? (JSON.parse(readFileSync(baseFile, 'utf8')) as typeof all)
        : null;
    if (PHASE === 'after' && !base) say('no figures-base.json: nothing compared');
    const flush = () => {
      writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(all, null, 1));
      writeFileSync(
        resolve(OUT, `report-${PHASE}.json`),
        JSON.stringify({ problems, notes }, null, 1),
      );
    };

    for (const cfg of CONFIGS) {
      await setMain(owner, cfg.theme);
      const room = await apiPost<{ id: string }>(owner.request, `/guilds/${guildId}/channels`, {
        name: `p104-${PHASE}-${RUN}-${cfg.tag}`.toLowerCase(),
        type: 0,
      });
      const { ctx, page } = await newPage(browser, owner, cfg);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      const fig: Record<string, unknown> = {};
      all[cfg.tag] = fig;
      const touch = cfg.touch;
      try {
        await page.goto(`/channels/${guildId}/${room.id}`, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        });
        await expect(page.locator('.lng-root').first()).toBeVisible({ timeout: 60_000 });
        fig['theme'] = await page.evaluate(() => document.documentElement.dataset['theme'] ?? null);

        /* ---- the tier list ---- */
        await openTool(page, 'tier list', /tier list/i);
        const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await expect(drawer).toBeVisible();
        await drawer.getByRole('button', { name: 'Start a new one' }).click();
        const rows = drawer.getByRole('button', { name: 'Add the rows S to D' });
        if (await rows.isVisible().catch(() => false)) await rows.click();
        const names = ['a', 'b', 'c'].map((s) => `P104 ${RUN} ${s}`);
        for (const n of names) {
          await drawer.getByLabel('Item', { exact: true }).fill(n);
          await drawer.getByRole('button', { name: 'Add item' }).click();
          await expect(chipOf(drawer, n)).toHaveCount(1);
        }
        await moveBy(page, drawer, names[0]!, 'S');
        await moveBy(page, drawer, names[1]!, 'A');
        await page.waitForTimeout(500);
        const tier = await measureTier(drawer);
        fig['tier'] = tier;
        say(
          `${cfg.tag}: chips ${JSON.stringify(tier.chips.map((c) => [c.chip?.w, c.chip?.h, c.move?.w, c.move?.h]))}`,
        );
        say(
          `${cfg.tag}: rows ${JSON.stringify(tier.rows.map((r) => r.h))} head ${tier.rows[0]?.head}`,
        );
        await drawer.getByRole('list', { name: 'S', exact: true }).scrollIntoViewIfNeeded();
        await shot(page, cfg, 'tier');

        const opened = await openCollapsed(page, drawer);
        const hitRows = await hitTestAll(page, drawer, touch ? {} : { reach: 'box' });
        const h = countRows(hitRows);
        fig['hit'] = { counts: h, fails: fails(hitRows), opened: opened.length };
        say(`${cfg.tag}: hit ${JSON.stringify(h)} fails ${JSON.stringify(fails(hitRows))}`);
        if (h.notReached)
          problems.push(`${cfg.tag}: ${h.notReached} controls not reached (probe error)`);

        // colours of the room shell: its input beside the room's kit selects
        const sels = drawer.locator('select');
        if ((await sels.count()) > 0) {
          fig['room'] = {
            input: await fieldFig(drawer.getByLabel('Item', { exact: true }), drawer),
            select: await fieldFig(sels.first(), drawer),
            selectCount: await sels.count(),
          };
        }

        if (cfg.menu && PHASE === 'after') {
          const btn = chipOf(drawer, names[2]!).locator('.sr-room-move');
          await btn.scrollIntoViewIfNeeded();
          await btn.click();
          const menu = page.getByRole('menu');
          await expect(menu).toBeVisible();
          // the pointer on a plain item (not the checked one), no submenu open (D-943)
          const plain = menu.getByRole('menuitemradio', { name: 'B', exact: true });
          await plain.hover();
          await expect(plain).toBeVisible();
          const mh = await hitTestAll(page, menu, {
            also: MENU_ITEM_SELECTOR,
            ...(touch ? {} : { reach: 'box' }),
          });
          fig['menu'] = {
            counts: countRows(mh),
            fails: fails(mh),
            items: await menu.getByRole('menuitemradio').allTextContents(),
            checked: await menu.getByRole('menuitemradio', { checked: true }).allTextContents(),
            box: await boxOf(menu),
            itemBoxes: await Promise.all(
              (await menu.getByRole('menuitemradio').all()).map((i) => boxOf(i)),
            ),
          };
          say(`${cfg.tag}: menu ${JSON.stringify(fig['menu'])}`);
          await plain.hover();
          await shot(page, cfg, 'tier-menu');
          await page.keyboard.press('Escape');
          await expect(menu).toHaveCount(0);
        }

        if (cfg.keyboard && PHASE === 'after') {
          const walk: Record<string, unknown> = {};
          const nameC = names[2]!;
          const active = () =>
            page.evaluate(() => {
              const a = document.activeElement as HTMLElement | null;
              return a
                ? `${a.tagName.toLowerCase()}|${a.getAttribute('aria-label') ?? ''}|${a.closest('[data-chip]') ? 'chip' : ''}`
                : 'none';
            });
          await drawer.getByLabel('New tier', { exact: true }).focus();
          await page.keyboard.press('Tab');
          await page.keyboard.press('Tab');
          walk['tabReaches'] = await active();
          await page.keyboard.press('Enter');
          await expect(page.getByRole('menu')).toBeVisible();
          walk['enterOpens'] = true;
          walk['firstFocus'] = await page.evaluate(
            () => document.activeElement?.textContent ?? 'none',
          );
          await page.keyboard.press('ArrowDown');
          const arrowed = await page.evaluate(() => document.activeElement?.textContent ?? 'none');
          walk['arrowMoved'] = arrowed;
          await page.keyboard.press('Enter');
          await expect(page.getByRole('menu')).toHaveCount(0);
          await page.waitForTimeout(400);
          const where = await drawer
            .locator('li.sr-room-chip', { hasText: nameC })
            .locator('xpath=ancestor::ul[1]')
            .getAttribute('aria-label');
          walk['chosen'] = arrowed;
          walk['itemNowIn'] = where;
          walk['focusAfterMove'] = await active();
          // Space opens, Escape closes with focus back on this chip's button
          await page.keyboard.press('Space');
          const spaceOpens = await page.getByRole('menu').isVisible();
          await page.keyboard.press('Escape');
          await page.waitForTimeout(250);
          walk['spaceOpens'] = spaceOpens;
          walk['escapeFocus'] = await active();
          fig['keyboard'] = walk;
          say(`${cfg.tag}: keyboard ${JSON.stringify(walk)}`);
          if (!/^button\|Move /.test(String(walk['focusAfterMove'])))
            problems.push(
              `${cfg.tag}: focus after a menu move is ${String(walk['focusAfterMove'])}`,
            );
          if (!/^button\|Move /.test(String(walk['escapeFocus'])))
            problems.push(`${cfg.tag}: focus after Escape is ${String(walk['escapeFocus'])}`);

          // drag: from the chip's text it moves; from the Move button it neither moves nor opens the menu
          const textB = await chipOf(drawer, names[0]!).locator('.sr-room-chip-text').boundingBox();
          const dropB = await drawer.getByRole('list', { name: 'C', exact: true }).boundingBox();
          const drag: Record<string, unknown> = {};
          if (textB && dropB) {
            await page.mouse.move(textB.x + 4, textB.y + textB.height / 2);
            await page.mouse.down();
            await page.mouse.move(dropB.x + 40, dropB.y + dropB.height / 2, { steps: 12 });
            await page.mouse.up();
            await page.waitForTimeout(500);
            drag['fromText'] = await drawer
              .locator('li.sr-room-chip', { hasText: names[0]! })
              .locator('xpath=ancestor::ul[1]')
              .getAttribute('aria-label');
          }
          const mb = await chipOf(drawer, names[1]!).locator('.sr-room-move').boundingBox();
          const dropA = await drawer.getByRole('list', { name: 'D', exact: true }).boundingBox();
          if (mb && dropA) {
            await page.mouse.move(mb.x + mb.width / 2, mb.y + mb.height / 2);
            await page.mouse.down();
            await page.mouse.move(dropA.x + 40, dropA.y + dropA.height / 2, { steps: 12 });
            await page.mouse.up();
            await page.waitForTimeout(500);
            drag['fromButtonStaysIn'] = await drawer
              .locator('li.sr-room-chip', { hasText: names[1]! })
              .locator('xpath=ancestor::ul[1]')
              .getAttribute('aria-label');
            drag['menuOpenAfter'] = await page.getByRole('menu').isVisible();
          }
          fig['drag'] = drag;
          say(`${cfg.tag}: drag ${JSON.stringify(drag)}`);
          if (drag['fromText'] !== 'C')
            problems.push(`${cfg.tag}: drag from the text did not move the chip`);
          if (drag['fromButtonStaysIn'] !== 'A')
            problems.push(`${cfg.tag}: a press on the button started a drag`);
          if (drag['menuOpenAfter'])
            problems.push(`${cfg.tag}: the menu opened at the end of a drag`);
          await page.keyboard.press('Escape');
        }
        await closeTool(page);

        /* ---- the Invite card ---- */
        await openTool(page, 'invite card', /invite card/i);
        const dr2 = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await expect(dr2).toBeVisible();
        await page.waitForTimeout(600);
        await expect(dr2.locator('.sr-card')).toBeVisible();
        const card = dr2.locator('.sr-card');
        const inputs = card.locator('input,textarea,select,button');
        const boxes: Record<string, Box | null> = {};
        for (let i = 0; i < (await inputs.count()); i++) {
          const l = inputs.nth(i);
          const key = `${i}:${await l.evaluate((e) => e.tagName.toLowerCase())}:${(await l.getAttribute('aria-label')) ?? (await l.textContent())?.slice(0, 20) ?? ''}`;
          boxes[key] = await boxOf(l);
        }
        const inv: Record<string, unknown> = { boxes };
        inv['input'] = await fieldFig(card.locator('input:not([type=checkbox])').first(), card);
        if ((await card.locator('textarea').count()) > 0)
          inv['textarea'] = await fieldFig(card.locator('textarea').first(), card);
        if ((await card.locator('select').count()) > 0)
          inv['select'] = await fieldFig(card.locator('select').first(), card);
        inv['sel3'] = await card.locator('select').count();
        inv['scroll'] = await dr2.evaluate((el) => {
          const de = document.documentElement;
          return [de.scrollWidth, de.clientWidth, el.scrollWidth, el.clientWidth];
        });
        fig['invite'] = inv;
        say(
          `${cfg.tag}: invite ${JSON.stringify({ i: inv['input'], t: inv['textarea'], s: inv['select'] })}`,
        );
        await card.locator('input').first().scrollIntoViewIfNeeded();
        await shot(page, cfg, 'invite');
        await closeTool(page);
      } catch (e) {
        problems.push(`${cfg.tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      } finally {
        if (errors.length) say(`page errors ${cfg.tag}: ${errors.join('; ')}`);
        await ctx.close();
        flush();
      }
    }

    /* ---- compare with the base ---- */
    if (base) {
      for (const cfg of CONFIGS) {
        const b = base[cfg.tag] as Record<string, any> | undefined; // eslint-disable-line @typescript-eslint/no-explicit-any
        const a = all[cfg.tag] as Record<string, any> | undefined; // eslint-disable-line @typescript-eslint/no-explicit-any
        if (!a || !b?.['tier'] || !a['tier']) continue;
        const bt = b['tier'] as TierFig;
        const at = a['tier'] as TierFig;
        const pointer = !cfg.touch;
        if (pointer) {
          bt.rows.forEach((r, i) => {
            const d = at.rows[i]!.h - r.h;
            if (Math.abs(d) > 1)
              problems.push(`${cfg.tag}: tier row ${r.name} ${r.h} -> ${at.rows[i]!.h}`);
            if (Math.abs(at.rows[i]!.head - r.head) > 0.5)
              problems.push(`${cfg.tag}: tier label column ${r.head} -> ${at.rows[i]!.head}`);
          });
          bt.chips.forEach((c, i) => {
            const n = at.chips[i]!;
            if (c.x && n.x && (Math.abs(c.x.w - n.x.w) > 0.5 || Math.abs(c.x.h - n.x.h) > 0.5))
              problems.push(`${cfg.tag}: x box ${c.x.w}x${c.x.h} -> ${n.x.w}x${n.x.h}`);
            if (c.chip && n.chip && n.chip.h > c.chip.h + 0.5)
              problems.push(`${cfg.tag}: chip grew ${c.chip.h} -> ${n.chip.h}`);
            if (c.chip && n.chip && n.chip.w > c.chip.w + 0.5)
              problems.push(`${cfg.tag}: chip widened ${c.chip.w} -> ${n.chip.w}`);
          });
        } else {
          notes.push(
            `${cfg.tag}: rows ${bt.rows.map((r) => r.h).join(',')} -> ${at.rows.map((r) => r.h).join(',')}; chips ${bt.chips.map((c) => `${c.chip?.w}x${c.chip?.h}`).join(' ')} -> ${at.chips.map((c) => `${c.chip?.w}x${c.chip?.h}`).join(' ')}`,
          );
        }
        if (at.scroll[0]! > at.scroll[1]! || at.scroll[2]! > at.scroll[3]!)
          if (!(bt.scroll[0]! > bt.scroll[1]! || bt.scroll[2]! > bt.scroll[3]!))
            problems.push(`${cfg.tag}: new sideways scroll ${at.scroll.join('/')}`);
        const hb = b['hit'].counts;
        const ha = a['hit'].counts;
        if (ha.fail > hb.fail)
          problems.push(
            `${cfg.tag}: hit fails ${hb.fail} -> ${ha.fail}: ${JSON.stringify(a['hit'].fails)}`,
          );
        const bi = (b['invite']?.boxes ?? {}) as Record<string, Box | null>;
        const ai = (a['invite']?.boxes ?? {}) as Record<string, Box | null>;
        for (const k of Object.keys(bi)) {
          const x = bi[k];
          const y = ai[k];
          if (
            x &&
            y &&
            (Math.abs(x.w - y.w) > 0.5 || Math.abs(x.h - y.h) > 0.5 || Math.abs(x.y - y.y) > 0.5)
          )
            problems.push(
              `${cfg.tag}: invite box ${k} ${JSON.stringify(x)} -> ${JSON.stringify(y)}`,
            );
        }
      }
    }
    flush();
    for (const n of notes) say(`NOTE ${n}`);
    for (const p of problems) say(`PROBLEM ${p}`);
    say(`${PHASE}: problems ${problems.length}`);
    if (PHASE === 'after') expect(problems, 'no problems').toEqual([]);
  } finally {
    await owner.close();
  }
});
