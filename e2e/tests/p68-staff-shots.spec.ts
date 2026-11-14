import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * Wave 68, staff tools (SEC-AUD-213, 216): Admin > Review queues, Submissions, with the names of submitters and
 * claimers (named row, a gone account, a deleted account, filtered, filtered with no rows, the detail), and Admin >
 * Cinema transcoding, Workers, with the GPU and throughput (a GPU worker that is running, a CPU worker that is idle,
 * an offline one). A fresh stack has neither submissions nor workers, so the answers are faked with `page.route`
 * (the queue honours the `submitter` parameter like the API) and the flows are clicked through on the fake. Editorial,
 * Lounge Light and Lounge Dark, desktop 1280x800 and phone 375x812. Local only (SR_P68_STAFF=1); shots go to
 * SR_P68_STAFF_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P68_STAFF_OUT ?? resolve('test-results/p68-staff');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const T = 1_800_000_000_000;

test.skip(process.env.SR_P68_STAFF !== '1', 'local visual check only');
test.setTimeout(1_200_000);

const MAYA = '310000000000000001';
const SAM = '310000000000000002';
const GONE = '310000000000000003';
const USERS = [
  { id: MAYA, username: 'maya.lindqvist', global_name: 'Maya Lindqvist', avatar: null },
  { id: SAM, username: 'sam', global_name: null, avatar: null },
];

const row = (i: number) => ({
  id: String(500 + i),
  kind: i % 3 === 0 ? 'library.items' : 'library.series',
  state: i === 1 ? 'in_review' : 'submitted',
  title: ['Frieren: Beyond Journey', 'Dungeon Meshi', 'Monster', 'Vinland Saga', 'Berserk'][i % 5],
  summary: '',
  ref_id: '900',
  payload: {},
  flags: [],
  submitter_id: i === 4 ? null : [MAYA, SAM, GONE][i % 3],
  submitted_at_ms: T - (i + 1) * 3_600_000 * 7,
  updated_at_ms: T,
  created_at_ms: T - 86_400_000,
  decided_by: null,
  decision_reason: null,
  decided_at_ms: null,
  published_at_ms: null,
  // Row 1 is held by Sam for another 20 minutes (the clock of the page is real, so the lease is far ahead).
  claimed_by: i === 1 ? SAM : null,
  claim_expires_at_ms: i === 1 ? Date.now() + 3 * 3_600_000 : null,
});
const ALL = Array.from({ length: 6 }, (_, i) => row(i));

const detailOf = (s: ReturnType<typeof row>) => ({
  submission: s,
  items: [1, 2].map((n) => ({
    id: `i${n}`,
    submission_id: s.id,
    ref_id: `c${n}`,
    label: `Chapter ${n}`,
    position: n,
    state: 'pending',
    note: null,
  })),
  events: [
    {
      id: '1',
      submission_id: s.id,
      kind: 'created',
      actor_id: s.submitter_id,
      body: null,
      internal: false,
      data: {},
      created_at_ms: T - 86_400_000,
    },
  ],
  kind_label: 'Library series',
  users: USERS,
});

const libraryPreview = {
  series_id: '900',
  new_series: true,
  series: {
    kind: 'manga',
    title: 'Dungeon Meshi',
    alt_titles: [],
    authors: ['Kui'],
    artists: [],
    status: 'ongoing',
    year: 2014,
    synopsis: 'Delicious.',
    genres: ['fantasy'],
    tags: [],
    reading_direction: 'rtl',
    age_restricted: false,
    has_cover: false,
  },
  editions: [{ id: 'e1', language: 'en', label: 'Default' }],
  existing_chapters: 0,
  chapters: [1, 2].map((n) => ({
    id: `c${n}`,
    edition_id: 'e1',
    number: n,
    title: null,
    format: 'images',
    page_count: 12,
    release_group: null,
    published: false,
  })),
};

