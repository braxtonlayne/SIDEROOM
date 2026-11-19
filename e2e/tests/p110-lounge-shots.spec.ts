import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, test } from '../support/fixtures';
import {
  MENU_ITEM_SELECTOR,
  hitTestAll,
  openCollapsed,
  type ControlRow,
} from '../support/hit-test';

/**
 * Wave 110, loungeshots (SHOTS BUILDER of pod lounge; SEC-AUD-542, 545 to 549, 552; D-1035, D-1022): measures the work
 * of msgs (embed title, forward rows, file card, image viewer's bar) and lounge (the message toolbar on touch, the
 * Friends rows' icon buttons, the forward dialog's two fields) at 375 x 812 with touch and 1024 x 768 with a mouse,
 * Lounge Light, on the seeded sweep world (`extras`). Nothing is seeded: attachments are injected with `page.route`
 * as the census did. Two phases (SR_P110_LOUNGE_PHASE=base|after): both write `figures-<phase>.json` and their shots
 * (base shots with the `base-` prefix) into SR_P110_LOUNGE_OUT; the after phase compares with `figures-base.json`
 * when it is there and lists every broken limit as a problem line (D-984). SR_P110_LOUNGE_PART=1 runs the 375 pass
 * only, 2 the 1024 pass only (figures-<phase>-part<n>.json; the comparison then reads the same part's base file);
 * unset runs both.
 *
 * The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p110-integ/apps/web/dist --env SR_P110_LOUNGE_ON=1 --env SR_P110_LOUNGE_PHASE=after --env SR_P110_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-lounge -- tests/p110-lounge-shots.spec.ts
 */
const ON = process.env.SR_P110_LOUNGE_ON === '1';
const PHASE = process.env.SR_P110_LOUNGE_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P110_LOUNGE_PART ?? '';
const OUT =
  process.env.SR_P110_LOUNGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p110-lounge';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SUFFIX = PART ? `-part${PART}` : '';

test.skip(!ON, 'local visual check only');
test.setTimeout(480_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);
const r1 = (n: number) => Math.round(n * 10) / 10;

interface World {
  guildId: string;
  channelId: string;
  extras?: { friendName: string; groupDmId: string; extrasChannelId: string };
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Cfg {
  tag: '375' | '1024';
  w: number;
  h: number;
  touch: boolean;
}
const C375: Cfg = { tag: '375', w: 375, h: 812, touch: true };
const C1024: Cfg = { tag: '1024', w: 1024, h: 768, touch: false };

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}
interface HitFig {
  status: string;
  name: string;
  tag: string;
  box: Box | null;
  taker: string | null;
}
interface Figures {
  phase: string;
  /** Named boxes: `<cfg>|<group>.<what>`. */
  boxes: Record<string, Box | null>;
  /** Plain readings (strings, numbers, booleans). */
  vals: Record<string, string | number | boolean | null>;
  /** Hit tests: `<cfg>|<scenario>` -> rows. */
  hits: Record<string, HitFig[]>;
  notes: string[];
  problems: string[];
  covered: string[];
}
const fig: Figures = {
  phase: PHASE,
  boxes: {},
  vals: {},
  hits: {},
  notes: [],
  problems: [],
  covered: [],
};
const note = (s: string) => {
  fig.notes.push(s);
  say(`note: ${s}`);
};

async function newPage(browser: Browser, owner: BrowserContext, cfg: Cfg, decorateFiles = false) {
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
  if (decorateFiles) await decorate(page);
  return { ctx, page };
}

/** As the sweep and the census do: the `[files]` message gets an image and a file; their bytes are served here. */
const PNG = (() => {
  const dir = resolve(repoRoot, 'apps', 'web', 'public', 'icons');
  const f = existsSync(dir) ? readdirSync(dir).find((n) => n.endsWith('.png')) : undefined;
  return f ? readFileSync(resolve(dir, f)) : Buffer.alloc(0);
})();
async function decorate(page: Page) {
  await page.route(/\/api\/v1\/channels\/\d+\/messages(\?.*)?$/, async (route: Route) => {
    if (route.request().method() !== 'GET') return route.continue();
    const res = await route.fetch();
    let body: unknown;
    try {
      body = await res.json();
    } catch {
      return route.fulfill({ response: res });
    }
    if (Array.isArray(body)) {
      for (const m of body as { id: string; content?: string; attachments?: unknown[] }[]) {
        if (m.content?.includes('[files]')) {
          m.attachments = [
            {
              id: `${m.id}1`,
              filename: 'lounge-photo.png',
              size: 48211,
              url: '/cdn/attachments/sweep/lounge-photo.png',
              content_type: 'image/png',
              width: 512,
              height: 512,
            },
            {
              id: `${m.id}2`,
              filename: 'meeting-notes-for-the-autumn-planning-session-final-v2.pdf',
              size: 1_204_331,
              url: '/cdn/attachments/sweep/notes.pdf',
              content_type: 'application/pdf',
            },
          ];
        }
      }
    }
    return route.fulfill({ response: res, json: body });
  });
  await page.route(/\/cdn\/attachments\/sweep\//, (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: PNG }),
  );
}

