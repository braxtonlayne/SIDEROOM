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
import { over, parseColor, ratio, type Rgba } from '../support/contrast';

/**
 * Wave 109, censusa (CENSUS, measure only; SEC-AUD-537 pages 1 to 5 and 8, part of SEC-AUD-529): every control that
 * fails the hit test on the Lounge surfaces the wave-108 seed made reachable, at 375 x 812 with touch (default reach)
 * and at 1024 x 768 without touch (`reach: 'box'`), Lounge Light. It changes nothing and makes nothing: the world is
 * read from `world.json` (`extras`). Message attachments are injected in the browser with `page.route`, as the sweep
 * does (`page-sweep.spec.ts` `decorate`): the seeded `[files]` message of the main channel gets an image and a file.
 *
 * Parts (SR_P109_CENSUSA_PART): 1 Friends (All, Pending, a row's More menu), the group DM (page, header strip, member
 * pane, identity, message menu) and the New Group DM dialog; 2 the extras channel (the 14-reaction message and its
 * wrapped lines, the add-reaction button, the spoiler before and after reveal, the message menu, the forward dialog);
 * 3 the injected image and file (their controls, the image viewer), and the channel search. Every part runs the 375
 * pass, then the 1024 pass. Rows are written whole into `figures-part<n>.json`, with the reaction analysis
 * (`reactions`) and the field readings (`fields`: rest border, hover, ring, placeholder, height against the field
 * contract of D-1024 / D-1018). One viewport shot per page and overlay (`base-` prefix).
 *
 * Local only (SR_P109_CENSUSA_ON=1); base only. The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot base --wt lanes --env SR_P109_CENSUSA_ON=1 --env SR_P109_CENSUSA_PART=1 --env SR_P109_CENSUSA_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusa -- tests/p109-censusa-shots.spec.ts
 * (an after round is not part of this census; if ever asked: bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p109-integ/apps/web/dist --env SR_P109_CENSUSA_ON=1 --env SR_P109_CENSUSA_PART=1 --env SR_P109_CENSUSA_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusa -- tests/p109-censusa-shots.spec.ts)
 */
const ON = process.env.SR_P109_CENSUSA_ON === '1';
const PART = process.env.SR_P109_CENSUSA_PART ?? '1';
const OUT =
  process.env.SR_P109_CENSUSA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p109-censusa';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const msg = (e: unknown) => String(e).replace(/\s+/g, ' ').slice(0, 160);
const r2 = (n: number) => Math.round(n * 100) / 100;
const shotName = (n: string) => `base-${n}`;

interface World {
  guildId: string;
  channelId: string;
  extras?: {
    friendName: string;
    askerName: string;
    groupDmId: string;
    groupDmName: string;
    extrasChannelId: string;
    reactionsMessageId: string;
    spoilerMessageId: string;
  };
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

interface Fig {
  cfg: string;
  page: string;
  state: string;
  index: number;
  tag: string;
  role: string | null;
  type: string | null;
  name: string;
  status: string;
  reason: string | null;
  box: { x: number; y: number; width: number; height: number } | null;
  offset: number | null;
  edge: string | null;
  takers: { at: string; tag: string; role: string | null; name: string; path: string }[];
  takerKind: 'control' | 'container' | null;
  path: string;
}
const figs: Fig[] = [];
const notes: string[] = [];
const covered: string[] = [];
const reactions: unknown[] = [];
const fields: FieldFig[] = [];

const CONTROL_TAGS = new Set(['button', 'a', 'input', 'select', 'textarea', 'summary', 'label']);
const CONTROL_ROLES = new Set([
  'button',
  'link',
  'combobox',
  'checkbox',
  'switch',
  'radio',
  'tab',
  'menuitem',
  'menuitemcheckbox',
  'menuitemradio',
  'option',
  'slider',
]);

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

/** As the sweep does (`page-sweep.spec.ts`): the `[files]` message gets an image and a file; their bytes are served here. */
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
      .catch(() => notes.push(`${url}: ready locator not seen`));
  await settle(page);
}

