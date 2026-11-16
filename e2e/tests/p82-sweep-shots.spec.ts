import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { apiPatch, apiPost, test } from '../support/fixtures';

/**
 * SEC-AUD-342 (wave 82; D-848): every signed-in page outside the rooms, measured for sideways scroll at 375x812
 * (a phone context with touch) and 1280x800. The Owner's account, main theme Lounge Dark (set with the account's
 * settings and localStorage, read back). It reuses the visual sweep's route list and its seeded world
 * (`e2e/.state/sweep/world.json`, made by `page-sweep.spec.ts`'s "seed the sweep world"; this spec never edits that
 * spec), and walks the section list of Settings (member's and server's) and Admin from the page itself.
 *
 * Per route and size it logs one JSON line to `measure.json`: `documentElement.scrollWidth - clientWidth`, the same for
 * the page's main scroller, and, when either is over 0, the elements whose right edge is past the viewport (deepest
 * first), plus a shot of the viewport. A box that scrolls sideways inside itself is not a fault: elements inside a
 * clipping ancestor whose own edge is inside the viewport are left out. Out of scope on purpose: the Library, the
 * Cinema, the Arcade, the Study (waves 78-81) and the Developers pages (the developers builder).
 *
 * Local only (SR_P82_SWEEP=1); shots and measure.json go to SR_P82_SWEEP_OUT; SR_P82_ONLY is a regex over route ids.
 */
const ON = process.env.SR_P82_SWEEP === '1';
const OUT =
  process.env.SR_P82_SWEEP_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-sweep';
const ONLY = process.env.SR_P82_ONLY ? new RegExp(process.env.SR_P82_ONLY) : null;
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const THEME = 'lounge-dark';
const PHONE_UA =
  'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36';
const SIZES = {
  phone: { width: 375, height: 812 },
  desktop: { width: 1280, height: 800 },
} as const;
type Size = keyof typeof SIZES;
/** The limit for one route (the brief: 20 s). */
const LIMIT_MS = 20_000;

test.skip(!ON, 'local visual check only');
test.setTimeout(1_800_000);

interface World {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
  messageId: string;
  threadId: string;
  dmId: string | null;
  appId: string | null;
  inviteCode: string | null;
  memberName: string;
}
const loadWorld = (): World => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as World;

interface Offender {
  tag: string;
  cls: string;
  box: number[];
  text: string;
  up: string[];
}
interface Measure {
  doc: number;
  body: number;
  scroller: string | null;
  scrollerOver: number;
  scrollers: { sel: string; over: number }[];
  vw: number;
  theme: string | null;
  offenders: Offender[];
}

