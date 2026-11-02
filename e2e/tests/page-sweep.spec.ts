import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Page, Route } from '@playwright/test';
import { repoRoot } from '../support/cli';
import { testActivityBundle } from '../support/activityBundle';
import {
  apiPatch,
  apiPost,
  apiPostBytes,
  apiPut,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';
import { apiDelete, buildWorld, dropServer, soloServer } from '../support/world';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * XC-VIS-001: the visual sweep. Every route of the app (signed in) and of the public surfaces (signed out) is opened
 * at desktop 1280 x 800 and phone 375 x 812, in a light and a dark theme, and screenshotted to a folder outside git.
 * Per capture it records the document status, console errors, page errors, failed requests, error-boundary text, a
 * blank main area, horizontal overflow, interactive elements off screen and text that looks like a raw i18n key,
 * then writes `summary.json` and `summary.md` and one contact sheet per two routes for review.
 *
 * Opt-in: it is not a gate (nothing here fails on what it finds; the findings are the output). Chromium only.
 *   SR_E2E_SWEEP=1 pnpm --filter @sideroom/e2e exec playwright test page-sweep.spec.ts --project chromium
 * Options: SR_SWEEP_OUT (default: <repo>/../visual-sweep-shots), SR_SWEEP_ONLY (regex over route names: re-run a
 * few pages; the summary then merges with the captures already on disk).
 *
 * Themes: "dark" is the default theme a new account sees (Editorial, a dark palette); "light" is Lounge Light chosen
 * for everything. Public pages follow `prefers-color-scheme` where they support it.
 */
const ON = process.env.SR_E2E_SWEEP === '1';
const OUT = process.env.SR_SWEEP_OUT ?? resolve(repoRoot, '..', 'visual-sweep-shots');
const ONLY = process.env.SR_SWEEP_ONLY ? new RegExp(process.env.SR_SWEEP_ONLY) : null;
const DATA = resolve(OUT, 'data');
const STATE = resolve(repoRoot, 'e2e', '.state', 'sweep');
const WORLD_FILE = resolve(STATE, 'world.json');
const OWNER_STATE = resolve(STATE, 'owner.json');
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

const VIEWPORTS = {
  desktop: { width: 1280, height: 800 },
  phone: { width: 375, height: 812 },
} as const;
type Viewport = keyof typeof VIEWPORTS;
const THEMES = ['light', 'dark'] as const;
type Theme = (typeof THEMES)[number];

test.skip(!ON, 'Opt-in sweep: set SR_E2E_SWEEP=1.');
test.use({ bypassCSP: true });

interface SweepWorld {
  guildId: string;
  channelId: string;
  voiceChannelId: string | null;
  messageId: string;
  threadId: string;
  titleId: string;
  partyId: string | null;
  dmId: string | null;
  appId: string | null;
  activityAppId: string | null;
  tvChannelId: string | null;
  inviteCode: string | null;
  memberName: string;
  seedLog: string[];
}

interface Row {
  name: string;
  path: (w: SweepWorld) => string | null;
  /** Requests whose 4xx/5xx is expected on this page (regex over the URL). */
  expect4xx?: RegExp;
  /** Mock the shelves API (no object storage here). */
  shelves?: boolean;
}

// ---- the routes ----------------------------------------------------------------------------------------------------

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
const SERVER_SETTINGS = [
  'overview',
  'roles',
  'support',
  'tag',
  'channels',
  'emoji',
  'members',
  'invites',
  'bans',
  'features',
  'extensions',
  'watch-library',
  'storage',
  'integrations',
  'bridge',
  'gameservers',
  'moderation',
  'audit-log',
  'community',
  'delete',
];
const ADMIN = [
  'dashboard',
  'accounts',
  'bans',
  'moderation',
  'data-requests',
  'storage',
  'invites',
  'invite-graph',
  'servers',
  'staff',
  'review',
  'jobs',
  'ingest-sources',
  'library-catalog',
  'cinema-catalog',
  'cinema-collections',
  'cinema-transcoding',
  'cinema-live-tv',
  'donations',
  'donation-invoices',
  'donation-entitlements',
  'donation-pricing',
  'features',
];
const APP_TABS = ['general', 'oauth2', 'bot', 'installation', 'commands', 'review'];

const SIGNED_IN: Row[] = [
  { name: 'lounge-home', path: () => '/channels/@me' },
  { name: 'lounge-dm', path: (w) => (w.dmId ? `/channels/@me/${w.dmId}` : null) },
  { name: 'lounge-channel', path: (w) => `/channels/${w.guildId}/${w.channelId}` },
  {
    name: 'lounge-message-link',
    path: (w) => `/channels/${w.guildId}/${w.channelId}/${w.messageId}`,
  },
  {
    name: 'lounge-thread',
    path: (w) => `/channels/${w.guildId}/${w.channelId}/threads/${w.threadId}`,
  },
  { name: 'lounge-scrapbook', path: (w) => `/channels/${w.guildId}/scrapbook` },
  {
    name: 'lounge-voice',
    path: (w) => (w.voiceChannelId ? `/channels/${w.guildId}/${w.voiceChannelId}` : null),
  },
  { name: 'app-door', path: () => '/app' },
  ...USER_SETTINGS.map((s): Row => ({
    name: `settings-user-${s}`,
    path: () => `/settings/user/${s}`,
  })),
  { name: 'settings-server', path: (w) => `/settings/server/${w.guildId}` },
  ...SERVER_SETTINGS.map((s): Row => ({
    name: `settings-server-${s}`,
    path: (w) => `/settings/server/${w.guildId}/${s}`,
  })),
  { name: 'cinema-home', path: () => '/cinema' },
  { name: 'cinema-movies', path: () => '/cinema/movies' },
  { name: 'cinema-series', path: () => '/cinema/series' },
  { name: 'cinema-anime', path: () => '/cinema/anime' },
  { name: 'cinema-search', path: () => '/cinema/search?q=film' },
  { name: 'cinema-title', path: (w) => `/cinema/t/${w.titleId}` },
  { name: 'cinema-submit', path: () => '/cinema/submit' },
  {
    name: 'cinema-player',
    path: (w) => `/cinema/watch/${w.titleId}`,
    expect4xx: /\/cdn\/|m3u8|playback/,
  },
  { name: 'cinema-my-list', path: () => '/cinema/my-list' },
  { name: 'cinema-tv', path: () => '/cinema/tv' },
  {
    name: 'cinema-tv-watch',
    path: (w) => (w.tvChannelId ? `/cinema/tv/watch/${w.tvChannelId}` : null),
    expect4xx: /m3u8|\/live\//,
  },
  { name: 'cinema-party', path: (w) => (w.partyId ? `/cinema/party/${w.partyId}` : null) },
  { name: 'cinema-shelves', path: () => '/cinema/shelves', shelves: true },
  { name: 'library-home', path: () => '/library' },
  { name: 'library-browse', path: () => '/library/browse' },
  { name: 'library-submissions', path: () => '/library/submissions' },
  { name: 'library-shelves', path: () => '/library/shelves', shelves: true },
  { name: 'study', path: () => '/study', shelves: true },
  { name: 'arcade', path: () => '/games' },
  { name: 'emulator', path: () => '/games/emulator' },
  { name: 'admin', path: () => '/admin' },
  ...ADMIN.map((s): Row => ({ name: `admin-${s}`, path: () => `/admin/${s}` })),
  { name: 'developers', path: () => '/developers' },
  { name: 'developers-applications', path: () => '/developers/applications' },
  ...APP_TABS.map((tab): Row => ({
    name: `developers-app-${tab}`,
    path: (w) => (w.appId ? `/developers/applications/${w.appId}?tab=${tab}` : null),
  })),
  { name: 'developers-packages', path: () => '/developers/packages' },
  { name: 'developers-docs', path: () => '/developers/docs' },
  { name: 'developers-doc-page', path: () => '/developers/docs/getting-started' },
  {
    name: 'oauth2-authorize',
    path: (w) =>
      w.appId
        ? `/oauth2/authorize?client_id=${w.appId}&scope=${encodeURIComponent('bot applications.commands')}&permissions=2048`
        : null,
  },
  { name: 'oauth2-authorized', path: () => '/oauth2/authorized' },
  { name: 'support', path: () => '/support' },
  { name: 'support-leaderboards', path: () => '/support/leaderboards' },
  { name: 'profile-u', path: () => '/u/owner' },
  { name: 'profile-tilde-app', path: () => '/~owner' },
  { name: 'profile-member', path: (w) => `/u/${w.memberName}` },
  { name: 'profile-page-edit', path: () => '/profile-page/edit' },
  { name: 'help', path: () => '/help' },
  { name: 'help-whats-new', path: () => '/help/whats-new' },
  { name: 'help-operators', path: () => '/help/operators' },
  { name: 'share', path: () => '/share' },
  { name: 'storage-mount', path: () => '/storage/123456', expect4xx: /storage\/mounts/ },
  {
    name: 'invite-app',
    path: (w) => (w.inviteCode ? `/invite/${w.inviteCode}` : null),
  },
  { name: 'template-app', path: () => '/template/abcd1234', expect4xx: /templates?\// },
  { name: 'app-404', path: () => '/channels/@me/does/not/exist/here' },
  { name: 'app-unknown-route', path: () => '/nothing-lives-here' },
];

/** Signed out: the public surfaces (D-385), the legal and error pages. */
const SIGNED_OUT: Row[] = [
  { name: 'pub-landing', path: () => '/' },
  { name: 'pub-join', path: () => '/join/abcd1234' },
  { name: 'pub-login', path: () => '/login' },
  { name: 'pub-login-2fa', path: () => '/login/2fa' },
  { name: 'pub-login-link', path: () => '/login/link' },
  { name: 'pub-register', path: () => '/register' },
  { name: 'pub-forgot', path: () => '/forgot' },
  { name: 'pub-reset', path: () => '/reset/dummytoken', expect4xx: /reset/ },
  { name: 'pub-verify-email', path: () => '/verify-email/dummytoken', expect4xx: /verify/ },
  { name: 'pub-email-change-cancel', path: () => '/email-change/cancel', expect4xx: /email/ },
  { name: 'pub-discord-callback', path: () => '/oauth/discord/callback', expect4xx: /discord/ },
  { name: 'pub-invite-real', path: (w) => (w.inviteCode ? `/invite/${w.inviteCode}` : null) },
  { name: 'pub-invite-bad', path: () => '/invite/abcd1234', expect4xx: /invites?\// },
  { name: 'pub-template', path: () => '/template/abcd1234', expect4xx: /templates?\// },
  { name: 'pub-no-invitation', path: () => '/no-invitation' },
  { name: 'pub-profile', path: () => '/~owner', expect4xx: /profile/ },
  { name: 'pub-profile-missing', path: () => '/~nobody-here', expect4xx: /profile|users/ },
  {
    name: 'pub-play',
    path: () => `/play/${'ab12cd34'.repeat(8)}`,
    expect4xx: /guest|play/,
  },
  { name: 'pub-play-bad', path: () => '/play/not-a-token', expect4xx: /guest|play/ },
  { name: 'pub-house-notes', path: () => '/house-notes' },
  { name: 'pub-404-html', path: () => '/404.html' },
  { name: 'pub-50x-html', path: () => '/50x.html' },
  { name: 'pub-offline-html', path: () => '/offline.html' },
  { name: 'pub-unknown', path: () => '/login/nothing-here', expect4xx: /./ },
  { name: 'pub-app-route-signed-out', path: () => '/channels/@me', expect4xx: /./ },
];

/** With the Showroom switch on and the preview turned on for this browser (UX-SHOW-001). */
const SHOWROOM: Row[] = [
  { name: 'showroom-home', path: () => '/channels/@me' },
  { name: 'showroom-channel', path: (w) => `/channels/${w.guildId}/${w.channelId}` },
  { name: 'showroom-study', path: () => '/study' },
  { name: 'showroom-arcade', path: () => '/games' },
  { name: 'showroom-cinema', path: () => '/cinema' },
  { name: 'showroom-settings', path: () => '/settings/user/appearance' },
];

// ---- helpers -------------------------------------------------------------------------------------------------------

const loadWorld = (): SweepWorld => JSON.parse(readFileSync(WORLD_FILE, 'utf8')) as SweepWorld;
const want = (name: string) => !ONLY || ONLY.test(name);

async function csrfHeaders(r: APIRequestContext) {
  const res = await r.get('/api/v1/sr/csrf', { headers: { Origin: ORIGIN } });
  const token = ((await res.json()) as { token: string }).token;
  return { Origin: ORIGIN, 'X-SR-CSRF': token };
}

/** A small PNG for the injected attachments and link previews (the server's storage is in memory here). */
const PNG = (() => {
  const dir = resolve(repoRoot, 'apps', 'web', 'public', 'icons');
  const f = existsSync(dir) ? readdirSync(dir).find((n) => n.endsWith('.png')) : undefined;
  return f ? readFileSync(resolve(dir, f)) : Buffer.alloc(0);
})();

/**
 * The message list answers with the seeded messages; the one marked `[files]` gets an image and a file attachment,
 * because uploads need object storage this stack does not have. Attachment and preview images are served here.
 */
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
  // Embed images go through the media proxy (/cdn/external/<sig>/<base64 url>), which cannot reach `.invalid`.
  await page.route(/sweep-preview\.invalid|\/cdn\/external\//, (r) =>
    r.fulfill({ status: 200, contentType: 'image/png', body: PNG }),
  );
}

/** The shelves of The Study, as study-shelves.spec.ts fakes them (BYOS needs a real bucket). */
async function fakeShelves(page: Page) {
  const json = (r: Route, body: unknown) =>
    r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });
  const files: Record<string, string[]> = {
    '9501': [
      'Films/heat.mp4',
      'Music/blue.mp3',
      'Books/dune.epub',
      'Films/a-very-long-file-name-that-should-wrap-or-truncate-cleanly.mkv',
    ],
    '9502': ['Shared/akira.cbz'],
  };
  await page.route('**/api/v1/shelves/visible', (r) =>
    json(r, {
      shelves: [
        { shelf_id: '9501', label: 'Home', via: 'own', object_count: 4, can_manage: true },
        {
          shelf_id: '9502',
          label: 'Friend',
          via: 'friend',
          owner_id: '777',
          owner_handle: 'pal',
          object_count: 1,
          can_manage: false,
        },
      ],
    }),
  );
  await page.route(/\/api\/v1\/shelves\/\d+\/objects(\?.*)?$/, (r) => {
    const shelf = new URL(r.request().url()).pathname.split('/')[4] ?? '';
    return json(r, {
      objects: (files[shelf] ?? []).map((path, i) => ({
        object_hash: `${shelf}${i}`.padEnd(64, '0'),
        path,
        size: 1000 * (i + 1),
        class: 'other',
        title: '',
        cover: '',
        note: '',
        updated_at: '2026-10-01T00:00:00Z',
      })),
      next_cursor: null,
      can_edit: true,
    });
  });
}

interface Capture {
  route: string;
  url: string;
  viewport: Viewport;
  theme: Theme;
  signedIn: boolean;
  showroom: boolean;
  file: string;
  status: number | null;
  finalUrl: string;
  consoleErrors: string[];
  pageErrors: string[];
  failedRequests: string[];
  errorBoundary: string | null;
  blank: boolean;
  overflowX: number;
  offscreen: string[];
  rawKeys: string[];
  notes: string[];
}

const CHECKS = () => {
  const vw = window.innerWidth;
  const out = {
    overflowX: Math.max(document.documentElement.scrollWidth, document.body?.scrollWidth ?? 0) - vw,
    blank: false,
    errorBoundary: null as string | null,
    offscreen: [] as string[],
    rawKeys: [] as string[],
  };
  const main =
    document.querySelector('main') ?? document.querySelector('[role="main"]') ?? document.body;
  const text = (main as HTMLElement).innerText?.trim() ?? '';
  const media = main.querySelectorAll('img,svg,canvas,video,iframe').length;
  out.blank = text.length === 0 && media === 0;
  const body = document.body?.innerText ?? '';
  const m =
    /Something went wrong!?|Application error|Unexpected Application Error|Minified React error/.exec(
      body,
    );
  out.errorBoundary = m ? m[0] : null;
  // Interactive elements that are visible but sit outside the viewport horizontally (phone layouts).
  const seen = new Set<string>();
  for (const el of Array.from(
    document.querySelectorAll<HTMLElement>(
      'a,button,input,select,textarea,[role="button"],[role="tab"]',
    ),
  )) {
    const r = el.getBoundingClientRect();
    if (r.width === 0 || r.height === 0) continue;
    const cs = getComputedStyle(el);
    if (cs.visibility === 'hidden' || cs.display === 'none' || Number(cs.opacity) === 0) continue;
    // An inert subtree (an off-screen phone pane) is out of the tab order and the accessibility tree.
    if (el.closest('[inert]')) continue;
    // Inside a horizontal scroller is fine (tab strips, carousels).
    let p: HTMLElement | null = el.parentElement;
    let scroller = false;
    while (p && p !== document.body) {
      const ps = getComputedStyle(p);
      if (/(auto|scroll|hidden|clip)/.test(ps.overflowX) && p.scrollWidth > p.clientWidth + 1) {
        scroller = true;
        break;
      }
      p = p.parentElement;
    }
    if (scroller) continue;
    // Skip links park far off screen on purpose until focused; a closed off-canvas drawer sits just off the edge.
    if (r.right < -1000) continue;
    if (r.right > vw + 2 || r.left < -2) {
      const label = (el.getAttribute('aria-label') ?? el.textContent ?? el.tagName)
        .trim()
        .slice(0, 40);
      const key = `${el.tagName.toLowerCase()} "${label}" x=${Math.round(r.left)}..${Math.round(r.right)}`;
      if (!seen.has(key)) seen.add(key);
    }
  }
  out.offscreen = [...seen].slice(0, 12);
  // Text that looks like a catalog key: dotted lower-camel segments, at least three, not a URL or file name.
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const keys = new Set<string>();
  const re = /(^|[\s(])([a-z][a-zA-Z0-9]*(?:\.[a-z][a-zA-Z0-9]*){2,})(?=$|[\s),.:;])/g;
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const parent = n.parentElement;
    if (!parent || parent.closest('script,style,code,pre,[contenteditable="true"],textarea'))
      continue;
    const t = n.textContent ?? '';
    for (const mm of t.matchAll(re)) {
      const k = mm[2]!;
      if (
        /\.(com|org|net|test|invalid|png|jpe?g|pdf|md|json|js|ts|mp4|mp3|zip|html|txt|epub|cbz|mkv|example)$/i.test(
          k,
        )
      )
        continue;
      if (/^(www|api|cdn|platform|guild|lounge|e\.g|i\.e)\./.test(k) && !/[A-Z]/.test(k)) {
        // platform.* switch ids and similar are shown on purpose in admin; keep only camelCase-looking keys.
        continue;
      }
      keys.add(k);
    }
  }
  out.rawKeys = [...keys].slice(0, 12);
  return out;
};

