// ACT-CT-001 live case: play one hand of Texas Hold'em in two contexts through the SFU data channel.
// Alice launched the Activity, so she is the host and the dealer; Bob joined the instance. The page marks what
// the case reads with data-testids: holdem-sit, holdem-deal, holdem-hole (my cards), holdem-board,
// holdem-status, holdem-fold, holdem-check-call, holdem-raise (+ holdem-raise-input), holdem-warning.

import { expect, type FrameLocator } from '@playwright/test';
import type { ActivitySession } from '../activities';

const cardsIn = async (frame: FrameLocator, scope: string) =>
  frame
    .locator(`${scope} [data-card]`)
    .evaluateAll((els) => els.map((e) => e.getAttribute('data-card') as string));

export async function holdemCase(s: ActivitySession) {
  const A = s.a.frame;
  const B = s.b.frame;

  // The lobby says plainly that this is a friends' table.
  await expect(A.getByTestId('holdem-warning')).toContainText('Not cheat-proof', {
    timeout: 30_000,
  });
  await expect(B.getByTestId('holdem-warning')).toContainText('not a casino');

  // Both sit down; the host's table shows both.
  await A.getByTestId('holdem-sit').click();
  await B.getByTestId('holdem-sit').click();
  await expect(A.locator('.seat:not(.empty)')).toHaveCount(2, { timeout: 15_000 });
  await expect(B.locator('.seat:not(.empty)')).toHaveCount(2, { timeout: 15_000 });

  // Alice deals. Each player sees two cards of their own; the other's seat shows card backs.
  await A.getByTestId('holdem-deal').click();
  await expect(A.getByTestId('holdem-hole').locator('[data-card]')).toHaveCount(2, {
    timeout: 15_000,
  });
  await expect(B.getByTestId('holdem-hole').locator('[data-card]')).toHaveCount(2, {
    timeout: 15_000,
  });
  const aliceHole = await cardsIn(A, '[data-testid=holdem-hole]');
  const bobHole = await cardsIn(B, '[data-testid=holdem-hole]');
  expect(new Set([...aliceHole, ...bobHole]).size).toBe(4);
  // Bob's page never shows Alice's hole cards before the showdown, nor the other way round.
  const bobSees = await cardsIn(B, '.holdem');
  for (const c of aliceHole) expect(bobSees).not.toContain(c);
  const aliceSees = await cardsIn(A, '.holdem');
  for (const c of bobHole) expect(aliceSees).not.toContain(c);

  // Play the hand out by checking and calling: whoever has the Check/Call button is up.
  const status = B.getByTestId('holdem-status');
  for (let i = 0; i < 40; i++) {
    if (/ wins /.test((await status.textContent()) ?? '')) break;
    for (const f of [A, B]) {
      const btn = f.getByTestId('holdem-check-call');
      if (await btn.isVisible()) {
        await btn.click();
        break;
      }
    }
    await s.a.page.waitForTimeout(250);
  }
  await expect(status).toContainText(' wins ', { timeout: 15_000 });
  await expect(A.getByTestId('holdem-status')).toHaveText((await status.textContent()) ?? '');

  // Showdown: five board cards, both hands revealed, chips still add up to 2 x 1000.
  await expect(B.getByTestId('holdem-board').locator('[data-card]')).toHaveCount(5);
  const chips = async (f: FrameLocator) =>
    (await f.locator('.seat:not(.empty) .chips').allTextContents()).map(Number);
  const total = (await chips(B)).reduce((sum, n) => sum + n, 0);
  expect(total).toBe(2000);
  for (const c of aliceHole) expect(await cardsIn(B, '.holdem')).toContain(c);
}
