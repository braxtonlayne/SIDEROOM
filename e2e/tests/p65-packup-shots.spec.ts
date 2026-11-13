import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page, Route } from '@playwright/test';
import { zip } from '../support/activityBundle';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-177 (wave 65): the Activity package upload on the developer page. The live stack uses the in-memory
 * store, which cannot sign parts, so the page's real path here is the single request (its limit line, the
 * upload, the refusal that names an entry). The resumable states (progress, "Checking", the bundle, the
 * refusal after the check, a file over the limit) are shot with `page.route` fakes of the five new routes and
 * of the bucket. Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P65_PACKUP=1);
 * shots go to SR_P65_PACKUP_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P65_PACKUP_OUT ?? resolve('test-results/p65-packup');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const MIB = 1024 * 1024;

test.skip(process.env.SR_P65_PACKUP !== '1', 'local visual check only');
test.setTimeout(1_800_000);

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

/** The limits answer the fakes give. */
const limits = (over: Record<string, unknown> = {}) => ({
  max_bytes: String(4096 * MIB),
  max_files: 20000,
  tier: 'reviewed',
  resumable: true,
  part_size: 16 * MIB,
  ...over,
});

const API = '**/api/v1/applications/*/activity';

/** Fakes the resumable upload routes. `mode.status` is what the status route answers while the test holds. */
async function fakeUploads(
  page: Page,
  size: number,
  mode: { status: 'checking' | 'ready' | 'refused'; release: Promise<void> },
  l = limits(),
) {
  await page.route(`${API}/package-limits`, (r) => json(r, l));
  await page.route(`${API}/uploads`, (r) =>
    json(
      r,
      {
        upload_id: '9',
        part_size: 16 * MIB,
        part_count: 3,
        parts: [1, 2, 3].map((n) => ({ part_number: n, url: `https://bucket.test/p${n}` })),
      },
      201,
    ),
  );
  await page.route('https://bucket.test/**', async (r) => {
    const cors = {
      'access-control-allow-origin': '*',
      'access-control-allow-methods': 'PUT, OPTIONS',
      'access-control-allow-headers': '*',
    };
    if (r.request().method() === 'OPTIONS') return r.fulfill({ status: 204, headers: cors });
    // The last part waits for the test, so the page shows a part-way progress bar.
    if (r.request().url().endsWith('/p3')) await mode.release;
    return r.fulfill({ status: 200, headers: cors, body: '' });
  });
  await page.route(`${API}/uploads/9/complete`, (r) =>
    json(
      r,
      {
        state: 'checking',
        received: [1, 2, 3],
        checked_bytes: '0',
        total_bytes: String(size),
      },
      202,
    ),
  );
  let polls = 0;
  await page.route(`${API}/uploads/9`, (r) => {
    if (r.request().method() === 'DELETE') return r.fulfill({ status: 204 });
    polls += 1;
    const base = { received: [1, 2, 3], total_bytes: String(size) };
    if (mode.status === 'ready')
      return json(r, { ...base, state: 'ready', checked_bytes: String(size), bundle_id: '9001' });
    if (mode.status === 'refused')
      return json(r, {
        ...base,
        state: 'refused',
        checked_bytes: String(Math.floor(size / 3)),
        error: 'levels/boss.exe: this file type is not allowed',
      });
    return json(r, {
      ...base,
      state: 'checking',
      checked_bytes: String(Math.min(size - MIB, polls * 7 * MIB)),
    });
  });
  await page.route(`${API}/bundles/9001`, (r) =>
    json(r, {
      bundle: {
        id: '9001',
        sha256: 'ab'.repeat(32),
        byte_size: size,
        entry_count: 812,
        state: 'draft',
        uploaded_at_ms: Date.now(),
        reviewed_at_ms: null,
        note: null,
        capabilities: [],
        tier: null,
        tip_links: [],
      },
      entries: [{ path: 'index.html', size: 1200, content_type: 'text/html' }],
    }),
  );
}

