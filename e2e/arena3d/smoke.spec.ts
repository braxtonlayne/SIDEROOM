import { expect, test } from '@playwright/test';

// ACT-A3D-001 smoke: two players open the built starter with the local stand-in platform (?user=a, ?user=b; tabs of
// one browser context find each other over a BroadcastChannel and the first one hosts). Both press Play and both
// see both names in the roster. Headless browsers have no WebGPU adapter, so this runs on the WebGL2 fallback,
// which is fine; Chromium only (the cross-engine proof is replay.spec.ts).

test('two players over the fake platform both appear in the roster', async ({
  browser,
  browserName,
}) => {
  test.skip(
    browserName !== 'chromium',
    'the roster smoke runs in Chromium; replay.spec.ts covers every engine',
  );
  const context = await browser.newContext();
  const errors: string[] = [];
  const open = async (user: string) => {
    const page = await context.newPage();
    page.on('pageerror', (e) => errors.push(`${user}: ${e.message}`));
    await page.goto(`/index.html?user=${user}&instance=smoke`);
    const play = page.locator('#btn-play');
    await expect(play).toBeEnabled({ timeout: 60_000 });
    return page;
  };
  const a = await open('a');
  const b = await open('b');
  for (const page of [a, b]) {
    await expect(page.locator('#status')).toContainText(/WebGL2|WebGPU/);
    await page.locator('#btn-play').click();
    await expect(page.locator('#hud')).toBeVisible();
  }
  await expect(a.locator('#status')).toContainText('you are hosting');
  await expect(b.locator('#status')).toContainText('joining the host');
  for (const page of [a, b]) {
    const roster = page.locator('#hud-players li');
    await expect(roster).toHaveCount(2, { timeout: 30_000 });
    await expect(roster.filter({ hasText: /^a\b/ })).toHaveCount(1);
    await expect(roster.filter({ hasText: /^b\b/ })).toHaveCount(1);
  }
  await expect(a.locator('#hud-players')).toContainText('a (you) · host');
  await expect(b.locator('#hud-players')).toContainText('b (you)');
  expect(errors).toEqual([]);
  await context.close();
});