async function settle(page: Page, ms = 4000) {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
  await page.waitForTimeout(700);
}

async function go(page: Page, url: string, ready?: Locator) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  if (ready)
    await ready
      .first()
      .waitFor({ timeout: 30_000 })
      .catch(() => note(`${url}: ready locator not seen`));
  await settle(page);
}

async function shot(page: Page, cfg: Cfg, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  if (sw > cw) fig.problems.push(`${cfg.tag} ${name}: the page scrolls sideways (${sw} > ${cw})`);
  say(`${cfg.tag} ${name}: scrollWidth/clientWidth ${sw}/${cw}`);
  await page.screenshot({
    path: resolve(OUT, `${PHASE === 'base' ? 'base-' : ''}${name}-${cfg.tag}.png`),
  });
}

// ---- readings --------------------------------------------------------------------------------------------------------

async function boxOf(loc: Locator): Promise<Box | null> {
  if ((await loc.count()) === 0) return null;
  const b = await loc
    .first()
    .boundingBox()
    .catch(() => null);
  return b ? { x: r1(b.x), y: r1(b.y), w: r1(b.width), h: r1(b.height) } : null;
}
async function rec(cfg: Cfg, key: string, loc: Locator) {
  fig.boxes[`${cfg.tag}|${key}`] = await boxOf(loc);
}
const val = (cfg: Cfg, key: string, v: string | number | boolean | null) => {
  fig.vals[`${cfg.tag}|${key}`] = v;
};

/** Every `.lng-msg-tools` on the page that a member could see (computed style and a box), and how many are in `scope`. */
async function toolbarsShown(
  page: Page,
  scope?: Locator,
): Promise<{ page: number; scope: number }> {
  const all = await page.evaluate(() =>
    Array.from(document.querySelectorAll('.lng-msg-tools')).map((el) => {
      const c = getComputedStyle(el);
      const r = el.getBoundingClientRect();
      return {
        shown:
          c.display !== 'none' &&
          c.visibility !== 'hidden' &&
          parseFloat(c.opacity) > 0 &&
          r.width > 0 &&
          r.height > 0,
        inScope: false,
      };
    }),
  );
  let inScope = 0;
  if (scope) {
    inScope = await scope.evaluate(
      (art) =>
        Array.from(art.querySelectorAll('.lng-msg-tools')).filter((el) => {
          const c = getComputedStyle(el);
          const r = el.getBoundingClientRect();
          return c.display !== 'none' && c.visibility !== 'hidden' && r.width > 0 && r.height > 0;
        }).length,
    );
  }
  return { page: all.filter((x) => x.shown).length, scope: inScope };
}

/** Hit test every control under `root`; rows are stored compactly under `<cfg>|<scenario>`. */
async function hits(
  page: Page,
  cfg: Cfg,
  scenario: string,
  root: Locator,
  opts: { also?: string; keep?: (r: ControlRow) => boolean } = {},
) {
  if ((await root.count()) === 0) {
    note(`${cfg.tag} ${scenario}: hit-test root not found`);
    return;
  }
  try {
    await openCollapsed(page, root.first());
    const rows = await hitTestAll(page, root.first(), {
      ...(opts.also ? { also: opts.also } : {}),
      ...(cfg.touch ? {} : { reach: 'box' as const }),
    });
    const keep = opts.keep ?? (() => true);
    const out: HitFig[] = rows.filter(keep).map((r) => {
      const failing = (r.result?.points ?? []).find((p) => !p.ok);
      return {
        status: r.status,
        name: r.name,
        tag: r.tag,
        box: r.result?.box
          ? {
              x: r1(r.result.box.x),
              y: r1(r.result.box.y),
              w: r1(r.result.box.width),
              h: r1(r.result.box.height),
            }
          : null,
        taker: failing?.taker
          ? `${failing.taker.tag} "${failing.taker.name}" ${failing.taker.path}`
          : null,
      };
    });
    fig.hits[`${cfg.tag}|${scenario}`] = out;
    const n = (s: string) => out.filter((r) => r.status === s).length;
    say(
      `${cfg.tag} ${scenario}: ${out.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}, skipped ${n('skipped')}`,
    );
    for (const r of out.filter((x) => x.status === 'fail' || x.status === 'not-reached'))
      say(
        `  ${r.status.toUpperCase()} ${r.tag} "${r.name}" ${r.box ? `${r.box.w}x${r.box.h}` : '-'} taker ${r.taker ?? '-'}`,
      );
  } catch (e) {
    note(`${cfg.tag} ${scenario}: hit test failed (${msg(e)})`);
  }
}