/** In the page: the sideways scroll of the document and of the main scroller, and what sticks out past the edge. */
function measureInPage(): Measure {
  const de = document.documentElement;
  const vw = de.clientWidth;
  const sel = (e: Element) => {
    const cls = typeof e.className === 'string' ? e.className.trim().split(/\s+/).slice(0, 3) : [];
    return `${e.tagName.toLowerCase()}${cls.length ? `.${cls.join('.')}` : ''}`;
  };
  const area = (e: Element) => e.clientWidth * e.clientHeight;
  const vp = vw * de.clientHeight;
  // Scrollers: anything that scrolls vertically (or both) and fills a good part of the viewport, plus <main>.
  const cands = Array.from(document.querySelectorAll<HTMLElement>('body *')).filter((e) => {
    if (e.clientWidth === 0) return false;
    const cs = getComputedStyle(e);
    if (cs.display === 'none') return false;
    const y = /(auto|scroll|overlay)/.test(cs.overflowY);
    return (y && area(e) >= vp * 0.2) || e.tagName === 'MAIN';
  });
  cands.sort((a, b) => area(b) - area(a));
  const scrollers = cands
    .map((e) => ({ el: e, sel: sel(e), over: e.scrollWidth - e.clientWidth }))
    .filter((s) => s.over > 0);
  const main = cands[0] ?? null;
  const scrollerOver = main ? main.scrollWidth - main.clientWidth : 0;
  const doc = de.scrollWidth - vw;
  const body = (document.body?.scrollWidth ?? 0) - vw;
  const offenders: Offender[] = [];
  if (doc > 0 || scrollerOver > 0 || body > 0) {
    const edge =
      doc > 0 || body > 0 || !main ? vw : Math.min(vw, main.getBoundingClientRect().right);
    const clipped = (e: Element) => {
      for (let a = e.parentElement; a && a !== document.body; a = a.parentElement) {
        const cs = getComputedStyle(a);
        if (cs.overflowX !== 'visible' && a.getBoundingClientRect().right <= edge + 1) return true;
      }
      return false;
    };
    const depth = (e: Element) => {
      let d = 0;
      for (let a = e.parentElement; a; a = a.parentElement) d++;
      return d;
    };
    const past = Array.from(document.querySelectorAll<HTMLElement>('body *'))
      .filter((e) => {
        const r = e.getBoundingClientRect();
        if (r.width === 0 || r.height === 0 || r.right <= edge + 1) return false;
        const cs = getComputedStyle(e);
        if (cs.display === 'none' || cs.visibility === 'hidden') return false;
        return !clipped(e);
      })
      .map((e) => ({ e, d: depth(e), r: e.getBoundingClientRect().right }));
    past.sort((a, b) => b.d - a.d || b.r - a.r);
    for (const { e } of past.slice(0, 14)) {
      const r = e.getBoundingClientRect();
      const up: string[] = [];
      for (let a = e.parentElement, n = 0; a && n < 3; a = a.parentElement, n++) up.push(sel(a));
      offenders.push({
        tag: e.tagName.toLowerCase(),
        cls: typeof e.className === 'string' ? e.className.slice(0, 80) : '',
        box: [Math.round(r.left), Math.round(r.right), Math.round(r.width)],
        text: (e.textContent ?? '').trim().slice(0, 24),
        up,
      });
    }
  }
  const themed = document.querySelector('[data-theme]');
  return {
    doc,
    body,
    scroller: main ? sel(main) : null,
    scrollerOver,
    scrollers: scrollers.slice(0, 5).map((s) => ({ sel: s.sel, over: s.over })),
    vw,
    theme: themed?.getAttribute('data-theme') ?? null,
    offenders,
  };
}

const PNG = (() => {
  const dir = resolve(repoRoot, 'apps', 'web', 'public', 'icons');
  const f = existsSync(dir)
    ? ['icon-192.png', 'icon-512.png'].find((n) => existsSync(resolve(dir, n)))
    : undefined;
  return f ? readFileSync(resolve(dir, f)) : Buffer.alloc(0);
})();

/** As the sweep does: the message marked `[files]` gets a picture and a file (object storage is not on this stack). */
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
  await page.route(/\/cdn\/attachments\/sweep\/|sweep-preview\.invalid|\/cdn\/external\//, (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: PNG }),
  );
}

/** A real finger swipe to the member list pane (the DevTools protocol's touch events, as p69's spec does). */
async function swipeToMembers(page: Page) {
  const cdp = await page.context().newCDPSession(page);
  const touch = (type: 'touchStart' | 'touchMove' | 'touchEnd', x: number) =>
    cdp.send('Input.dispatchTouchEvent', {
      type,
      touchPoints: type === 'touchEnd' ? [] : [{ x, y: 400 }],
    });
  await touch('touchStart', 330);
  for (const x of [280, 220, 160, 100, 50]) await touch('touchMove', x);
  await touch('touchEnd', 50);
  await cdp.detach();
}

interface Step {
  /** Route id (the row's name in the table). */
  id: string;
  /** Path to open; a step with no path continues on the page the previous step left (same group). */
  path: (w: World) => string | null;
  /** What to do after the page settles (open a panel, press a tab). */
  act?: (page: Page, size: Size, w: World) => Promise<void>;
}

const idle = async (page: Page, ms = 3000) => {
  await page.waitForLoadState('networkidle', { timeout: ms }).catch(() => undefined);
  await page.waitForTimeout(500);
};
const press = async (page: Page, name: RegExp, role: 'button' | 'link' | 'tab' = 'button') => {
  await page.getByRole(role, { name }).first().click({ timeout: 5000 });
  await idle(page, 2500);
};

