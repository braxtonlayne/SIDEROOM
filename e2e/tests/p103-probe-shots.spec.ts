import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { CONTROL_SELECTOR, hitTest, hitTestAll, openCollapsed } from '../support/hit-test';
import type { ControlRow, HitOptions } from '../support/hit-test';
import { seedActivityTier } from '../support/p100-seed';

/**
 * Wave 103, probe (SEC-AUD-467 / D-983): the shared hit test gets `reach: 'box'`.
 *
 * WHAT IT PROVES, in one run on one world:
 *  1. At 1024 x 768 on Admin > Arcade (two "Coming soon" entries, seeded through `seedActivityTier`, own names, removed
 *     in a `finally`) and on the Lounge's Notification Settings dialog (kit select): `hitTestAll` with the default and
 *     with `reach: 'box'` over the same root. The default lists the 28 px buttons as fails; `'box'` lists none for them.
 *  2. A real overlap still fails with `'box'`: the spec lays an element of its own (`aria-label="p103-probe-overlay"`)
 *     over the lower half of one 28 px button, by `evaluate`, and removes it again. `'box'` must list that button as a
 *     fail with the overlay as taker (and the same button without the overlay must pass).
 *  3. At 375 x 812 touch: the unedited helper (the text of commit 050be81d, written to `e2e/.state/p103-probe/` at the
 *     start of the run) and the new helper's default give the same rows (name, status, takers), and `'box'` gives the
 *     same rows as the default for every control that is 44 px or taller (offset 21 for both).
 *  4. At 1024, the new default against the unedited helper's default: the same rows too.
 *
 * Needs the visual sweep's seeded world (`e2e/.state/sweep/owner.json`, `world.json`) and a stack with
 * `SR_ADMIN_REQUIRE_REAUTH=0`. The account's settings are put back at the end. Every figure goes to `figures.json`.
 *
 * The command the commander runs (from `e2e/`, after the sweep world is seeded on the same database; ports 8808 /
 * 5808 / 9508, database sr_p103_probe_ui):
 *   SR_P103_PROBE_ON=1 SR_E2E_BASE_URL=http://localhost:5808 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
 *   DATABASE_URL=<db> SR_P103_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-probe \
 *   npx playwright test tests/p103-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
 * The stack: sr-server with SR_DATABASE__URL=<db> SR_DATABASE__AUTO_MIGRATE=true SR_SERVER__BIND=127.0.0.1:8808
 * SR_SERVER__PUBLIC_URL=http://localhost:5808 SR_STORAGE__ENDPOINT=memory: SR_ACTIVITIES__ENABLED=true
 * SR_SERVER__METRICS_BIND=127.0.0.1:9508 SR_ADMIN_REQUIRE_REAUTH=0; Vite with SR_DEV_API_PORT=8808 VITE_SR_E2E=1
 * VITE_ACTIVITIES_ENABLED=true on 5808.
 * Variables: SR_P103_PROBE_ON, SR_P103_PROBE_OUT, SR_P103_PROBE_BASE_REF (default 050be81d: the helper's last text
 * before wave 103; needs `git` and the repository).
 */
const ON = process.env.SR_P103_PROBE_ON === '1';
const OUT =
  process.env.SR_P103_PROBE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p103-probe';
const BASE_REF = process.env.SR_P103_PROBE_BASE_REF ?? '050be81d';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SHIPPED = 'lounge-editorial';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

type HitAll = (page: Page, root: Locator, opts?: HitOptions) => Promise<ControlRow[]>;

interface Slim {
  i: number;
  tag: string;
  name: string;
  status: string;
  h: number | null;
  offset: number | null;
  takers: string[];
}
const slim = (rows: ControlRow[]): Slim[] =>
  rows.map((r) => ({
    i: r.index,
    tag: r.tag,
    name: r.name,
    status: r.status,
    h: r.result ? Math.round(r.result.box.height * 10) / 10 : null,
    offset: (r.result as { offset?: number } | undefined)?.offset ?? null,
    takers: (r.result?.points ?? [])
      .filter((p) => !p.ok)
      .map(
        (p) =>
          `${p.at}: ${p.taker?.tag ?? 'none'} "${p.taker?.name ?? ''}" (${p.taker?.path ?? ''})`,
      ),
  }));
