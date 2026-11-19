import { mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import { hitTestAll, openCollapsed, type ControlRow } from '../support/hit-test';

/**
 * Wave 109, lounge (D-1022 with the edge rule D-1025): the Lounge Home's section bar (SEC-AUD-530), the inline edit's
 * textarea (SEC-AUD-534) and the channel page's last fails of the 44 px hit test. Two phases (SR_P109_LOUNGE_PHASE
 * base|after), built mode, the seeded sweep world, Lounge Light.
 *
 * At 375 x 812 touch: (1) Lounge Home (/channels/@me): the section bar's box, its links' boxes, what lies under and beside
 * each link within 30 px, the tops of the rail and the sidebar, `hitTestAll` over the chrome outside `main`; (2) a channel
 * with the inline edit open (a room made through the API, removed in a `finally`): the textarea, the time above it, the
 * hint line below it, the message row, `hitTestAll` over the edit; (3) the world's channel and (4) the new seed's channel
 * (`world.json` extras: the 14-reaction and spoiler messages): `hitTestAll` over `main`, every fail with its selector path,
 * box and taker, sorted into piles by `pileOf` (a: inline in running text, accepted; b: standalone; c: probe's own).
 * At 1024 x 768 (Lounge Light): the Home bar, rail and sidebar boxes, a `reach: 'box'` hit test of the chrome, the channel
 * page's bar if it shows. Shots: home-375, inline-edit-375, channel-375, extras-channel-375, home-1024 (`base-` twins).
 *
 * In the after phase, when `figures-base.json` is in OUT, it lists problem lines: a row that passed in the base and does
 * not now; a width at 1024 that changed; the rail's or the sidebar's width that changed; the bar below 44 px on the phone;
 * the textarea below 44 px; a link of the bar below 44 px; scroll sideways. It never fails on them (it prints them).
 *
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p109-integ/apps/web/dist --env SR_P109_LOUNGE_ON=1 --env SR_P109_LOUNGE_PHASE=after --env SR_P109_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-lounge -- tests/p109-lounge-shots.spec.ts
 */
const ON = process.env.SR_P109_LOUNGE_ON === '1';
const PHASE = process.env.SR_P109_LOUNGE_PHASE === 'after' ? 'after' : 'base';
const OUT =
  process.env.SR_P109_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 200);

