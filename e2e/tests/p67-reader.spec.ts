import type { Browser, BrowserContext } from '@playwright/test';
import { bootstrapState, expect, loginApi, test } from '../support/fixtures';
import {
  CHAPTER_ID,
  PAGES,
  dispatch,
  installFakeLibrary,
  newFake,
  progressFrame,
} from '../support/p67-reader-fake';

/**
 * SEC-AUD-207 (wave 67, D-690): one account on two devices (two browser contexts). Device A reads, device B has the
 * chapter open: B is offered the jump, never moved; B saving from an older position is asked once; the Library home
 * of B shows A's progress.
 *
 * Local only (SR_P67_READER=1): a fresh stack has no readable chapter without an upload and a conversion, so the
 * chapter and the progress routes are answered by `page.route` (e2e/support/p67-reader-fake.ts, with the server's
 * conflict rule) and the gateway frame is dispatched into the page. The 5 seconds of the spec's acceptance row are
 * therefore NOT measured end to end; the spec records the client's own latency from the frame to the home.
 */
test.skip(process.env.SR_P67_READER !== '1', 'local only: the chapter is faked');
test.setTimeout(240_000);

const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;

async function device(browser: Browser, base: BrowserContext) {
  return browser.newContext({
    baseURL: ORIGIN,
    viewport: { width: 1280, height: 800 },
    storageState: await base.storageState(),
  });
}

test('the reader on two devices', async ({ browser }) => {
  const fake = newFake();
  const owner = bootstrapState().owner;
  const login = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(login.request, owner);
  const ctxA = await device(browser, login);
  const ctxB = await device(browser, login);
  await installFakeLibrary(ctxA, fake);
  await installFakeLibrary(ctxB, fake);
  const a = await ctxA.newPage();
  const b = await ctxB.newPage();
  const home = await ctxB.newPage();
  const status = (p: typeof a) => p.getByTestId('reader-status');

  // A reads to page 5 (LTR: ArrowRight turns forward) and the debounced save lands.
  await a.goto(`/library/r/${CHAPTER_ID}`);
  await expect(status(a)).toContainText(`Page 1 of ${PAGES}`);
  for (let i = 0; i < 4; i++) await a.keyboard.press('ArrowRight');
  await expect(status(a)).toContainText(`Page 5 of ${PAGES}`);
  await expect.poll(() => fake.progress?.page_index, { timeout: 10_000 }).toBe(4);
  expect(fake.puts[0]!.base).toBe(0);

  // B opens the chapter where A is, and B's Library home is open too.
  await b.goto(`/library/r/${CHAPTER_ID}`);
  await expect(status(b)).toContainText(`Page 5 of ${PAGES}`);
  await home.goto('/library');
  await expect(home.getByText('Page 5 of 8')).toBeVisible();

  // A goes on to page 7. The frame reaches B: the home follows, the open reader is offered the jump and stays put.
  for (let i = 0; i < 2; i++) await a.keyboard.press('ArrowRight');
  await expect.poll(() => fake.progress?.page_index, { timeout: 10_000 }).toBe(6);
  const savedAt = Date.now();
  await dispatch(b, 'SR_LIBRARY_UPDATE', progressFrame(fake));
  await dispatch(home, 'SR_LIBRARY_UPDATE', progressFrame(fake));
  await expect(home.getByText('Page 7 of 8')).toBeVisible({ timeout: 5_000 });
  const homeMs = Date.now() - savedAt;
  console.log(`[p67-reader] frame to Library home showing page 7: ${homeMs} ms (client side only)`);
  await expect(b.getByRole('button', { name: 'Jump to page 7' })).toBeVisible();
  await expect(status(b)).toContainText(`Page 5 of ${PAGES}`);

  // B goes back to page 2 and turns a page: its save starts from the old position, so it is asked, once.
  await b.getByRole('button', { name: 'Dismiss' }).click();
  for (let i = 0; i < 3; i++) await b.keyboard.press('ArrowLeft');
  await expect(status(b)).toContainText(`Page 2 of ${PAGES}`);
  await b.keyboard.press('ArrowRight');
  const dialog = b.getByRole('dialog', { name: 'You read further on another device' });
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  await expect(dialog).toContainText('page 7 of 8');
  const putsBefore = fake.puts.length;
  await b.keyboard.press('ArrowRight'); // the dialog traps focus: nothing more is saved
  await b.waitForTimeout(2600);
  expect(fake.puts.length).toBe(putsBefore);
  expect(fake.progress!.page_index).toBe(6);

  // "Continue from page 7" jumps there; the next save is applied.
  await dialog.getByRole('button', { name: 'Continue from page 7' }).click();
  await expect(status(b)).toContainText(`Page 7 of ${PAGES}`);
  await b.keyboard.press('ArrowRight');
  await expect.poll(() => fake.progress?.page_index, { timeout: 10_000 }).toBe(7);
  expect(fake.puts.at(-1)!.applied).toBe(true);

  // A, still on page 7, turns back: its save starts from an old position and B has gone further, so A is asked.
  // "Stay here" writes A's position over B's.
  await a.keyboard.press('ArrowLeft');
  await expect(status(a)).toContainText(`Page 6 of ${PAGES}`);
  const dialogA = a.getByRole('dialog', { name: 'You read further on another device' });
  await expect(dialogA).toBeVisible({ timeout: 10_000 });
  await dialogA.getByRole('button', { name: 'Stay here' }).click();
  await expect.poll(() => fake.progress?.page_index, { timeout: 10_000 }).toBe(5);
  expect(fake.puts.at(-1)).toMatchObject({ base: null, applied: true });
  await expect(dialogA).toHaveCount(0);

  await login.close();
  await ctxA.close();
  await ctxB.close();
});
