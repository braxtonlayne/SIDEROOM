// ACT-WB-001 live-spec case, for the integrator to paste into e2e/tests/activities-live.spec.ts (act-channel
// owns that file). It is not compiled here. It needs the same stack as the other cases of that spec
// (SR_E2E_ACTIVITIES=1, SR_E2E_VOICE=1, Chromium) and the Whiteboard bundle uploaded and approved as the test
// Activity: `cd packages/starter-packs/activities/whiteboard && vite build`, then zip the contents of `dist`
// (index.html at the zip root) and use it where the spec uses `testActivityBundle`.
//
// The bundle exposes numbers on the canvas element so the case never reads pixels:
//   #board[data-objects]      visible objects on the current page
//   #board[data-all-objects]  visible objects on every page
//   #board[data-pages]        number of pages
//   #board[data-compactions]  snapshots this participant has loaded
//   #board[data-live]         log entries not yet folded into a snapshot
//
// `a` and `b` are the two media contexts of the existing case (alice launches the Activity and is the host;
// bob joins the instance). `frame(page)` is the Activity iframe of that page.

import { expect, type Page } from '@playwright/test';

const frame = (page: Page) => page.frameLocator('iframe[src*=".activities."]').first();

async function drag(page: Page, from: [number, number], to: [number, number]) {
  const board = frame(page).locator('#board');
  const box = await board.boundingBox();
  if (!box) throw new Error('the board is not visible');
  await page.mouse.move(box.x + box.width * from[0], box.y + box.height * from[1]);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width * ((from[0] + to[0]) / 2), box.y + box.height * to[1], {
    steps: 6,
  });
  await page.mouse.move(box.x + box.width * to[0], box.y + box.height * to[1], { steps: 6 });
  await page.mouse.up();
}

export async function whiteboardCase(a: { page: Page }, b: { page: Page }) {
  const countA = frame(a.page).locator('#board');
  const countB = frame(b.page).locator('#board');
  await expect(countA).toBeVisible({ timeout: 30_000 });
  await expect(countB).toBeVisible({ timeout: 30_000 });
  await expect(countA).toHaveAttribute('data-objects', '0');

  // Alice draws a pen stroke; bob sees it through the data channel.
  await drag(a.page, [0.1, 0.2], [0.4, 0.5]);
  await expect(countB).toHaveAttribute('data-objects', '1', { timeout: 15_000 });

  // Bob picks the rectangle tool and draws; alice sees it.
  await frame(b.page).getByRole('radio', { name: 'Rectangle' }).click();
  await drag(b.page, [0.5, 0.3], [0.8, 0.7]);
  await expect(countA).toHaveAttribute('data-objects', '2', { timeout: 15_000 });

  // Undo is per author: bob's undo removes his rectangle for both, alice's stroke stays.
  await frame(b.page).getByRole('button', { name: 'Undo' }).click();
  await expect(countA).toHaveAttribute('data-objects', '1', { timeout: 15_000 });
  await expect(countB).toHaveAttribute('data-objects', '1');

  // A second page, created by bob, appears for alice and is empty.
  await frame(b.page).getByRole('button', { name: 'New page' }).click();
  await expect(countA).toHaveAttribute('data-pages', '2', { timeout: 15_000 });
  await expect(countB).toHaveAttribute('data-objects', '0');

  // A late rejoin reads the document: reload bob's Activity and he sees the stroke again on page 1.
  await b.page.reload();
  await expect(frame(b.page).locator('#board')).toHaveAttribute('data-all-objects', '1', {
    timeout: 30_000,
  });
}