interface World {
  guildId: string;
  channelId: string;
  extras?: { extrasChannelId?: string; reactionsMessageId?: string; spoilerMessageId?: string };
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Cfg {
  w: number;
  h: number;
  touch: boolean;
}
const PHONE: Cfg = { w: 375, h: 812, touch: true };
const DESK: Cfg = { w: 1024, h: 768, touch: false };

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface RowFig {
  page: string;
  index: number;
  tag: string;
  name: string;
  path: string;
  status: string;
  box: Rect | null;
  takers: string[];
  pile: 'a' | 'b' | 'c' | null;
}
const figs: Record<string, unknown> = {};
const hitRows: RowFig[] = [];
const problems: string[] = [];
const notes: string[] = [];

const r1 = (n: number) => Math.round(n * 10) / 10;

/** The pile of a failing row: a = inline in running text (accepted, D-1022 A.4), c = the probe's own, b = standalone. */
function pileOf(r: RowFig): 'a' | 'b' | 'c' {
  if (/sr-md|name-btn|mention|spoiler|embed-title/.test(r.path) || /^@/.test(r.name)) return 'a';
  if (r.status === 'not-reached') return 'c';
  return 'b';
}

async function newPage(browser: Browser, owner: BrowserContext, cfg: Cfg) {
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: cfg.w, height: cfg.h },
    storageState: await owner.storageState(),
    bypassCSP: true,
    ...(cfg.touch ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
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

async function settle(page: Page, ms = 4000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}
async function go(page: Page, url: string, ready: Locator) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  await ready
    .first()
    .waitFor({ timeout: 30_000 })
    .catch(() => notes.push(`${url}: ready locator not seen`));
  await settle(page);
}

async function shot(page: Page, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  say(`${name}: scrollWidth/clientWidth ${sw}/${cw}`);
  if (sw > cw) problems.push(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  await page.mouse.move(1, 1).catch(() => undefined);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}.png`) });
}

/** Boxes of the first `max` matches of a selector (display none skipped). */
async function boxes(page: Page, sel: string, max = 6): Promise<Rect[]> {
  return page
    .locator(sel)
    .evaluateAll(
      (els, m) =>
        els
          .slice(0, m)
          .map((e) => {
            const r = e.getBoundingClientRect();
            return {
              x: Math.round(r.x * 10) / 10,
              y: Math.round(r.y * 10) / 10,
              w: Math.round(r.width * 10) / 10,
              h: Math.round(r.height * 10) / 10,
            };
          })
          .filter((b) => b.w > 0),
      max,
    )
    .catch(() => []);
}

async function hits(page: Page, pageName: string, root: Locator, reach?: 'box') {
  await openCollapsed(page, root);
  const rows: ControlRow[] = await hitTestAll(page, root, reach ? { reach } : {});
  const out: RowFig[] = rows.map((r) => {
    const f: RowFig = {
      page: pageName,
      index: r.index,
      tag: r.tag,
      name: r.name,
      path: r.path,
      status: r.status,
      box: r.result?.box
        ? {
            x: r1(r.result.box.x),
            y: r1(r.result.box.y),
            w: r1(r.result.box.width),
            h: r1(r.result.box.height),
          }
        : null,
      takers: (r.result?.points ?? [])
        .filter((p) => !p.ok)
        .map(
          (p) =>
            `${p.at}@${Math.round(p.y)}: ${p.taker?.tag ?? '?'}${p.taker?.role ? `[${p.taker.role}]` : ''} ${p.taker?.path ?? ''}`,
        ),
      pile: null,
    };
    if (f.status === 'fail' || f.status === 'not-reached') f.pile = pileOf(f);
    return f;
  });
  hitRows.push(...out);
  const count = (s: string) => out.filter((x) => x.status === s).length;
  say(
    `${pageName}: ${out.length} rows, pass ${count('pass')}, fail ${count('fail')}, not-reached ${count('not-reached')}, skipped ${count('skipped')}`,
  );
  for (const f of out.filter((x) => x.pile)) {
    say(
      `  FAIL[${f.pile}] ${pageName} #${f.index} ${f.tag} "${f.name}" box ${f.box ? `${f.box.w}x${f.box.h} at y${f.box.y}` : '-'} path ${f.path.slice(0, 110)}`,
    );
    for (const t of f.takers.slice(0, 2)) say(`      taker ${t.slice(0, 140)}`);
  }
  const piles = (p: string) => out.filter((x) => x.pile === p).length;
  say(`${pageName}: piles a=${piles('a')} b=${piles('b')} c=${piles('c')}`);
  return out;
}

async function homeFigures(page: Page, label: string) {
  const bar = await boxes(page, '.lng-sectionbar', 1);
  const links = await boxes(page, '.lng-sectionbar-link', 4);
  const rail = await boxes(page, '.lng-rail', 1);
  const sidebar = await boxes(page, '.lng-sidebar', 1);
  const leftCol = await boxes(page, '.lng-left-col', 1);
  const sideTop = await boxes(page, '.lng-sidebar-top', 1);
  const homeLink = await boxes(page, '.lng-rail-item a, .lng-rail-item button', 1);
  const find = await boxes(page, '.lng-find', 1);
  const tabbar = await boxes(page, 'nav.lng-tabbar', 1);
  const fig = { bar, links, rail, sidebar, leftCol, sideTop, homeLink, find, tabbar };
  figs[`home-${label}`] = fig;
  say(`home ${label}: ${JSON.stringify(fig)}`);
  // What lies under and beside each link, within 30 px.
  const around = await page.evaluate(() => {
    const out: string[] = [];
    document.querySelectorAll('.lng-sectionbar-link').forEach((a) => {
      const r = a.getBoundingClientRect();
      const cx = r.x + r.width / 2;
      const desc = (x: number, y: number) => {
        const e = document.elementFromPoint(
          Math.min(Math.max(x, 0), innerWidth - 1),
          Math.min(Math.max(y, 0), innerHeight - 1),
        );
        return e
          ? `${e.tagName.toLowerCase()}.${(typeof e.className === 'string' ? e.className : '').split(' ')[0]}`
          : 'null';
      };
      const under = [4, 14, 29].map((d) => `+${d}:${desc(cx, r.bottom + d)}`);
      const beside = [
        `L8:${desc(r.x - 8, r.y + r.height / 2)}`,
        `R8:${desc(r.right + 8, r.y + r.height / 2)}`,
      ];
      out.push(`${(a.textContent ?? '').trim()} ${under.join(' ')} ${beside.join(' ')}`);
    });
    return out;
  });
  for (const a of around) say(`home ${label} around: ${a}`);
  figs[`home-${label}-around`] = around;
}

