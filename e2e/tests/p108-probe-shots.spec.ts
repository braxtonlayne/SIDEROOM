import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, expect, test } from '../support/fixtures';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-1025 (wave 108, probe). The hit-test helper's edge rule: a test point outside the viewport cannot be tapped and is
 * not tested (the point records `edge`); a control passes when every point inside the viewport passes and at least one is
 * inside; a control under 44 px that sits at the screen's edge also gets a third point ('span') at the far side of a 44 px
 * span measured from the edge inward, so it cannot pass by hiding half its target off screen. Rows gain `edge`.
 *
 * It runs `openCollapsed` then `hitTestAll` (default touch reach) at 375 x 812 with touch, theme lounge-light, over six
 * roots, every row written whole to `figures-<phase>.json`:
 *  - the section bars (the chrome outside `main`, as the censuses reached it): Library Browse, Cinema Browse, Lounge Home;
 *  - three ordinary roots: a channel page (main), Server settings > Members (main), Library Browse (main).
 * Before the helper is edited PHASE=base is run, after it PHASE=after. In the after phase, when `figures-base.json` is in
 * OUT, it compares: on the ordinary roots every row is equal (tag, role, type, name, path, status, reason, box, offset and
 * the points that are inside the viewport) except a row whose control touches the viewport's edge (a point of the base
 * lay outside, or the helper now reports an edge): each such row is listed as a note, old and new. On the section bars
 * every row that is not skipped has a real verdict (pass, or fail with a taker named), never a null or not-reached.
 * `REPORT bar` lines give the section bars' table (box, visible reach, verdict, taker).
 *
 * It never seeds and makes nothing. Local only (SR_P108_PROBE_ON=1). Both phases run on the base bundle (`shoot base`).
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p108-integ/apps/web/dist --env SR_P108_PROBE_ON=1 --env SR_P108_PROBE_PHASE=after --env SR_P108_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-probe -- tests/p108-probe-shots.spec.ts
 */
const ON = process.env.SR_P108_PROBE_ON === '1';
const PHASE = process.env.SR_P108_PROBE_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P108_PROBE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p108-probe';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const VW = 375;
const VH = 812;
const SECTIONS = {
  library: 'sideroom',
  cinema: 'sideroom',
  'live-tv': 'sideroom-broadcast',
  arcade: 'lounge-editorial',
};

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () =>
  JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string; channelId: string };
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(600);
};

interface PointFig {
  at: string;
  x: number;
  y: number;
  relation: string | null;
  ok: boolean;
  edge: string | null;
  takerTag: string | null;
  takerRole: string | null;
  takerPath: string | null;
  takerName: string | null;
}
interface RowFig {
  index: number;
  tag: string;
  role: string | null;
  type: string | null;
  name: string;
  path: string;
  status: string;
  reason: string | null;
  edge: string | null;
  box: { x: number; y: number; width: number; height: number } | null;
  offset: number | null;
  points: PointFig[];
}
interface RootFig {
  root: string;
  kind: 'bar' | 'ordinary';
  opened: number;
  rows: RowFig[];
}

// The helper's `edge` fields do not exist before D-1025: read them through a loose type so the spec compiles both ways.
type Loose = { edge?: string | null };

async function record(
  page: Page,
  root: Locator,
  rootName: string,
  kind: 'bar' | 'ordinary',
): Promise<RootFig> {
  const opened = await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root);
  const figs: RowFig[] = rows.map((r) => ({
    index: r.index,
    tag: r.tag,
    role: r.role,
    type: r.type,
    name: r.name,
    path: r.path,
    status: r.status,
    reason: r.reason ?? null,
    edge: (r as Loose).edge ?? null,
    box: r.result?.box ?? null,
    offset: r.result?.offset ?? null,
    points: (r.result?.points ?? []).map((p) => ({
      at: p.at,
      x: p.x,
      y: p.y,
      relation: p.relation,
      ok: p.ok,
      edge: (p as Loose).edge ?? null,
      takerTag: p.taker?.tag ?? null,
      takerRole: p.taker?.role ?? null,
      takerPath: p.taker?.path ?? null,
      takerName: p.taker?.name ?? null,
    })),
  }));
  const count = (s: string) => rows.filter((r) => r.status === s).length;
  say(
    `${rootName} (${kind}): ${rows.length} rows, pass ${count('pass')}, fail ${count('fail')}, not-reached ${count('not-reached')}, skipped ${count('skipped')}`,
  );
  for (const f of figs)
    say(
      `  ${rootName} #${f.index} ${f.tag} "${f.name}" ${f.status}${f.reason ? ` (${f.reason.slice(0, 90)})` : ''} ` +
        `box ${f.box ? `y${Math.round(f.box.y)} ${Math.round(f.box.width)}x${Math.round(f.box.height)}` : '-'} edge ${f.edge}`,
    );
  return { root: rootName, kind, opened: opened.length, rows: figs };
}

