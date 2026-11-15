import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-298 (wave 76; D-794), on the real stack with the Library's upload answers faked by `page.route`:
 * the upload dock draws one bar per upload (the overall bar only for two or more) and the row's bar sits
 * under the name at the row's full width. Shots: one running upload with a long name; three uploads (running,
 * paused, ended with wave 73's long sentence); an ended upload without its file; a finished upload.
 * Editorial, Lounge Light and Lounge Dark, desktop and phone. Local only (SR_P76_DOCK=1); shots go to SR_P76_DOCK_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P76_DOCK_OUT ?? 'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p76-dock';
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const SENTENCE =
  'A part of this upload was larger than announced. The upload has ended; start a new one.';
const NAME = 'Very Long Series Name - Volume 12 - Chapter 104 (Digital) [Group].cbz';

test.skip(process.env.SR_P76_DOCK !== '1', 'local visual check only');
test.setTimeout(1_800_000);

type Seed = {
  id: string;
  filename: string;
  state: 'running' | 'paused' | 'failed';
  done: number[];
  ended?: boolean;
  error?: string;
  noFile?: boolean;
};

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

async function fake(page: Page, sub: string) {
  const base = `**/api/v1/submissions/${sub}/library`;
  await page.route(`${base}/uploads/*/parts`, async (route) => {
    await route.fulfill({
      status: 200,
      json: {
        part_size: 100,
        part_count: 4,
        received: [
          { part_number: 1, size: 100 },
          { part_number: 2, size: 100 },
        ],
        missing: [
          { part_number: 3, url: 'https://s.test/hang/3' },
          { part_number: 4, url: 'https://s.test/hang/4' },
        ],
        expires_in_secs: 600,
      },
    });
  });
  await page.route(`${base}/chapters/*/uploads`, async (route) => {
    await route.fulfill({
      status: 201,
      json: {
        upload: { id: '9100' },
        multipart: {
          part_size: 4,
          part_count: 1,
          parts: [{ part_number: 1, url: 'https://s.test/p1' }],
        },
      },
    });
  });
  await page.route('https://s.test/**', async (route) => {
    if (route.request().url().includes('/hang/')) return; // never answers: the upload stays running
    await new Promise((r) => setTimeout(r, 1500));
    await route.fulfill({ status: 200, body: '' });
  });
  await page.route(`${base}/uploads/*/complete`, async (route) => {
    await route.fulfill({ status: 200, json: {} });
  });
  await page.route(`${base}/uploads/*`, async (route) => {
    await route.fulfill({ status: 200, json: {} });
  });
}

/** Replace the page's ledger with these entries (what a closed tab leaves behind). */
async function seed(page: Page, sub: string, entries: Seed[]) {
  await page.evaluate(
    ([sid, list]) =>
      new Promise<void>((ok, bad) => {
        const req = indexedDB.open('sideroom-uploads');
        req.onerror = () => bad(new Error('open'));
        req.onsuccess = () => {
          const db = req.result;
          const tx = db.transaction('uploads', 'readwrite');
          const store = tx.objectStore('uploads');
          store.clear();
          for (const e of list as Seed[]) {
            store.put({
              id: e.id,
              submissionId: sid,
              chapterId: '1',
              uploadId: e.id,
              filename: e.filename,
              size: 400,
              partSize: 100,
              partCount: 4,
              done: e.done,
              state: e.state,
              ...(e.ended ? { ended: true } : {}),
              ...(e.error ? { error: e.error } : {}),
              ...(e.noFile ? {} : { file: new Blob(['abcd']) }),
            });
          }
          tx.oncomplete = () => {
            db.close();
            ok();
          };
          tx.onerror = () => bad(new Error('put'));
        };
      }),
    [sub, entries] as const,
  );
}