const tap = async (loc: Locator) => {
  await loc
    .first()
    .scrollIntoViewIfNeeded({ timeout: 5000 })
    .catch(() => undefined);
  await loc.first().click({ timeout: 6000 });
};

/** A touch long press (the kit's ContextMenu opens after 500 ms of a resting touch, Q60). */
async function longPress(page: Page, loc: Locator) {
  await loc.scrollIntoViewIfNeeded({ timeout: 5000 });
  await loc.evaluate((el) => {
    const r = el.getBoundingClientRect();
    const x = r.left + Math.min(r.width / 2, 120);
    const y = r.top + Math.min(r.height / 2, 40);
    const t = new Touch({ identifier: 1, target: el, clientX: x, clientY: y });
    el.dispatchEvent(
      new TouchEvent('touchstart', {
        bubbles: true,
        cancelable: true,
        touches: [t],
        targetTouches: [t],
        changedTouches: [t],
      }),
    );
  });
  await page.waitForTimeout(800);
}

/** A message's action menu: a long press with touch, a right click without. */
async function openMsgMenu(page: Page, cfg: Cfg, art: Locator) {
  if (cfg.touch) await longPress(page, art);
  else {
    await art.scrollIntoViewIfNeeded({ timeout: 5000 });
    await art.click({ button: 'right', position: { x: 60, y: 10 }, timeout: 5000 });
    await page.waitForTimeout(500);
  }
  await page.getByRole('menu').last().waitFor({ timeout: 4000 });
}

const dialog = (page: Page) => page.getByRole('dialog').last();
const articles = (page: Page) => page.locator('article.lng-msg');

// ---- scenario 1: the message toolbar (lounge, D-1035) -------------------------------------------------------------------

