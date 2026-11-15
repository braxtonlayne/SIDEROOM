import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';

/**
 * SEC-AUD-299 and SEC-AUD-300 (wave 76; D-795, D-796), on the real stack with the storage API faked by `page.route`
 * (the binary is older than the Rust of waves 73 and 74 and no bucket is reachable):
 *  1. User Settings > Storage, one active R2 mount whose last test holds a warn `cors` row: shot before Apply CORS
 *     (seven rows), after Apply CORS (six rows, the section Ready), after Test connection again (seven rows, CORS a pass);
 *  2. the bucket browser at /storage/501, top level and one folder: "New folder" has its text width at the start edge.
 *     The measurement is also taken with the wrapper turned into `display: contents`, which is the base layout
 *     (the button as the grid item), to show what the base did.
 * Editorial, Lounge Light and Lounge Dark, desktop 1280x800 and phone 375x812. Local only (SR_P76_STORAGE=1);
 * shots go to SR_P76_STORAGE_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P76_STORAGE_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p76-storage';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const T_TEST = '2026-11-14T10:00:00.000Z';
const T_CORS = '2026-11-14T10:05:00.000Z';
const T_RETEST = '2026-11-14T10:10:00.000Z';

test.skip(process.env.SR_P76_STORAGE !== '1', 'local visual check only');
test.setTimeout(1_800_000);

const probe = (id: string, status: string, message: string) => ({ id, status, message });
const rows = (cors: { status: string; message: string }) => [
  probe('head_bucket', 'pass', 'The bucket exists and the key can reach it.'),
  probe('list', 'pass', 'Listing the prefix works.'),
  probe('put', 'pass', 'Write, read and delete of a probe object work.'),
  probe('response_type', 'pass', 'The service answers with the response type SIDE ROOM asks for.'),
  probe('multipart', 'pass', 'Multipart uploads can start and abort.'),
  probe('cors', cors.status, cors.message),
  probe('list_buckets', 'pass', 'The key cannot see other buckets.'),
];
const REQUIRED = [
  'GET, HEAD, PUT from the server origin',
  'allowed header: content-type',
  'exposed: etag, content-range, accept-ranges, content-length',
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

/** The faked storage API of one page; `state` is what the "server" holds for mount 501. */
async function fake(page: Page) {
  const state = {
    last_test: {
      at: T_TEST,
      rows: rows({
        status: 'warn',
        message: 'The bucket has no CORS rules yet. Use Apply CORS below.',
      }),
    } as { at: string; rows: unknown[] },
    cors_ok_at: null as string | null,
  };
  const mount = () => ({
    id: '501',
    owner_kind: 'user',
    owner_id: '1',
    label: 'Family R2',
    provider: 'r2',
    endpoint: null,
    region: 'auto',
    bucket: 'family-shelf',
    prefix: '',
    path_style: false,
    access_key_id: 'AKIAEXAMPLE',
    key_hint: 'MPLE',
    status: 'active',
    cors_ok_at: state.cors_ok_at,
    last_test_at: state.last_test.at,
    last_test: state.last_test,
    created_at: '2026-11-13T10:00:00.000Z',
  });
  const cors = (st: string, at: string) => ({
    state: st,
    required: REQUIRED,
    found: st === 'ok' ? REQUIRED : [],
    manual_steps: null,
    checked_at: at,
  });
  await page.route('**/api/v1/users/@me/storage/mounts**', async (route) => {
    const path = new URL(route.request().url()).pathname;
    const method = route.request().method();
    if (path.endsWith('/cors/check'))
      return route.fulfill({ json: cors(state.cors_ok_at ? 'ok' : 'missing', T_CORS) });
    if (path.endsWith('/cors/apply')) {
      state.cors_ok_at = T_CORS;
      return route.fulfill({ json: cors('ok', T_CORS) });
    }
    if (path.endsWith('/test')) {
      state.last_test = {
        at: T_RETEST,
        rows: rows({ status: 'pass', message: 'Bucket rules can be read.' }),
      };
      return route.fulfill({ json: state.last_test });
    }
    if (method === 'GET' && path.endsWith('/mounts')) return route.fulfill({ json: [mount()] });
    return route.fulfill({ json: mount() });
  });
  await page.route('**/api/v1/storage/mounts/501/objects**', async (route) => {
    const prefix = new URL(route.request().url()).searchParams.get('prefix') ?? '';
    const top = prefix === '';
    await route.fulfill({
      json: {
        folders: top ? ['photos/', 'documents/'] : [],
        objects: [
          {
            key: `${prefix}holiday-2026.jpg`,
            size: 2_400_000,
            last_modified: '2026-11-12T09:00:00.000Z',
            content_type: 'image/jpeg',
          },
          {
            key: `${prefix}notes.txt`,
            size: 1_800,
            last_modified: '2026-11-11T09:00:00.000Z',
            content_type: 'text/plain',
          },
        ],
        next_cursor: null,
        can_upload: true,
        can_manage: true,
      },
    });
  });
}

