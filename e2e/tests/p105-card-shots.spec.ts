import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { textRatio } from '../support/contrast';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';

/**
 * SEC-AUD-489, 483, 490; D-1000 (wave 105, solo builder card): the Invite card's and the room shells' inputs state the
 * kit's placeholder colour and draw the kit's field ground `--ui-bg-raised`; the tier chip's remove button is a 44 px tall
 * target at 375.
 *
 * Runs on the seeded sweep world (owner.json). Data (D-942): one guild `P105 card shots` (found by that name, else made),
 * per configuration one text channel `p105-<phase>-<run>-<tag>` (deleted in a finally), items `P105 <run> a|b`.
 * Per configuration (1024 Lounge Light / Dark / Editorial, 375 Light touch) it records: the Invite card's inputs, textarea
 * and boxes; computed ground, text, placeholder colour and ratios; `--ui-bg-raised` against `--ui-text-faint` read from the
 * page; the tier list room's item input, the chip boxes, the x button's box and hit test; at 1024 Light also the poll
 * builder's input. Phases: `SR_P105_CARD_PHASE=base|after`; `figures-<phase>.json`; the after phase compares with
 * `figures-base.json` when it is in the folder (`report-after.json`).
 *
 * Command: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_CARD_ON=1 --env SR_P105_CARD_PHASE=after --env SR_P105_CARD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-card -- tests/p105-card-shots.spec.ts
 */
const ON = process.env.SR_P105_CARD_ON === '1';
const PHASE = process.env.SR_P105_CARD_PHASE === 'base' ? 'base' : 'after';
const OUT =
  process.env.SR_P105_CARD_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-card';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const GUILD_NAME = 'P105 card shots';
const RUN = Date.now().toString(36);

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

type Theme = 'lounge-editorial' | 'lounge-light' | 'lounge-dark';
interface Cfg {
  tag: string;
  width: number;
  height: number;
  touch: boolean;
  theme: Theme;
  shots: string[];
}
const CONFIGS: Cfg[] = [
  {
    tag: '1024-light',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-light',
    shots: ['invite', 'room'],
  },
  {
    tag: '1024-dark',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-dark',
    shots: ['invite', 'room'],
  },
  {
    tag: '1024-editorial',
    width: 1024,
    height: 768,
    touch: false,
    theme: 'lounge-editorial',
    shots: ['invite'],
  },
  {
    tag: '375-light',
    width: 375,
    height: 812,
    touch: true,
    theme: 'lounge-light',
    shots: ['invite', 'tier'],
  },
];

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
const say = (s: string) => console.log(`REPORT ${s}`);
const r1 = (n: number) => Math.round(n * 10) / 10;
const r2 = (n: number) => Math.round(n * 100) / 100;
const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};

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

/** Computed ground, colours and ratios of one field; the two kit tokens are read where the field is. */
async function fieldFig(l: Locator) {
  const f = await l.evaluate((el) => {
    const cs = getComputedStyle(el);
    const ph = getComputedStyle(el, '::placeholder');
    const probe = document.createElement('span');
    el.parentElement?.appendChild(probe);
    const tok = (n: string) => {
      probe.style.color = `var(${n})`;
      return getComputedStyle(probe).color;
    };
    const raised = tok('--ui-bg-raised');
    const faint = tok('--ui-text-faint');
    probe.remove();
    return {
      bg: cs.backgroundColor,
      color: cs.color,
      border: cs.borderTopColor,
      placeholder: ph.color,
      placeholderOpacity: ph.opacity,
      hasPlaceholder: !!(el as HTMLInputElement).placeholder,
      raised,
      faint,
      tag: el.tagName.toLowerCase(),
    };
  });
  return {
    ...f,
    text: r2(textRatio(f.color, f.bg)),
    placeholderRatio: f.hasPlaceholder ? r2(textRatio(f.placeholder, f.bg)) : null,
    faintOnRaised: r2(textRatio(f.faint, f.raised)),
  };
}