async function scenToolbar(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const ex = w.extras;
  if (!ex) return note('the world has no `extras`');
  const { ctx, page } = await newPage(browser, owner, cfg);
  try {
    const url = `/channels/${w.guildId}/${ex.extrasChannelId}`;
    const reacted = () => articles(page).filter({ has: page.locator('.lng-reactions') });
    const spoiled = () =>
      articles(page).filter({
        has: page.getByRole('button', { name: 'Spoiler, click to reveal' }),
      });

    if (cfg.touch) {
      // 1a. A tap on a message's text (the message with the 14 reactions).
      await go(page, url, page.getByRole('log'));
      const art = reacted().first();
      await art.scrollIntoViewIfNeeded();
      const body = art.locator('.lng-msg-content').first();
      const b = await body.boundingBox();
      if (!b) throw new Error('no message body box');
      await page.touchscreen.tap(b.x + 8, b.y + Math.min(b.height / 2, 10));
      await page.waitForTimeout(600);
      const s = await toolbarsShown(page, art);
      val(cfg, 'tap.toolbarsShownOnPage', s.page);
      val(cfg, 'tap.toolbarShownInMessage', s.scope > 0);
      await rec(cfg, 'tb.afterTap', art.locator('.lng-msg-tools'));
      say(`375 tap on text: toolbars shown on page ${s.page}, in the message ${s.scope}`);
      if (PHASE === 'after' && s.page > 0)
        fig.problems.push(
          `375 a tap on a message's text still shows the toolbar (${s.page} shown)`,
        );
      await shot(page, cfg, 'tap-message');
      fig.covered.push('375 tap on a message');

      // 1b. A tap on the seeded spoiler (first line of its message), after a tap that, in the base, brought the toolbar.
      await go(page, url, page.getByRole('log'));
      await spoiled()
        .first()
        .evaluate((e) => e.setAttribute('data-p110s', '1'));
      const sart = page.locator('[data-p110s="1"]');
      await sart.scrollIntoViewIfNeeded();
      const stime = sart.locator('time').first();
      const sb0 = (await stime.count()) > 0 ? await stime.boundingBox() : await sart.boundingBox();
      if (sb0) await page.touchscreen.tap(sb0.x + sb0.width / 2, sb0.y + sb0.height / 2);
      await page.waitForTimeout(500);
      const sbtn = sart.getByRole('button', { name: 'Spoiler, click to reveal' }).first();
      const sbox = await sbtn.boundingBox();
      if (!sbox) throw new Error('no spoiler box');
      const cx = sbox.x + sbox.width / 2;
      const cy = sbox.y + sbox.height / 2;
      const topmost = await page.evaluate(
        ([x, y]) => {
          const e = document.elementFromPoint(x!, y!);
          return e
            ? `${e.tagName.toLowerCase()}.${String((e as HTMLElement).className).split(/\s+/)[0] ?? ''}${e.closest('.lng-msg-tools') ? ' (inside the toolbar)' : ''}`
            : null;
        },
        [cx, cy],
      );
      val(cfg, 'spoiler.topmostAtCentre', topmost);
      await page.touchscreen.tap(cx, cy);
      await page.waitForTimeout(600);
      const stillHidden = await sart
        .getByRole('button', { name: 'Spoiler, click to reveal' })
        .count();
      const revealed = stillHidden === 0;
      val(cfg, 'spoiler.revealedByTap', revealed);
      await rec(cfg, 'spoiler.button', sbtn);
      say(`375 spoiler: topmost at its centre ${topmost}; revealed by a tap ${revealed}`);
      if (PHASE === 'after' && !revealed)
        fig.problems.push('375 a tap on the spoiler did not reveal it');
      await shot(page, cfg, 'spoiler-tap');
      fig.covered.push('375 tap on the spoiler');

      // 1c. The long-press menu: every item, hit test.
      await go(page, url, page.getByRole('log'));
      try {
        await openMsgMenu(page, cfg, articles(page).last());
        const menu = page.getByRole('menu').last();
        val(cfg, 'longpress.menuItems', await menu.locator(MENU_ITEM_SELECTOR).count());
        await hits(page, cfg, 'longpress-menu', menu, { also: MENU_ITEM_SELECTOR });
        await page.keyboard.press('Escape');
      } catch (e) {
        note(`375 long-press menu not opened (${msg(e)})`);
      }

      // 1d. Keyboard focus at 375: the toolbar shows.
      await go(page, url, page.getByRole('log'));
      const kart = reacted().first();
      await kart.scrollIntoViewIfNeeded();
      await page.keyboard.press('Shift');
      await kart.locator('button, a[href], [tabindex="0"]').first().focus();
      await page.waitForTimeout(500);
      const ks = await toolbarsShown(page, kart);
      val(cfg, 'keyboard.toolbarShownInMessage', ks.scope > 0);
      say(`375 keyboard focus: toolbar shown in the message ${ks.scope > 0}`);
      if (PHASE === 'after' && ks.scope === 0)
        fig.problems.push('375 keyboard focus on a message does not show the toolbar');
      fig.covered.push('375 keyboard focus');
    } else {
      // 1e. A mouse at 1024: hover shows the toolbar; "Reply" opens the composer's reply bar.
      await go(page, url, page.getByRole('log'));
      const art = reacted().first();
      await art.scrollIntoViewIfNeeded();
      const b = await art.locator('.lng-msg-content').first().boundingBox();
      if (!b) throw new Error('no message body box');
      await page.mouse.move(b.x + 20, b.y + Math.min(b.height / 2, 10));
      await page.waitForTimeout(500);
      const s = await toolbarsShown(page, art);
      val(cfg, 'hover.toolbarShownInMessage', s.scope > 0);
      await rec(cfg, 'tb.onHover', art.locator('.lng-msg-tools'));
      const tools = art.locator('.lng-msg-tools');
      val(cfg, 'hover.toolbarButtons', await tools.getByRole('button').count());
      say(`1024 hover: toolbar shown ${s.scope > 0}`);
      if (PHASE === 'after' && s.scope === 0)
        fig.problems.push('1024 hover on a message does not show the toolbar');
      await shot(page, cfg, 'hover-message');
      const reply = tools.getByRole('button', { name: 'Reply' }).first();
      if ((await reply.count()) > 0) {
        await reply.click({ timeout: 5000 });
        await page.waitForTimeout(400);
        const bar = page.locator('.lng-composer-wrap .lng-bar[role="status"]').first();
        const shown = (await bar.count()) > 0 && (await bar.isVisible());
        val(cfg, 'hover.replyBarShown', shown);
        say(`1024 Reply: reply bar shown ${shown}`);
        if (PHASE === 'after' && !shown) fig.problems.push('1024 Reply did not show the reply bar');
      } else note('1024 the toolbar has no Reply button');
      fig.covered.push('1024 hover toolbar');
    }
  } catch (e) {
    note(`${cfg.tag} toolbar scenario stopped (${msg(e)})`);
  } finally {
    await ctx.close();
  }
}

