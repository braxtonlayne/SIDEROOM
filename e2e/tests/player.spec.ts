import { expect, type Page } from '@playwright/test';
import { blocking, describe, runAxe, settle } from '../support/a11y';
import {
  currentTime,
  expectAdvancing,
  goInApp,
  isPaused,
  serveTone,
  setSkin,
  startPlaying,
} from '../support/playerAudio';
import { test } from '../support/world';

/**
 * MUS-PLY-001: the persistent floating player in a real browser, signed in as the Owner (full stack: API and
 * database, like the other signed-in specs). The player belongs to the signed-in app only, so none of these
 * cases runs on a public page; `player-layout.spec.ts` covers the geometry with a mocked session and no stack.
 *
 * The page is never reloaded in these cases (a reload would end any audio): moves between routes are in-app
 * history changes, which the router follows exactly like a clicked link. The player is driven through
 * `window.__srPlayerActions` / `window.__srPlayer`, which the dev server build exposes for browser tests; the
 * audio is a generated WAV tone served from a route handler, so no media file or bucket is needed.
 */
test.use({ bypassCSP: true });

/** The floating player itself; Settings > Appearance has its own "Music player" card with the same name. */
const playerRegion = (page: Page) =>
  page.getByRole('region', { name: 'Music player' }).and(page.locator('.sr-ply'));

test('a track started in Cinema keeps playing in Settings', async ({ solo }) => {
  const page = solo.a.page;
  await serveTone(page);
  await page.goto('/cinema');
  await expect(page.getByRole('main')).toBeVisible();
  await startPlaying(page);
  await expect(playerRegion(page)).toBeVisible();

  await goInApp(page, '/settings/user/appearance');
  await expect(page.getByRole('heading', { name: 'Appearance' })).toBeVisible();
  // Same page load, same audio element, still advancing, and the player is still on screen.
  await expect(playerRegion(page)).toBeVisible();
  await expectAdvancing(page);

  // The skin picker in Settings changes the live player.
  await page.getByRole('combobox', { name: 'Player skin' }).first().click();
  await page.getByRole('option', { name: 'Compact Pill / Dock' }).click();
  await expect(page.locator('[data-skin="pill"].sr-ply')).toBeVisible();
  await expectAdvancing(page);
});

test('playback survives the Lounge, Library and Settings routes, and its keys work there', async ({
  solo,
}) => {
  const page = solo.a.page;
  await serveTone(page);
  await page.goto('/cinema');
  await expect(page.getByRole('main')).toBeVisible();
  await startPlaying(page);
  const origin = await page.evaluate(() => performance.timeOrigin);

  for (const path of ['/channels/@me', '/library', '/settings/user/appearance']) {
    await goInApp(page, path);
    await expect(playerRegion(page)).toBeVisible();
    await expectAdvancing(page);
  }
  expect(await page.evaluate(() => performance.timeOrigin)).toBe(origin);
  // The two gapless decks (MUS-LIB-001), the same elements since the first track.
  expect(await page.locator('audio').count()).toBe(2);

  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect.poll(() => isPaused(page)).toBe(true);
  await page.keyboard.press('Alt+Shift+P');
  await expect.poll(() => isPaused(page)).toBe(false);
  await page.keyboard.press('Alt+Shift+ArrowRight');
  await expect(page.getByText('2/3')).toBeVisible();
});

test('the player is not served on public pages, and a sign-out ends it', async ({ solo }) => {
  const page = solo.a.page;
  await serveTone(page);
  await page.goto('/cinema');
  await expect(page.getByRole('main')).toBeVisible();
  await startPlaying(page);
  // Landing on a public page (what a sign-out does) removes the player and stops the sound.
  await goInApp(page, '/help/whats-new');
  await expect(playerRegion(page)).toHaveCount(0);
  await expect(page.locator('audio')).toHaveCount(0);
});

test('each skin works and passes axe', async ({ solo }) => {
  const page = solo.a.page;
  await serveTone(page);
  await page.goto('/cinema');
  await expect(page.getByRole('main')).toBeVisible();
  await startPlaying(page);

  for (const skin of ['retro', 'pill', 'library'] as const) {
    await setSkin(page, skin);
    const skinRoot = page.locator(`.sr-ply[data-skin="${skin}"]`);
    await settle(page);
    const found = await runAxe(page, '.sr-ply-dock');
    expect(blocking(found), describe(found).join('\n')).toEqual([]);
    // Every skin pauses and resumes the same audio element.
    await skinRoot.getByRole('button', { name: 'Pause', exact: true }).click();
    await expect.poll(() => isPaused(page)).toBe(true);
    await skinRoot.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(() => isPaused(page)).toBe(false);
  }
  await expectAdvancing(page);
  expect(await currentTime(page)).toBeGreaterThan(0);
});

test.describe('reduced motion', () => {
  test.use({ reducedMotion: 'reduce' });
  test('stops the ticker', async ({ solo }) => {
    const page = solo.a.page;
    await serveTone(page);
    await page.goto('/cinema');
    await expect(page.getByRole('main')).toBeVisible();
    await startPlaying(page);
    await expect(page.locator('.sr-ply-ticker').first()).toHaveAttribute('data-motion', 'off');
    const animated = await page.evaluate(() =>
      document
        .getAnimations()
        .some((a) => a instanceof CSSAnimation && a.animationName.startsWith('sr-ply')),
    );
    expect(animated).toBe(false);
  });
});
