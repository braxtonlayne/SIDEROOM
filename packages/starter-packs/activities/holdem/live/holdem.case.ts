// ACT-CT-001 live-spec case: play one hand. For the integrator to paste into e2e/tests/activities-live.spec.ts
// (act-channel owns that file); it is not compiled here. It needs the stack of the other cases of that spec
// (SR_E2E_ACTIVITIES=1, SR_E2E_VOICE=1, Chromium) and the Hold'em bundle uploaded and approved as the test
// Activity: `cd packages/starter-packs/activities/holdem && vite build`, then zip the contents of `dist`
// (index.html at the zip root) and use it where the spec uses `testActivityBundle`.
//
// `a` and `b` are the two media contexts of the existing case: alice launches the Activity and is the host and
// the dealer; bob joins the instance. The page marks what the case reads with `data-testid`s:
//   holdem-sit, holdem-deal, holdem-hole (my cards), holdem-board, holdem-status, holdem-fold,
//   holdem-check-call, holdem-raise (+ holdem-raise-input), holdem-warning (the not-cheat-proof notice).

import { expect, type FrameLocator, type Page } from '@playwright/test';

const frame = (page: Page): FrameLocator => page.frameLocator('iframe[src*=".activities."]').first();

const cardsIn = async (page: Page, scope: string) =>
  frame(page)
    .locator(`${scope} [data-card]`)
    .evaluateAll((els) => els.map((e) => e.getAttribute('data-card') as string));

export async function holdemCase(a: { page: Page }, b: { page: Page }) {
  const A = frame(a.page);
  const B = frame(b.page);

  // The lobby says plainly that this is a friends' table.
  await expect(A.getByTestId('holdem-warning')).toContainText('Not cheat-proof', { timeout: 30_000 });
  await expect(B.getByTestId('holdem-warning')).toContainText('not a casino');

  // Both sit down; the host's table shows both.
  await A.getByTestId('holdem-sit').click();
  await B.getByTestId('holdem-sit').click();
  await expect(A.locator('.seat:not(.empty)')).toHaveCount(2, { timeout: 15_000 });
  await expect(B.locator('.seat:not(.empty)')).toHaveCount(2, { timeout: 15_000 });

  // Alice deals. Each player sees two cards of their own; the other's seat shows card backs.
  await A.getByTestId('holdem-deal').click();
  await expect(A.getByTestId('holdem-hole').locator('[data-card]')).toHaveCount(2, { timeout: 15_000 });
  await expect(B.getByTestId('holdem-hole').locator('[data-card]')).toHaveCount(2, { timeout: 15_000 });
  const aliceHole = await cardsIn(a.page, '[data-testid=holdem-hole]');
  const bobHole = await cardsIn(b.page, '[data-testid=holdem-hole]');
  expect(new Set([...aliceHole, ...bobHole]).size).toBe(4);
  // Bob's page never shows Alice's hole cards before the showdown, nor the other way round.
  const bobSees = await cardsIn(b.page, '.holdem');
  for (const c of aliceHole) expect(bobSees).not.toContain(c);
  const aliceSees = await cardsIn(a.page, '.holdem');
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
    await a.page.waitForTimeout(250);
  }
  await expect(status).toContainText(' wins ', { timeout: 15_000 });
  await expect(A.getByTestId('holdem-status')).toHaveText((await status.textContent()) ?? '');

  // Showdown: five board cards, both hands revealed, chips still add up to 2 x 1000.
  await expect(B.getByTestId('holdem-board').locator('[data-card]')).toHaveCount(5);
  const chips = async (p: Page) =>
    (await frame(p).locator('.seat:not(.empty) .chips').allTextContents()).map(Number);
  const total = (await chips(b.page)).reduce((s, n) => s + n, 0);
  expect(total).toBe(2000);
  for (const c of aliceHole) expect(await cardsIn(b.page, '.holdem')).toContain(c);
}