async function runPart(browser: Browser, owner: BrowserContext, w: World) {
  // 1. Lounge Home at 375 touch.
  {
    const { ctx, page } = await newPage(browser, owner, PHONE);
    try {
      await go(page, '/channels/@me', page.getByRole('main'));
      await homeFigures(page, '375');
      await hits(
        page,
        'home-chrome-375',
        page.locator('header:not(main header), nav:not(main nav)'),
      );
      await shot(page, 'home-375');
      // The channel page's bar, same class.
      await go(page, `/channels/${w.guildId}/${w.channelId}`, page.getByRole('log'));
      figs['channel-bar-375'] = await boxes(page, '.lng-sectionbar', 1);
      say(`channel bar 375: ${JSON.stringify(figs['channel-bar-375'])}`);
      await hits(page, 'channel-375', page.getByRole('main'));
      await shot(page, 'channel-375');
      // The new seed's channel.
      const ex = w.extras?.extrasChannelId;
      if (ex) {
        await go(page, `/channels/${w.guildId}/${ex}`, page.getByRole('log'));
        await hits(page, 'extras-channel-375', page.getByRole('main'));
        await shot(page, 'extras-channel-375');
      } else notes.push('world.json has no extras channel');
    } catch (e) {
      notes.push(`phone pages: ${msg(e)}`);
    } finally {
      await ctx.close();
    }
  }
  // 2. The inline edit (a room of our own).
  let roomId: string | null = null;
  try {
    const room = await apiPost<{ id: string }>(owner.request, `/guilds/${w.guildId}/channels`, {
      name: `p109lounge-room-${STAMP}`,
      type: 0,
    });
    roomId = room.id;
    await sleep(800);
    await apiPost(owner.request, `/channels/${room.id}/messages`, {
      content: 'p109 lounge message to edit',
    });
    const { ctx, page } = await newPage(browser, owner, PHONE);
    try {
      await go(page, `/channels/${w.guildId}/${room.id}`, page.getByRole('log'));
      const ta = page.locator('.lng-composer .lng-textarea').first();
      await ta.click({ timeout: 6000 }).catch(() => undefined);
      await page.keyboard.press('ArrowUp');
      await page.waitForTimeout(800);
      if ((await page.locator('.lng-inline-textarea').count()) === 0) {
        notes.push('the inline edit did not open');
      } else {
        const edit = {
          textarea: await boxes(page, '.lng-inline-textarea', 1),
          hint: await boxes(page, '.lng-inline-hint', 1),
          edit: await boxes(page, '.lng-inline-edit', 1),
          row: await boxes(page, 'article.lng-msg:has(.lng-inline-edit)', 1),
          timeAbove: await boxes(page, 'article.lng-msg:has(.lng-inline-edit) time', 2),
        };
        figs['inline-edit-375'] = edit;
        say(`inline edit 375: ${JSON.stringify(edit)}`);
        await hits(page, 'inline-edit-375', page.locator('.lng-inline-edit'));
        await shot(page, 'inline-edit-375');
      }
    } finally {
      await ctx.close();
    }
  } catch (e) {
    notes.push(`inline edit: ${msg(e)}`);
  } finally {
    if (roomId)
      await apiDelete(owner.request, `/channels/${roomId}`).catch((e) =>
        notes.push(`room ${roomId} (p109lounge-room-${STAMP}) not deleted: ${msg(e)}`),
      );
  }
  // 3. 1024: Home bar, rail and sidebar; a box-reach test of the chrome; the channel's bar.
  {
    const { ctx, page } = await newPage(browser, owner, DESK);
    try {
      await go(page, '/channels/@me', page.getByRole('main'));
      await homeFigures(page, '1024');
      await hits(
        page,
        'home-chrome-1024',
        page.locator('header:not(main header), nav:not(main nav)'),
        'box',
      );
      await shot(page, 'home-1024');
      await go(page, `/channels/${w.guildId}/${w.channelId}`, page.getByRole('log'));
      figs['channel-bar-1024'] = await boxes(page, '.lng-sectionbar', 1);
      figs['channel-layout-1024'] = {
        rail: await boxes(page, '.lng-rail', 1),
        sidebar: await boxes(page, '.lng-sidebar', 1),
        main: await boxes(page, '#lounge-main', 1),
      };
      say(
        `channel 1024: ${JSON.stringify(figs['channel-bar-1024'])} ${JSON.stringify(figs['channel-layout-1024'])}`,
      );
    } catch (e) {
      notes.push(`1024 pages: ${msg(e)}`);
    } finally {
      await ctx.close();
    }
  }
}

