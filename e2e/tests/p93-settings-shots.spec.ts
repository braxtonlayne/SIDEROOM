import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import {
  CONTROL_SELECTOR,
  hitTest,
  hitTestAll,
  openCollapsed,
  type ControlRow,
  type HitResult,
} from '../support/hit-test';

/**
 * Wave 93, settings builder: SEC-AUD-412, 413, 415 (D-923: three small user Settings targets get 44 px on a phone) and
 * SEC-AUD-414 (D-921: the 20-page hit-test pass without the "view as" preview; the link blocks of the real profile page).
 * NEEDS THE SWEEP'S SEEDED WORLD (`e2e/.state/sweep/world.json`, made by
 *   SR_E2E_SWEEP=1 npx playwright test tests/page-sweep.spec.ts -g "seed the sweep world" --project=chromium \
 *     --workers=1 --retries=0 --reporter=line
 * from e2e/; that spec is never edited; DATABASE_URL and SR_CLI_BIN set for the seed). The Owner's account.
 * Local only: without SR_P93_SETTINGS_ON=1 it skips itself.
 *
 * The commander's command, from e2e/, on the merged tree's stack with the sweep's world (API 8712, Vite 5712):
 *   SR_E2E_BASE_URL=http://localhost:5712 SR_P93_SETTINGS_ON=1 SR_P93_SETTINGS_STRICT=1 \
 *   SR_P93_SETTINGS_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-settings \
 *   npx playwright test tests/p93-settings-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * SR_P93_SETTINGS_STRICT=1 asserts 0 failures on the 20-page pass (set it only on the merged tree: the kit's controls of
 * Voice & Video and "view as" fail until the kit's change is in). SR_P93_SETTINGS_ONLY=1,2,3,4 picks cases.
 *
 * "Base" is this worktree's own CSS with the D-923 rules switched off at run time: the spec deletes the one
 * `@media (max-width: 720px)` rule that holds them from the loaded style sheet (CSSOM), so base and after run in the
 * same round on the same stack. The hit tests are `e2e/support/hit-test.ts`; nothing private.
 *
 * Case 1: 375 touch, Lounge Light (base, after) and Lounge Dark (after): My Account's two "Edit" buttons, Profiles'
 *   "Edit your profile page", Support / Patron's "View leaderboards": box, the points, the taker of each failing point,
 *   the rows' pitch, the height of the three cards, sideways scroll; a tap 21 px under Username's "Edit".
 * Case 2: 1280 Editorial, base and after: every `.us-field`, `.us-card`, and the two links: boxes equal.
 * Case 3: the 20 user Settings pages at 375 touch Lounge Light, base and after: `openCollapsed` then `hitTestAll` on the
 *   content pane, rows inside `.us-view-as [role="region"]` left out (filtered here: the helper is not edited).
 * Case 4: the Owner's profile page signed in (`/u/owner`), 375 touch: `hitTestAll` over its links (report only).
 */
const ON = process.env.SR_P93_SETTINGS_ON === '1';
const STRICT = process.env.SR_P93_SETTINGS_STRICT === '1';
const OUT =
  process.env.SR_P93_SETTINGS_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-settings';
const ONLY = (process.env.SR_P93_SETTINGS_ONLY ?? '1,2,3,4').split(',').map((s) => s.trim());
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const PHONE = { width: 375, height: 812 };
const DESK = { width: 1280, height: 800 };

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

type Mode = 'base' | 'after';
const report: Record<string, unknown> = {};
const save = () => {
  mkdirSync(OUT, { recursive: true });
  writeFileSync(resolve(OUT, 'p93-settings.json'), JSON.stringify(report, null, 1));
};
const say = (s: string) => console.log(`REPORT ${s}`);

async function setMain(ownerCtx: BrowserContext, themeId: string) {
  await apiPatch(ownerCtx.request, '/users/@me/settings', {
    settings: { user_prefs: { theme: themeId, zoom: 100 } },
  });
  const back = (await (await ownerCtx.request.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  expect(back.settings?.user_prefs?.theme).toBe(themeId);
}

async function newPage(browser: Browser, ownerCtx: BrowserContext, themeId: string, phone = true) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: phone ? PHONE : DESK,
    storageState: await ownerCtx.storageState(),
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
  }, themeId);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  return { ctx, page };
}