async function capture(
  browser: Browser,
  row: Row,
  world: SweepWorld,
  opts: { viewport: Viewport; theme: Theme; signedIn: boolean; showroom: boolean },
): Promise<void> {
  const path = row.path(world);
  const file = `${row.name}-${opts.viewport}-${opts.theme}.png`;
  const rec: Capture = {
    route: row.name,
    url: path ?? '',
    viewport: opts.viewport,
    theme: opts.theme,
    signedIn: opts.signedIn,
    showroom: opts.showroom,
    file,
    status: null,
    finalUrl: '',
    consoleErrors: [],
    pageErrors: [],
    failedRequests: [],
    errorBoundary: null,
    blank: false,
    overflowX: 0,
    offscreen: [],
    rawKeys: [],
    notes: [],
  };
  if (!path) {
    rec.notes.push('skipped: the world has no data for this route');
    writeFileSync(resolve(DATA, `${file}.json`), JSON.stringify(rec, null, 1));
    return;
  }
  const context: BrowserContext = await browser.newContext({
    baseURL: ORIGIN,
    viewport: VIEWPORTS[opts.viewport],
    colorScheme: opts.theme === 'dark' ? 'dark' : 'light',
    storageState: opts.signedIn ? OWNER_STATE : undefined,
    bypassCSP: true,
  });
  const theme = opts.theme === 'light' ? 'lounge-light' : 'lounge-editorial';
  await context.addInitScript(
    ([t, showroom]) => {
      try {
        localStorage.setItem(
          'sr.user.prefs',
          JSON.stringify({ state: { prefs: { theme: t } }, version: 2 }),
        );
        if (showroom) localStorage.setItem('sr.showroom.v1', '1');
        else localStorage.removeItem('sr.showroom.v1');
      } catch {
        /* no storage */
      }
    },
    [theme, opts.showroom] as const,
  );
  const page = await context.newPage();
  page.on('console', (m) => {
    if (m.type() === 'error') rec.consoleErrors.push(m.text().slice(0, 300));
  });
  page.on('pageerror', (e) => rec.pageErrors.push(String(e).slice(0, 300)));
  page.on('response', (r) => {
    const s = r.status();
    if (s < 400) return;
    const u = r.url();
    if (row.expect4xx?.test(u)) return;
    // "No watch party in this channel" is a 404 by design.
    if (s === 404 && /\/channels\/\d+\/watch-party$/.test(u)) return;
    // Signed out, the public pages ask "who am I" and expect 401.
    if (!opts.signedIn && s === 401 && /\/api\/v1\/(users\/@me|sr\/csrf)$/.test(u)) return;
    rec.failedRequests.push(`${s} ${r.request().method()} ${u.replace(ORIGIN, '')}`.slice(0, 200));
  });
  page.on('requestfailed', (r) => {
    const u = r.url();
    if (/^(data|blob|chrome-extension):/.test(u)) return;
    const why = r.failure()?.errorText ?? '';
    if (/ERR_ABORTED/.test(why)) return;
    rec.failedRequests.push(`FAILED ${u.replace(ORIGIN, '')} ${why}`.slice(0, 200));
  });
  try {
    if (opts.signedIn) await decorate(page);
    if (row.shelves) await fakeShelves(page);
    const res = await page.goto(path, { waitUntil: 'domcontentloaded', timeout: 45_000 });
    rec.status = res?.status() ?? null;
    await page.waitForLoadState('networkidle', { timeout: 6000 }).catch(() => undefined);
    // Let lazy chunks, fonts and the catalog settle; skeletons fade.
    await page.waitForTimeout(900);
    await page.evaluate(() => document.fonts?.ready).catch(() => undefined);
    rec.finalUrl = page.url().replace(ORIGIN, '');
    const c = await page.evaluate(CHECKS);
    Object.assign(rec, c);
    await page.screenshot({ path: resolve(OUT, file), fullPage: true, timeout: 30_000 });
  } catch (e) {
    rec.notes.push(`capture failed: ${String(e).slice(0, 300)}`);
    await page.screenshot({ path: resolve(OUT, file), timeout: 15_000 }).catch(() => undefined);
  } finally {
    writeFileSync(resolve(DATA, `${file}.json`), JSON.stringify(rec, null, 1));
    await context.close();
  }
}

