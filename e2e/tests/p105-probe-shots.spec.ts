import { mkdirSync, readFileSync, existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, expect, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { CONTROL_SELECTOR, hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * D-1012 (wave 105, probe): SEC-AUD-485. The hit-test helper names a control from its own `<label>` when it has no
 * other name. Order: `aria-label`; `aria-labelledby`; the text of the control's own `<label>`; its own text;
 * `placeholder`; `title`. Statuses, takers, reach and counts must not change; only the `name` of a row may.
 *
 * It runs `openCollapsed` then `hitTestAll` over four roots: Server settings > Members at 375 touch (main: the table and
 * the bulk bar), Admin > Donations > Entitlements' toolbar at 375 touch (a checkbox in a label), a forum channel's
 * toolbar at 375 touch, and Server settings > Members at 1024 with `reach: 'box'`. Every row is written whole (name,
 * status, takers, path, box, and what the element's name could be made of) to `figures-<phase>.json`.
 *
 * Both phases run the same spec on the base bundle (`shoot base`); the helper is the one in this worktree, so PHASE=base
 * is run before the helper is edited and PHASE=after after it. In the after phase, when `figures-base.json` is in OUT,
 * it compares: same row count; same status, reason, path, box, offset, points and takers (tag, role, path) per row; a
 * name differs only where the base's was "" or where the element's own label text now comes first (and then the new
 * name is that label's text); no row is named "" unless the element has no aria-label, label, text, placeholder or title.
 *
 * It makes one forum channel through the API (`p105probe-forum-<stamp>`) and removes it in a `finally`. It never seeds.
 *
 * Local only (SR_P105_PROBE_ON=1). The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_PROBE_ON=1 --env SR_P105_PROBE_PHASE=after --env SR_P105_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-probe -- tests/p105-probe-shots.spec.ts
 */
const ON = process.env.SR_P105_PROBE_ON === '1';
const PHASE = process.env.SR_P105_PROBE_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P105_PROBE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-probe';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);
const NAV = '[role="dialog"] nav ul li > button';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as { guildId: string };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(500);
};

interface Parts {
  ariaLabel: string;
  labelledby: string;
  /** The text of the element's own label (the control itself left out), folded, 40 characters. */
  label: string;
  own: string;
  placeholder: string;
  title: string;
}
interface PointFig {
  at: string;
  x: number;
  y: number;
  relation: string | null;
  ok: boolean;
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
  box: { x: number; y: number; width: number; height: number } | null;
  offset: number | null;
  points: PointFig[];
  parts: Parts;
}
interface RootFig {
  root: string;
  reach: string;
  opened: number;
  rows: RowFig[];
}

async function parts(root: Locator, i: number): Promise<Parts> {
  return root
    .locator(CONTROL_SELECTOR)
    .nth(i)
    .evaluate((el) => {
      const fold = (x: string) => x.trim().replace(/\s+/g, ' ').slice(0, 40);
      const labels = (el as HTMLInputElement).labels;
      const lab = labels ? labels[0] : el instanceof HTMLLabelElement ? null : el.closest('label');
      const text = (n: Node): string =>
        n === el
          ? ''
          : n.nodeType === 3
            ? (n.textContent ?? '')
            : Array.from(n.childNodes).map(text).join(' ');
      const by = el.getAttribute('aria-labelledby');
      return {
        ariaLabel: (el.getAttribute('aria-label') ?? '').trim(),
        labelledby: by
          ? fold(
              by
                .split(/\s+/)
                .map((id) => document.getElementById(id)?.textContent ?? '')
                .join(' '),
            )
          : '',
        label: lab ? fold(labels ? text(lab) : (lab.textContent ?? '')) : '',
        own: fold(el.textContent ?? ''),
        placeholder: el.getAttribute('placeholder') ?? '',
        title: el.getAttribute('title') ?? '',
      };
    });
}

async function record(
  page: Page,
  root: Locator,
  rootName: string,
  reach: 'touch' | 'box',
): Promise<RootFig> {
  const opened = await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root, reach === 'box' ? { reach: 'box' } : {});
  const figs: RowFig[] = [];
  for (const r of rows) {
    figs.push({
      index: r.index,
      tag: r.tag,
      role: r.role,
      type: r.type,
      name: r.name,
      path: r.path,
      status: r.status,
      reason: r.reason ?? null,
      box: r.result?.box ?? null,
      offset: r.result?.offset ?? null,
      points: (r.result?.points ?? []).map((p) => ({
        at: p.at,
        x: p.x,
        y: p.y,
        relation: p.relation,
        ok: p.ok,
        takerTag: p.taker?.tag ?? null,
        takerRole: p.taker?.role ?? null,
        takerPath: p.taker?.path ?? null,
        takerName: p.taker?.name ?? null,
      })),
      parts: await parts(root, r.index),
    });
  }
  const count = (s: string) => rows.filter((r) => r.status === s).length;
  say(
    `${rootName} (${reach}): ${rows.length} rows, pass ${count('pass')}, fail ${count('fail')}, not-reached ${count('not-reached')}, skipped ${count('skipped')}; ` +
      `names "" : ${figs.filter((f) => f.name === '').length}`,
  );
  for (const f of figs)
    say(
      `  ${rootName} #${f.index} ${f.tag}${f.type ? `[${f.type}]` : ''} name="${f.name}" ${f.status}${f.reason ? ` (${f.reason.slice(0, 60)})` : ''} ` +
        `box ${f.box ? `${Math.round(f.box.width)}x${Math.round(f.box.height)}` : '-'} label="${f.parts.label}"`,
    );
  return { root: rootName, reach, opened: opened.length, rows: figs };
}

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
      const same = (k: string, x: unknown, y: unknown) => {
        if (JSON.stringify(x) !== JSON.stringify(y))
          problems.push(`${id}: ${k} ${JSON.stringify(x)} -> ${JSON.stringify(y)}`);
      };
      same('tag/role/type', [br.tag, br.role, br.type], [ar.tag, ar.role, ar.type]);
      same('status', br.status, ar.status);
      same('reason', br.reason, ar.reason);
      same('path', br.path, ar.path);
      same('box', br.box, ar.box);
      same('offset', br.offset, ar.offset);
      same(
        'points',
        br.points.map((p) => ({ ...p, takerName: undefined })),
        ar.points.map((p) => ({ ...p, takerName: undefined })),
      );
      if (br.name !== ar.name) {
        const labelFirst =
          ar.parts.ariaLabel === '' &&
          ar.parts.labelledby === '' &&
          ar.parts.label !== '' &&
          ar.name === ar.parts.label;
        if (br.name !== '' && !labelFirst)
          problems.push(`${id}: name "${br.name}" -> "${ar.name}" is not the label's text`);
        else notes.push(`${a.root} #${ar.index} ${ar.tag}: name "${br.name}" -> "${ar.name}"`);
      }
      if (ar.name === '') {
        const p = ar.parts;
        if (p.ariaLabel || p.labelledby || p.label || p.own || p.placeholder || p.title)
          problems.push(`${id}: named "" but has ${JSON.stringify(p)}`);
        else notes.push(`${a.root} #${ar.index} ${ar.tag}: named "" and truly has no name source`);
      }
      // Takers: an element's takers keep their tag/role/path; their names may change.
      ar.points.forEach((p, k) => {
        const bp = br.points[k];
        if (bp && bp.takerName !== p.takerName)
          notes.push(
            `${a.root} #${ar.index} taker ${p.at} ${p.takerTag}: name "${bp.takerName}" -> "${p.takerName}"`,
          );
      });
    }
  }
}