const idle = async (page: Page, ms = 1500) => {
  await page.waitForLoadState('networkidle', { timeout: 3000 }).catch(() => undefined);
  await page.waitForTimeout(ms);
};
const go = async (page: Page, path: string) => {
  await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 20_000 });
  await idle(page);
};
const shot = async (page: Page, name: string) => {
  mkdirSync(OUT, { recursive: true });
  await page.screenshot({ path: resolve(OUT, `${name}.png`), timeout: 10_000 });
};
const envOf = (page: Page) =>
  page.evaluate(() => ({
    theme: document.querySelector('[data-theme]')?.getAttribute('data-theme') ?? null,
    scroll: [document.documentElement.scrollWidth, document.documentElement.clientWidth],
  }));

/** Base case: delete the D-923 rule block from the loaded sheets (CSSOM), idempotent. Returns how many were deleted. */
async function strip(page: Page): Promise<number> {
  return page.evaluate(() => {
    let n = 0;
    const walk = (rules: CSSRuleList, owner: CSSStyleSheet | CSSGroupingRule) => {
      for (let i = rules.length - 1; i >= 0; i--) {
        const r = rules[i]!;
        if (
          r.cssText.includes('.us-field:has(dd button)') &&
          r.cssText.includes('.us-actions > a')
        ) {
          owner.deleteRule(i);
          n++;
        } else if ('cssRules' in r && (r as CSSGroupingRule).cssRules.length)
          walk((r as CSSGroupingRule).cssRules, r as CSSGroupingRule);
      }
    };
    for (const s of Array.from(document.styleSheets)) {
      try {
        walk(s.cssRules, s);
      } catch {
        /* cross-origin sheet */
      }
    }
    return n;
  });
}

async function openSection(page: Page, label: string) {
  const toggle = page.locator('button[class*="navToggle"]').first();
  if ((await toggle.isVisible()) && (await toggle.getAttribute('aria-expanded')) === 'false')
    await toggle.click();
  await page.locator('button[class*="navItem"]', { hasText: label }).first().click();
  await idle(page, 1200);
}

const brief = (r: HitResult) => ({
  box: { w: +r.box.width.toFixed(1), h: +r.box.height.toFixed(1) },
  pass: r.pass,
  points: r.points.map((p) => ({
    at: p.at,
    ok: p.ok,
    taker: p.ok ? null : `${p.taker?.tag} "${p.taker?.name}" (${p.taker?.path})`,
  })),
});

/** Rows of every page that the D-923 rules can move: height and text. */
const movers = (page: Page) =>
  page.evaluate(() =>
    [
      ...Array.from(document.querySelectorAll<HTMLElement>('.us-field')).filter((e) =>
        e.querySelector('dd button'),
      ),
      ...Array.from(
        document.querySelectorAll<HTMLElement>('.us-card > a.us-link, .us-actions > a'),
      ),
    ].map((e) => ({
      sel: `${e.tagName.toLowerCase()}.${e.className.split(' ')[0]}`,
      text: (e.textContent ?? '').trim().replace(/\s+/g, ' ').slice(0, 40),
      h: +e.getBoundingClientRect().height.toFixed(1),
    })),
  );