// ---- seeding -------------------------------------------------------------------------------------------------------

async function seedWorld(browser: Browser, owner: ReturnType<typeof bootstrapState>['owner']) {
  const log: string[] = [];
  const step = async <T>(name: string, fn: () => Promise<T>): Promise<T | null> => {
    try {
      const v = await fn();
      log.push(`ok   ${name}`);
      return v;
    } catch (e) {
      log.push(`FAIL ${name}: ${String(e).slice(0, 240)}`);
      return null;
    }
  };
  const ownerUser = { id: '', ...owner };
  const s = await soloServer(
    async (u) => {
      const context = await browser.newContext({ baseURL: ORIGIN });
      await loginApi(context.request, u);
      return { user: u, context, page: await context.newPage() };
    },
    ownerUser,
    'Sweep Lounge',
  );
  const o = s.a.context.request;
  await s.a.context.storageState({ path: OWNER_STATE });

  // Name the server like a real one and give the Owner a display name.
  await step('rename server', () =>
    apiPatch(o, `/guilds/${s.guildId}`, { name: 'The Back Booth' }),
  );
  await step('owner display name', () =>
    apiPatch(o, '/users/@me', { global_name: 'Rowan (owner)' }),
  );

  // A member with a role.
  const anon = await browser.newContext({ baseURL: ORIGIN });
  const member = await registerUser(anon.request, 'sweepmate');
  const mctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(mctx.request, member);
  const m = mctx.request;
  await step('member display name', () => apiPatch(m, '/users/@me', { global_name: 'Juniper' }));
  const invite = await step('invite', () =>
    apiPost<{ code: string }>(o, `/channels/${s.channelId}/invites`, {}),
  );
  if (invite) await step('member joins', () => apiPost(m, `/invites/${invite.code}`));
  const meMember = await step('member id', async () => {
    const r = await m.get('/api/v1/users/@me');
    return ((await r.json()) as { id: string }).id;
  });
  const role = await step('role', () =>
    apiPost<{ id: string }>(o, `/guilds/${s.guildId}/roles`, {
      name: 'Night Shift',
      color: 0x5b8def,
      hoist: true,
      mentionable: true,
    }),
  );
  await step('role 2', () =>
    apiPost(o, `/guilds/${s.guildId}/roles`, { name: 'Regulars', color: 0xe0a458, hoist: true }),
  );
  if (role && meMember)
    await step('assign role', () =>
      apiPut(o, `/guilds/${s.guildId}/members/${meMember}/roles/${role.id}`),
    );
  const voice = await step('voice channel', () =>
    apiPost<{ id: string }>(o, `/guilds/${s.guildId}/channels`, { name: 'Hangout', type: 2 }),
  );
  await step('second text channel', () =>
    apiPost(o, `/guilds/${s.guildId}/channels`, {
      name: 'announcements-and-very-long-channel-name',
      type: 0,
      topic: 'Read me first',
    }),
  );

  // Messages: markdown, mentions, a rich embed, a link preview, attachments (injected), a long unbroken string.
  // The channel allows a few messages per second: pace them.
  const post = async (r: APIRequestContext, body: unknown) => {
    await new Promise((ok) => setTimeout(ok, 1200));
    return apiPost<{ id: string }>(r, `/channels/${s.channelId}/messages`, body);
  };
  await step('messages', async () => {
    await post(o, { content: 'Welcome in! **Bold**, *italic*, `inline code`, and a ||spoiler||.' });
    await post(m, { content: `Hey <@${meMember ?? ''}> here — thanks for the invite :wave:` });
    await post(o, {
      content:
        'Here is the plan for Friday:\n```js\nconst night = { film: "Heat", snacks: true };\n```\n> quoted line\n- one\n- two',
    });
    await post(m, {
      content: 'Link preview: https://sweep-preview.invalid/article',
      embeds: [
        {
          type: 'link',
          url: 'https://sweep-preview.invalid/article',
          title: 'A long-form article about late-night diners and the people who keep them open',
          description:
            'Every city has one: a diner that never closes. We spent a week behind the counter to find out what keeps it running.',
          provider: { name: 'Sweep Preview Weekly' },
          thumbnail: { url: 'https://sweep-preview.invalid/thumb.png', width: 400, height: 300 },
        },
      ],
    });
    await post(o, {
      content: 'A bot-style card:',
      embeds: [
        {
          type: 'rich',
          title: 'Movie night poll',
          description: 'Vote with a reaction. Results at 9 pm.',
          color: 0xc2b08f,
          fields: [
            { name: 'Option A', value: 'Heat (1995)', inline: true },
            { name: 'Option B', value: 'Collateral (2004)', inline: true },
            { name: 'Snacks', value: 'Popcorn, the good kind', inline: false },
          ],
          footer: { text: 'Poll closes tonight' },
          image: { url: 'https://sweep-preview.invalid/poster.png', width: 600, height: 300 },
        },
      ],
    });
    await post(m, { content: '[files] Photos from last week and the notes' });
    await post(m, {
      content:
        'unbroken: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
    });
  });

  // The a11y world: three messages, a thread, a published title and a watch party.
  await new Promise((ok) => setTimeout(ok, 6000));
  const w = await buildWorld(s);
  await step('reaction', async () => {
    const r = await o.put(
      `/api/v1/channels/${s.channelId}/messages/${w.messageId}/reactions/${encodeURIComponent('👍')}/@me`,
      { headers: await csrfHeaders(o) },
    );
    if (!r.ok()) throw new Error(String(r.status()));
  });

  // A DM between the Owner and the member.
  const dm = await step('dm', async () => {
    const ownerId = ((await (await o.get('/api/v1/users/@me')).json()) as { id: string }).id;
    const ch = await apiPost<{ id: string }>(m, '/users/@me/channels', { recipient_id: ownerId });
    await apiPost(m, `/channels/${ch.id}/messages`, { content: 'Are we still on for Friday?' });
    await apiPost(o, `/channels/${ch.id}/messages`, { content: 'Yes! Bring the projector.' });
    return ch.id;
  });

  // A moderation case: the member reports one of the Owner's messages.
  await step('reports switch', async () => {
    try {
      await apiPatch(o, '/admin/features/moderation.reports', {
        enabled: true,
        reason: 'sweep: a sample report',
      });
    } catch {
      await apiPatch(o, `/guilds/${s.guildId}/sr/features/moderation.reports`, { enabled: true });
    }
  });
  await step('report', () =>
    apiPost(m, '/moderation/reports', {
      target_kind: 'message',
      target_id: w.messageId,
      channel_id: s.channelId,
      category: 'spam',
      note: 'Sweep: a sample report so the queues are not empty.',
    }),
  );

  // A developer application and an approved Activity (Arcade card).
  const app = await step('developer app', async () => {
    const r = await apiPost<{ application: { id: string } }>(o, '/applications', {
      name: 'Projector Bot',
      description: 'Posts the movie night schedule.',
    });
    return r.application.id;
  });
  const activity = await step('activity', async () => {
    // The member is the developer: nobody reviews their own application.
    const { application } = await apiPost<{ application: { id: string } }>(m, '/applications', {
      name: 'Doodle Board',
      description: 'A shared whiteboard for the booth.',
    });
    await apiPatch(m, `/applications/${application.id}`, {
      privacy_policy_url: 'https://example.test/privacy',
      terms_of_service_url: 'https://example.test/terms',
    });
    const version = await apiPost<{ id: string }>(m, `/applications/${application.id}/versions`, {
      note: 'sweep',
    });
    await apiPost(o, `/admin/developers/versions/${version.id}/decision`, { decision: 'approve' });
    const up = await apiPostBytes<{ bundle: { id: string } }>(
      m,
      `/applications/${application.id}/activity/bundles`,
      testActivityBundle(),
      'application/zip',
    );
    await apiPost(m, `/applications/${application.id}/activity/submit`, {
      bundle_id: up.bundle.id,
    });
    await apiPost(o, `/admin/developers/activities/${application.id}/decision`, {
      decision: 'approve',
      bundle_id: up.bundle.id,
    });
    return application.id;
  });

  const tv = await step('tv channel', async () => {
    // Number 7 from an earlier sweep on this database is reused.
    const list = await o.get('/api/v1/admin/tv/channels');
    const found = list.ok()
      ? ((await list.json()) as
          { id: string; number: number }[] | { items: { id: string; number: number }[] })
      : [];
    const rows = Array.isArray(found) ? found : found.items;
    const seven = rows.find((c) => c.number === 7);
    if (seven) return seven.id;
    const r = await apiPost<{ id: string }>(o, '/admin/tv/channels', {
      number: 7,
      name: 'Night Owl TV',
      kind: 'manual',
      timezone: 'UTC',
      enabled: true,
    });
    return r.id;
  });

  const world: SweepWorld = {
    guildId: s.guildId,
    channelId: s.channelId,
    voiceChannelId: voice?.id ?? null,
    messageId: w.messageId,
    threadId: w.threadId,
    titleId: w.titleId,
    partyId: w.partyId,
    dmId: dm,
    appId: app,
    activityAppId: activity,
    tvChannelId: tv,
    inviteCode: invite?.code ?? null,
    memberName: member.username,
    seedLog: log,
  };
  await anon.close();
  await mctx.close();
  await s.a.context.close();
  return world;
}