test('the package upload: limit line, progress, checking, uploaded and the refusals', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const o = octx.request;
  const { application } = await apiPost<{ application: { id: string } }>(o, '/applications', {
    name: `Package Booth ${Date.now().toString(36)}`,
    description: 'Shots of the package upload.',
  });
  const tab = `/developers/applications/${application.id}?tab=activities`;
  const small = zip({ 'index.html': '<!doctype html><title>Booth</title><p>hello</p>' });
  const exe = zip({ 'index.html': '<!doctype html><p>x</p>', 'levels/boss.exe': 'MZ' });
  const big = Buffer.alloc(34 * MIB, 1);

  for (const theme of THEMES) {
    await apiPatch(o, '/users/@me/settings', { settings: { user_prefs: { theme, zoom: 100 } } });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      page.setDefaultTimeout(30_000);
      const shot = async (name: string) => {
        await page.waitForTimeout(400);
        await page.screenshot({ path: resolve(OUT, `${name}-${tag}.png`), fullPage: false });
      };
      const picker = () => page.getByLabel('Activity bundle (zip)');
      const scroll = async () =>
        page
          .getByTestId('package-limit')
          .scrollIntoViewIfNeeded()
          .catch(() => undefined);

      // 1. The live server: the limit line, then a small zip through the single request.
      await page.goto(tab);
      await expect(page.getByTestId('package-limit')).toContainText('Packages up to');
      await scroll();
      await shot('limit-line');
      await picker().setInputFiles({
        name: 'booth.zip',
        mimeType: 'application/zip',
        buffer: small,
      });
      await expect(page.getByText(/^Uploaded: 1 files/)).toBeVisible();
      await scroll();
      await shot('uploaded-small');

      // 2. A zip the checker refuses: the entry is named.
      await picker().setInputFiles({ name: 'boss.zip', mimeType: 'application/zip', buffer: exe });
      await expect(page.getByRole('alert')).toContainText('levels/boss.exe');
      await scroll();
      await shot('refused-checker');

      // 3. A file over the application's limit is refused in the browser.
      await page.route(`${API}/package-limits`, (r) =>
        json(r, limits({ max_bytes: String(8 * MIB), tier: 'standard' })),
      );
      await page.goto(tab);
      await expect(page.getByTestId('package-limit')).toContainText('Packages up to 8 MiB');
      await picker().setInputFiles({
        name: 'huge.zip',
        mimeType: 'application/zip',
        buffer: Buffer.alloc(10 * MIB, 1),
      });
      await expect(page.getByRole('alert')).toContainText(
        'The limit for this application is 8 MiB',
      );
      await scroll();
      await shot('refused-over-limit');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 4. A big file in parts: progress, "Checking", then the bundle.
      let go!: () => void;
      const release = new Promise<void>((r) => (go = r));
      const mode: { status: 'checking' | 'ready' | 'refused'; release: Promise<void> } = {
        status: 'checking',
        release,
      };
      await fakeUploads(page, big.length, mode);
      await page.goto(tab);
      await expect(page.getByTestId('package-limit')).toContainText('Packages up to 4 GiB');
      await scroll();
      await shot('limit-line-reviewed');
      await picker().setInputFiles({ name: 'epic.zip', mimeType: 'application/zip', buffer: big });
      await expect(page.getByText(/of 34 MiB sent/)).toBeVisible();
      await expect(page.getByText('32 MiB of 34 MiB sent')).toBeVisible();
      await scroll();
      await shot('uploading');
      go();
      await expect(page.getByText('Checking the package...').first()).toBeVisible();
      await expect(page.getByText(/MiB of 34 MiB checked/)).toBeVisible();
      await scroll();
      await shot('checking');
      mode.status = 'ready';
      await expect(page.getByText(/^Uploaded: 812 files/)).toBeVisible({ timeout: 20_000 });
      await scroll();
      await shot('uploaded-big');
      await page.unrouteAll({ behavior: 'ignoreErrors' });

      // 5. The check refuses the big package: the server's text, with the entry.
      let go2!: () => void;
      const release2 = new Promise<void>((r) => (go2 = r));
      go2();
      await fakeUploads(page, big.length, { status: 'refused', release: release2 });
      await page.goto(tab);
      await picker().setInputFiles({ name: 'epic.zip', mimeType: 'application/zip', buffer: big });
      await expect(page.getByRole('alert')).toContainText('levels/boss.exe: this file type', {
        timeout: 20_000,
      });
      await scroll();
      await shot('refused-after-check');
      await page.unrouteAll({ behavior: 'ignoreErrors' });
      await c.close();
    }
  }
  await apiPatch(o, '/users/@me/settings', {
    settings: { user_prefs: { theme: 'lounge-editorial', zoom: 100 } },
  });
});