/* ---------- case 1: the four controls ---------- */
async function case1(browser: Browser, owner: BrowserContext, mode: Mode, themeId: string) {
  const tag = `375-${themeId}-${mode}`;
  await setMain(owner, themeId);
  const { ctx, page } = await newPage(browser, owner, themeId);
  await go(page, '/settings/user/account');
  const stripped = mode === 'base' ? await strip(page) : 0;
  if (mode === 'base') expect(stripped).toBeGreaterThan(0);
  const fig: Record<string, unknown> = { mode, themeId, stripped };

  // My Account: the rows, then the two "Edit" buttons.
  const rowsInfo = await page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLElement>('.us-field'))
      .filter((e) => e.querySelector('dd button'))
      .map((e) => {
        const r = e.getBoundingClientRect();
        const b = e.querySelector('dd button')!.getBoundingClientRect();
        return {
          label: e.querySelector('dt')?.textContent ?? '',
          top: Math.round(r.top + scrollY),
          h: +r.height.toFixed(1),
          btn: `${b.width.toFixed(0)} x ${b.height.toFixed(0)}`,
        };
      }),
  );
  fig['accountRows'] = rowsInfo;
  fig['pitch'] = rowsInfo.slice(1).map((r, i) => r.top - rowsInfo[i]!.top);
  fig['accountCardH'] = await page.evaluate(
    () => document.querySelector('.us-field')?.closest('.us-card')?.getBoundingClientRect().height,
  );
  const edits = page
    .locator('.us-field', { has: page.locator('dt', { hasText: /^(Username|Display name)$/i }) })
    .locator('dd button');
  expect(await edits.count()).toBe(2);
  fig['editUsername'] = brief(await hitTest(page, edits.nth(0)));
  fig['editDisplayName'] = brief(await hitTest(page, edits.nth(1)));
  fig['accountEnv'] = await envOf(page);
  fig['accountMovers'] = await movers(page);
  // A tap 21 px under Username's "Edit": which editor opens.
  await hitTest(page, edits.nth(0));
  const b = (await edits.nth(0).boundingBox())!;
  await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2 + 21);
  await page.waitForTimeout(700);
  const dlg = page.getByRole('dialog').last();
  fig['tapBelowUsername'] = (await dlg.count())
    ? ((await dlg.getAttribute('aria-label')) ??
      (await dlg
        .locator('h1,h2,h3')
        .first()
        .textContent()
        .catch(() => null)) ??
      'dialog')
    : 'nothing opened';
  say(`case1 ${tag} tap below Username Edit: ${String(fig['tapBelowUsername'])}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  await hitTest(page, edits.nth(0));
  await shot(page, `1-account-${tag}`);

  // Profiles.
  await openSection(page, 'Profiles');
  const pl = page.locator('#us-profile-page a.us-link');
  fig['profilesCardH'] = await page
    .locator('#us-profile-page')
    .evaluate((e) => e.getBoundingClientRect().height);
  fig['profileLink'] = brief(await hitTest(page, pl));
  fig['profilesEnv'] = await envOf(page);
  if (mode === 'after') await shot(page, `2-profiles-${tag}`);

  // Support / Patron.
  await openSection(page, 'Support');
  const sl = page.locator('#sup-privacy .us-actions a');
  fig['supportCardH'] = await page
    .locator('#sup-privacy')
    .evaluate((e) => e.getBoundingClientRect().height);
  fig['leaderboardsLink'] = brief(await hitTest(page, sl));
  fig['supportEnv'] = await envOf(page);
  if (mode === 'after') await shot(page, `3-support-${tag}`);
  const cases = (report['case1'] ??= {}) as Record<string, unknown>;
  cases[tag] = fig;
  save();
  say(`case1 ${tag}: ${JSON.stringify(fig)}`);
  await ctx.close();
}

/* ---------- case 2: 1280 Editorial, boxes equal ---------- */
async function boxes(page: Page) {
  const out: Record<string, string[]> = {};
  for (const [label, path] of [
    ['account', '/settings/user/account'],
    ['profiles', '/settings/user/profile'],
    ['support', '/settings/user/support'],
  ] as const) {
    await go(page, path);
    out[label] = await page.evaluate(() =>
      Array.from(
        document.querySelectorAll<HTMLElement>(
          '.us-field, .us-card, .us-card > a, .us-actions > a, .us-actions > button, dd button',
        ),
      ).map((e) => {
        const r = e.getBoundingClientRect();
        return `${e.tagName.toLowerCase()}.${e.className.split(' ')[0]} ${r.x.toFixed(1)},${(r.y + scrollY).toFixed(1)},${r.width.toFixed(1)},${r.height.toFixed(1)}`;
      }),
    );
  }
  return out;
}
async function case2(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-editorial');
  const res: Record<Mode, Record<string, string[]>> = { base: {}, after: {} };
  for (const mode of ['base', 'after'] as Mode[]) {
    const { ctx, page } = await newPage(browser, owner, 'lounge-editorial', false);
    await go(page, '/settings/user/account');
    // each `go` reloads the page, so the base case strips again after every load.
    if (mode === 'base') {
      res[mode] = {};
      for (const [label, path] of [
        ['account', '/settings/user/account'],
        ['profiles', '/settings/user/profile'],
        ['support', '/settings/user/support'],
      ] as const) {
        await go(page, path);
        await strip(page);
        res[mode][label] = (await boxesHere(page)) as string[];
      }
    } else res[mode] = await boxes(page);
    await ctx.close();
  }
  const moved: string[] = [];
  let total = 0;
  for (const k of Object.keys(res.base)) {
    const a = res.base[k]!;
    const b = res.after[k] ?? [];
    total += a.length;
    a.forEach((v, i) => v !== b[i] && moved.push(`${k}: ${v} -> ${b[i]}`));
    if (a.length !== b.length) moved.push(`${k}: count ${a.length} -> ${b.length}`);
  }
  report['case2'] = { total, moved };
  say(
    `case2 1280 Editorial: ${total} boxes, moved ${moved.length}: ${moved.slice(0, 5).join(' | ')}`,
  );
  save();
  expect(moved).toEqual([]);
}
const boxesHere = (page: Page) =>
  page.evaluate(() =>
    Array.from(
      document.querySelectorAll<HTMLElement>(
        '.us-field, .us-card, .us-card > a, .us-actions > a, .us-actions > button, dd button',
      ),
    ).map((e) => {
      const r = e.getBoundingClientRect();
      return `${e.tagName.toLowerCase()}.${e.className.split(' ')[0]} ${r.x.toFixed(1)},${(r.y + scrollY).toFixed(1)},${r.width.toFixed(1)},${r.height.toFixed(1)}`;
    }),
  );

/* ---------- case 3: the 20 pages ---------- */
interface PageRow {
  page: string;
  tested: number;
  opened: number;
  leftOut: number;
  notReached: number;
  skipped: number;
  fails: unknown[];
  movers: unknown;
  error?: string;
}
async function case3(browser: Browser, owner: BrowserContext, mode: Mode) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light');
  await go(page, '/settings/user/account');
  if (mode === 'base') await strip(page);
  const labels = (await page.locator('button[class*="navItem"]').allTextContents())
    .map((s) => s.trim())
    .filter(Boolean);
  expect(labels.length).toBeGreaterThanOrEqual(15);
  const pages: PageRow[] = [];
  for (const label of labels) {
    const row: PageRow = {
      page: label,
      tested: 0,
      opened: 0,
      leftOut: 0,
      notReached: 0,
      skipped: 0,
      fails: [],
      movers: [],
    };
    pages.push(row);
    try {
      await openSection(page, label);
      const main = page.locator('main[class*="content"]').first();
      await expect(main).toBeVisible();
      row.opened = (await openCollapsed(page, main)).length;
      row.movers = await movers(page);
      const inRegion = new Set<number>(
        await main.evaluate((m, sel) => {
          const out: number[] = [];
          Array.from(m.querySelectorAll(sel)).forEach((e, i) => {
            if (e.closest('.us-view-as [role="region"]')) out.push(i);
          });
          return out;
        }, CONTROL_SELECTOR),
      );
      const rows = await hitTestAll(page, main);
      const kept = rows.filter((r: ControlRow) => !inRegion.has(r.index));
      row.leftOut = rows.length - kept.length;
      row.tested = kept.filter((r) => r.status === 'pass' || r.status === 'fail').length;
      row.skipped = kept.filter((r) => r.status === 'skipped').length;
      row.notReached = kept.filter((r) => r.status === 'not-reached').length;
      row.fails = kept
        .filter((r) => r.status === 'fail')
        .map((r) => ({
          control: `${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}"`,
          path: r.path,
          box: r.result
            ? `${r.result.box.width.toFixed(0)} x ${r.result.box.height.toFixed(0)}`
            : null,
          points: (r.result?.points ?? [])
            .filter((p) => !p.ok)
            .map((p) => `${p.at}: ${p.taker?.tag} "${p.taker?.name}" (${p.taker?.path})`),
        }));
      say(
        `case3 ${mode} ${label}: tested ${row.tested}, opened ${row.opened}, left out ${row.leftOut}, skipped ${row.skipped}, not reached ${row.notReached}, fails ${row.fails.length}`,
      );
    } catch (e) {
      row.error = String(e).slice(0, 300);
      say(`case3 ${mode} ${label} ERROR ${row.error}`);
    }
    const c3 = (report['case3'] ??= {}) as Record<string, unknown>;
    c3[mode] = pages;
    save();
  }
  await ctx.close();
  const bad = pages.filter((p) => p.notReached > 0 || p.error);
  expect(bad.map((p) => `${p.page}: ${p.notReached} ${p.error ?? ''}`)).toEqual([]);
  const fails = pages.flatMap((p) => p.fails.map((f) => ({ page: p.page, ...(f as object) })));
  say(`case3 ${mode}: ${fails.length} failures: ${JSON.stringify(fails)}`);
  if (STRICT && mode === 'after') expect(fails).toEqual([]);
}