async function setThemeOnAccount(browser: Browser, theme: Theme) {
  const context = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  try {
    await apiPatch(context.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: theme === 'light' ? 'lounge-light' : 'lounge-editorial' } },
    });
  } finally {
    await context.close();
  }
}

async function setShowroom(enabled: boolean) {
  const context = await (await import('@playwright/test')).request.newContext({ baseURL: ORIGIN });
  try {
    await loginApi(context, bootstrapState().owner);
    await apiPatch(context, '/admin/features/platform.showroom', {
      enabled,
      reason: enabled ? 'sweep: showroom pass' : 'sweep: done',
    });
  } finally {
    await context.dispose();
  }
}

// ---- the run -------------------------------------------------------------------------------------------------------

test.describe('visual sweep @sweep', () => {
  test.beforeEach(({ browserName }) => {
    test.skip(browserName !== 'chromium', 'Chromium only.');
  });

  test('seed the sweep world', async ({ browser, owner }) => {
    test.setTimeout(300_000);
    mkdirSync(STATE, { recursive: true });
    mkdirSync(DATA, { recursive: true });
    if (existsSync(WORLD_FILE) && process.env.SR_SWEEP_RESEED !== '1') return;
    const world = await seedWorld(browser, owner);
    writeFileSync(WORLD_FILE, JSON.stringify(world, null, 1));
    // The storefront surfaces are off by default (D-400); the sweep shows them once, like their own specs do.
    const ctx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
    await setStudySurfacesWith(ctx.request, true);
    await ctx.close();
    console.log(world.seedLog.join('\n'));
  });

  for (const theme of THEMES) {
    test(`theme ${theme} on the account`, async ({ browser }) => {
      await setThemeOnAccount(browser, theme);
    });
    for (const viewport of Object.keys(VIEWPORTS) as Viewport[]) {
      for (const row of SIGNED_IN.filter((r) => want(r.name))) {
        test(`in ${viewport} ${theme} ${row.name}`, async ({ browser }) => {
          test.setTimeout(120_000);
          await capture(browser, row, loadWorld(), {
            viewport,
            theme,
            signedIn: true,
            showroom: false,
          });
        });
      }
      for (const row of SIGNED_OUT.filter((r) => want(r.name))) {
        test(`out ${viewport} ${theme} ${row.name}`, async ({ browser }) => {
          test.setTimeout(120_000);
          await capture(browser, row, loadWorld(), {
            viewport,
            theme,
            signedIn: false,
            showroom: false,
          });
        });
      }
    }
  }

  if (SHOWROOM.some((r) => want(r.name))) {
    test('showroom switch on', async () => {
      await setShowroom(true);
    });
    for (const theme of THEMES) {
      test(`theme ${theme} for the showroom`, async ({ browser }) => {
        await setThemeOnAccount(browser, theme);
      });
      for (const viewport of Object.keys(VIEWPORTS) as Viewport[]) {
        for (const row of SHOWROOM.filter((r) => want(r.name))) {
          test(`showroom ${viewport} ${theme} ${row.name}`, async ({ browser }) => {
            test.setTimeout(120_000);
            await capture(browser, row, loadWorld(), {
              viewport,
              theme,
              signedIn: true,
              showroom: true,
            });
          });
        }
      }
    }
    test('showroom switch off', async () => {
      await setShowroom(false);
    });
  }

  test('write the summary and the contact sheets', async ({ browser }) => {
    test.setTimeout(600_000);
    const caps = readdirSync(DATA)
      .filter((f) => f.endsWith('.png.json'))
      .map((f) => JSON.parse(readFileSync(resolve(DATA, f), 'utf8')) as Capture);
    const flagged = (c: Capture) =>
      c.consoleErrors.length > 0 ||
      c.pageErrors.length > 0 ||
      c.failedRequests.length > 0 ||
      c.errorBoundary !== null ||
      c.blank ||
      c.overflowX > 0 ||
      c.offscreen.length > 0 ||
      c.rawKeys.length > 0 ||
      c.notes.some((n) => n.startsWith('capture failed')) ||
      (c.status !== null && c.status >= 400);
    writeFileSync(resolve(OUT, 'summary.json'), JSON.stringify(caps, null, 1));
    const routes = [...new Set(caps.map((c) => c.route))].sort();
    const lines = [
      '# Visual sweep summary (XC-VIS-001)',
      '',
      `${routes.length} routes, ${caps.length} captures. Generated ${new Date().toISOString()}.`,
      '',
      '| route | url | captures | flagged | status | issues |',
      '|---|---|---|---|---|---|',
    ];
    for (const r of routes) {
      const cs = caps.filter((c) => c.route === r);
      const issues = new Set<string>();
      for (const c of cs) {
        const tag = `${c.viewport}/${c.theme}`;
        if (c.errorBoundary) issues.add(`error boundary (${tag})`);
        if (c.blank) issues.add(`blank (${tag})`);
        if (c.overflowX > 0) issues.add(`overflow-x ${c.overflowX}px (${tag})`);
        if (c.offscreen.length)
          issues.add(`offscreen: ${c.offscreen.slice(0, 2).join('; ')} (${tag})`);
        if (c.rawKeys.length) issues.add(`raw keys: ${c.rawKeys.join(', ')}`);
        for (const e of c.pageErrors) issues.add(`pageerror: ${e.slice(0, 120)}`);
        for (const e of c.consoleErrors.slice(0, 3)) issues.add(`console: ${e.slice(0, 120)}`);
        for (const e of c.failedRequests.slice(0, 4)) issues.add(`req: ${e.slice(0, 120)}`);
        for (const n of c.notes) issues.add(n.slice(0, 160));
      }
      const statuses = [...new Set(cs.map((c) => c.status ?? '-'))].join('/');
      lines.push(
        `| ${r} | \`${cs[0]?.url ?? ''}\` | ${cs.length} | ${cs.filter(flagged).length} | ${statuses} | ${[...issues].join('<br>').replace(/\|/g, '\\|')} |`,
      );
    }
    writeFileSync(resolve(OUT, 'summary.md'), lines.join('\n') + '\n');

    // Contact sheets: two routes per image, each row desktop light, desktop dark, phone light, phone dark (top of
    // the page only; the full captures are next to them).
    const sheets = resolve(OUT, 'sheets');
    mkdirSync(sheets, { recursive: true });
    const page = await browser.newPage({ viewport: { width: 1720, height: 900 } });
    const uri = (f: string) =>
      existsSync(resolve(OUT, f))
        ? `data:image/png;base64,${readFileSync(resolve(OUT, f)).toString('base64')}`
        : '';
    const cell = (f: string, w: number, h: number) =>
      `<div style="width:${w}px;height:${h}px;overflow:hidden;border:1px solid #888;background:#fff">${
        uri(f) ? `<img src="${uri(f)}" style="width:${w}px;display:block">` : `<p>missing</p>`
      }</div>`;
    const toSheet = routes.filter((r) => !ONLY || ONLY.test(r));
    for (let i = 0; i < toSheet.length; i += 2) {
      const pair = toSheet.slice(i, i + 2);
      const html = pair
        .map(
          (r) =>
            `<div style="font:14px sans-serif;margin:4px 0">${r}</div><div style="display:flex;gap:6px">` +
            cell(`${r}-desktop-light.png`, 640, 400) +
            cell(`${r}-desktop-dark.png`, 640, 400) +
            cell(`${r}-phone-light.png`, 188, 406) +
            cell(`${r}-phone-dark.png`, 188, 406) +
            '</div>',
        )
        .join('');
      await page.setContent(`<body style="margin:6px;background:#ddd">${html}</body>`);
      await page.waitForTimeout(100);
      await page.screenshot({
        path: resolve(sheets, `${String(i / 2).padStart(3, '0')}-${pair.join('+')}.png`),
        fullPage: true,
      });
    }
    await page.close();
    expect(caps.length).toBeGreaterThan(0);
  });
});

/** Run by hand after a sweep (SR_SWEEP_CLEANUP=1): remove the world's server and title. */
test('remove the sweep world', async ({ browser }) => {
  test.skip(process.env.SR_SWEEP_CLEANUP !== '1', 'SR_SWEEP_CLEANUP=1 removes the world.');
  const w = loadWorld();
  const ctx = await browser.newContext({ baseURL: ORIGIN, storageState: OWNER_STATE });
  await apiDelete(ctx.request, `admin/cinema/titles/${w.titleId}`).catch(() => undefined);
  await dropServer({
    a: { user: { id: '', ...bootstrapState().owner }, context: ctx, page: await ctx.newPage() },
    guildId: w.guildId,
  });
  await setStudySurfacesWith(ctx.request, false);
  await ctx.close();
});
