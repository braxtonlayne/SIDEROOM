import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';
import { apiDelete } from '../support/world';
import {
  MENU_ITEM_SELECTOR,
  hitTestAll,
  openCollapsed,
  type ControlRow,
} from '../support/hit-test';

/**
 * Wave 106, toucha (CENSUS, measure only): every control that fails the 44 px hit test (D-856, D-917, D-983) at
 * 375 x 812 with touch, Lounge Light, in the Lounge and in User settings. It changes nothing; it makes one forum
 * channel with one post through the API (named `p106toucha-forum-<stamp>`) and removes it in a `finally`.
 *
 * For each page: go there as a member does, settle, `openCollapsed`, `hitTestAll` over the page's main region with the
 * default reach; then open what a member opens with one tap (a menu, a sheet, a dialog, a drawer; at most three a page)
 * and test it too (a menu with `{ also: MENU_ITEM_SELECTOR }`). Every row is written whole into
 * `figures-part<n>.json` (page, state, name, role or tag, box, status, taker, path); one viewport shot per page and per
 * overlay. An overlay that did not open is written as a note in the same file, never as a failure.
 *
 * Parts (SR_P106_TOUCHA_PART; "12" runs 1 and 2 in one job): 1 the text channel (+ overlays) and a thread; 2 forum and post, DM, voice channel,
 * scrapbook; 3 the Home list, Friends and its tabs, Message Requests, a channel's list pane (server rail, sidebar), the
 * inbox; 4 User settings, the first ten sections; 5 the last ten.
 *
 * Local only (SR_P106_TOUCHA_ON=1); base only (no app file is changed). The commander's command, whole:
 * bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p106-integ/apps/web/dist --env SR_P106_TOUCHA_ON=1 --env SR_P106_TOUCHA_PHASE=after --env SR_P106_TOUCHA_PART=1 --env SR_P106_TOUCHA_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-toucha -- tests/p106-toucha-shots.spec.ts
 */
const ON = process.env.SR_P106_TOUCHA_ON === '1';
const PHASE = process.env.SR_P106_TOUCHA_PHASE === 'after' ? 'after' : 'base';
const PART = process.env.SR_P106_TOUCHA_PART ?? '1';
const OUT =
  process.env.SR_P106_TOUCHA_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p106-toucha';
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const OWNER_STATE = resolve(STATE, 'owner.json');
const WORLD_FILE = resolve(STATE, 'world.json');
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const STAMP = String(Date.now()).slice(-6);

const USER_SETTINGS = [
  'account',
  'profile',
  'privacy',
  'activity',
  'apps',
  'modules',
  'devices',
  'connections',
  'invites',
  'storage',
  'support',
  'appearance',
  'accessibility',
  'text',
  'notifications',
  'voice',
  'keybinds',
  'language',
  'streamer',
  'advanced',
];

test.skip(!ON, 'local visual check only');
test.setTimeout(540_000);

const say = (s: string) => console.log(`REPORT ${s}`);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const shotName = (n: string) => (PHASE === 'base' ? `base-${n}` : n);

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
  threadId: string;
  dmId: string | null;
  memberName: string;
}
const world = () => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Fig {
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
  takers: { at: string; tag: string; role: string | null; name: string; path: string }[];
  takerKind: 'control' | 'container' | null;
  path: string;
}
const figs: Fig[] = [];
const notes: string[] = [];
const covered: string[] = [];

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

async function newPage(browser: Browser, owner: BrowserContext, settingsTheme = true) {
  void settingsTheme;
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 375, height: 812 },
    storageState: await owner.storageState(),
    bypassCSP: true,
    userAgent: PHONE_UA,
    isMobile: true,
    hasTouch: true,
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

async function go(page: Page, url: string, ready?: Locator) {
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30_000 });
  if (ready)
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
  if (sw > cw) notes.push(`${name}: the page scrolls sideways (${sw} > ${cw})`);
  say(`${name}: scrollWidth/clientWidth ${sw}/${cw}`);
  await page.screenshot({ path: resolve(OUT, `${shotName(name)}-375.png`) });
}

