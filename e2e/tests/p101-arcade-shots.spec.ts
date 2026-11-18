import { appendFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed } from '../support/hit-test';
import { seedActivityTier } from '../support/p100-seed';

/**
 * Wave 101, arcade (SEC-AUD-445, SEC-AUD-446, D-956). The Coming soon card's Repository link on a phone, and Admin >
 * Arcade's Coming soon list with each entry's tier, measured and shot. Two tiers are seeded through the Operator's
 * route (`seedActivityTier`, own names `p100-seed-<random>`, D-942) and removed again in a `finally`.
 *
 * One run measures one PHASE: `SR_P101_ARCADE_PHASE=base` (the files as the base has them: prefix `base-` on the two
 * base shots) or `after` (default). The commander puts `arcade.css` and `ArcadeAdmin.tsx` back to the base's text for
 * the base run (from `git show <base>:<path>`, never a stash), then restores them. Every measure is one JSON line in
 * `measure.json` (phase, scenario, boxes, hit test rows with takers). The cards and the list entries are picked by the
 * titles `seedActivityTier` returns, never by a row count or the page's length. A hit-test row `not-reached` is a
 * spec error and throws.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, made by `page-sweep.spec.ts`'s "seed the sweep
 * world"; never edited) and a stack with `SR_ADMIN_REQUIRE_REAUTH=0`. The account's theme settings are put back at
 * the end (the shipped `sectionThemes.arcade = 'lounge-editorial'` is set explicitly and read back).
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8783 /
 * 5783 / 9483, database sr_p101_arcade_ui):
 *   SR_P101_ARCADE_ON=1 SR_E2E_BASE_URL=http://localhost:5783 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P101_ARCADE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-arcade \
 *   npx playwright test tests/p101-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8783
 * SR_SERVER__PUBLIC_URL=http://localhost:5783 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9483 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8783 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5783.
 * Variables: SR_P101_ARCADE_ON, SR_P101_ARCADE_OUT, SR_P101_ARCADE_PHASE (base | after).
 */
const ON = process.env.SR_P101_ARCADE_ON === '1';
const OUT =
  process.env.SR_P101_ARCADE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-arcade';
const PHASE = process.env.SR_P101_ARCADE_PHASE === 'base' ? 'base' : 'after';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OWNER_STATE = resolve(repoRoot, 'e2e', '.state', 'sweep', 'owner.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SHIPPED = 'lounge-editorial';

test.skip(!ON, 'local visual check only');
test.setTimeout(420_000);

interface Scenario {
  key: string;
  page: 'soon' | 'admin';
  w: number;
  h: number;
  phone: boolean;
  main: string;
  arcade: string;
  /** The shot's file name (without the phase prefix), or null for a measure only. */
  shot: string | null;
}
const SCENARIOS: Scenario[] = [
  {
    key: 'soon-375-editorial',
    page: 'soon',
    w: 375,
    h: 812,
    phone: true,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'soon-375-editorial.png',
  },
  {
    key: 'soon-375-lounge-light',
    page: 'soon',
    w: 375,
    h: 812,
    phone: true,
    main: 'lounge-light',
    arcade: 'lounge-light',
    shot: 'soon-375-lounge-light.png',
  },
  {
    key: 'soon-1024-editorial',
    page: 'soon',
    w: 1024,
    h: 768,
    phone: false,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'soon-1024-editorial.png',
  },
  {
    key: 'soon-1280-editorial',
    page: 'soon',
    w: 1280,
    h: 800,
    phone: false,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: null,
  },
  {
    key: 'admin-1024-lounge-light',
    page: 'admin',
    w: 1024,
    h: 768,
    phone: false,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'admin-arcade-1024-lounge-light.png',
  },
  {
    key: 'admin-1024-lounge-dark',
    page: 'admin',
    w: 1024,
    h: 768,
    phone: false,
    main: 'lounge-dark',
    arcade: SHIPPED,
    shot: 'admin-arcade-1024-lounge-dark.png',
  },
  {
    key: 'admin-375-lounge-light',
    page: 'admin',
    w: 375,
    h: 812,
    phone: true,
    main: 'lounge-light',
    arcade: SHIPPED,
    shot: 'admin-arcade-375-lounge-light.png',
  },
];
/** The two shots the base run keeps (prefix `base-`); the after run keeps all of them. */
const BASE_SHOTS = new Set(['soon-375-editorial.png', 'admin-arcade-375-lounge-light.png']);

const r1 = (n: number) => Math.round(n * 10) / 10;
type Box = { x: number; y: number; w: number; h: number };
const boxOf = async (l: Locator): Promise<Box | null> => {
  const b = await l.boundingBox();
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
};
const overlap = (a: Box | null, b: Box | null) =>
  !a || !b
    ? 0
    : Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x)) *
      Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));

