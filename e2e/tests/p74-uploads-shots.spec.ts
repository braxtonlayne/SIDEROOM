import { mkdirSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPost, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-293 (wave 74; D-781), on the real stack with the Library's answers faked by `page.route`:
 * an upload the server ended (`parts` answers 409 with wave 73's sentence) is not asked about again.
 *  1. the ledger holds a running upload with its file; the page loads, `parts` answers 409, the row fails
 *     with "Upload {name} again" (shot), the button is pressed, a new slot is asked for and the new upload
 *     runs and ends (shot while it runs, shot when done);
 *  2. after a reload the ledger entry is ended and holds no file: the row says to choose the file again
 *     and has only the remove button (shot).
 * The dock lives on /library/submissions/:id (My submissions, one draft). Editorial, Lounge Light and Lounge
 * Dark, desktop and phone. Local only (SR_P74_UPLOADS=1); shots go to SR_P74_UPLOADS_OUT.
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT = process.env.SR_P74_UPLOADS_OUT ?? resolve('test-results/p74-uploads');
const THEMES = ['lounge-editorial', 'lounge-light', 'lounge-dark'] as const;
const VIEWPORTS = { desktop: { width: 1280, height: 800 }, phone: { width: 375, height: 812 } };
const SENTENCE =
  'A part of this upload was larger than announced. The upload has ended; start a new one.';
const NAME = 'Very Long Series Name - Volume 12 - Chapter 104 (Digital) [Group].cbz';

test.skip(process.env.SR_P74_UPLOADS !== '1', 'local visual check only');
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

/** Fake the Library's upload answers; `calls` records what the page asked. */
async function fake(page: Page, sub: string, calls: string[]) {
  const base = `**/api/v1/submissions/${sub}/library`;
  await page.route(`${base}/uploads/*/parts`, async (route) => {
    calls.push('parts');
    await route.fulfill({ status: 409, json: { code: 40900, message: SENTENCE } });
  });
  await page.route(`${base}/chapters/*/uploads`, async (route) => {
    calls.push('begin');
    await route.fulfill({
      status: 201,
      json: {
        upload: { id: '9001' },
        multipart: {
          part_size: 4,
          part_count: 1,
          parts: [{ part_number: 1, url: 'https://s.test/p1' }],
        },
      },
    });
  });
  await page.route('https://s.test/**', async (route) => {
    calls.push('put');
    await new Promise((r) => setTimeout(r, 1200));
    await route.fulfill({ status: 200, body: '' });
  });
  await page.route(`${base}/uploads/*/complete`, async (route) => {
    calls.push('complete');
    await route.fulfill({ status: 200, json: {} });
  });
  await page.route(`${base}/uploads/*`, async (route) => {
    if (route.request().method() === 'DELETE') calls.push('abort');
    await route.fulfill({ status: 200, json: {} });
  });
}

/** Put a running upload with its file into the page's ledger (what a closed tab leaves behind). */
async function seedLedger(page: Page, sub: string) {
  await page.evaluate(
    ([sid, name]) =>
      new Promise<void>((ok, bad) => {
        const req = indexedDB.open('sideroom-uploads');
        req.onerror = () => bad(new Error('open'));
        req.onsuccess = () => {
          const db = req.result;
          const tx = db.transaction('uploads', 'readwrite');
          tx.objectStore('uploads').put({
            id: '9000',
            submissionId: sid,
            chapterId: '1',
            uploadId: '9000',
            filename: name,
            size: 4,
            partSize: 4,
            partCount: 1,
            done: [],
            state: 'running',
            file: new Blob(['abcd']),
          });
          tx.oncomplete = () => {
            db.close();
            ok();
          };
          tx.onerror = () => bad(new Error('put'));
        };
      }),
    [sub, NAME],
  );
}

test('an ended upload: start again, and after a reload choose the file again', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true); // the Library's submissions are hidden while the switch is off
  const d = await apiPost<{ submission: { id: string } }>(octx.request, '/submissions', {
    kind: 'library.series',
    payload: {
      kind: 'manga',
      title: 'Ended Upload 74',
      authors: ['E2E'],
      genres: [],
      synopsis: '',
    },
  });
  const sub = d.submission.id;

  for (const theme of THEMES) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      const tag = `${vp}-${theme}`;
      const c = await context(browser, octx, size, theme);
      const page = await c.newPage();
      const calls: string[] = [];
      await fake(page, sub, calls);
      await page.goto(`/library/submissions/${sub}`);
      await expect(page.getByRole('heading', { name: 'Ended Upload 74' })).toBeVisible();
      await seedLedger(page, sub);
      await page.reload();
      // The ledger's running upload asks `parts`, the server says it has ended: failed, with the file held.
      const again = page.getByRole('button', { name: `Upload ${NAME} again` });
      await expect(again).toBeVisible();
      await expect(page.getByRole('region', { name: 'Uploads' })).toContainText(SENTENCE);
      expect(calls.filter((x) => x === 'parts')).toHaveLength(1);
      await page.getByRole('region', { name: 'Uploads' }).scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `ended-with-file-${tag}.png`) });
      // Press it: a new slot is asked for, the old row goes, the new upload runs and ends.
      await again.click();
      await expect(
        page.getByRole('region', { name: 'Uploads' }).getByText('Uploading'),
      ).toBeVisible();
      await expect(page.getByRole('region', { name: 'Uploads' }).getByRole('listitem')).toHaveCount(
        1,
      );
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `started-again-${tag}.png`) });
      await expect(
        page.getByRole('region', { name: 'Uploads' }).getByText('Uploaded'),
      ).toBeVisible();
      expect(calls).toContain('begin');
      expect(calls).not.toContain('abort');
      expect(calls.filter((x) => x === 'parts')).toHaveLength(1);
      await page.screenshot({ path: resolve(OUT, `finished-${tag}.png`) });

      // The other state: the ended entry after a reload, without its file. Seed again, load (ended and kept
      // in memory), reload once more: the ledger holds no file.
      await seedLedger(page, sub);
      await page.reload();
      await expect(page.getByRole('button', { name: `Upload ${NAME} again` })).toBeVisible();
      await page.reload();
      await expect(page.getByText('Choose the file again to upload it.')).toBeVisible();
      await expect(page.getByRole('button', { name: /again|Retry|Resume/ })).toHaveCount(0);
      expect(calls.filter((x) => x === 'parts')).toHaveLength(2); // not asked at the third load
      await page.getByRole('region', { name: 'Uploads' }).scrollIntoViewIfNeeded();
      await page.waitForTimeout(300);
      await page.screenshot({ path: resolve(OUT, `ended-no-file-${tag}.png`) });
      // Overflow: the dock must fit the viewport width.
      const over = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(over).toBeLessThanOrEqual(0);
      // Remove the row (cleans the ledger for the next combination).
      await page.getByRole('button', { name: `Cancel ${NAME}` }).click();
      await expect(page.getByRole('region', { name: 'Uploads' })).toHaveCount(0);
      await c.close();
    }
  }
  await setStudySurfacesWith(octx.request, false);
  await octx.close();
});