/** The section bars' table: box, how much of the 44 px span from the edge the control covers, the verdict. */
function barTable(roots: RootFig[]) {
  for (const r of roots.filter((x) => x.kind === 'bar')) {
    for (const f of r.rows) {
      if (!f.box) {
        say(`bar ${r.root} #${f.index} "${f.name}" ${f.status} (no box) ${f.reason ?? ''}`);
        continue;
      }
      const top = Math.max(0, f.box.y);
      const bottom = Math.min(VH, f.box.y + f.box.height);
      const fromTop = Math.round(bottom);
      const taker = f.points.find((p) => !p.ok);
      say(
        `bar ${r.root} #${f.index} ${f.tag} "${f.name}" ${f.status} box y${Math.round(f.box.y)}..${Math.round(f.box.y + f.box.height)} ` +
          `h${Math.round(f.box.height)} w${Math.round(f.box.width)} on-screen ${Math.round(bottom - top)} (to ${fromTop} px from the top edge; 44 asked) ` +
          `edge ${f.edge} points [${f.points.map((p) => `${p.at}@${Math.round(p.y)}${p.edge ? `:edge-${p.edge}` : p.ok ? ':ok' : ':FAIL'}`).join(' ')}]` +
          (taker
            ? ` taker ${taker.takerTag}${taker.takerRole ? `[${taker.takerRole}]` : ''} ${taker.takerPath}`
            : ''),
      );
    }
  }
}

const insidePoints = (f: RowFig) =>
  f.points
    .filter((p) => p.x >= 0 && p.y >= 0 && p.x < VW && p.y < VH)
    .map((p) => ({ ...p, takerName: undefined, edge: undefined }));

function compare(base: RootFig[], after: RootFig[], problems: string[], notes: string[]) {
  for (const a of after) {
    const b = base.find((x) => x.root === a.root);
    if (!b) {
      problems.push(`${a.root}: not in figures-base.json`);
      continue;
    }
    if (a.rows.length !== b.rows.length)
      problems.push(`${a.root}: row count ${b.rows.length} -> ${a.rows.length}`);
    for (const ar of a.rows) {
      const br = b.rows[ar.index];
      const id = `${a.root} #${ar.index} ${ar.tag} "${ar.name}"`;
      if (!br) continue;
      if (a.kind === 'bar') {
        if (ar.status === 'not-reached')
          problems.push(`${id}: still not-reached (${ar.reason ?? ''}); base ${br.status}`);
        if (ar.status === 'fail' && !ar.points.some((p) => !p.ok && p.takerTag))
          problems.push(`${id}: fail with no taker named`);
        notes.push(`${id}: base ${br.status} -> ${ar.status}, edge ${ar.edge}`);
        continue;
      }
      const touchesEdge =
        br.points.some((p) => p.x < 0 || p.y < 0 || p.x >= VW || p.y >= VH) ||
        br.status === 'not-reached' ||
        ar.edge !== null ||
        (br.box !== null && (br.box.y < 22 || br.box.y + br.box.height > VH - 22));
      const same = (k: string, x: unknown, y: unknown) => {
        if (JSON.stringify(x) === JSON.stringify(y)) return;
        if (touchesEdge)
          notes.push(`${id}: EDGE ROW ${k} ${JSON.stringify(x)} -> ${JSON.stringify(y)}`);
        else problems.push(`${id}: ${k} ${JSON.stringify(x)} -> ${JSON.stringify(y)}`);
      };
      same('tag/role/type', [br.tag, br.role, br.type], [ar.tag, ar.role, ar.type]);
      same('name', br.name, ar.name);
      same('path', br.path, ar.path);
      same('status', br.status, ar.status);
      same('reason', br.reason, ar.reason);
      same('box', br.box, ar.box);
      same('offset', br.offset, ar.offset);
      same('points inside the viewport', insidePoints(br), insidePoints(ar));
      if (touchesEdge)
        notes.push(
          `${id}: touches the edge; base ${br.status}, after ${ar.status}, edge ${ar.edge}`,
        );
    }
  }
}