// ---- scenario 2: Friends (lounge) ---------------------------------------------------------------------------------------

async function openFriends(page: Page, tab: string) {
  await go(page, '/channels/@me', page.locator('.lng-left'));
  await tap(
    page.getByRole('button', { name: /^Friends/ }).or(page.getByRole('link', { name: /^Friends/ })),
  );
  await page.locator('section.lng-friends').first().waitFor({ timeout: 10_000 });
  await settle(page, 2500);
  await tap(
    page
      .getByRole('tab', { name: new RegExp(`^${tab}`) })
      .or(page.getByRole('button', { name: new RegExp(`^${tab}`) })),
  );
  await settle(page, 2000);
}

async function scenFriends(browser: Browser, owner: BrowserContext, cfg: Cfg) {
  const { ctx, page } = await newPage(browser, owner, cfg);
  try {
    for (const tab of ['All', 'Pending']) {
      try {
        await openFriends(page, tab);
        const sec = page.locator('section.lng-friends');
        const rows = page.locator('.lng-friend-row');
        const n = await rows.count();
        const hs: number[] = [];
        for (let i = 0; i < n; i++) hs.push((await boxOf(rows.nth(i)))?.h ?? 0);
        val(cfg, `friends.${tab}.rowHeights`, hs.join(','));
        await rec(cfg, `fr.${tab}.firstRow`, rows);
        await hits(page, cfg, `friends-${tab.toLowerCase()}`, sec, {
          keep: (r) => r.path.includes('lng-friend-row') || r.tag === 'button',
        });
        await shot(page, cfg, `friends-${tab.toLowerCase()}`);
        fig.covered.push(`${cfg.tag} friends ${tab}`);
      } catch (e) {
        note(`${cfg.tag} friends ${tab}: failed (${msg(e)})`);
      }
    }
  } finally {
    await ctx.close();
  }
}

// ---- scenario 3: the forward dialog; the emoji picker's own search ----------------------------------------------------