async function newPage(browser: Browser, owner: BrowserContext, touch: boolean, width: number) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width, height: touch ? 812 : 768 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript(() => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme: 'lounge-light', zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  });
  const page = await ctx.newPage();
  page.setDefaultTimeout(15_000);
  return { ctx, page };
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const sideways = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sideways.join('/')}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

test('SEC-AUD-485: the hit-test helper names a control from its label', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  const gid = world().guildId;
  const problems: string[] = [];
  const notes: string[] = [];
  const roots: RootFig[] = [];
  let forumId: string | null = null;
  try {
    await apiPatch(o, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
      settings?: { user_prefs?: { theme?: string } };
    };
    expect(back.settings?.user_prefs?.theme).toBe('lounge-light');

    // 1. Members at 375 touch, then 2. at 1024 with 'box'
    const members = async (touch: boolean) => {
      const { ctx, page } = await newPage(browser, ownerCtx, touch, touch ? 375 : 1024);
      try {
        await page.goto(`/settings/server/${gid}/members`, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        });
        await page.waitForSelector('main table tbody tr', { timeout: 25_000 });
        await idle(page, 4000);
        const fig = await record(
          page,
          page.getByRole('main'),
          touch ? 'members-375' : 'members-1024',
          touch ? 'touch' : 'box',
        );
        roots.push(fig);
        if (touch) await shot(page, 'probe-members-375');
      } finally {
        await ctx.close();
      }
    };
    await members(true);

    // 3. Admin > Donations > Entitlements, the toolbar, at 375 touch
    {
      const { ctx, page } = await newPage(browser, ownerCtx, true, 375);
      try {
        await page.goto('/admin/dashboard', { waitUntil: 'domcontentloaded', timeout: 30_000 });
        await idle(page, 4000);
        await page
          .locator(NAV)
          .filter({ hasText: /^Entitlements$/ })
          .first()
          .evaluate((el) => (el as HTMLElement).click());
        await idle(page, 3000);
        const toolbar = page
          .locator('label')
          .filter({ has: page.locator('input[type=checkbox]') })
          .first()
          .locator('xpath=..');
        await toolbar.waitFor({ timeout: 20_000 });
        roots.push(await record(page, toolbar, 'entitlements-toolbar-375', 'touch'));
        await toolbar.scrollIntoViewIfNeeded();
        await shot(page, 'probe-entitlements-375');
      } finally {
        await ctx.close();
      }
    }

    // 4. A forum channel's toolbar at 375 touch (one channel made through the API, removed below)
    {
      const f = await (async () => {
        try {
          return await apiPost<{ id: string }>(o, `/guilds/${gid}/channels`, {
            name: `p105probe-forum-${STAMP}`,
            type: 15,
          });
        } catch (e) {
          if (!String(e).includes('429')) throw e;
          say('429 on forum create: one retry after 4 s');
          await sleep(4000);
          return apiPost<{ id: string }>(o, `/guilds/${gid}/channels`, {
            name: `p105probe-forum-${STAMP}`,
            type: 15,
          });
        }
      })();
      forumId = f.id;
      const { ctx, page } = await newPage(browser, ownerCtx, true, 375);
      try {
        await page.goto(`/channels/${gid}/${f.id}`, {
          waitUntil: 'domcontentloaded',
          timeout: 30_000,
        });
        const sort = page.getByRole('combobox', { name: 'Sort by' });
        await sort.waitFor({ timeout: 40_000 });
        await idle(page, 4000);
        const toolbar = sort.locator('xpath=ancestor::label/..');
        roots.push(await record(page, toolbar, 'forum-toolbar-375', 'touch'));
        await shot(page, 'probe-forum-375');
      } finally {
        await ctx.close();
      }
    }

    await members(false);

    for (const r of roots) {
      if (r.rows.length === 0) problems.push(`${r.root}: no controls found`);
      const nr = r.rows.filter((x) => x.status === 'not-reached').length;
      if (nr) problems.push(`${r.root}: ${nr} rows not reached (a probe error)`);
    }
    writeFileSync(resolve(OUT, `figures-${PHASE}.json`), JSON.stringify(roots, null, 1));
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile)) {
      compare(JSON.parse(readFileSync(baseFile, 'utf8')) as RootFig[], roots, problems, notes);
    }
    for (const n of notes) say(`note: ${n}`);
    say(`problems: ${JSON.stringify(problems, null, 1)}`);
    expect(problems).toEqual([]);
  } finally {
    if (forumId)
      await apiDelete(o, `/channels/${forumId}`).catch((e) =>
        say(`forum ${forumId} (p105probe-forum-${STAMP}) not deleted: ${String(e).slice(0, 160)}`),
      );
    await ownerCtx.close();
  }
});