const WORKERS = [
  {
    id: 'w1',
    name: 'gpu-box-rtx',
    version: '0.9.2',
    token_id: 't1',
    token_name: 'gpu-box',
    token_revoked: false,
    capabilities: {
      nvenc: true,
      hwaccels: ['cuda'],
      concurrency: 2,
      gpu: 'NVIDIA GeForce RTX 3060 Laptop GPU',
    },
    last_seen_ms: Date.now() - 5_000,
    online: true,
    drain: false,
    active_jobs: 2,
    speed_now: 2.43,
    jobs_done_24h: 14,
    jobs_failed_24h: 1,
  },
  {
    id: 'w2',
    name: 'cpu-nuc',
    version: '0.9.2',
    token_id: 't2',
    token_name: 'cpu-nuc',
    token_revoked: false,
    capabilities: { nvenc: false, hwaccels: [], concurrency: 1 },
    last_seen_ms: Date.now() - 8_000,
    online: true,
    drain: false,
    active_jobs: 0,
    speed_now: null,
    jobs_done_24h: 3,
    jobs_failed_24h: 0,
  },
  {
    id: 'w3',
    name: 'old-vaapi-server-with-a-long-name',
    version: '0.8.0',
    token_id: 't3',
    token_name: 'old',
    token_revoked: false,
    capabilities: { nvenc: false, hwaccels: ['vaapi'], concurrency: 4 },
    last_seen_ms: Date.now() - 3 * 86_400_000,
    online: false,
    drain: false,
    active_jobs: 0,
    speed_now: null,
    jobs_done_24h: 0,
    jobs_failed_24h: 0,
  },
];

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  theme: string,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
  });
  await c.addInitScript(
    (p) => {
      localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
    },
    { theme, zoom: 100 },
  );
  return c;
}

const json = (r: Route, body: unknown, status = 200) =>
  r.fulfill({ status, contentType: 'application/json', body: JSON.stringify(body) });

async function pause(page: Page) {
  await page.waitForTimeout(400);
}