/** What two helpers must agree on: control, status and takers (heights and offsets are the same page state). */
const key = (s: Slim) => `${s.i}|${s.tag}|${s.name}|${s.status}|${s.takers.join(';')}`;
const fails = (s: Slim[]) => s.filter((r) => r.status === 'fail');
const notReached = (s: Slim[]) => s.filter((r) => r.status === 'not-reached');

const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };

async function openNotification(page: Page): Promise<Locator> {
  const w = world();
  await page.goto(`/channels/${w.guildId}/${w.channelId}`, {
    waitUntil: 'domcontentloaded',
    timeout: 20_000,
  });
  await page
    .getByTestId('channel-sidebar')
    .first()
    .waitFor({ timeout: 20_000 })
    .catch(() => undefined);
  await page.waitForTimeout(1200);
  await page.evaluate(
    async (d) => {
      const m = (await import(/* @vite-ignore */ String('/src/lounge/store.ts'))) as {
        useLoungeUi: { getState: () => { openDialog: (x: unknown) => void } };
      };
      m.useLoungeUi.getState().openDialog(d);
    },
    { kind: 'notifications', guildId: w.guildId },
  );
  const dlg = page.getByRole('dialog', { name: /Notification Settings/ });
  await dlg.waitFor({ timeout: 15_000 });
  await dlg.locator('select').first().waitFor({ timeout: 15_000 });
  await page.waitForLoadState('networkidle', { timeout: 4000 }).catch(() => undefined);
  await page.waitForTimeout(600);
  return dlg;
}

