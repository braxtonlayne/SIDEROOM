import { expect, test, type Page } from '@playwright/test';

/**
 * Presses "Open the door" on the filled-in registration form and waits for the Lounge.
 *
 * Registration allows 5 accounts per minute per address (`POST /sr/register`, doc 06), and every spec
 * registers from 127.0.0.1, so a full run can register its sixth account inside one minute: the server
 * answers 429 with `retry_after` and the page shows "Too many tries". Like `registerUser` in
 * `fixtures.ts` does for the API, this waits out that window (the test's time budget is extended by the
 * wait) and presses again, instead of failing the spec on run order.
 */
export async function openTheDoor(page: Page): Promise<void> {
  for (let attempt = 0; ; attempt++) {
    const answer = page.waitForResponse(
      (r) => r.request().method() === 'POST' && new URL(r.url()).pathname.endsWith('/sr/register'),
    );
    await page.getByRole('button', { name: 'Open the door' }).click();
    const res = await answer;
    if (res.status() === 429 && attempt < 3) {
      const body = (await res.json().catch(() => ({}))) as { retry_after?: number };
      const retry = Number(body.retry_after ?? 60);
      // The 20-a-day rule answers with hours: nothing to wait out, let the URL check fail.
      if (retry > 70) break;
      const wait = retry + 1;
      test.info().setTimeout(test.info().timeout + wait * 1000);
      await page.waitForTimeout(wait * 1000);
      continue;
    }
    break;
  }
  await expect(page).toHaveURL(/\/channels/);
}