test('Card and room fields, the chip remove button', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const problems: string[] = [];
  const notes: string[] = [];
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
      writeFileSync(
        resolve(OUT, `report-${PHASE}.json`),
        JSON.stringify({ problems, notes }, null, 1),
      );
    };

    for (const cfg of CONFIGS) {
      await setMain(owner, cfg.theme);
      const room = await apiPost<{ id: string }>(owner.request, `/guilds/${guildId}/channels`, {
        name: `p105-${PHASE}-${RUN}-${cfg.tag}`.toLowerCase(),
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
          problems.push(
            `${cfg.tag}: data-theme is ${JSON.stringify(fig['theme'])}, wanted ${cfg.theme}`,
          );

        /* ---- the Invite card ---- */
        await openTool(page, 'invite card', /invite card/i);
        const dr = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await expect(dr.locator('.sr-card')).toBeVisible();
        await page.waitForTimeout(500);
        const card = dr.locator('.sr-card');
        const ctl = card.locator('input,textarea,select,button');
        const boxes: Record<string, Box | null> = {};
        for (let i = 0; i < (await ctl.count()); i++) {
          const l = ctl.nth(i);
          const key = `${i}:${await l.evaluate((e) => e.tagName.toLowerCase())}:${(await l.getAttribute('aria-label')) ?? (await l.textContent())?.slice(0, 20) ?? ''}`;
          boxes[key] = await boxOf(l);
        }
        const inv: Record<string, unknown> = { boxes };
        const phInputs = card.locator('input[placeholder]');
        inv['placeholderInputs'] = await phInputs.count();
        if ((await phInputs.count()) > 0) inv['phInput'] = await fieldFig(phInputs.first());
        inv['input'] = await fieldFig(card.locator('input:not([type=checkbox])').first());
        if ((await card.locator('textarea').count()) > 0)
          inv['textarea'] = await fieldFig(card.locator('textarea').first());
        if ((await card.locator('select').count()) > 0)
          inv['select'] = await fieldFig(card.locator('select').first());
        inv['scroll'] = await sideways(page);
        fig['invite'] = inv;
        say(
          `${cfg.tag}: invite ${JSON.stringify({ ph: inv['phInput'], i: inv['input'], t: inv['textarea'], s: inv['select'] })}`,
        );
        await card
          .locator('input[placeholder], textarea[placeholder]')
          .first()
          .scrollIntoViewIfNeeded();
        if (cfg.shots.includes('invite')) await shot(page, `invite-${cfg.tag}`);
        await closeTool(page);

        /* ---- the tier list room ---- */
        await openTool(page, 'tier list', /tier list/i);
        const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
        await expect(drawer).toBeVisible();
        await drawer.getByRole('button', { name: 'Start a new one' }).click();
        const rowsBtn = drawer.getByRole('button', { name: 'Add the rows S to D' });
        if (await rowsBtn.isVisible().catch(() => false)) await rowsBtn.click();
        const names = ['a', 'b'].map((s) => `P105 card ${s}`);
        for (const n of names) {
          await drawer.getByLabel('Item', { exact: true }).fill(n);
          await drawer.getByRole('button', { name: 'Add item' }).click();
          await expect(drawer.locator('li.sr-room-chip').filter({ hasText: n })).toHaveCount(1);
        }
        await page.waitForTimeout(400);
        const item = drawer.getByLabel('Item', { exact: true });
        const rm: Record<string, unknown> = { input: await fieldFig(item) };
        const chips = drawer.locator('li.sr-room-chip');
        const chipFig: Record<string, Box | null>[] = [];
        for (let i = 0; i < (await chips.count()); i++) {
          const c = chips.nth(i);
          chipFig.push({
            chip: await boxOf(c),
            move: await boxOf(c.locator('.sr-room-move')),
            x: await boxOf(c.locator('.sr-room-x')),
            text: await boxOf(c.locator('.sr-room-chip-text')),
          });
        }
        rm['chips'] = chipFig;
        rm['rowHeights'] = await drawer
          .locator('section.sr-tier-row')
          .evaluateAll((els) =>
            els.map((e) => Math.round(e.getBoundingClientRect().height * 10) / 10),
          );
        rm['itemBox'] = await boxOf(item);
        rm['scroll'] = await sideways(page);
        const tool = drawer.locator('.sr-room').first();
        await openCollapsed(page, tool);
        const hit = await hitTestAll(page, tool, cfg.touch ? {} : { reach: 'box' });
        const xRows = hit.filter((r) => /^remove P105/i.test(r.name));
        rm['xHit'] = xRows.map((r) => ({
          name: r.name,
          status: r.status,
          taker: (r.result?.points ?? []).filter((p) => !p.ok).map((p) => p.taker),
        }));
        rm['hitFails'] = hit.filter(
          (r) => r.status === 'fail' || r.status === 'not-reached',
        ).length;
        rm['hitNotReached'] = hit.filter((r) => r.status === 'not-reached').length;
        fig['room'] = rm;
        say(
          `${cfg.tag}: room ${JSON.stringify({ i: rm['input'], chips: chipFig, xHit: rm['xHit'], fails: rm['hitFails'] })}`,
        );
        if (cfg.shots.includes('room')) {
          await item.scrollIntoViewIfNeeded();
          await shot(page, `room-${cfg.tag}`);
        }
        if (cfg.shots.includes('tier')) {
          await chips.first().scrollIntoViewIfNeeded();
          await shot(page, `tier-${cfg.tag}`);
        }
        await closeTool(page);

        /* ---- the poll builder (the room shell's other user), 1024 Light ---- */
        if (cfg.tag === '1024-light') {
          await openTool(page, 'poll', /poll/i);
          const pd = page.getByRole('dialog', { name: 'Drawer', exact: true });
          await expect(pd).toBeVisible();
          const start = pd.getByRole('button', { name: 'Start a new one' });
          if (await start.isVisible().catch(() => false)) await start.click();
          await page.waitForTimeout(600);
          const pin = pd.locator('.sr-room input:not([type=checkbox]):not([type=radio])').first();
          if ((await pin.count()) > 0) {
            fig['poll'] = { input: await fieldFig(pin), box: await boxOf(pin) };
            say(`${cfg.tag}: poll ${JSON.stringify(fig['poll'])}`);
          } else notes.push(`${cfg.tag}: poll builder input not found`);
          await closeTool(page);
        }
      } catch (e) {
        problems.push(`${cfg.tag}: ${String(e instanceof Error ? e.message : e).split('\n')[0]}`);
      } finally {
        await ctx.close();
        flush();
      }
    }

    /* ---- compare with the base ---- */
    if (base) {
      const near = (a: Box | null | undefined, b: Box | null | undefined) =>
        !a ||
        !b ||
        (Math.abs(a.w - b.w) <= 0.5 && Math.abs(a.h - b.h) <= 0.5 && Math.abs(a.y - b.y) <= 0.5);
      for (const cfg of CONFIGS) {
        /* eslint-disable @typescript-eslint/no-explicit-any */
        const b = base[cfg.tag] as Record<string, any> | undefined;
        const a = all[cfg.tag] as Record<string, any> | undefined;
        /* eslint-enable @typescript-eslint/no-explicit-any */
        if (!a || !b) continue;
        const bi = b['invite']?.boxes ?? {};
        const ai = a['invite']?.boxes ?? {};
        for (const k of Object.keys(bi))
          if (!near(bi[k], ai[k]))
            problems.push(
              `${cfg.tag}: invite box ${k} ${JSON.stringify(bi[k])} -> ${JSON.stringify(ai[k])}`,
            );
        for (const key of ['phInput', 'input', 'textarea']) {
          const bf = b['invite']?.[key];
          const af = a['invite']?.[key];
          if (!bf || !af) continue;
          if (bf.color !== af.color)
            problems.push(`${cfg.tag}: invite ${key} text ${bf.color} -> ${af.color}`);
          if (bf.border !== af.border) problems.push(`${cfg.tag}: invite ${key} border changed`);
          if (af.bg !== af.raised)
            problems.push(
              `${cfg.tag}: invite ${key} ground ${af.bg} is not --ui-bg-raised ${af.raised}`,
            );
          if (af.placeholderRatio !== null && af.placeholderRatio < 4.5)
            problems.push(`${cfg.tag}: invite ${key} placeholder ${af.placeholderRatio}:1`);
        }
        const sel = a['invite']?.select;
        if (sel && b['invite']?.select && sel.bg !== b['invite'].select.bg)
          problems.push(`${cfg.tag}: kit select ground changed`);
        const br = b['room'];
        const ar = a['room'];
        if (br && ar) {
          if (!near(br.itemBox, ar.itemBox)) problems.push(`${cfg.tag}: room input box moved`);
          if (br.input.color !== ar.input.color)
            problems.push(`${cfg.tag}: room input text colour changed`);
          if (ar.input.bg !== ar.input.raised)
            problems.push(`${cfg.tag}: room input ground ${ar.input.bg} is not --ui-bg-raised`);
          if (ar.input.placeholderRatio !== null && ar.input.placeholderRatio < 4.5)
            problems.push(`${cfg.tag}: room input placeholder ${ar.input.placeholderRatio}:1`);
          if (JSON.stringify(br.rowHeights) !== JSON.stringify(ar.rowHeights))
            problems.push(`${cfg.tag}: tier rows ${br.rowHeights} -> ${ar.rowHeights}`);
          (br.chips as Record<string, Box | null>[]).forEach((c, i) => {
            const n = ar.chips[i] as Record<string, Box | null> | undefined;
            if (!n) return;
            if (!near(c['chip'], n['chip']))
              problems.push(
                `${cfg.tag}: chip ${i} box ${JSON.stringify(c['chip'])} -> ${JSON.stringify(n['chip'])}`,
              );
            if (!near(c['move'], n['move']))
              problems.push(`${cfg.tag}: chip ${i} Move box changed`);
            if (c['x'] && n['x']) {
              if (Math.abs(c['x'].w - n['x'].w) > 0.5)
                problems.push(`${cfg.tag}: chip ${i} x width ${c['x'].w} -> ${n['x'].w}`);
              if (!cfg.touch && Math.abs(n['x'].h - c['x'].h) > 0.5)
                problems.push(`${cfg.tag}: chip ${i} x height moved at ${cfg.width}`);
              if (cfg.touch && n['x'].h < 43.5)
                problems.push(`${cfg.tag}: chip ${i} x height ${n['x'].h}, wanted 44`);
            }
          });
          if (ar.hitFails > br.hitFails)
            problems.push(`${cfg.tag}: hit fails ${br.hitFails} -> ${ar.hitFails}`);
          if (cfg.touch && ar.xHit.some((r: { status: string }) => r.status !== 'pass'))
            problems.push(`${cfg.tag}: x hit test ${JSON.stringify(ar.xHit)}`);
        }
        const bp = b['poll'];
        const ap = a['poll'];
        if (bp && ap) {
          if (!near(bp.box, ap.box)) problems.push(`${cfg.tag}: poll input box moved`);
          if (ap.input.bg !== ap.input.raised)
            problems.push(`${cfg.tag}: poll input ground not raised`);
        }
      }
    }
    flush();
    for (const n of notes) say(`NOTE ${n}`);
    for (const p of problems) say(`PROBLEM ${p}`);
    say(`${PHASE}: problems ${problems.length}`);
    if (PHASE === 'after') expect(problems, 'no problems').toEqual([]);
  } finally {
    for (const id of made)
      await owner.request.delete(`/api/v1/channels/${id}`).catch(() => undefined);
    await owner.close();
  }
});