test('the review queue names people: named, gone, deleted, filtered, filtered with no rows, the detail', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const requests: string[] = [];
  try {
    for (const theme of THEMES) {
      await apiPatch(octx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      for (const [vp, size] of Object.entries(VIEWPORTS)) {
        const tag = `${vp}-${theme}`;
        const c = await context(browser, octx, size, theme);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await page.route(/\/api\/v1\/admin\/review\/queue(\?.*)?$/, (r) => {
          const u = new URL(r.request().url());
          requests.push(u.search);
          const only = u.searchParams.get('submitter');
          const items = only ? ALL.filter((s) => s.submitter_id === only) : ALL;
          // The names the page shows: the submitters and claimers of the rows, and the filter's account even when
          // no row matches. The deleted account (GONE) is absent, as the server leaves it out.
          const ids = new Set<string>([
            ...items.flatMap((s) => [s.submitter_id, s.claimed_by].filter((x): x is string => !!x)),
            ...(only ? [only] : []),
          ]);
          return json(r, {
            items,
            total: items.length,
            limit: 25,
            offset: 0,
            users: USERS.filter((u) => ids.has(u.id)),
          });
        });
        await page.route(/\/api\/v1\/admin\/review\/submissions\/501$/, (r) =>
          json(r, detailOf(ALL[1]!)),
        );
        await page.route(/\/api\/v1\/admin\/review\/submissions\/501\/preview$/, (r) =>
          json(r, { preview: libraryPreview }),
        );
        await page.route(/\/api\/v1\/admin\/review\/submissions\/501\/duplicates$/, (r) =>
          json(r, { matches: [], resolution: null }),
        );
        await page.route(/\/api\/v1\/admin\/review\/submissions\/501\/media$/, (r) =>
          json(r, { items: [] }),
        );
        const shot = async (name: string) => {
          await pause(page);
          await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`) });
        };

        await page.goto('/admin/review');
        await page.getByRole('tab', { name: 'Submissions' }).click();
        const table = page.getByRole('table', { name: 'Review queue' });
        await expect(table).toBeVisible();
        await expect(table.getByRole('row')).toHaveCount(ALL.length + 1);
        // Named, a gone account (its number), a deleted one, and the claimer by name.
        await expect(
          table.getByRole('button', { name: /^@maya\.lindqvist\./ }).first(),
        ).toBeVisible();
        await expect(
          table.getByRole('button', { name: new RegExp(`^${GONE}\\.`) }).first(),
        ).toBeVisible();
        await expect(table.getByText('Deleted account')).toBeVisible();
        await expect(table.getByText(/@sam until/)).toBeVisible();
        await shot('queue-named');

        // Press a name: the chip reads "Submitted by @username", the address keeps the id.
        await table
          .getByRole('button', { name: /^@maya\.lindqvist\./ })
          .first()
          .click();
        const chip = page.getByRole('button', { name: /Submitted by @maya\.lindqvist/ });
        await expect(chip).toBeVisible();
        await expect(page).toHaveURL(new RegExp(`submitter=${MAYA}`));
        expect(requests.at(-1)).toContain(`submitter=${MAYA}`);
        await shot('queue-filtered');

        // A reload keeps it, name included.
        await page.reload();
        await expect(chip).toBeVisible();
        await chip.click();
        await expect(chip).toHaveCount(0);

        // A filter that matches nothing still shows the name (the server sends the filter's account).
        await page.goto(`/admin/review?submitter=${SAM}`);
        await page.getByRole('tab', { name: 'Submissions' }).click();
        const none = page.getByRole('button', { name: /Submitted by @sam/ });
        await expect(none).toBeVisible();
        await page.unroute(/\/api\/v1\/admin\/review\/queue(\?.*)?$/);
        await page.route(/\/api\/v1\/admin\/review\/queue(\?.*)?$/, (r) =>
          json(r, { items: [], total: 0, limit: 25, offset: 0, users: [USERS[1]] }),
        );
        await page.reload();
        await expect(page.getByRole('button', { name: /Submitted by @sam/ })).toBeVisible();
        await expect(page.getByText('Nothing to review')).toBeVisible();
        await shot('queue-filtered-empty');
        await c.close();

        // The detail: the submitter by name with the id beside it, and who holds the claim.
        const c2 = await context(browser, octx, size, theme);
        const p2 = await c2.newPage();
        p2.setDefaultTimeout(20_000);
        await p2.route(/\/api\/v1\/admin\/review\/submissions\/501$/, (r) =>
          json(r, detailOf(ALL[1]!)),
        );
        await p2.route(/\/api\/v1\/admin\/review\/submissions\/501\/preview$/, (r) =>
          json(r, { preview: libraryPreview }),
        );
        await p2.route(/\/api\/v1\/admin\/review\/submissions\/501\/duplicates$/, (r) =>
          json(r, { matches: [], resolution: null }),
        );
        await p2.route(/\/api\/v1\/admin\/review\/submissions\/501\/media$/, (r) =>
          json(r, { items: [] }),
        );
        await p2.goto('/admin/review?submission=501');
        const details = p2.getByLabel('Submission details');
        await expect(details).toContainText('@sam');
        await expect(details).toContainText(SAM);
        await expect(p2.getByText(/@sam is reviewing this until/)).toBeVisible();
        await pause(p2);
        await p2.screenshot({ path: resolve(OUT, `detail-${tag}.png`) });
        await c2.close();
      }
    }
  } finally {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
    });
    await setStudySurfacesWith(octx.request, false);
  }
});

test('transcoding workers show the GPU and throughput', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  try {
    for (const theme of THEMES) {
      await apiPatch(octx.request, '/users/@me/settings', {
        settings: { user_prefs: { theme, zoom: 100 } },
      });
      for (const [vp, size] of Object.entries(VIEWPORTS)) {
        const tag = `${vp}-${theme}`;
        const c = await context(browser, octx, size, theme);
        const page = await c.newPage();
        page.setDefaultTimeout(20_000);
        await page.route(/\/api\/v1\/admin\/transcode\/workers(\?.*)?$/, (r) => json(r, WORKERS));
        await page.route(/\/api\/v1\/admin\/transcode\/queue$/, (r) =>
          json(r, { queued: 2, running: 2, failed_last_day: 1, succeeded_last_day: 17 }),
        );
        await page.route(/\/api\/v1\/admin\/transcode\/jobs(\?.*)?$/, (r) => json(r, []));
        await page.goto('/admin/cinema-transcoding');
        await page.getByRole('tab', { name: 'Workers' }).click();
        const list = page.getByRole('list', { name: 'Workers' });
        await expect(list).toBeVisible();
        await expect(page.getByTestId('worker-gear-w1')).toContainText(
          'NVIDIA GeForce RTX 3060 Laptop GPU',
        );
        await expect(page.getByTestId('worker-gear-w1')).toContainText('2.4x now');
        await expect(page.getByTestId('worker-gear-w1')).toContainText('14 done, 1 failed in 24 h');
        await expect(page.getByTestId('worker-gear-w2')).toContainText('no hardware encoder');
        await expect(page.getByTestId('worker-gear-w3')).toContainText('vaapi');
        // Nothing runs past the page's edge at 375 px.
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
        await pause(page);
        await page.screenshot({ path: resolve(OUT, `workers-${tag}.png`) });
        await c.close();
      }
    }
  } finally {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
    });
  }
});