async function newPage(browser: Browser, owner: BrowserContext) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: VW, height: VH },
    storageState: await owner.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
  });
  await ctx.addInitScript(
    (p) => {
      try {
        localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
      } catch {
        /* no storage */
      }
    },
    { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS },
  );
  const page = await ctx.newPage();
  page.setDefaultTimeout(20_000);
  return { ctx, page };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  await page.evaluate(() => window.scrollTo(0, 0)).catch(() => undefined);
  const sideways = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sideways.join('/')}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

const CHROME = 'header:not(main header), nav:not(main nav)';

test('D-1025: the hit-test helper judges controls at the screen edge', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  const problems: string[] = [];
  const notes: string[] = [];
  const roots: RootFig[] = [];
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100, sectionThemes: SECTIONS } },
    });

    const visit = async (
      key: string,
      path: string,
      ready: (p: Page) => Promise<unknown>,
      what: 'bar' | 'ordinary',
      root: (p: Page) => Locator,
    ) => {
      const { ctx, page } = await newPage(browser, ownerCtx);
      try {
        await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await ready(page);
        await idle(page, 4000);
        const r = root(page);
        if ((await r.count()) === 0) {
          problems.push(`${key}: root not found`);
          return;
        }
        roots.push(await record(page, r, key, what));
        await shot(page, `probe-${key}-375`);
      } catch (e) {
        problems.push(`${key}: ${String(e).slice(0, 300)}`);
      } finally {
        await ctx.close();
      }
    };
    const main = (p: Page) => p.getByRole('main');
    const waitMain = (p: Page) => main(p).first().waitFor({ timeout: 30_000 });

    // The section bars (the chrome outside `main`).
    await visit(
      'bar-library',
      '/library/browse',
      async (p) => {
        await waitMain(p);
        await p.waitForTimeout(1500);
      },
      'bar',
      (p) => p.locator(CHROME),
    );
    await visit(
      'bar-cinema',
      '/cinema/movies',
      async (p) => {
        await waitMain(p);
        await p.waitForTimeout(1500);
      },
      'bar',
      (p) => p.locator(CHROME),
    );
    await visit('bar-lounge-home', '/channels/@me', waitMain, 'bar', (p) => p.locator(CHROME));
    // Three ordinary roots.
    await visit(
      'channel',
      `/channels/${w.guildId}/${w.channelId}`,
      (p) => p.getByRole('log').first().waitFor({ timeout: 30_000 }),
      'ordinary',
      main,
    );
    await visit(
      'members',
      `/settings/server/${w.guildId}/members`,
      (p) => p.waitForSelector('main table tbody tr', { timeout: 25_000 }),
      'ordinary',
      main,
    );
    await visit(
      'library-browse',
      '/library/browse',
      async (p) => {
        await waitMain(p);
        await p.waitForTimeout(1500);
      },
      'ordinary',
      main,
    );

    for (const r of roots) {
      if (r.rows.length === 0) problems.push(`${r.root}: no controls found`);
      if (r.kind === 'ordinary') {
        const nr = r.rows.filter((x) => x.status === 'not-reached').length;
        if (nr) problems.push(`${r.root}: ${nr} rows not reached (a probe error)`);
      }
    }
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(roots, null, 1));
    barTable(roots);
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      compare(JSON.parse(readFileSync(baseFile, 'utf8')) as RootFig[], roots, problems, notes);
    }
    for (const n of notes) say(`note: ${n}`);
    say(`problems: ${JSON.stringify(problems, null, 1)}`);
    expect(problems).toEqual([]);
  } finally {
    await ownerCtx.close();
  }
});