test('SEC-AUD-467: hitTestAll reach "box" against the default, the unedited helper and a forced overlap', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const stateDir = resolve(repoRoot, 'e2e', '.state', 'p103-probe');
  mkdirSync(stateDir, { recursive: true });
  const baseFile = resolve(stateDir, 'hit-test.base.ts');
  writeFileSync(
    baseFile,
    execFileSync('git', ['--no-pager', 'show', `${BASE_REF}:e2e/support/hit-test.ts`], {
      cwd: repoRoot,
      encoding: 'utf8',
    }),
  );
  const baseMod = (await import(/* @vite-ignore */ pathToFileURL(baseFile).href)) as {
    hitTestAll: HitAll;
    openCollapsed: typeof openCollapsed;
  };
  expect(typeof baseMod.hitTestAll, 'the base helper loaded').toBe('function');

  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const notes: string[] = [];
  const figures: Record<string, unknown> = {};
  const problems: string[] = [];
  const say = (s: string) => console.log(`REPORT ${s}`);
  const orig = await seedActivityTier({ owner: o }, 'TIER_1_ORIGINAL');
  const reimpl = await seedActivityTier({ owner: o }, 'TIER_2_REIMPLEMENTATION');
  const seeded = [orig, reimpl];

  const readPrefs = async () => {
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: Record<string, unknown> & { theme?: string } };
    };
    return back.settings?.user_prefs ?? {};
  };
  const original = await readPrefs();
  const prefs = {
    theme: 'lounge-light',
    zoom: 100,
    sectionThemes: { cinema: 'sideroom', library: 'sideroom', arcade: SHIPPED },
  };
  await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: prefs } });
  expect((await readPrefs()).theme, 'account theme read back').toBe('lounge-light');

  const newPage = async (ctxw: BrowserContext | null, phone: boolean) => {
    const ctx = await browser.newContext({
      baseURL: ORIGIN,
      viewport: phone ? { width: 375, height: 812 } : { width: 1024, height: 768 },
      storageState: await (ctxw ?? ownerCtx).storageState(),
      bypassCSP: true,
      ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
    });
    await ctx.addInitScript((p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    }, prefs);
    const page = await ctx.newPage();
    page.setDefaultTimeout(10_000);
    return { ctx, page };
  };
  const gotoAdmin = async (page: Page) => {
    await page.goto('/admin/arcade', { waitUntil: 'domcontentloaded', timeout: 15_000 });
    await page.waitForLoadState('networkidle', { timeout: 5000 }).catch(() => undefined);
    await page.waitForTimeout(500);
    const admin = page.locator('.arc-admin');
    await admin.waitFor({ timeout: 10_000 });
    for (const s of seeded)
      await expect(
        admin.locator('li', { has: page.locator('strong', { hasText: s.title }) }),
      ).toHaveCount(1);
    return admin;
  };
  const sideways = (page: Page) =>
    page.evaluate(() => [
      document.documentElement.scrollWidth,
      document.documentElement.clientWidth,
    ]);

  /** The three runs over one root, in the page state as it is (nothing is changed between them). */
  const runAll = async (page: Page, root: Locator, label: string, withBox: boolean) => {
    await openCollapsed(page, root);
    const baseRows = slim(await baseMod.hitTestAll(page, root));
    const defRows = slim(await hitTestAll(page, root));
    const boxRows = withBox ? slim(await hitTestAll(page, root, { reach: 'box' })) : null;
    figures[label] = { base: baseRows, default: defRows, box: boxRows };
    for (const set of [baseRows, defRows, ...(boxRows ? [boxRows] : [])]) {
      if (notReached(set).length) problems.push(`${label}: not-reached rows (probe error)`);
    }
    // The new default equals the unedited helper: control, status, takers.
    const a = baseRows.map(key);
    const b = defRows.map(key);
    if (JSON.stringify(a) !== JSON.stringify(b))
      problems.push(`${label}: default differs from the unedited helper`);
    say(
      `${label}: ${defRows.length} controls; base fails ${fails(baseRows).length}; default fails ${fails(defRows).length}${boxRows ? `; box fails ${fails(boxRows).length}` : ''}`,
    );
    return { baseRows, defRows, boxRows };
  };

  try {
    // ---------- 1024: Admin > Arcade ----------
    const desk = await newPage(null, false);
    const admin1 = await gotoAdmin(desk.page);
    await admin1.locator('li').first().scrollIntoViewIfNeeded();
    const r1024 = await runAll(desk.page, admin1, 'admin-arcade-1024', true);
    const short = r1024.defRows.filter((r) => r.h !== null && r.h < 44);
    const shortFailsDefault = short.filter((r) => r.status === 'fail');
    say(
      `admin-arcade-1024: controls under 44 px ${short.length}; of them default fails ${shortFailsDefault.length}; box fails ${fails(r1024.boxRows!).length}`,
    );
    if (shortFailsDefault.length === 0)
      problems.push(
        'admin-arcade-1024: the default lists no short control as a fail (expected the 28 px buttons)',
      );
    if (fails(r1024.boxRows!).length)
      problems.push(
        `admin-arcade-1024: box still lists fails: ${JSON.stringify(fails(r1024.boxRows!))}`,
      );
    for (const r of r1024.boxRows!)
      if (r.h !== null && r.offset !== Math.max(1, Math.min(21, Math.floor(r.h / 2) - 1)))
        problems.push(`box offset wrong for "${r.name}": h ${r.h}, offset ${r.offset}`);
    expect((await sideways(desk.page))[0]).toBeLessThanOrEqual((await sideways(desk.page))[1]!);
    await desk.page.screenshot({ path: resolve(OUT, 'admin-arcade-1024-clean.png') });

    // ---------- the forced overlap ----------
    const target =
      short.find((r) => r.tag === 'button' && r.status === 'fail' && r.h === 28) ??
      short.find((r) => r.tag === 'button');
    if (!target) throw new Error('no short button to cover');
    const loc = admin1.locator(CONTROL_SELECTOR).nth(target.i);
    const before = await hitTest(desk.page, loc, { reach: 'box' });
    figures['forced-overlap'] = { button: target.name, h: target.h, before: before.pass };
    if (!before.pass)
      problems.push(`forced overlap: "${target.name}" does not pass with box before the overlay`);
    const rect = await loc.evaluate((el) => {
      const b = el.getBoundingClientRect();
      return { x: b.left, y: b.top, w: b.width, h: b.height };
    });
    await loc.evaluate((el) => {
      const b = el.getBoundingClientRect();
      const d = document.createElement('div');
      d.setAttribute('aria-label', 'p103-probe-overlay');
      d.className = 'p103-probe-overlay';
      d.style.cssText = `position:fixed;left:${b.left}px;top:${b.top + b.height / 2}px;width:${b.width}px;height:${b.height / 2}px;background:rgba(220,40,40,.55);outline:2px solid #b00;z-index:2147483000`;
      document.body.appendChild(d);
    });
    try {
      const withBox = await hitTest(desk.page, loc, { reach: 'box' });
      const takers = withBox.points.filter((p) => !p.ok);
      figures['forced-overlap'] = {
        button: target.name,
        h: target.h,
        before: before.pass,
        rect,
        offset: withBox.offset,
        pass: withBox.pass,
        takers: takers.map((p) => `${p.at}: ${p.taker?.tag} "${p.taker?.name}" (${p.taker?.path})`),
      };
      say(`forced overlap on "${target.name}": ${JSON.stringify(figures['forced-overlap'])}`);
      if (withBox.pass)
        problems.push('forced overlap: box passed with an element over the lower half');
      if (!takers.some((p) => p.at === 'below' && p.taker?.name === 'p103-probe-overlay'))
        problems.push('forced overlap: the overlay is not named as the taker of the point below');
      await desk.page.screenshot({ path: resolve(OUT, 'admin-arcade-1024.png') });
    } finally {
      await desk.page.evaluate(() => document.querySelector('.p103-probe-overlay')?.remove());
    }
    const after = await hitTest(desk.page, loc, { reach: 'box' });
    if (!after.pass)
      problems.push('forced overlap: the button does not pass again after the overlay is removed');
    await desk.ctx.close();

    // ---------- 1024: Notification dialog ----------
    const deskN = await newPage(null, false);
    const dlg1 = await openNotification(deskN.page);
    const n1024 = await runAll(deskN.page, dlg1, 'notification-1024', true);
    say(
      `notification-1024: selects heights ${JSON.stringify(n1024.defRows.filter((r) => r.tag === 'select').map((r) => r.h))}; default fails ${JSON.stringify(fails(n1024.defRows).map((r) => [r.name, r.h]))}; box fails ${JSON.stringify(fails(n1024.boxRows!).map((r) => [r.name, r.h]))}`,
    );
    if (fails(n1024.boxRows!).length)
      problems.push(`notification-1024: box lists fails: ${JSON.stringify(fails(n1024.boxRows!))}`);
    await dlg1.scrollIntoViewIfNeeded();
    await deskN.page.screenshot({ path: resolve(OUT, 'notification-dialog-1024.png') });
    await deskN.ctx.close();

    // ---------- 375 touch ----------
    const ph = await newPage(null, true);
    const admin3 = await gotoAdmin(ph.page);
    await admin3.locator('li').first().scrollIntoViewIfNeeded();
    const r375 = await runAll(ph.page, admin3, 'admin-arcade-375', true);
    const sw = await sideways(ph.page);
    figures['admin-arcade-375-sideways'] = sw;
    expect(sw[0]).toBeLessThanOrEqual(sw[1]!);
    await ph.page.screenshot({ path: resolve(OUT, 'admin-arcade-375.png') });
    await ph.ctx.close();
    const phN = await newPage(null, true);
    const dlg3 = await openNotification(phN.page);
    const n375 = await runAll(phN.page, dlg3, 'notification-375', true);
    await phN.ctx.close();
    for (const [label, set] of [
      ['admin-arcade-375', r375],
      ['notification-375', n375],
    ] as const) {
      const tall = (rows: Slim[]) => rows.filter((r) => r.h !== null && r.h >= 44).map(key);
      if (JSON.stringify(tall(set.defRows)) !== JSON.stringify(tall(set.boxRows!)))
        problems.push(`${label}: box differs from the default on the 44 px controls`);
      const offs = set.boxRows!.filter((r) => r.h !== null && r.h >= 44 && r.offset !== 21);
      if (offs.length) problems.push(`${label}: box offset is not 21 on a 44 px control`);
      const shortN = set.defRows.filter((r) => r.h !== null && r.h < 44).length;
      say(
        `${label}: controls ${set.defRows.length}, under 44 px ${shortN}; default fails ${fails(set.defRows).length}; box fails ${fails(set.boxRows!).length}; 44 px rows equal: ${!problems.some((p) => p.startsWith(label))}`,
      );
    }
  } finally {
    for (const s of seeded)
      await s
        .remove()
        .catch((e) => notes.push(`could not remove ${s.id}: ${String(e).slice(0, 80)}`));
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
      say(`settings put back: theme ${end.theme}`);
    } catch (e) {
      notes.push(`could not put the settings back: ${String(e).slice(0, 100)}`);
    }
    await ownerCtx.close();
    writeFileSync(
      resolve(OUT, 'figures.json'),
      JSON.stringify({ baseRef: BASE_REF, problems, notes, figures }, null, 1),
    );
  }
  for (const n of notes) say(`note ${n}`);
  say(`problems: ${JSON.stringify(problems)}`);
  expect(problems).toEqual([]);
});