async function scenForward(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const ex = w.extras;
  if (!ex) return note('the world has no `extras`');
  const { ctx, page } = await newPage(browser, owner, cfg);
  try {
    const url = `/channels/${w.guildId}/${ex.extrasChannelId}`;
    await go(page, url, page.getByRole('log'));
    const target = articles(page).last();
    await openMsgMenu(page, cfg, target);
    await tap(page.getByRole('menuitem', { name: /Forward/ }));
    const dlg = dialog(page);
    await dlg.waitFor({ timeout: 6000 });
    await settle(page, 2000);
    await rec(cfg, 'fw.dialog', dlg);
    const read = await dlg.evaluate((d) => {
      const fields = Array.from(d.querySelectorAll('input.lng-emoji-search')) as HTMLInputElement[];
      const out = fields.map((f) => {
        const cs = getComputedStyle(f);
        const host = f.parentElement as HTMLElement;
        const hs = getComputedStyle(host);
        const inner = host.clientWidth - parseFloat(hs.paddingLeft) - parseFloat(hs.paddingRight);
        const probe = document.createElement('canvas').getContext('2d')!;
        probe.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
        const phWidth = probe.measureText(f.placeholder).width;
        const avail = f.clientWidth - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
        return {
          name: f.getAttribute('aria-label') ?? f.placeholder,
          width: Math.round(f.getBoundingClientRect().width * 10) / 10,
          height: Math.round(f.getBoundingClientRect().height * 10) / 10,
          innerWidth: Math.round(inner * 10) / 10,
          scrollWidth: f.scrollWidth,
          clientWidth: f.clientWidth,
          phWidth: Math.round(phWidth * 10) / 10,
          phFits: phWidth <= avail + 0.5,
          cut: f.scrollWidth > f.clientWidth,
        };
      });
      const list = d.querySelector('.lng-forward-list') as HTMLElement | null;
      const rows = Array.from(d.querySelectorAll('.lng-forward-row')).map(
        (r) => Math.round(r.getBoundingClientRect().height * 10) / 10,
      );
      return {
        out,
        rows,
        list: list
          ? {
              clientHeight: list.clientHeight,
              scrollHeight: list.scrollHeight,
              maxHeight: getComputedStyle(list).maxHeight,
              scrolls: list.scrollHeight > list.clientHeight + 1,
            }
          : null,
      };
    });
    read.out.forEach((f, i) => {
      val(cfg, `fw.field${i}.width`, f.width);
      val(cfg, `fw.field${i}.innerWidth`, f.innerWidth);
      val(cfg, `fw.field${i}.fillsLine`, Math.abs(f.width - f.innerWidth) <= 1);
      val(cfg, `fw.field${i}.placeholderWhole`, f.phFits && !f.cut);
      val(cfg, `fw.field${i}.scroll/client`, `${f.scrollWidth}/${f.clientWidth}`);
      val(cfg, `fw.field${i}.height`, f.height);
      say(
        `${cfg.tag} forward field ${i} "${f.name}": width ${f.width} of ${f.innerWidth}, placeholder ${f.phWidth}px, scroll/client ${f.scrollWidth}/${f.clientWidth}`,
      );
    });
    val(cfg, 'fw.rowCount', read.rows.length);
    val(cfg, 'fw.rowHeights', read.rows.join(','));
    if (read.list) {
      val(cfg, 'fw.list.maxHeight', read.list.maxHeight);
      val(cfg, 'fw.list.clientHeight', read.list.clientHeight);
      val(cfg, 'fw.list.scrolls', read.list.scrolls);
    }
    // The list still scrolls: set scrollTop and read it back.
    const scrolled = await dlg.locator('.lng-forward-list').evaluate((l) => {
      l.scrollTop = 40;
      return (l as HTMLElement).scrollTop;
    });
    val(cfg, 'fw.list.scrollTopAfterSet40', scrolled);
    await dlg.locator('.lng-forward-list').evaluate((l) => {
      l.scrollTop = 0;
    });
    await rec(cfg, 'fw.list', dlg.locator('.lng-forward-list'));
    await rec(cfg, 'fw.row0', dlg.locator('.lng-forward-row'));
    await rec(cfg, 'fw.field0', dlg.locator('input.lng-emoji-search').nth(0));
    await rec(cfg, 'fw.field1', dlg.locator('input.lng-emoji-search').nth(1));
    await hits(page, cfg, 'forward', dlg);
    await shot(page, cfg, 'forward');
    fig.covered.push(`${cfg.tag} forward`);
    await page.keyboard.press('Escape');
    await page.waitForTimeout(400);

    // The emoji picker's own search must not change.
    await go(page, url, page.getByRole('log'));
    const rart = articles(page).filter({ has: page.locator('.lng-reactions') });
    await tap(rart.first().locator('.lng-reaction-add'));
    const pop = page.getByRole('dialog').last().or(page.getByRole('menu').last());
    await pop.first().waitFor({ timeout: 6000 });
    await settle(page, 1500);
    const search = page.locator('.lng-emoji-search').first();
    await rec(cfg, 'ep.search', search);
    fig.covered.push(`${cfg.tag} emoji picker search`);
  } catch (e) {
    note(`${cfg.tag} forward scenario stopped (${msg(e)})`);
  } finally {
    await ctx.close();
  }
}

// ---- scenario 4: the embed; scenario 5: the file card; scenario 6: the image viewer -----------------------------------

async function scenEmbed(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const { ctx, page } = await newPage(browser, owner, cfg);
  try {
    await go(page, `/channels/${w.guildId}/${w.channelId}`, page.getByRole('log'));
    const card = page.locator('.lng-embed').first();
    await card.waitFor({ timeout: 10_000 });
    await card.scrollIntoViewIfNeeded();
    await settle(page, 2000);
    const link = page.locator('.lng-embed-title a').first();
    await rec(cfg, 'em.title', link);
    const info = await link.evaluate((a) => {
      const c = getComputedStyle(a);
      const lh = parseFloat(c.lineHeight) || parseFloat(c.fontSize) * 1.3;
      return {
        display: c.display,
        lines: Math.round(a.getBoundingClientRect().height / lh),
        lineHeight: c.lineHeight,
        minBlock: c.minBlockSize,
      };
    });
    val(cfg, 'em.title.display', info.display);
    val(cfg, 'em.title.lines', info.lines);
    val(cfg, 'em.title.minBlockSize', info.minBlock);
    say(
      `${cfg.tag} embed title link: display ${info.display}, ~${info.lines} lines, min-block-size ${info.minBlock}`,
    );
    await rec(cfg, 'em.card', card);
    await rec(cfg, 'em.main', card.locator('.lng-embed-main'));
    await rec(cfg, 'em.provider', card.locator('.lng-embed-provider'));
    await rec(cfg, 'em.titleBox', card.locator('.lng-embed-title'));
    await rec(cfg, 'em.desc', card.locator('.lng-embed-desc'));
    await rec(cfg, 'em.image', card.locator('.lng-embed-image'));
    await rec(cfg, 'em.thumb', card.locator('.lng-embed-thumb'));
    await rec(cfg, 'em.footer', card.locator('.lng-embed-footer'));
    if (cfg.touch) await hits(page, cfg, 'embed', card);
    else await hits(page, cfg, 'embed', card);
    await shot(page, cfg, 'embed');
    fig.covered.push(`${cfg.tag} embed`);
  } catch (e) {
    note(`${cfg.tag} embed scenario stopped (${msg(e)})`);
  } finally {
    await ctx.close();
  }
}