async function shot(page: Page, name: string) {
  const dock = page.getByRole('region', { name: 'Uploads' });
  await dock.scrollIntoViewIfNeeded();
  await page.waitForTimeout(300);
  await page.screenshot({ path: resolve(OUT, `${name}.png`) });
  // Nothing overflows the page; every bar starts at its row's start edge, under the name.
  const over = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(over).toBeLessThanOrEqual(0);
  const geo = await dock.evaluate((el) =>
    Array.from(el.querySelectorAll('li')).map((li) => {
      const r = li.getBoundingClientRect();
      const b = li.querySelector('progress')!.getBoundingClientRect();
      const t = li.querySelector('strong')!.getBoundingClientRect();
      return { li: [r.left, r.width], bar: [b.left, b.width], bTop: b.top, nameBottom: t.bottom };
    }),
  );
  for (const g of geo) {
    expect(Math.abs(g.bar[0] - g.li[0])).toBeLessThan(1);
    expect(g.bTop).toBeGreaterThanOrEqual(g.nameBottom - 1);
    expect(g.bar[1]).toBeGreaterThan(g.li[1] * 0.4);
  }
  console.log(`geo ${name} ${JSON.stringify(geo)}`);
}

test('the upload dock: one bar per upload', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const d = await apiPost<{ submission: { id: string } }>(octx.request, '/submissions', {
    kind: 'library.series',
    payload: { kind: 'manga', title: 'Dock Bars 76', authors: ['E2E'], genres: [], synopsis: '' },
  });
  const sub = d.submission.id;

  for (const theme of THEMES) {
    await apiPatch(octx.request, '/users/@me/settings', {
      settings: { user_prefs: { theme, zoom: 100 } },
    });
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      const dock = page.getByRole('region', { name: 'Uploads' });
      await fake(page, sub);
      await page.goto(`/library/submissions/${sub}`);
      await expect(page.getByRole('heading', { name: 'Dock Bars 76' })).toBeVisible();
      const reload = async (entries: Seed[]) => {
        await seed(page, sub, entries);
        await page.reload();
        await expect(dock).toBeVisible();
      };

      // 1. One running upload, the long name: one bar, no "All uploads" line.
      await reload([{ id: '9000', filename: NAME, state: 'running', done: [1, 2] }]);
      await expect(dock.getByText('Uploading')).toBeVisible();
      await expect(dock.locator('progress')).toHaveCount(1);
      await expect(dock.getByText(/All uploads/)).toHaveCount(0);
      await shot(page, `one-running-${tag}`);

      // 2. Three uploads: running, paused, ended with the long sentence (file kept).
      await reload([
        { id: '9000', filename: NAME, state: 'running', done: [1, 2] },
        { id: '9001', filename: 'Second Chapter.cbz', state: 'paused', done: [1] },
        {
          id: '9002',
          filename: 'Third.cbz',
          state: 'failed',
          done: [],
          ended: true,
          error: SENTENCE,
        },
      ]);
      await expect(dock.getByText('Uploading')).toBeVisible();
      await expect(dock.locator('progress')).toHaveCount(4);
      await expect(dock.getByText(/^All uploads: \d+%$/)).toBeVisible();
      await shot(page, `three-${tag}`);
      // Click through: pause, resume, cancel.
      await dock.getByRole('button', { name: `Pause ${NAME}` }).click();
      await expect(dock.getByRole('button', { name: `Resume ${NAME}` })).toBeVisible();
      await dock.getByRole('button', { name: `Resume ${NAME}` }).click();
      await expect(dock.getByRole('button', { name: `Pause ${NAME}` })).toBeVisible();
      await dock.getByRole('button', { name: 'Cancel Third.cbz' }).click();
      await expect(dock.getByRole('listitem')).toHaveCount(2);
      await dock.getByRole('button', { name: 'Cancel Second Chapter.cbz' }).click();
      await expect(dock.locator('progress')).toHaveCount(1); // back to one upload: the overall bar goes
      await shot(page, `back-to-one-${tag}`);

      // 3. Ended, without its file.
      await reload([
        {
          id: '9003',
          filename: NAME,
          state: 'failed',
          done: [],
          ended: true,
          error: SENTENCE,
          noFile: true,
        },
      ]);
      await expect(page.getByText('Choose the file again to upload it.')).toBeVisible();
      await shot(page, `ended-no-file-${tag}`);

      // 4. Finished: an ended upload with its file, started again, runs to the end.
      await reload([
        { id: '9004', filename: NAME, state: 'failed', done: [], ended: true, error: SENTENCE },
      ]);
      await dock.getByRole('button', { name: `Upload ${NAME} again` }).click();
      await expect(dock.getByText('Uploaded')).toBeVisible();
      await shot(page, `finished-${tag}`);

      await seed(page, sub, []);
      await c.close();
    }
  }
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