/** Opens what is closed, tests every control under `root`, writes the rows. */
async function census(
  page: Page,
  pageName: string,
  state: string,
  root: Locator,
  also?: string,
): Promise<void> {
  if ((await root.count()) === 0) {
    notes.push(`${pageName} / ${state}: root not found`);
    return;
  }
  await openCollapsed(page, root.first());
  const rows: ControlRow[] = await hitTestAll(page, root.first(), also ? { also } : {});
  for (const r of rows) {
    const failing = (r.result?.points ?? []).filter((p) => !p.ok);
    const taker = failing[0]?.taker ?? null;
    figs.push({
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
    `${pageName} / ${state}: ${rows.length} rows, pass ${n('pass')}, fail ${n('fail')}, not-reached ${n('not-reached')}, skipped ${n('skipped')}`,
  );
  for (const r of rows.filter((x) => x.status === 'fail' || x.status === 'not-reached')) {
    const b = r.result?.box;
    const t = (r.result?.points ?? []).find((p) => !p.ok)?.taker;
    say(
      `  ${r.status.toUpperCase()} ${r.tag}${r.role ? `[${r.role}]` : ''} "${r.name}" ${b ? `${Math.round(b.width)}x${Math.round(b.height)}` : '-'} taker ${t ? `${t.tag}${t.role ? `[${t.role}]` : ''} "${t.name}"` : '-'} ${r.path}`,
    );
  }
}

/**
 * One overlay: `open` taps what a member taps; when it does not open, a note says so. `root` finds what to test; the
 * shot is named `<pageName>-<state>`. `close` leaves it (Escape by default); `reload` is for a pane the page cannot
 * leave by Escape.
 */
async function overlay(
  page: Page,
  pageName: string,
  state: string,
  open: () => Promise<void>,
  root: () => Locator,
  opts: { also?: string; reload?: string } = {},
): Promise<void> {
  try {
    await open();
    await settle(page, 2500);
    const r = root();
    await r.first().waitFor({ timeout: 6000 });
    await census(page, pageName, state, r, opts.also);
    await shot(page, `${pageName}-${state.replace(/[^a-z0-9]+/gi, '-').toLowerCase()}`);
    covered.push(`${pageName} / ${state}`);
  } catch (e) {
    notes.push(`${pageName} / ${state}: not opened (${String(e).split('\n')[0]?.slice(0, 140)})`);
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

const main = (page: Page) => page.locator('#lounge-main');
const dialog = (page: Page) => page.getByRole('dialog').last();

async function pageCensus(page: Page, name: string, root: Locator) {
  await census(page, name, 'page', root);
  await shot(page, name);
  covered.push(`${name} / page`);
}

// ---- part 1: the text channel and a thread -------------------------------------------------------------------------

async function part1(browser: Browser, owner: BrowserContext, w: World) {
  const { ctx, page } = await newPage(browser, owner);
  try {
    const url = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, url, page.getByRole('log'));
    await pageCensus(page, 'channel', main(page));

    await overlay(
      page,
      'channel',
      'header more strip',
      async () => {
        await openMore(page);
      },
      () => page.locator('.lng-dm-extra'),
      { reload: url },
    );
    await page.waitForTimeout(500);
    await overlay(
      page,
      'channel',
      'member list sheet',
      async () => {
        await openMore(page);
        await tap(page.getByRole('button', { name: 'Member List' }));
      },
      () => page.locator('aside.lng-right'),
      { reload: url },
    );
    await overlay(
      page,
      'channel',
      'message action menu',
      async () => {
        await longPress(page, page.locator('article.lng-msg').first());
      },
      () => page.getByRole('menu').last(),
      { also: MENU_ITEM_SELECTOR },
    );
    await overlay(
      page,
      'channel',
      'add to message menu',
      async () => {
        await tap(page.getByRole('button', { name: 'Add to message' }));
      },
      () => page.getByRole('menu').last(),
      { also: MENU_ITEM_SELECTOR },
    );
    await overlay(
      page,
      'channel',
      'emoji picker',
      async () => {
        await tap(page.getByRole('button', { name: /emoji picker/i }));
      },
      () => page.getByRole('dialog', { name: /emoji picker/i }).last(),
    );
    await overlay(
      page,
      'channel',
      'GIF picker',
      async () => {
        await tap(page.getByRole('button', { name: /GIF picker/i }));
      },
      () => page.getByRole('dialog', { name: /GIF/i }).last(),
    );
    await overlay(
      page,
      'channel',
      'phone search sheet',
      async () => {
        await tap(page.getByRole('button', { name: 'Search this conversation' }));
      },
      () => page.locator('.lng-search-sheet'),
      { reload: url },
    );

    // A thread.
    const turl = `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`;
    await go(page, turl, page.getByRole('log'));
    await pageCensus(page, 'thread', page.locator('aside.lng-thread-pane'));
    await overlay(
      page,
      'thread',
      'message action menu',
      async () => {
        await longPress(
          page,
          page.locator('aside.lng-thread-pane').locator('article.lng-msg').first(),
        );
      },
      () => page.getByRole('menu').last(),
      { also: MENU_ITEM_SELECTOR },
    );
    covered.push('(message link page: the same page as `channel`, not run)');
  } finally {
    await ctx.close();
  }
}

// ---- part 2: forum, DM, voice, scrapbook ---------------------------------------------------------------------------

async function part2(browser: Browser, owner: BrowserContext, w: World) {
  const o = owner.request;
  let forumId: string | null = null;
  const { ctx, page } = await newPage(browser, owner);
  try {
    try {
      const f = await apiPost<{ id: string }>(o, `/guilds/${w.guildId}/channels`, {
        name: `p106toucha-forum-${STAMP}`,
        type: 15,
      });
      forumId = f.id;
      await sleep(1200);
      await apiPost(o, `/channels/${f.id}/threads`, {
        name: `p106toucha-post-${STAMP}`,
        message: { content: 'A post for the touch census.' },
      });
    } catch (e) {
      notes.push(`forum not made: ${String(e).slice(0, 160)}`);
    }
    if (forumId) {
      await go(
        page,
        `/channels/${w.guildId}/${forumId}`,
        page.getByRole('combobox', { name: 'Sort by' }),
      );
      await pageCensus(page, 'forum', main(page));
      await overlay(
        page,
        'forum',
        'post opened',
        async () => {
          await tap(page.locator('.lng-post-btn'));
        },
        () => main(page),
        { reload: `/channels/${w.guildId}/${forumId}` },
      );
      await overlay(
        page,
        'forum',
        'new post',
        async () => {
          await tap(page.getByRole('button', { name: /new post|create post/i }));
        },
        () => dialog(page),
      );
    }

    // DM
    if (w.dmId) {
      const durl = `/channels/@me/${w.dmId}`;
      // The seeded DM is a message request from the member: test that view, then accept it and test the DM.
      await go(page, durl);
      await pageCensus(page, 'dm-request', main(page));
      await tap(main(page).getByRole('button', { name: /^Accept/ }));
      await page.getByRole('log').first().waitFor({ timeout: 20_000 });
      await settle(page);
      await pageCensus(page, 'dm', main(page));
      await overlay(
        page,
        'dm',
        'header more strip',
        async () => {
          await openMore(page);
        },
        () => page.locator('.lng-dm-extra'),
        { reload: durl },
      );
      await overlay(
        page,
        'dm',
        'profile pane',
        async () => {
          await openMore(page);
          await tap(page.getByRole('button', { name: 'User profile' }));
        },
        () => page.locator('aside.lng-right'),
        { reload: durl },
      );
      await overlay(
        page,
        'dm',
        'message action menu',
        async () => {
          await longPress(page, page.locator('article.lng-msg').first());
        },
        () => page.getByRole('menu').last(),
        { also: MENU_ITEM_SELECTOR },
      );
    } else notes.push('dm: the world has no DM');
    covered.push('(group DM: not covered, see the report)');

    // Voice channel (not in a call)
    if (w.voiceChannelId) {
      await go(page, `/channels/${w.guildId}/${w.voiceChannelId}`);
      await pageCensus(page, 'voice-channel', main(page));
    } else notes.push('voice-channel: the world has no voice channel');

    // Scrapbook gallery
    await go(page, `/channels/${w.guildId}/scrapbook`);
    await pageCensus(page, 'scrapbook', main(page));
  } finally {
    await ctx.close();
    if (forumId)
      await apiDelete(o, `/channels/${forumId}`).catch((e) =>
        notes.push(
          `forum ${forumId} (p106toucha-forum-${STAMP}) not deleted: ${String(e).slice(0, 160)}`,
        ),
      );
  }
}

// ---- part 3: Home list, Friends, requests, a channel's list pane, inbox --------------------------------------------

async function part3(browser: Browser, owner: BrowserContext, w: World) {
  const { ctx, page } = await newPage(browser, owner);
  try {
    // The Home list pane: rail, sidebar, user panel, bottom tab bar.
    await go(page, '/channels/@me', page.locator('.lng-left'));
    const leftPane = () => page.locator('.lng-left, nav.lng-tabbar');
    await census(page, 'home-list', 'page', leftPane());
    await shot(page, 'home-list');
    covered.push('home-list / page');

    // Friends page and its tabs.
    const openFriends = async () => {
      await go(page, '/channels/@me', page.locator('.lng-left'));
      await tap(
        page
          .getByRole('button', { name: /^Friends$/ })
          .or(page.getByRole('link', { name: /^Friends$/ })),
      );
      await settle(page, 2500);
    };
    try {
      await openFriends();
      await page.locator('section.lng-friends').first().waitFor({ timeout: 8000 });
      await pageCensus(page, 'friends', page.locator('section.lng-friends'));
      for (const tab of ['Online', 'Pending', 'Add Friend']) {
        await overlay(
          page,
          'friends',
          `tab ${tab}`,
          async () => {
            await tap(
              page
                .getByRole('tab', { name: tab, exact: true })
                .or(page.getByRole('button', { name: tab, exact: true })),
            );
          },
          () => page.locator('section.lng-friends'),
        );
      }
    } catch (e) {
      notes.push(`friends: not opened (${String(e).split('\n')[0]?.slice(0, 140)})`);
    }

    // Message Requests.
    try {
      await go(page, '/channels/@me', page.locator('.lng-left'));
      await tap(
        page
          .getByRole('button', { name: /Message Requests/ })
          .or(page.getByRole('link', { name: /Message Requests/ })),
      );
      await settle(page, 2500);
      const sec = page.getByRole('region', { name: 'Message Requests' });
      await sec.first().waitFor({ timeout: 8000 });
      await pageCensus(page, 'message-requests', sec);
    } catch (e) {
      notes.push(`message-requests: not opened (${String(e).split('\n')[0]?.slice(0, 140)})`);
    }

    // A server's list pane: the server rail and the channel sidebar (back from a channel).
    const curl = `/channels/${w.guildId}/${w.channelId}`;
    await go(page, curl, page.getByRole('log'));
    await tap(page.getByRole('button', { name: 'Back to channels' }));
    await settle(page, 2500);
    await census(page, 'server-list', 'page', leftPane());
    await shot(page, 'server-list');
    covered.push(
      'server-list / page (the server rail, the channel sidebar, the user panel, the tab bar)',
    );
    await overlay(
      page,
      'server-list',
      'server menu',
      async () => {
        await tap(page.locator('.lng-left-col').getByRole('button', { name: /The Back Booth/ }));
      },
      () => page.getByRole('menu').last(),
      { also: MENU_ITEM_SELECTOR },
    );
    await overlay(
      page,
      'server-list',
      'add a server dialog',
      async () => {
        await tap(page.getByRole('button', { name: 'Add a Server' }));
      },
      () => dialog(page),
    );

    // The notification inbox: the header's Inbox button, then the tab bar's Notifications.
    await go(page, curl, page.getByRole('log'));
    await overlay(
      page,
      'inbox',
      'header inbox',
      async () => {
        await openMore(page);
        await tap(page.getByRole('button', { name: /^Inbox/ }));
      },
      () => dialog(page).or(page.getByRole('region', { name: /Inbox/ })),
      { reload: curl },
    );
    await overlay(
      page,
      'inbox',
      'tab bar notifications',
      async () => {
        await tap(page.locator('nav.lng-tabbar').getByRole('button', { name: /Notifications/ }));
      },
      () => dialog(page),
      { reload: curl },
    );
    // The "You" tab.
    await overlay(
      page,
      'you-tab',
      'you tab',
      async () => {
        await tap(page.locator('nav.lng-tabbar').getByRole('button', { name: /^You/ }));
      },
      () => page.locator('.lng-left'),
    );
  } finally {
    await ctx.close();
  }
}

// ---- parts 4 and 5: User settings ----------------------------------------------------------------------------------

const OPENERS =
  /^(Edit|Change|Add|Create|New|Delete|Manage|Set|Generate|Enable|Link|Connect|Upload|Customize|Choose|Record|Rename|Invite|View|Reset|Export|Import|Request|Show|Remove|Revoke)/i;

async function settingsPage(browser: Browser, owner: BrowserContext, sections: string[]) {
  for (const s of sections) {
    const { ctx, page } = await newPage(browser, owner);
    const name = `settings-${s}`;
    try {
      await go(page, `/settings/user/${s}`, page.locator('main, [role="main"]'));
      const root = page.locator('main, [role="main"]');
      const has = (await root.count()) > 0;
      if (!has) notes.push(`${name}: no main region, tested the whole page`);
      await census(page, name, 'page', has ? root.first() : page.locator('body'));
      await shot(page, name);
      covered.push(`${name} / page`);

      // One dialog: the first control of the page that opens one.
      const r = has ? root.first() : page.locator('body');
      const btns = await r.getByRole('button').all();
      let tried = 0;
      let done = false;
      for (const b of btns) {
        if (done || tried >= 4) break;
        const label = (
          (await b.getAttribute('aria-label')) ??
          (await b.textContent()) ??
          ''
        ).trim();
        if (!OPENERS.test(label) || !(await b.isVisible()) || !(await b.isEnabled())) continue;
        tried++;
        try {
          await b.scrollIntoViewIfNeeded({ timeout: 3000 });
          await b.click({ timeout: 4000 });
          await page.getByRole('dialog').last().waitFor({ timeout: 3500 });
          await settle(page, 2000);
          await census(
            page,
            name,
            `dialog "${label.slice(0, 30)}"`,
            page.getByRole('dialog').last(),
          );
          await shot(page, `${name}-dialog`);
          covered.push(`${name} / dialog "${label.slice(0, 30)}"`);
          done = true;
          await page.keyboard.press('Escape');
        } catch {
          await page.keyboard.press('Escape').catch(() => undefined);
          await page.waitForTimeout(300);
        }
      }
      if (!done) notes.push(`${name}: no dialog opened (${tried} candidate buttons tried)`);
    } catch (e) {
      notes.push(`${name}: failed (${String(e).split('\n')[0]?.slice(0, 160)})`);
    } finally {
      await ctx.close();
    }
  }
}

test(`wave 106 toucha: the 44 px census, part ${PART}`, async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const w = world();
  try {
    await apiPatch(ownerCtx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-light', zoom: 100 } },
    });
    if (PART === '1' || PART === '12') await part1(browser, ownerCtx, w);
    if (PART === '2' || PART === '12') await part2(browser, ownerCtx, w);
    if (PART === '3') await part3(browser, ownerCtx, w);
    if (PART === '4') await settingsPage(browser, ownerCtx, USER_SETTINGS.slice(0, 10));
    if (PART === '5') await settingsPage(browser, ownerCtx, USER_SETTINGS.slice(10));
  } finally {
    writeFileSync(
      resolve(OUT, `figures-part${PART}.json`),
      JSON.stringify({ phase: PHASE, part: PART, rows: figs, notes, covered }, null, 1),
    );
    for (const n of notes) say(`note: ${n}`);
    const fails = figs.filter((f) => f.status === 'fail').length;
    const nr = figs.filter((f) => f.status === 'not-reached');
    say(`part ${PART}: ${figs.length} rows, ${fails} fail, ${nr.length} not-reached`);
    for (const r of nr)
      say(`not-reached: ${r.page} / ${r.state} ${r.tag} "${r.name}" ${r.reason ?? ''}`);
    await ownerCtx.close();
  }
});