async function scenAttach(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const { ctx, page } = await newPage(browser, owner, cfg, true);
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    const art = articles(page)
      .filter({ has: page.locator('.lng-attachments, .lng-attach-file, .lng-tile') })
      .first();
    await art.waitFor({ timeout: 10_000 });
    await art.scrollIntoViewIfNeeded();
    await settle(page, 2500);
    // 5. The file card.
    const fcard = art.locator('.lng-attach-file:not(.lng-expired):not(.lng-media-card)').first();
    await rec(cfg, 'fc.card', fcard);
    await rec(cfg, 'fc.name', fcard.locator('.lng-file-name'));
    await rec(cfg, 'fc.dl', fcard.locator('.lng-icon-btn'));
    await rec(cfg, 'fc.meta', fcard.locator('.lng-file-meta'));
    await rec(cfg, 'fc.size', fcard.locator('.lng-file-meta .lng-muted'));
    await hits(page, cfg, 'filecard', fcard);
    await fcard.scrollIntoViewIfNeeded();
    await shot(page, cfg, 'filecard');
    fig.covered.push(`${cfg.tag} file card`);

    // 6. The image viewer.
    await tap(art.locator('.lng-tile-btn'));
    const dlg = dialog(page);
    await dlg.waitFor({ timeout: 6000 });
    await settle(page, 2000);
    const bar = dlg.locator('.lng-lightbox-bar');
    await rec(cfg, 'vw.dialog', dlg);
    await rec(cfg, 'vw.bar', bar);
    await rec(cfg, 'vw.stage', dlg.locator('.lng-lightbox-stage'));
    await rec(cfg, 'vw.image', dlg.locator('.lng-lightbox-stage img'));
    const items = await bar.evaluate((b) => {
      const out: { name: string; box: Box; lines: number }[] = [];
      for (const el of Array.from(b.querySelectorAll('a, button, span[aria-live]'))) {
        const r = el.getBoundingClientRect();
        const range = document.createRange();
        range.selectNodeContents(el);
        const tops = new Set(Array.from(range.getClientRects()).map((q) => Math.round(q.top)));
        out.push({
          name: (el.getAttribute('aria-label') ?? el.textContent ?? '').trim().slice(0, 30),
          box: {
            x: Math.round(r.x * 10) / 10,
            y: Math.round(r.y * 10) / 10,
            w: Math.round(r.width * 10) / 10,
            h: Math.round(r.height * 10) / 10,
          },
          lines: el.textContent?.trim() ? tops.size : 0,
        });
      }
      return out;
    });
    items.forEach((it) => {
      fig.boxes[`${cfg.tag}|vw.bar.${it.name}`] = it.box;
      val(cfg, `vw.bar.${it.name}.lines`, it.lines);
      say(
        `${cfg.tag} viewer bar "${it.name}": ${it.box.w}x${it.box.h} at ${it.box.x},${it.box.y}, ${it.lines} line(s)`,
      );
    });
    val(cfg, 'vw.bar.rowsOfItems', new Set(items.map((i) => Math.round(i.box.y))).size);
    await hits(page, cfg, 'viewer-bar', bar);
    await shot(page, cfg, 'viewer');
    fig.covered.push(`${cfg.tag} viewer`);
  } catch (e) {
    note(`${cfg.tag} attachments scenario stopped (${msg(e)})`);
  } finally {
    await ctx.close();
  }
}

// ---- comparison (after phase) -------------------------------------------------------------------------------------------

const MAY_375 = /^(em\.title|fw\.|fc\.|vw\.)/;
const MAY_1024 = /^(fw\.field|fc\.name|fc\.dl)/;
const near = (a: number, b: number) => Math.abs(a - b) <= 0.6;