test('SEC-AUD-445 and 446: the Repository link on a phone, the tier in Admin > Arcade', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  const log = (rec: Record<string, unknown>) =>
    appendFileSync(jsonl, `${JSON.stringify({ phase: PHASE, ...rec })}\n`);
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const shots: string[] = [];
  const notes: string[] = [];
  const orig = await seedActivityTier({ owner: o }, 'TIER_1_ORIGINAL');
  const reimpl = await seedActivityTier({ owner: o }, 'TIER_2_REIMPLEMENTATION');
  const seeded = [
    { s: orig, label: 'Original' },
    { s: reimpl, label: 'Clean-room reimplementation' },
  ];
  console.log(`REPORT phase ${PHASE}; seeded ${orig.id} ${orig.tier}; ${reimpl.id} ${reimpl.tier}`);

  const readPrefs = async () => {
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: Record<string, unknown> & { theme?: string } };
    };
    return back.settings?.user_prefs ?? {};
  };
  const original = await readPrefs();
  const idle = async (p: Page, ms = 3000) => {
    await p.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
    await p.waitForTimeout(500);
  };
  const hits = async (page: Page, root: Locator) => {
    await openCollapsed(page, root);
    const rows = await hitTestAll(page, root);
    const notReached = rows.filter((r) => r.status === 'not-reached');
    if (notReached.length)
      throw new Error(`hit test: ${notReached.length} not-reached (probe error)`);
    return rows.map((r) => ({
      name: r.name,
      status: r.status,
      box: r.result ? `${Math.round(r.result.box.width)}x${Math.round(r.result.box.height)}` : null,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
        ),
    }));
  };
  const sideways = (page: Page) =>
    page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
    }));

  try {
    for (const sc of SCENARIOS) {
      const prefs = {
        theme: sc.main,
        zoom: 100,
        sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: sc.arcade },
      };
      await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
      const back = await readPrefs();
      const backArcade = (back.sectionThemes as { arcade?: string } | undefined)?.arcade;
      expect(back.theme, 'account theme read back').toBe(sc.main);
      expect(backArcade, 'arcade section theme read back').toBe(sc.arcade);

      const ctx = await browser.newContext({
        baseURL: ORIGIN,
        viewport: { width: sc.w, height: sc.h },
        storageState: await ownerCtx.storageState(),
        bypassCSP: true,
        ...(sc.phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
      });
      await ctx.addInitScript((p) => {
        try {
          localStorage.setItem(
            'sr.user.prefs',
            JSON.stringify({ state: { prefs: p }, version: 2 }),
          );
        } catch {
          /* no storage */
        }
      }, prefs);
      const page = await ctx.newPage();
      page.setDefaultTimeout(10_000);
      let file = sc.shot;

      if (sc.page === 'soon') {
        await page.goto('/games', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await page.locator('.arc-root').waitFor({ timeout: 30_000 });
        await idle(page, 4000);
        const soon = page.locator('#coming-soon');
        await soon.waitFor({ timeout: 15_000 });
        const theme = await page
          .locator('[data-theme]:has(.arc-root)')
          .last()
          .getAttribute('data-theme');
        expect(theme, 'data-theme read back from the Arcade root').toBe(sc.arcade);
        const cards = seeded.map(({ s }) =>
          soon.locator('.arc-card', { has: page.getByText(s.title, { exact: true }) }),
        );
        for (const c of cards) await expect(c).toHaveCount(1);
        await cards[0]!.evaluate((el) => el.scrollIntoView({ block: 'center' }));
        await page.waitForTimeout(400);
        const facts = [];
        for (const [i, c] of cards.entries()) {
          const link = c.locator('a.arc-link');
          const actions = c.locator('div.arc-actions');
          const notify = c.getByRole('button', { name: 'Notify me' });
          const lb = await boxOf(link);
          const nb = await boxOf(notify);
          facts.push({
            title: seeded[i]!.s.title,
            card: await boxOf(c),
            actions: await boxOf(actions),
            link: lb,
            notify: nb,
            linkText: (await link.textContent())?.trim(),
            linkNotifyOverlapPx2: overlap(lb, nb),
          });
        }
        const sw = await sideways(page);
        // The shot first (the hit test scrolls), the cards in view.
        if (file && (PHASE === 'after' || BASE_SHOTS.has(file))) {
          const name = `${PHASE === 'base' ? 'base-' : ''}${file}`;
          await page.screenshot({ path: resolve(OUT, name) });
          shots.push(name);
        }
        const hit: Record<string, unknown> = {};
        for (const [i, c] of cards.entries()) hit[seeded[i]!.s.title] = await hits(page, c);
        log({ scenario: sc.key, theme, facts, sideways: sw, hit });
        const linkRows = Object.values(hit)
          .flat()
          .filter((h) => (h as { name: string }).name.toLowerCase().includes('repository'));
        console.log(
          `REPORT ${sc.key}: sideways ${sw.scrollWidth}/${sw.clientWidth}; link/card boxes ${JSON.stringify(facts.map((f) => [f.link, f.actions, f.card, f.linkNotifyOverlapPx2]))}; repository hit rows ${JSON.stringify(linkRows)}; all hit rows fails ${JSON.stringify(
            Object.values(hit)
              .flat()
              .filter((h) => (h as { status: string }).status === 'fail'),
          )}`,
        );
      } else {
        await page.goto('/admin/arcade', { waitUntil: 'domcontentloaded', timeout: 15_000 });
        await idle(page, 5000);
        const admin = page.locator('.arc-admin');
        await admin.waitFor({ timeout: 10_000 });
        const theme = await page.evaluate(
          () =>
            document.querySelector('.ss-root[data-theme]')?.getAttribute('data-theme') ?? 'none',
        );
        expect(theme, 'data-theme read back from the Admin root').toBe(sc.main);
        const items = seeded.map(({ s }) =>
          admin.locator('li', { has: page.locator('strong', { hasText: s.title }) }),
        );
        for (const it of items) await expect(it).toHaveCount(1);
        const facts = [];
        for (const [i, it] of items.entries()) {
          await it.scrollIntoViewIfNeeded();
          const text = ((await it.textContent()) ?? '').replace(/\s+/g, ' ').trim();
          facts.push({
            title: seeded[i]!.s.title,
            text,
            chip: (await it.locator('.arc-chip').allTextContents()).map((x) => x.trim()),
            li: await boxOf(it),
            form: await boxOf(it.locator('form.arc-release')),
            field: await boxOf(it.locator('.arc-field')),
            buttons: await Promise.all(
              (await it.getByRole('button').all()).map(async (b) => ({
                name: ((await b.textContent()) ?? '').trim().slice(0, 24),
                box: await boxOf(b),
              })),
            ),
          });
          if (PHASE === 'after')
            expect(text, `${seeded[i]!.s.title} shows its tier`).toContain(seeded[i]!.label);
        }
        const sw = await sideways(page);
        await items[0]!.scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        if (file && (PHASE === 'after' || BASE_SHOTS.has(file))) {
          const name = `${PHASE === 'base' ? 'base-' : ''}${file}`;
          await page.screenshot({ path: resolve(OUT, name) });
          shots.push(name);
        }
        const hit = await hits(page, admin);
        log({ scenario: sc.key, theme, facts, sideways: sw, hit });
        console.log(
          `REPORT ${sc.key}: sideways ${sw.scrollWidth}/${sw.clientWidth}; entries ${JSON.stringify(facts.map((f) => [f.text, f.li, f.form]))}; hit ${hit.length} controls, fails ${JSON.stringify(hit.filter((h) => h.status === 'fail'))}`,
        );
      }
      file = null;
      await ctx.close();
    }
  } finally {
    for (const { s } of seeded)
      await s
        .remove()
        .catch((e) => notes.push(`could not remove ${s.id}: ${String(e).slice(0, 80)}`));
    // The account's settings back as found, with the Arcade's shipped theme set explicitly.
    try {
      await apiPatch(o, '/users/@me/settings', {
        settings: {
          user_prefs: {
            ...original,
            sectionThemes: {
              ...((original.sectionThemes as Record<string, string> | undefined) ?? {}),
              arcade: SHIPPED,
            },
          },
        },
      });
      const end = await readPrefs();
      console.log(
        `REPORT settings put back: theme ${end.theme}, arcade ${(end.sectionThemes as { arcade?: string } | undefined)?.arcade}`,
      );
    } catch (e) {
      notes.push(`could not put the settings back: ${String(e).slice(0, 100)}`);
    }
    await ownerCtx.close();
  }
  console.log(`REPORT shots ${shots.length}: ${shots.join(', ')}`);
  for (const n of notes) console.log(`REPORT note ${n}`);
});