async function shot(page: Page, cfg: Cfg, name: string) {
  mkdirSync(OUT, { recursive: true });
  const [sw, cw] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  if (sw > cw) notes.push(`${cfg.tag} ${name}: the page scrolls sideways (${sw} > ${cw})`);
  say(`${cfg.tag} ${name}: scrollWidth/clientWidth ${sw}/${cw}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}-${cfg.tag}.png`) });
}

/** Opens what is closed, tests every control under `root`, writes the rows. */
async function census(
  page: Page,
  cfg: Cfg,
  pageName: string,
  state: string,
  root: Locator,
  also?: string,
): Promise<void> {
  if ((await root.count()) === 0) {
    notes.push(`${cfg.tag} ${pageName} / ${state}: root not found`);
    return;
  }
  await openCollapsed(page, root.first());
  const rows: ControlRow[] = await hitTestAll(page, root.first(), {
    ...(also ? { also } : {}),
    ...(cfg.touch ? {} : { reach: 'box' as const }),
  });
  for (const r of rows) {
    const failing = (r.result?.points ?? []).filter((p) => !p.ok);
    const taker = failing[0]?.taker ?? null;
    figs.push({
      cfg: cfg.tag,
      page: pageName,
      state,
      index: r.index,
      tag: r.tag,
      role: r.role,
      type: r.type,
      name: r.name,
      status: r.status,
      reason: r.reason ?? null,
      box: r.result?.box ?? null,
      offset: r.result?.offset ?? null,
      edge: r.edge,
      takers: failing.map((p) => ({
        at: p.at,
        tag: p.taker?.tag ?? '',
        role: p.taker?.role ?? null,
        name: p.taker?.name ?? '',
        path: p.taker?.path ?? '',
      })),
      takerKind: taker
        ? CONTROL_TAGS.has(taker.tag) || (taker.role !== null && CONTROL_ROLES.has(taker.role))
          ? 'control'
          : 'container'
        : null,
      path: r.path,
    });
  }
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  say(
    `${cfg.tag} ${pageName} / ${state}: ${rows.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}, skipped ${n('skipped')}`,
  );
  for (const r of rows.filter((x) => x.status === 'fail' || x.status === 'not-reached')) {
    const b = r.result?.box;
    const t = (r.result?.points ?? []).find((p) => !p.ok)?.taker;
    say(
      `  ${r.status.toUpperCase()} ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" ${b ? `${Math.round(b.width)}x${Math.round(b.height)}` : '-'} taker ${t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}"` : '-'} ${r.path}`,
    );
  }
}

// ---- the field contract (D-1024, D-1018), read as p106-fields reads a field ------------------------------------------

interface FieldFig {
  cfg: string;
  where: string;
  tag: string;
  type: string;
  cls: string;
  name: string;
  boxFrom: string;
  height: number;
  borderWidth: string;
  rest: string;
  restIsContract: boolean;
  restBorderRatioOutside: number | null;
  hover: string;
  hoverIsFaint: boolean;
  ring: string;
  ringIsContract: boolean;
  placeholder: string | null;
  placeholderIsFaint: boolean | null;
  placeholderOpacity: string | null;
  tall44: boolean | null;
}

interface RawField {
  tag: string;
  type: string;
  cls: string;
  name: string;
  boxFrom: string;
  height: number;
  borderWidth: string;
  borderStyle: string;
  rest: string;
  bg: string;
  stack: string[];
  placeholder: string | null;
  placeholderOpacity: string | null;
  contract: { rest: string; faint: string; focus: string };
}

const FIELD_SELECTOR =
  'input:not([type=checkbox]):not([type=radio]):not([type=hidden]):not([type=range]):not([type=file]), textarea, [role=textbox], [contenteditable=true], [role=searchbox], [role=combobox]';

/** Read one field: own border, or the nearest ancestor's that draws one; tag it `data-p109f` for the state reads. */
async function readField(loc: Locator, n: number): Promise<RawField | null> {
  return loc.evaluate((el, id) => {
    const cs = (e: Element, p?: string) => getComputedStyle(e, p);
    const hasBorder = (e: Element) => {
      const c = cs(e);
      return c.borderStyle !== 'none' && parseFloat(c.borderTopWidth) > 0;
    };
    let box: Element = el;
    let from = 'self';
    if (!hasBorder(el)) {
      let p: Element | null = el.parentElement;
      for (let i = 0; i < 4 && p; i++, p = p.parentElement) {
        if (hasBorder(p)) {
          box = p;
          from = `ancestor ${p.tagName.toLowerCase()}.${String((p as HTMLElement).className).split(/\s+/)[0] ?? ''}`;
          break;
        }
      }
    }
    box.setAttribute('data-p109f', String(id));
    el.setAttribute('data-p109e', String(id));
    const c = cs(box);
    const stack: string[] = [];
    for (let p: Element | null = box.parentElement; p; p = p.parentElement)
      stack.push(cs(p).backgroundColor);
    const probe = document.createElement('span');
    probe.style.cssText =
      'color: var(--ui-text-faint); outline-color: var(--ui-focus); border: 1px solid; border-color: color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%);';
    (el.parentElement ?? document.body).appendChild(probe);
    const pc = cs(probe);
    const contract = {
      faint: pc.color,
      focus: pc.outlineColor,
      rest: pc.borderTopColor,
    };
    probe.remove();
    const ph = (el as HTMLInputElement).placeholder ?? null;
    const phc = ph ? cs(el, '::placeholder') : null;
    const r = box.getBoundingClientRect();
    const label =
      el.getAttribute('aria-label') ??
      (el as HTMLInputElement).placeholder ??
      el.getAttribute('name') ??
      '';
    return {
      tag: el.tagName.toLowerCase(),
      type: (el as HTMLInputElement).type ?? '',
      cls: String((el as HTMLElement).className).slice(0, 60),
      name: label.slice(0, 40),
      boxFrom: from,
      height: r.height,
      borderWidth: c.borderTopWidth,
      borderStyle: c.borderTopStyle,
      rest: c.borderTopColor,
      bg: c.backgroundColor,
      stack,
      placeholder: phc ? phc.color : null,
      placeholderOpacity: phc ? phc.opacity : null,
      contract,
    };
  }, n);
}

const flatten = (stack: string[]): Rgba => {
  let g: Rgba = { r: 255, g: 255, b: 255, a: 1 };
  for (const s of [...stack].reverse()) g = over(parseColor(s), g);
  return g;
};

/** Every visible text field under `root`: the contract's five parts. */
async function measureFields(page: Page, cfg: Cfg, where: string, root: Locator) {
  try {
    const all = await root.first().locator(FIELD_SELECTOR).all();
    let n = 0;
    for (const loc of all) {
      if (!(await loc.isVisible().catch(() => false))) continue;
      n++;
      const id = Math.floor(Math.random() * 1e6);
      const f = await readField(loc, id);
      if (!f) continue;
      const box = page.locator(`[data-p109f="${id}"]`).first();
      const el = page.locator(`[data-p109e="${id}"]`).first();
      const outside = flatten(f.stack);
      const field = over(parseColor(f.bg), outside);
      const hasBorder = f.borderStyle !== 'none' && parseFloat(f.borderWidth) > 0;
      const bv = hasBorder ? over(parseColor(f.rest), field) : null;
      let hover = 'not read';
      let ring = 'not read';
      try {
        await box.scrollIntoViewIfNeeded({ timeout: 2000 });
        await page.mouse.move(2, 2);
        await box.hover({ timeout: 3000, force: true });
        hover = await box.evaluate((e) => getComputedStyle(e).borderTopColor);
        await page.mouse.move(2, 2);
        await page.keyboard.press('Shift');
        await el.focus({ timeout: 2000 });
        ring = await el.evaluate((e0) => {
          const pick = (c: CSSStyleDeclaration) =>
            c.outlineStyle !== 'none' && parseFloat(c.outlineWidth) > 0
              ? `${c.outlineWidth} ${c.outlineStyle} ${c.outlineColor} offset ${c.outlineOffset}`
              : '';
          let e: Element | null = e0;
          for (let i = 0; i < 5 && e; i++, e = e.parentElement) {
            const r = pick(getComputedStyle(e));
            if (r)
              return i === 0
                ? r
                : `${r} (on ancestor ${e.tagName.toLowerCase()}.${String((e as HTMLElement).className).split(/s+/)[0] ?? ''})`;
          }
          return 'none';
        });
        await el.blur();
      } catch (e) {
        say(`${cfg.tag} ${where}: hover/focus of ${f.tag} "${f.name}" not read: ${msg(e)}`);
      }
      const faint = f.contract.faint;
      const ringOk =
        ring.startsWith('2px solid') &&
        ring.includes(f.contract.focus) &&
        ring.includes('offset 1px');
      fields.push({
        cfg: cfg.tag,
        where,
        tag: f.tag,
        type: f.type,
        cls: f.cls,
        name: f.name,
        boxFrom: f.boxFrom,
        height: r2(f.height),
        borderWidth: f.borderWidth,
        rest: f.rest,
        restIsContract: hasBorder && f.rest === f.contract.rest,
        restBorderRatioOutside: bv ? r2(ratio(bv, outside)) : null,
        hover,
        hoverIsFaint: hover === faint,
        ring,
        ringIsContract: ringOk,
        placeholder: f.placeholder,
        placeholderIsFaint: f.placeholder === null ? null : f.placeholder === faint,
        placeholderOpacity: f.placeholderOpacity,
        tall44: cfg.touch ? f.height >= 43.5 : null,
      });
    }
    if (n === 0) notes.push(`${cfg.tag} ${where}: no field found`);
  } catch (e) {
    notes.push(`${cfg.tag} ${where}: fields not read (${msg(e)})`);
  }
}

/** One overlay: `open` taps what a member taps; when it does not open, a note says so. */
async function overlay(
  page: Page,
  cfg: Cfg,
  pageName: string,
  state: string,
  open: () => Promise<void>,
  root: () => Locator,
  opts: { also?: string; reload?: string; fields?: boolean } = {},
): Promise<void> {
  try {
    await open();
    await settle(page, 2500);
    const r = root();
    await r.first().waitFor({ timeout: 6000 });
    await census(page, cfg, pageName, state, r, opts.also);
    if (opts.fields) await measureFields(page, cfg, `${pageName} / ${state}`, r);
    await shot(page, cfg, `${pageName}-${state.replace(/[^a-z0-9]+/gi, '-').toLowerCase()}`);
    covered.push(`${cfg.tag} ${pageName} / ${state}`);
  } catch (e) {
    notes.push(
      `${cfg.tag} ${pageName} / ${state}: not opened (${String(e).split('\n')[0]?.slice(0, 140)})`,
    );
  }
  if (opts.reload) {
    await go(page, opts.reload);
  } else {
    await page.keyboard.press('Escape').catch(() => undefined);
    await page.waitForTimeout(400);
  }
}

const tap = async (loc: Locator) => {
  await loc
    .first()
    .scrollIntoViewIfNeeded({ timeout: 5000 })
    .catch(() => undefined);
  await loc.first().click({ timeout: 6000 });
};

/** The header's "More actions" strip, when the header has one (a phone). */
const openMore = async (page: Page) => {
  const more = page.getByRole('button', { name: 'More actions' });
  if ((await more.count()) > 0 && (await more.first().getAttribute('aria-expanded')) === 'false')
    await more.first().click({ timeout: 5000 });
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

/**
 * A message's hover toolbar (`.lng-msg-tools`): shown by `:hover` or `:focus-within`, so a tap on a phone (sticky hover)
 * and a pointer hover show it. The page is reloaded first so no earlier state leaks in.
 */
async function toolbarState(
  page: Page,
  cfg: Cfg,
  pageName: string,
  url: string,
  pick: (p: Page) => Locator,
) {
  try {
    await go(page, url, page.getByRole('log'));
    const art = pick(page).first();
    await art.scrollIntoViewIfNeeded({ timeout: 5000 });
    const time = art.locator('time').first();
    const b = (await time.count()) > 0 ? await time.boundingBox() : await art.boundingBox();
    if (!b) throw new Error('no box');
    if (cfg.touch) await page.touchscreen.tap(b.x + b.width / 2, b.y + b.height / 2);
    else await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2);
    await page.waitForTimeout(600);
    const tools = art.locator('.lng-msg-tools');
    if (
      !(await tools
        .first()
        .isVisible()
        .catch(() => false))
    ) {
      notes.push(
        `${cfg.tag} ${pageName} toolbar: not shown after ${cfg.touch ? 'a tap' : 'a hover'}`,
      );
      return;
    }
    await census(page, cfg, pageName, 'message toolbar (after a tap or hover)', tools);
    await shot(page, cfg, `${pageName}-toolbar`);
    covered.push(`${cfg.tag} ${pageName} / message toolbar`);
  } catch (e) {
    notes.push(`${cfg.tag} ${pageName} toolbar: failed (${msg(e)})`);
  }
}

const main = (page: Page) => page.locator('#lounge-main');
const dialog = (page: Page) => page.getByRole('dialog').last();

async function pageCensus(page: Page, cfg: Cfg, name: string, root: Locator) {
  await census(page, cfg, name, 'page', root);
  await shot(page, cfg, name);
  covered.push(`${cfg.tag} ${name} / page`);
}

/** Home, then the Friends page (a tab bar button or a left-column link), then a tab by name. */
async function openFriends(page: Page, tab?: string) {
  await go(page, '/channels/@me', page.locator('.lng-left'));
  await tap(
    page.getByRole('button', { name: /^Friends/ }).or(page.getByRole('link', { name: /^Friends/ })),
  );
  await page.locator('section.lng-friends').first().waitFor({ timeout: 10_000 });
  await settle(page, 2500);
  if (tab)
    await tap(
      page
        .getByRole('tab', { name: tab, exact: true })
        .or(page.getByRole('button', { name: tab, exact: true })),
    );
  await settle(page, 2000);
}

// ---- part 1: Friends, the group DM, the New Group DM dialog -------------------------------------------------------------

async function part1(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const { ctx, page } = await newPage(browser, owner, cfg);
  const ex = w.extras;
  try {
    const sec = () => page.locator('section.lng-friends');
    // 1. Friends, All.
    try {
      await openFriends(page, 'All');
      await pageCensus(page, cfg, 'friends-all', sec());
      await overlay(
        page,
        cfg,
        'friends-all',
        'row More actions menu',
        async () => {
          await tap(page.locator('.lng-friend-list').getByRole('button', { name: /More actions/ }));
        },
        () => page.getByRole('menu').last(),
        { also: MENU_ITEM_SELECTOR },
      );
      await measureFields(page, cfg, 'friends-all / page', sec());
    } catch (e) {
      notes.push(`${cfg.tag} friends-all: failed (${msg(e)})`);
    }
    // 2. Friends, Pending.
    try {
      await openFriends(page, 'Pending');
      await pageCensus(page, cfg, 'friends-pending', sec());
    } catch (e) {
      notes.push(`${cfg.tag} friends-pending: failed (${msg(e)})`);
    }
    // 3. The New Group DM dialog, from Friends.
    try {
      await openFriends(page);
      await overlay(
        page,
        cfg,
        'new-group-dm',
        'dialog',
        async () => {
          await tap(page.getByRole('button', { name: 'New Group DM' }));
        },
        () => dialog(page),
        { fields: true },
      );
    } catch (e) {
      notes.push(`${cfg.tag} new-group-dm: failed (${msg(e)})`);
    }
    // 3. The group DM: page, header, member strip, identity, message menu, composer.
    if (!ex) {
      notes.push('the world has no `extras` (a world seeded before wave 108)');
    } else {
      const url = `/channels/@me/${ex.groupDmId}`;
      await go(page, url, page.getByRole('log'));
      await pageCensus(page, cfg, 'group-dm', main(page));
      await measureFields(page, cfg, 'group-dm / composer', main(page));
      await overlay(
        page,
        cfg,
        'group-dm',
        'header more strip',
        async () => {
          await openMore(page);
        },
        () => (cfg.touch ? page.locator('.lng-dm-extra') : main(page).locator('.lng-header')),
        { reload: url },
      );
      await overlay(
        page,
        cfg,
        'group-dm',
        'member pane',
        async () => {
          await openMore(page);
          const mb = page.getByRole('button', { name: 'Member List' });
          const pressed = (await mb.first().getAttribute('aria-pressed')) === 'true';
          if (!pressed) await tap(mb);
          else if (cfg.touch) {
            await tap(mb);
            await page.waitForTimeout(600);
            await openMore(page);
            await tap(mb);
          }
        },
        () => page.locator('aside.lng-right'),
        { reload: url },
      );
      await overlay(
        page,
        cfg,
        'group-dm',
        'identity (group panel)',
        async () => {
          await tap(page.getByRole('button', { name: /Show members of/i }));
        },
        () => page.locator('aside.lng-right').or(dialog(page)),
        { reload: url },
      );
      await overlay(
        page,
        cfg,
        'group-dm',
        'add friends to DM',
        async () => {
          await openMore(page);
          await tap(page.getByRole('button', { name: /Add friends/i }));
        },
        () => dialog(page),
        { fields: true, reload: url },
      );
      await overlay(
        page,
        cfg,
        'group-dm',
        'message action menu',
        async () => {
          await openMsgMenu(page, cfg, page.locator('article.lng-msg').last());
        },
        () => page.getByRole('menu').last(),
        { also: MENU_ITEM_SELECTOR },
      );
    }
    if (ex)
      await toolbarState(page, cfg, 'group-dm', `/channels/@me/${ex.groupDmId}`, (p) =>
        p.locator('article.lng-msg').last(),
      );
  } finally {
    await ctx.close();
  }
}

// ---- part 2: the extras channel: reactions, spoiler, menu, forward dialog -----------------------------------------------

/** The wrapped reaction lines of one message: each pill's box and tap area, the lines' gap, the message's height. */
async function analyseReactions(cfg: Cfg, art: Locator) {
  const data = await art.evaluate((el) => {
    const host = el.querySelector('.lng-reactions');
    if (!host) return null;
    const hs = getComputedStyle(host);
    const pills = Array.from(host.querySelectorAll('.lng-reaction')).map((p) => {
      const r = p.getBoundingClientRect();
      const c = getComputedStyle(p);
      const a = getComputedStyle(p, '::after');
      const hasArea = a.content !== 'none' && a.position === 'absolute';
      const bt = parseFloat(c.borderTopWidth) || 0;
      const top = hasArea ? r.top + bt + parseFloat(a.top) : r.top;
      const bottom = hasArea ? top + parseFloat(a.height) : r.bottom;
      return {
        name: (p.getAttribute('aria-label') ?? p.textContent ?? '').slice(0, 40),
        add: p.classList.contains('lng-reaction-add'),
        x: Math.round(r.x),
        top: Math.round(r.top * 10) / 10,
        bottom: Math.round(r.bottom * 10) / 10,
        height: Math.round(r.height * 10) / 10,
        width: Math.round(r.width * 10) / 10,
        hasArea,
        areaTop: Math.round(top * 10) / 10,
        areaBottom: Math.round(bottom * 10) / 10,
        afterTop: hasArea ? a.top : null,
        afterHeight: hasArea ? a.height : null,
      };
    });
    const art = el.getBoundingClientRect();
    return {
      rowGap: hs.rowGap,
      gap: hs.gap,
      display: hs.display,
      articleHeight: Math.round(art.height * 10) / 10,
      hostHeight: Math.round(host.getBoundingClientRect().height * 10) / 10,
      pills,
    };
  });
  if (!data) {
    notes.push(`${cfg.tag} reactions: the message has no reaction row`);
    return;
  }
  // Lines: pills sharing a top (within 2 px).
  const lines: (typeof data.pills)[] = [];
  for (const p of [...data.pills].sort((a, b) => a.top - b.top || a.x - b.x)) {
    const last = lines[lines.length - 1];
    if (last && Math.abs(last[0]!.top - p.top) < 2) last.push(p);
    else lines.push([p]);
  }
  const pairs = lines.slice(1).map((l, i) => {
    const prev = lines[i]!;
    const boxGap = Math.min(...l.map((p) => p.top)) - Math.max(...prev.map((p) => p.bottom));
    const areaGap =
      Math.min(...l.map((p) => p.areaTop)) - Math.max(...prev.map((p) => p.areaBottom));
    return { line: i + 2, boxGap: r2(boxGap), areaGap: r2(areaGap), areasOverlap: areaGap < 0 };
  });
  const out = { cfg: cfg.tag, ...data, lineCount: lines.length, pairs };
  reactions.push(out);
  say(
    `${cfg.tag} reactions: ${data.pills.length} pills on ${lines.length} lines, row-gap ${data.rowGap}, article ${data.articleHeight}px, areas: ${pairs.map((p) => `line ${p.line}: box gap ${p.boxGap}, area gap ${p.areaGap}`).join('; ')}`,
  );
}

async function part2(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const ex = w.extras;
  if (!ex) {
    notes.push('the world has no `extras` (a world seeded before wave 108)');
    return;
  }
  const { ctx, page } = await newPage(browser, owner, cfg);
  try {
    const url = `/channels/${w.guildId}/${ex.extrasChannelId}`;
    await go(page, url, page.getByRole('log'));
    await pageCensus(page, cfg, 'extras-channel', main(page));
    await measureFields(page, cfg, 'extras-channel / composer', main(page));

    // 4. The 14-reaction message.
    const rart = page.locator('article.lng-msg').filter({ has: page.locator('.lng-reactions') });
    if ((await rart.count()) === 0) notes.push(`${cfg.tag} the reactions message not found`);
    else {
      await rart.first().scrollIntoViewIfNeeded();
      await analyseReactions(cfg, rart.first());
      await census(page, cfg, 'reactions', 'message (pills + add)', rart.first());
      await shot(page, cfg, 'reactions');
      covered.push(`${cfg.tag} reactions / message`);
      await overlay(
        page,
        cfg,
        'reactions',
        'add-reaction picker',
        async () => {
          await tap(rart.first().locator('.lng-reaction-add'));
        },
        () => page.getByRole('dialog').last().or(page.getByRole('menu').last()),
        { fields: true },
      );
    }

    // 5. The spoiler, before and after the reveal.
    await go(page, url, page.getByRole('log'));
    const sart = page.locator('article.lng-msg').filter({
      has: page.getByRole('button', { name: 'Spoiler, click to reveal' }),
    });
    if ((await sart.count()) === 0) notes.push(`${cfg.tag} the spoiler message not found`);
    else {
      await sart.first().scrollIntoViewIfNeeded();
      await sart.first().evaluate((e) => e.setAttribute('data-p109s', '1'));
      await census(page, cfg, 'spoiler', 'before reveal', sart.first());
      await shot(page, cfg, 'spoiler-before');
      covered.push(`${cfg.tag} spoiler / before reveal`);
      try {
        const sb = page.locator('[data-p109s="1"]').getByRole('button', {
          name: 'Spoiler, click to reveal',
        });
        await sb
          .first()
          .click({ timeout: 3000 })
          .catch(async (e) => {
            notes.push(
              `${cfg.tag} spoiler: a real tap failed (${String(e).slice(0, 700)}); clicked by script`,
            );
            await sb.first().evaluate((x) => (x as HTMLElement).click());
          });
        await page.waitForTimeout(500);
        await census(page, cfg, 'spoiler', 'after reveal', page.locator('[data-p109s="1"]'));
        await shot(page, cfg, 'spoiler-after');
        covered.push(`${cfg.tag} spoiler / after reveal`);
      } catch (e) {
        notes.push(`${cfg.tag} spoiler reveal failed (${msg(e)})`);
      }
    }

    // The message menu and the forward dialog.
    await go(page, url, page.getByRole('log'));
    const target = page.locator('article.lng-msg').last();
    await overlay(
      page,
      cfg,
      'extras-channel',
      'message action menu',
      async () => {
        await openMsgMenu(page, cfg, target);
      },
      () => page.getByRole('menu').last(),
      { also: MENU_ITEM_SELECTOR },
    );
    await overlay(
      page,
      cfg,
      'forward',
      'dialog',
      async () => {
        await openMsgMenu(page, cfg, target);
        await tap(page.getByRole('menuitem', { name: /Forward/ }));
      },
      () => dialog(page),
      { fields: true },
    );
    await toolbarState(
      page,
      cfg,
      'reactions',
      `/channels/${w.guildId}/${ex.extrasChannelId}`,
      (p) => p.locator('article.lng-msg').filter({ has: p.locator('.lng-reactions') }),
    );
    await toolbarState(page, cfg, 'spoiler', `/channels/${w.guildId}/${ex.extrasChannelId}`, (p) =>
      p.locator('article.lng-msg').last(),
    );
  } finally {
    await ctx.close();
  }
}

// ---- part 3: injected image and file; the channel search ------------------------------------------------------------------

async function part3(browser: Browser, owner: BrowserContext, w: World, cfg: Cfg) {
  const { ctx, page } = await newPage(browser, owner, cfg, true);
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    const art = page.locator('article.lng-msg').filter({
      has: page.locator('.lng-attachments, .lng-attach-file, .lng-tile'),
    });
    if ((await art.count()) === 0) notes.push(`${cfg.tag} no message with an injected attachment`);
    else {
      await art.first().scrollIntoViewIfNeeded();
      await settle(page, 2500);
      await census(page, cfg, 'attachments', 'message (image + file)', art.first());
      await shot(page, cfg, 'attachments');
      covered.push(`${cfg.tag} attachments / message`);
      await overlay(
        page,
        cfg,
        'attachments',
        'image viewer',
        async () => {
          await tap(art.first().locator('.lng-tile-btn'));
        },
        () => page.getByRole('dialog').last(),
        { reload: url },
      );
      await overlay(
        page,
        cfg,
        'attachments',
        'message action menu',
        async () => {
          await openMsgMenu(page, cfg, art.first());
        },
        () => page.getByRole('menu').last(),
        { also: MENU_ITEM_SELECTOR },
      );
    }

    // The channel search: a sheet on a phone, the inline box and its panel at 1024.
    await go(page, url, page.getByRole('log'));
    if (cfg.touch) {
      await overlay(
        page,
        cfg,
        'channel-search',
        'phone sheet',
        async () => {
          await tap(page.getByRole('button', { name: 'Search this conversation' }));
        },
        () => page.locator('.lng-search-sheet'),
        { fields: true, reload: url },
      );
    } else {
      await overlay(
        page,
        cfg,
        'channel-search',
        'inline box + panel',
        async () => {
          await tap(page.getByRole('search').locator('input').first());
        },
        () => page.locator('.lng-search-wrap'),
        { fields: true },
      );
    }
    await toolbarState(page, cfg, 'attachments', `/channels/${w.guildId}/${w.channelId}`, (p) =>
      p
        .locator('article.lng-msg')
        .filter({ has: p.locator('.lng-attachments, .lng-attach-file, .lng-tile') }),
    );
  } finally {
    await ctx.close();
  }
}

test(`wave 109 censusa: the Lounge census on the new seed, part ${PART}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    const run = PART === '1' ? part1 : PART === '2' ? part2 : part3;
    for (const cfg of [C375, C1024]) {
      try {
        await run(browser, ownerCtx, w, cfg);
      } catch (e) {
        notes.push(`${cfg.tag} part ${PART} stopped: ${msg(e)}`);
      }
    }
  } finally {
    writeFileSync(
      resolve(OUT, `figures-part${PART}.json`),
      JSON.stringify({ part: PART, rows: figs, fields, reactions, notes, covered }, null, 1),
    );
    for (const n of notes) say(`note: ${n}`);
    for (const f of fields)
      say(
        `field ${f.cfg} ${f.where} ${f.tag} "${f.name}" h${f.height} rest=${f.restIsContract} ratio=${f.restBorderRatioOutside} hover=${f.hoverIsFaint} ring=${f.ringIsContract} (${f.ring}) ph=${f.placeholderIsFaint}/${f.placeholderOpacity} 44=${f.tall44}`,
      );
    const fails = figs.filter((f) => f.status === 'fail').length;
    const nr = figs.filter((f) => f.status === 'not-reached');
    say(`part ${PART}: ${figs.length} rows, ${fails} fail, ${nr.length} not-reached`);
    for (const r of nr)
      say(`not-reached: ${r.cfg} ${r.page} / ${r.state} ${r.tag} "${r.name}" ${r.reason ?? ''}`);
    await ownerCtx.close();
  }
});
