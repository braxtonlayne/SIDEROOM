import { appendFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-336 (wave 81; D-839): a game's name on an Arcade card does not break inside a word that fits its column.
 * Logs every `.arc-card-title` on the Arcade home (text, font size, box, the column's width, lines, the width its
 * longest word needs), shipped (`sideroom`) and light (`lounge-light`), 1280x800 and 375x812, with each of the four
 * section links pressed. A title is broken inside a word when its longest word is wider than its column.
 * Local only (SR_P81_ARCADE=1); shots go to SR_P81_ARCADE_OUT; SR_P81_BASE=1 names the run `base`.
 * The spec logs; the shots are looked at by the builder.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_ARCADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-arcade';
const MODE = process.env.SR_P81_BASE === '1' ? 'base' : 'after';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
  mid: { width: 960, height: 800 },
} as const;
const LONG = 'Supercalifragilisticexpialidocious';
const LONGER = 'Pneumonoultramicroscopicsilicovolcanoconiosis';

test.skip(process.env.SR_P81_ARCADE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Prefs = { theme: string; zoom: number; sectionThemes: Record<string, string> };

const log = (line: string) => {
  console.log(`REPORT ${line}`);
  appendFileSync(resolve(OUT, `report-${MODE}.txt`), `${line}\n`);
};

const json = (r: Route, body: unknown) =>
  r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

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

interface TitleRow {
  text: string;
  fontPx: number;
  box: number[];
  column: number;
  lines: number;
  longestWord: string;
  wordNeeds: number;
  broken: boolean;
  overflowsCard: boolean;
  card: number[];
  size: string;
}

/** Every card title: the contract's log. */
async function titles(page: Page): Promise<{ rows: TitleRow[]; sw: number; cw: number }> {
  return page.evaluate(() => {
    const de = document.documentElement;
    const rows = Array.from(document.querySelectorAll<HTMLElement>('.arc-card-title')).map((h) => {
      const cs = getComputedStyle(h);
      const r = h.getBoundingClientRect();
      const card = h.closest('.arc-card') as HTMLElement;
      const cr = card.getBoundingClientRect();
      const text = h.textContent ?? '';
      const lh = parseFloat(cs.lineHeight) || parseFloat(cs.fontSize) * 1.1;
      const clone = document.createElement('span');
      clone.style.cssText = `position:absolute;visibility:hidden;white-space:nowrap;font:${cs.font};letter-spacing:${cs.letterSpacing};text-transform:${cs.textTransform}`;
      document.body.appendChild(clone);
      let best = '';
      let need = 0;
      for (const w of text.split(/\s+/).filter(Boolean)) {
        clone.textContent = w;
        const wd = clone.getBoundingClientRect().width;
        if (wd > need) {
          need = wd;
          best = w;
        }
      }
      clone.remove();
      const column = h.clientWidth;
      return {
        text,
        fontPx: parseFloat(cs.fontSize),
        box: [r.left, r.top, r.width, r.height].map((n) => Math.round(n * 10) / 10),
        column: Math.round(column * 10) / 10,
        lines: Math.round(r.height / lh),
        longestWord: best,
        wordNeeds: Math.round(need * 10) / 10,
        broken: need > column + 0.5,
        overflowsCard: r.right > cr.right + 0.5 || h.scrollWidth > column + 1,
        card: [cr.left, cr.top, cr.width, cr.height].map((n) => Math.round(n * 10) / 10),
        size: card.dataset.size ?? '',
      };
    });
    return { rows, sw: de.scrollWidth, cw: de.clientWidth };
  });
}

test('SEC-AUD-336: card titles', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const setPrefs = (p: Prefs) =>
    apiPatch(o, '/users/@me/settings', { settings: { user_prefs: p } });
  const guild = await apiPost<{ id: string }>(o, '/guilds', {
    name: `Arcade ${Date.now().toString(36)}`,
  });

  const tier1 = {
    browser_play: true,
    guest_links: true,
    featured: true,
    desktop_only: false,
    needs_base_game_hint: false,
  };
  const front = {
    featured: [
      {
        application_id: '900',
        name: 'Land.io',
        description: 'A roguelike for a call.',
        blurb: 'Our flagship',
        tier: 'TIER_1_ORIGINAL',
        eligibility: tier1,
        capabilities: ['datachannel', 'guest_play'],
        tip_links: [],
        bundle_sha256: 'a'.repeat(64),
        position: 0,
        featured_at_ms: 1,
      },
    ],
    coming_soon: [
      {
        id: '55',
        title: 'OpenMW',
        summary: 'An open engine in the works.',
        tier: 'TIER_2_REIMPLEMENTATION',
        repo_url: 'https://github.com/OpenMW/openmw',
        notify: false,
        created_at_ms: 3,
      },
      {
        id: '56',
        title: 'Whiteboard Remix',
        summary: 'A soon title with the same long word.',
        tier: 'TIER_2_REIMPLEMENTATION',
        repo_url: 'https://github.com/example/wb',
        notify: false,
        created_at_ms: 4,
      },
    ],
    flags: [],
  };
  const cart = (id: string, title: string) => ({
    id,
    guild_id: guild.id,
    title,
    description: 'A cartridge listed for the layout check.',
    kind: 'game',
    tier: 'TIER_1_ORIGINAL',
    eligibility: tier1,
    source_ref: 'https://example.com/game.zip',
    shelf_id: null,
    object_hash: null,
    capabilities: ['datachannel'],
    tip_links: [],
    added_by: '1',
    added_at_ms: 1,
  });
  const carts = [
    cart('71', 'Whiteboard'),
    cart('72', LONG),
    cart('73', `${LONG} Deluxe Edition`),
    cart('74', LONGER),
  ];
  const fake = async (c: BrowserContext) => {
    await c.route('**/api/v1/arcade', (r) => json(r, front));
    await c.route(/\/api\/v1\/guilds\/[^/]+\/arcade\/cartridges$/, (r) => json(r, carts));
  };

  const links = ['Playable now', 'Featured', 'Server cartridges', 'Coming soon'];
  let shots = 0;
  const all: Record<string, unknown> = {};
  for (const [variant, section] of [
    ['shipped', 'sideroom'],
    ['light', 'lounge-light'],
  ] as const) {
    const prefs: Prefs = {
      theme: 'lounge-dark',
      zoom: 100,
      sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: section },
    };
    await setPrefs(prefs);
    for (const [sizeName, size] of Object.entries(SIZES)) {
      const tag = `${variant}-${sizeName}`;
      const c = await context(browser, octx, size, prefs);
      await fake(c);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const errors: string[] = [];
      page.on('pageerror', (e) => errors.push(e.message));
      await page.goto('/games');
      await expect(page.locator('.arc-root')).toBeVisible({ timeout: 60_000 });
      await expect(page.locator('[data-theme]:has(.arc-root)').last()).toHaveAttribute(
        'data-theme',
        section,
      );
      await expect(page.getByText(LONG, { exact: true }).first()).toBeVisible({ timeout: 30_000 });
      await page.waitForTimeout(900);
      for (const name of links) {
        await page.locator('.arc-nav').getByRole('link', { name, exact: true }).click();
        await page.waitForTimeout(500);
        const m = await titles(page);
        all[`${name}-${tag}`] = m;
        log(`${tag} [${name}] sideways=${m.sw}/${m.cw}`);
        for (const r of m.rows) {
          log(
            `${tag} [${name}] "${r.text}" ${r.size} font=${r.fontPx} col=${r.column} lines=${r.lines} word="${r.longestWord}" needs=${r.wordNeeds} broken=${r.broken} overflow=${r.overflowsCard} card=${r.card.join(',')} box=${r.box.join(',')}`,
          );
        }
        if (sizeName !== 'mid') {
          await page.screenshot({
            path: resolve(OUT, `${MODE}-${tag}-${name.replace(/ /g, '-').toLowerCase()}.png`),
          });
          shots++;
        }
      }
      // the long-name cartridge cards, whole
      await page.locator('#cartridges').scrollIntoViewIfNeeded();
      if (sizeName !== 'mid') {
        await page.locator('#cartridges .arc-list').screenshot({
          path: resolve(OUT, `${MODE}-${tag}-longname.png`),
        });
        shots++;
      }
      // click-through: press Play on a built-in card, see the dialog, close it
      const play = page.getByRole('button', { name: /^Play/ }).first();
      if (await play.isVisible().catch(() => false)) {
        await play.scrollIntoViewIfNeeded();
        await play.click();
        await page.waitForTimeout(600);
        log(`${tag} click-through: dialogs after Play = ${await page.getByRole('dialog').count()}`);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(400);
        log(
          `${tag} click-through: dialogs after Escape = ${await page.getByRole('dialog').count()}`,
        );
      } else log(`${tag} click-through: no Play button visible`);
      expect(errors, 'no page errors').toEqual([]);
      await c.close();
    }
  }
  writeFileSync(resolve(OUT, `measure-${MODE}.json`), JSON.stringify(all, null, 2));
  log(`SUMMARY shots=${shots}`);
  await setPrefs({
    theme: 'lounge-dark',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: 'sideroom' },
  });
  await octx.close();
});