function compare(base: { figs: Record<string, unknown>; rows: RowFig[] }) {
  const bf = JSON.stringify;
  for (const k of ['home-1024', 'channel-bar-1024', 'channel-layout-1024']) {
    const b = base.figs[k] as Record<string, unknown> | undefined;
    const a = figs[k] as Record<string, unknown> | undefined;
    if (bf(b) !== bf(a)) problems.push(`${k} changed: ${bf(b)} -> ${bf(a)}`);
  }
  // 375: rail and sidebar widths, tops.
  const hb = base.figs['home-375'] as Record<string, Rect[]>;
  const ha = figs['home-375'] as Record<string, Rect[]>;
  for (const k of ['rail', 'sidebar', 'leftCol'])
    if (hb?.[k]?.[0] && ha?.[k]?.[0] && hb[k][0].w !== ha[k][0].w)
      problems.push(`home 375 ${k} width ${hb[k][0].w} -> ${ha[k][0].w}`);
  if (ha?.rail?.[0] && ha?.sidebar?.[0] && ha.rail[0].y !== ha.sidebar[0].y)
    notes.push(
      `home 375: rail top ${ha.rail[0].y} vs sidebar top ${ha.sidebar[0].y} (base ${hb?.rail?.[0]?.y} / ${hb?.sidebar?.[0]?.y})`,
    );
  if (ha?.bar?.[0] && ha.bar[0].h < 44)
    problems.push(`home 375: the bar is ${ha.bar[0].h} px tall`);
  for (const l of ha?.links ?? [])
    if (l.h < 44) problems.push(`home 375: a bar link is ${l.h} px tall`);
  const ie = figs['inline-edit-375'] as { textarea?: Rect[] } | undefined;
  if (ie?.textarea?.[0] && ie.textarea[0].h < 44)
    problems.push(`inline edit textarea ${ie.textarea[0].h} px tall`);
  // A row that passed in the base must pass now (same page, same index, same path).
  for (const a of hitRows) {
    const b = base.rows.find((x) => x.page === a.page && x.index === a.index && x.path === a.path);
    if (b && b.status === 'pass' && a.status !== 'pass')
      problems.push(`${a.page} #${a.index} ${a.tag} "${a.name}": pass -> ${a.status}`);
  }
  for (const pg of ['home-chrome-375', 'channel-375', 'extras-channel-375', 'inline-edit-375']) {
    const nb = base.rows.filter((x) => x.page === pg).length;
    const na = hitRows.filter((x) => x.page === pg).length;
    if (nb !== na) problems.push(`${pg}: row count ${nb} -> ${na}`);
    const fb = base.rows.filter((x) => x.page === pg && x.pile).length;
    const fa = hitRows.filter((x) => x.page === pg && x.pile).length;
    say(`fails ${pg}: base ${fb} -> after ${fa}`);
  }
}

test('lounge: section bar, inline edit and channel page at 375', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    await runPart(browser, ownerCtx, world());
    writeFileSync(
      resolve(OUT, `figures-${PHASE}.json`),
      JSON.stringify({ figs, rows: hitRows }, null, 1),
    );
    const baseFile = resolve(OUT, 'figures-base.json');
    if (PHASE === 'after' && existsSync(baseFile))
      compare(
        JSON.parse(readFileSync(baseFile, 'utf8')) as {
          figs: Record<string, unknown>;
          rows: RowFig[];
        },
      );
    for (const n of notes) say(`note: ${n}`);
    say(`problems: ${JSON.stringify(problems, null, 1)}`);
  } finally {
    await ownerCtx.close();
  }
});