function compare(base: Figures) {
  const P = fig.problems;
  for (const [key, b] of Object.entries(base.boxes)) {
    const [cfgTag, what] = key.split('|') as [string, string];
    const a = fig.boxes[key];
    const may = cfgTag === '375' ? MAY_375.test(what) : MAY_1024.test(what);
    if (!b && !a) continue;
    if (!b || !a) {
      if (!may) P.push(`${key}: ${b ? 'gone' : 'appeared'} after`);
      continue;
    }
    if (may) continue;
    const moved375 = !near(a.w, b.w) || !near(a.h, b.h);
    const moved1024 = moved375 || !near(a.x, b.x) || !near(a.y, b.y);
    if (cfgTag === '1024' ? moved1024 : moved375)
      P.push(
        `${key}: moved though it is not on the list: base ${b.w}x${b.h} at ${b.x},${b.y}, after ${a.w}x${a.h} at ${a.x},${a.y}`,
      );
  }
  for (const [key, b] of Object.entries(base.vals)) {
    const [cfgTag, what] = key.split('|') as [string, string];
    const a = fig.vals[key];
    if (a === b) continue;
    // Readings the tasks allow to change, and the ones the other checks above already judge.
    if (/^(tap\.|spoiler\.|keyboard\.)/.test(what)) continue;
    if (cfgTag === '375' && /^(em\.title|fw\.|vw\.)/.test(what)) continue;
    if (cfgTag === '1024' && /^fw\.field/.test(what)) continue;
    P.push(`${key}: base ${String(b)}, after ${String(a)}`);
  }
  for (const [key, rows] of Object.entries(base.hits)) {
    const after = fig.hits[key] ?? [];
    const used = new Set<number>();
    for (const r of rows) {
      const idx = after.findIndex((x, i) => !used.has(i) && x.name === r.name && x.tag === r.tag);
      if (idx < 0) {
        P.push(`${key}: "${r.name}" (${r.status} in base) is not in the after rows`);
        continue;
      }
      used.add(idx);
      const x = after[idx]!;
      if (r.status === 'pass' && x.status !== 'pass')
        P.push(
          `${key}: "${r.name}" passed in base and now ${x.status} (taker ${x.taker ?? '-'}, box ${x.box ? `${x.box.w}x${x.box.h}` : '-'})`,
        );
      else if (r.status === 'fail' && x.status === 'fail')
        P.push(
          `${key}: "${r.name}" still fails (box ${x.box ? `${x.box.w}x${x.box.h}` : '-'}, taker ${x.taker ?? '-'}; base box ${r.box ? `${r.box.w}x${r.box.h}` : '-'})`,
        );
    }
    if (after.length !== rows.length)
      P.push(`${key}: ${rows.length} rows in base, ${after.length} after`);
  }
  // Direct checks on the new figures.
  for (const tag of ['375', '1024']) {
    for (let i = 0; i < 2; i++) {
      const fills = fig.vals[`${tag}|fw.field${i}.fillsLine`];
      const whole = fig.vals[`${tag}|fw.field${i}.placeholderWhole`];
      if (fills === false) P.push(`${tag} forward field ${i} does not fill its dialog's line`);
      if (whole === false) P.push(`${tag} forward field ${i}'s placeholder is cut`);
    }
    if (fig.vals[`${tag}|fw.list.scrolls`] === false)
      P.push(`${tag} the forward list no longer scrolls`);
  }
  const title = fig.vals['375|em.title.display'];
  if (title === 'inline') P.push('375 the embed title link is still display: inline');
  const lines375 = Object.entries(fig.vals).filter(([k]) => /^375\|vw\.bar\..*\.lines$/.test(k));
  for (const [k, v] of lines375)
    if (typeof v === 'number' && v > 1) P.push(`${k}: ${v} lines at 375`);
}

// ---- the test -------------------------------------------------------------------------------------------------------------

test(`wave 110 loungeshots: the pod lounge, ${PHASE}${PART ? `, part ${PART}` : ''}`, async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    const cfgs = PART === '1' ? [C375] : PART === '2' ? [C1024] : [C375, C1024];
    for (const cfg of cfgs) {
      await scenToolbar(browser, ownerCtx, w, cfg);
      await scenFriends(browser, ownerCtx, cfg);
      await scenForward(browser, ownerCtx, w, cfg);
      await scenEmbed(browser, ownerCtx, w, cfg);
      await scenAttach(browser, ownerCtx, w, cfg);
    }
    const baseFile = resolve(OUT, `figures-base${SUFFIX}.json`);
    if (PHASE === 'after') {
      if (existsSync(baseFile)) compare(JSON.parse(readFileSync(baseFile, 'utf8')) as Figures);
      else note(`no ${baseFile}: nothing compared`);
    }
  } finally {
    writeFileSync(resolve(OUT, `figures-${PHASE}${SUFFIX}.json`), JSON.stringify(fig, null, 1));
    for (const p of fig.problems) say(`PROBLEM ${p}`);
    say(
      `${PHASE}: ${fig.covered.length} scenarios covered, ${fig.problems.length} problem line(s)`,
    );
    await ownerCtx.close();
  }
});