/* ---------- case 4: the profile page's link blocks ---------- */
async function case4(browser: Browser, owner: BrowserContext) {
  await setMain(owner, 'lounge-light');
  const { ctx, page } = await newPage(browser, owner, 'lounge-light');
  await go(page, '/u/owner');
  await page.waitForTimeout(1500);
  const root = page.locator('main').first();
  const have = (await root.count()) ? root : page.locator('body');
  const links = await have.locator('a[href^="http"], a[href^="bitcoin"]').count();
  const rows = await hitTestAll(page, have);
  const aRows = rows.filter((r) => r.tag === 'a');
  const out = aRows.map((r) => ({
    name: r.name,
    status: r.status,
    box: r.result ? `${r.result.box.width.toFixed(0)} x ${r.result.box.height.toFixed(0)}` : null,
    points: (r.result?.points ?? []).map(
      (p) => `${p.at}:${p.ok ? 'ok' : `${p.taker?.tag} (${p.taker?.path})`}`,
    ),
  }));
  const first = page.locator('a[href^="https://ko-fi.com"], a[href^="http"]').first();
  if (await first.count()) await hitTest(page, first).catch(() => undefined);
  await shot(page, '4-profile-page-375-lounge-light');
  report['case4'] = {
    externalLinks: links,
    anchors: out,
    all: rows.length,
    env: await envOf(page),
  };
  say(`case4: ${links} link blocks; anchors ${JSON.stringify(out)}`);
  save();
  await ctx.close();
}

test('p93 settings: three small targets, then the 20 pages without the preview', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const errors: string[] = [];
  const run = async (id: string, fn: () => Promise<void>) => {
    if (!ONLY.includes(id)) return;
    const t0 = Date.now();
    try {
      await fn();
    } catch (e) {
      errors.push(`case ${id}: ${String(e).slice(0, 600)}`);
      say(`case ${id} FAILED ${String(e).slice(0, 600)}`);
    }
    say(`case ${id} took ${Math.round((Date.now() - t0) / 1000)} s`);
  };
  await run('1', async () => {
    await case1(browser, owner, 'base', 'lounge-light');
    await case1(browser, owner, 'after', 'lounge-light');
    await case1(browser, owner, 'after', 'lounge-dark');
  });
  await run('2', () => case2(browser, owner));
  await run('3', async () => {
    await case3(browser, owner, 'base');
    await case3(browser, owner, 'after');
  });
  await run('4', () => case4(browser, owner));
  await owner.close();
  report['errors'] = errors;
  save();
  if (errors.length) throw new Error(errors.join('\n'));
});
