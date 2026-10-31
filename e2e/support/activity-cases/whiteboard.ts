// ACT-WB-001 live case: the Whiteboard drawn in two contexts through the SFU data channel.
//
// The bundle exposes numbers on the canvas element so the case never reads pixels:
//   #board[data-objects]      visible objects on the current page
//   #board[data-all-objects]  visible objects on every page
//   #board[data-pages]        number of pages
//   #board[data-compactions]  snapshots this participant has loaded
//   #board[data-live]         log entries not yet folded into a snapshot

import { expect } from '@playwright/test';
import { activityPanel, type ActivitySession, type ActivitySide } from '../activities';

async function drag(side: ActivitySide, from: [number, number], to: [number, number]) {
  const box = await side.frame.locator('#board').boundingBox();
  if (!box) throw new Error('the board is not visible');
  const at = (p: [number, number]) =>
    [box.x + box.width * p[0], box.y + box.height * p[1]] as const;
  await side.page.mouse.move(...at(from));
  await side.page.mouse.down();
  await side.page.mouse.move(...at([(from[0] + to[0]) / 2, (from[1] + to[1]) / 2]), { steps: 6 });
  await side.page.mouse.move(...at(to), { steps: 6 });
  await side.page.mouse.up();
}

export async function whiteboardCase(s: ActivitySession) {
  const { a, b } = s;
  const boardA = a.frame.locator('#board');
  const boardB = b.frame.locator('#board');
  await expect(boardA).toBeVisible({ timeout: 30_000 });
  await expect(boardB).toBeVisible({ timeout: 30_000 });
  await expect(boardA).toHaveAttribute('data-objects', '0');

  // Alice draws a pen stroke; Bob sees it through the data channel.
  await drag(a, [0.1, 0.2], [0.4, 0.5]);
  await expect(boardB).toHaveAttribute('data-objects', '1', { timeout: 15_000 });

  // Bob picks the rectangle tool and draws; Alice sees it.
  await b.frame.getByRole('radio', { name: 'Rectangle' }).click();
  await drag(b, [0.5, 0.3], [0.8, 0.7]);
  await expect(boardA).toHaveAttribute('data-objects', '2', { timeout: 15_000 });

  // Undo is per author: Bob's undo removes his rectangle for both, Alice's stroke stays.
  await b.frame.getByRole('button', { name: 'Undo' }).click();
  await expect(boardA).toHaveAttribute('data-objects', '1', { timeout: 15_000 });
  await expect(boardB).toHaveAttribute('data-objects', '1');

  // A second page, created by Bob, appears for Alice; Bob is on it and it is empty.
  await b.frame.getByRole('button', { name: 'New page' }).click();
  await expect(boardA).toHaveAttribute('data-pages', '2', { timeout: 15_000 });
  await expect(boardB).toHaveAttribute('data-objects', '0');

  // A late join reads the document from the host: Bob leaves the Activity and joins it again (a new frame and
  // a new SDK), and the stroke on page 1 and both pages are there.
  const panel = activityPanel(b.page, s.name);
  await panel.getByRole('button', { name: 'Leave', exact: true }).click();
  await expect(b.page.locator(`iframe[title="${s.name} (Activity)"]`)).toHaveCount(0, {
    timeout: 15_000,
  });
  await panel.getByRole('button', { name: 'Join', exact: true }).click();
  await expect(b.frame.locator('#board')).toHaveAttribute('data-all-objects', '1', {
    timeout: 30_000,
  });
  await expect(b.frame.locator('#board')).toHaveAttribute('data-pages', '2');
}