const STEPS: Step[] = [
  // ---- the Lounge: a server's channel (long word, code block, wide table, attached picture), a DM, a thread
  { id: 'lounge-home', path: () => '/channels/@me' },
  {
    id: 'lounge-friends',
    path: () => '/channels/@me',
    act: (p) => press(p, /^Friends/, 'link'),
  },
  {
    id: 'lounge-message-requests',
    path: () => '/channels/@me',
    act: (p) => press(p, /^Message Requests/),
  },
  { id: 'lounge-dm', path: (w) => (w.dmId ? `/channels/@me/${w.dmId}` : null) },
  {
    id: 'lounge-channel',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: async (p) => {
      // Messages are loaded and the list is at the newest; the member list stays as the page opens it.
      await p.waitForTimeout(600);
    },
  },
  {
    id: 'lounge-channel-members-open',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: async (p, size) => {
      const b = p.getByRole('button', { name: /Member List/ }).first();
      if ((await b.getAttribute('aria-pressed')) !== 'true') await b.click({ timeout: 5000 });
      await p.waitForTimeout(500);
      if (size === 'phone') await swipeToMembers(p);
      await p.waitForTimeout(500);
    },
  },
  {
    id: 'lounge-channel-members-closed',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: async (p) => {
      const b = p.getByRole('button', { name: /Member List/ }).first();
      if ((await b.getAttribute('aria-pressed')) === 'true') await b.click({ timeout: 5000 });
      await p.waitForTimeout(500);
    },
  },
  {
    id: 'lounge-thread',
    path: (w) => `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`,
  },
  {
    id: 'lounge-thread-members-open',
    path: (w) => `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`,
    act: async (p) => {
      const b = p.getByRole('button', { name: /Member List/ }).first();
      if ((await b.count()) && (await b.getAttribute('aria-pressed')) !== 'true')
        await b.click({ timeout: 5000 });
      await p.waitForTimeout(500);
    },
  },
  {
    id: 'lounge-inbox',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: (p) => press(p, /^Inbox$/),
  },
  {
    id: 'lounge-inbox-unreads',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: async (p) => {
      await press(p, /^Inbox$/);
      await press(p, /Unreads/, 'tab').catch(() => press(p, /Unreads/));
    },
  },
  {
    id: 'lounge-inbox-foryou',
    path: (w) => `/channels/${w.guildId}/${w.channelId}`,
    act: async (p) => {
      await press(p, /^Inbox$/);
      await press(p, /For you/i, 'tab').catch(() => press(p, /For you/i));
    },
  },
  {
    id: 'lounge-message-link',
    path: (w) => `/channels/${w.guildId}/${w.channelId}/${w.messageId}`,
  },
  { id: 'lounge-scrapbook', path: (w) => `/channels/${w.guildId}/scrapbook` },
  {
    id: 'lounge-voice-channel',
    path: (w) => (w.voiceChannelId ? `/channels/${w.guildId}/${w.voiceChannelId}` : null),
  },
  // ---- the rest of the sweep's signed-in list outside the rooms and the Developers pages
  { id: 'app-door', path: () => '/app' },
  {
    id: 'settings-server-channel-permissions',
    path: (w) => `/settings/server/${w.guildId}/channels?channel=${w.channelId}&tab=permissions`,
  },
  {
    id: 'settings-server-channel-voice',
    path: (w) =>
      w.voiceChannelId
        ? `/settings/server/${w.guildId}/channels?channel=${w.voiceChannelId}&tab=voice`
        : null,
  },
  { id: 'admin-root', path: () => '/admin' },
  {
    id: 'admin-review-applications',
    path: () => '/admin/review',
    act: (p) => press(p, /All applications/, 'tab'),
  },
  {
    id: 'oauth2-authorize',
    path: (w) =>
      w.appId
        ? `/oauth2/authorize?client_id=${w.appId}&scope=${encodeURIComponent('bot applications.commands')}&permissions=2048`
        : null,
  },
  { id: 'oauth2-authorized', path: () => '/oauth2/authorized' },
  { id: 'support', path: () => '/support' },
  { id: 'support-leaderboards', path: () => '/support/leaderboards' },
  { id: 'profile-u-owner', path: () => '/u/owner' },
  { id: 'profile-tilde-owner', path: () => '/~owner' },
  { id: 'profile-member', path: (w) => `/u/${w.memberName}` },
  { id: 'profile-page-edit', path: () => '/profile-page/edit' },
  { id: 'help', path: () => '/help' },
  { id: 'help-whats-new', path: () => '/help/whats-new' },
  { id: 'help-operators', path: () => '/help/operators' },
  { id: 'share', path: () => '/share' },
  { id: 'storage-mount', path: () => '/storage/123456' },
  { id: 'invite-app', path: (w) => (w.inviteCode ? `/invite/${w.inviteCode}` : null) },
  { id: 'template-app', path: () => '/template/abcd1234' },
  { id: 'app-404', path: () => '/channels/@me/does/not/exist/here' },
  { id: 'app-unknown-route', path: () => '/nothing-lives-here' },
];