test('the test table and its CORS row, and the New folder button', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  const rowCount = (page: Page) =>
    page.getByRole('table', { name: 'Connection test results' }).getByRole('row');

  for (const theme of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      await fake(page);

      // --- 299: the test table.
      await page.goto('/settings/user/storage');
      await expect(page.getByText('Family R2')).toBeVisible();
      await expect(rowCount(page)).toHaveCount(7);
      await expect(page.getByText('The bucket has no CORS rules yet')).toBeVisible();
      const shoot = async (name: string) => {
        await page.getByRole('table', { name: 'Connection test results' }).scrollIntoViewIfNeeded();
        await page.waitForTimeout(300);
        await page.screenshot({ path: resolve(OUT, `cors-${name}-${tag}.png`) });
      };
      await shoot('1-before-apply');
      await page.getByRole('button', { name: 'Apply CORS' }).click();
      await expect(page.getByText('Ready')).toBeVisible();
      await expect(rowCount(page)).toHaveCount(6);
      await expect(page.getByText('The bucket has no CORS rules yet')).toHaveCount(0);
      await shoot('2-after-apply');
      await page.getByRole('button', { name: 'Test connection' }).click();
      await expect(rowCount(page)).toHaveCount(7);
      await expect(page.getByText('Bucket rules can be read.')).toBeVisible();
      await shoot('3-after-test');

      // --- 300: the bucket browser.
      for (const [name, path] of [
        ['top', '/storage/501'],
        ['folder', '/storage/501?prefix=photos%2F'],
      ] as const) {
        await page.goto(path);
        const button = page.getByRole('button', { name: 'New folder' });
        await expect(button).toBeVisible();
        const box = await button.boundingBox();
        const wrap = await page.locator('.sto-write').boundingBox();
        const upload = await page.locator('.sto-upload').boundingBox();
        const rem = await page.evaluate(() =>
          parseFloat(getComputedStyle(document.documentElement).fontSize),
        );
        expect(box!.width).toBeLessThan(12 * rem);
        expect(Math.abs(box!.x - wrap!.x)).toBeLessThanOrEqual(1);
        expect(upload!.width).toBeGreaterThan(wrap!.width - 2); // the upload panel keeps the full width
        // The base layout (the button as the grid item): the same measurement.
        await page.addStyleTag({ content: '.sto-newfolder{display:contents !important}' });
        const base = await button.boundingBox();
        console.log(
          `new-folder ${tag} ${name}: fixed ${Math.round(box!.width)}px, container ${Math.round(wrap!.width)}px, base layout ${Math.round(base!.width)}px`,
        );
        expect(base!.width).toBeGreaterThanOrEqual(wrap!.width - 2);
        await page.reload();
        await expect(page.getByRole('button', { name: 'New folder' })).toBeVisible();
        await page.waitForTimeout(300);
        await page.screenshot({ path: resolve(OUT, `folder-${name}-${tag}.png`) });
        if (name === 'top') {
          await page.getByRole('button', { name: 'New folder' }).click();
          await expect(page.getByRole('dialog')).toBeVisible();
          await page.waitForTimeout(800); // let the open animation end
          await page.screenshot({ path: resolve(OUT, `folder-dialog-${tag}.png`) });
          await page.keyboard.press('Escape');
          await expect(page.getByRole('dialog')).toHaveCount(0);
        }
      }
      const over = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(over).toBeLessThanOrEqual(0);
      await c.close();
    }
  }
  await octx.close();
});