/** The section lists of the three settings shells: read from the page, never from a list of ours. */
const WALKS: { kind: string; path: (w: World) => string }[] = [
  { kind: 'settings-user', path: () => '/settings/user/account' },
  { kind: 'settings-server', path: (w) => `/settings/server/${w.guildId}` },
  { kind: 'admin', path: () => '/admin/dashboard' },
];

const slug = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '')
    .slice(0, 40);

async function withLimit<T>(what: string, fn: () => Promise<T>): Promise<T> {
  let timer: NodeJS.Timeout | undefined;
  try {
    return await Promise.race([
      fn(),
      new Promise<never>((_, rej) => {
        timer = setTimeout(() => rej(new Error(`${what}: over ${LIMIT_MS} ms`)), LIMIT_MS);
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

async function newPage(browser: Browser, size: Size, base: BrowserContext) {
  const phone = size === 'phone';
  const ctx = await browser.newContext({
    baseURL: ORIGIN,
    viewport: SIZES[size],
    storageState: await base.storageState(),
    bypassCSP: true,
    ...(phone ? { userAgent: PHONE_UA, isMobile: true, hasTouch: true } : {}),
  });
  await ctx.addInitScript((theme) => {
    try {
      localStorage.setItem(
        'sr.user.prefs',
        JSON.stringify({ state: { prefs: { theme, zoom: 100 } }, version: 2 }),
      );
    } catch {
      /* no storage */
    }
  }, THEME);
  const page = await ctx.newPage();
  page.setDefaultTimeout(8000);
  await decorate(page);
  return { ctx, page };
}

test('SEC-AUD-342: every signed-in page outside the rooms, measured for sideways scroll', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const jsonl = resolve(OUT, 'measure.json');
  if (!ONLY) writeFileSync(jsonl, '');
  const world = loadWorld();
  const ownerCtx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  const o = ownerCtx.request;
  // The main theme: the account's settings and localStorage (set in `newPage`), read back.
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: THEME, zoom: 100 } },
  });
  const back = (await (await o.get('/api/v1/users/@me/settings')).json()) as {
    settings?: { user_prefs?: { theme?: string } };
  };
  console.log(
    `REPORT account theme read back: ${back.settings?.user_prefs?.theme ?? JSON.stringify(back).slice(0, 120)}`,
  );

  // The states the sweep's world lacks: a wide Markdown table, a long code line and a long link, in the channel
  // and in the DM (once: they are recognised by their first words on a second run).
  const seen = async (channelId: string, needle: string) => {
    const r = await o.get(`/api/v1/channels/${channelId}/messages?limit=100`);
    return r.ok() && (await r.text()).includes(needle);
  };
  const extras = [
    {
      key: 'p82 wide table',
      content:
        'p82 wide table\n\n| Time slot | Film | Director | Snacks and the long note column | Another wide column of words | Seats left |\n|---|---|---|---|---|---|\n| Friday 9 pm | Heat (1995) | Michael Mann | Popcorn, the good kind, and the salted caramel | Doors open at eight, the projector warms up | 14 |\n| Saturday 7 pm | Collateral (2004) | Michael Mann | Nachos with the extra jalapeno slices and cheese | Bring a blanket, the back room is cold | 6 |',
    },
    {
      key: 'p82 long code',
      content:
        'p82 long code\n```\nconst schedule = buildSchedule({ guild: "the-back-booth", night: "friday", film: "heat", snacks: ["popcorn", "nachos", "pretzels"], doorsOpenAt: "20:00" });\n```',
    },
    {
      key: 'p82 long link',
      content:
        'p82 long link https://example.com/a/very/long/path/that/keeps/going/and/going/with/no/spaces/at/all/and/a/query/string?alpha=1&beta=2&gamma=3&delta=4&epsilon=5&zeta=6&eta=7&theta=8',
    },
  ];
  for (const ch of [world.channelId, world.dmId]) {
    if (!ch) continue;
    for (const x of extras) {
      if (await seen(ch, x.key)) continue;
      await new Promise((ok) => setTimeout(ok, 1300));
      await apiPost(o, `/channels/${ch}/messages`, { content: x.content }).catch((e) =>
        console.log(`REPORT extra message not posted (${x.key}): ${String(e).slice(0, 120)}`),
      );
    }
  }

  const counts: Record<string, { routes: number; over: number; failed: string[] }> = {
    phone: { routes: 0, over: 0, failed: [] },
    desktop: { routes: 0, over: 0, failed: [] },
  };

  for (const size of ['phone', 'desktop'] as Size[]) {
    let { ctx, page } = await newPage(browser, size, ownerCtx);
    const fresh = async () => {
      await ctx.close().catch(() => undefined);
      ({ ctx, page } = await newPage(browser, size, ownerCtx));
    };
    const record = async (id: string, extra: Record<string, unknown> = {}) => {
      await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
      const m = await page.evaluate(measureInPage);
      const over = m.doc > 0 || m.scrollerOver > 0 || m.body > 0;
      let shot: string | null = null;
      if (over) {
        shot = `${id}-${size}.png`;
        await page.screenshot({ path: resolve(OUT, shot), timeout: 10_000 }).catch(() => {
          shot = null;
        });
      }
      const line = {
        id,
        size,
        url: page.url().replace(ORIGIN, ''),
        ok: true,
        ...m,
        shot,
        ...extra,
      };
      appendFileSync(jsonl, `${JSON.stringify(line)}\n`);
      counts[size]!.routes++;
      if (over) counts[size]!.over++;
      console.log(
        `REPORT ${size} ${id}: doc ${m.doc} scroller ${m.scrollerOver} (${m.scroller}) body ${m.body} theme ${m.theme}${over ? ' OVER' : ''}`,
      );
    };
    const fail = async (id: string, e: unknown) => {
      counts[size]!.failed.push(id);
      appendFileSync(
        jsonl,
        `${JSON.stringify({ id, size, ok: false, error: String(e).slice(0, 200) })}\n`,
      );
      console.log(`REPORT ${size} ${id}: NOT MEASURED ${String(e).slice(0, 120)}`);
      await fresh();
    };

    for (const step of STEPS) {
      if (ONLY && !ONLY.test(step.id)) continue;
      const path = step.path(world);
      if (!path) {
        counts[size]!.failed.push(`${step.id} (no data in the world)`);
        appendFileSync(
          jsonl,
          `${JSON.stringify({ id: step.id, size, ok: false, error: 'no data' })}\n`,
        );
        continue;
      }
      try {
        await withLimit(step.id, async () => {
          await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          await idle(page, 4000);
          if (step.act) await step.act(page, size, world);
          await record(step.id);
        });
      } catch (e) {
        await fail(step.id, e);
      }
    }

    for (const walk of WALKS) {
      if (ONLY && !ONLY.test(walk.kind)) continue;
      const path = walk.path(world);
      let labels: string[] = [];
      try {
        await withLimit(`${walk.kind} list`, async () => {
          await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
          await idle(page, 4000);
          labels = await page
            .locator('[role="dialog"] nav ul li > button')
            .evaluateAll((els) => els.map((e) => (e.textContent ?? '').trim()));
        });
      } catch (e) {
        await fail(`${walk.kind}-list`, e);
        continue;
      }
      console.log(`REPORT ${size} ${walk.kind}: ${labels.length} tabs: ${labels.join(' | ')}`);
      for (let i = 0; i < labels.length; i++) {
        const id = `${walk.kind}-${i + 1}-${slug(labels[i] ?? '')}`;
        if (ONLY && !ONLY.test(id) && !ONLY.test(walk.kind)) continue;
        try {
          await withLimit(id, async () => {
            const btns = page.locator('[role="dialog"] nav ul li > button');
            if ((await btns.count()) !== labels.length) {
              // A tab led out of the shell (or the page reloaded): back to the shell.
              await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 });
              await idle(page, 3000);
            }
            await page
              .locator('[role="dialog"] nav ul li > button')
              .nth(i)
              .evaluate((el) => (el as HTMLElement).click());
            await idle(page, 2500);
            await record(id, { tab: labels[i] });
          });
        } catch (e) {
          await fail(id, e);
          await page
            .goto(path, { waitUntil: 'domcontentloaded', timeout: 15_000 })
            .catch(() => undefined);
        }
      }
    }
    await ctx.close().catch(() => undefined);
  }
  await ownerCtx.close();
  console.log(`REPORT totals ${JSON.stringify(counts)}`);
  writeFileSync(resolve(OUT, 'totals.json'), JSON.stringify(counts, null, 1));
});
