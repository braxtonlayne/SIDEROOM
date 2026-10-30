import { expect, test as base, type Page } from '@playwright/test';
import { blocking, describe, runAxe, settle } from '../support/a11y';
import { test as worldTest } from '../support/world';

/**
 * MUS-PLY-001: the persistent floating player in a real browser.
 *
 * - `a track started in Cinema keeps playing in Settings` is the acceptance case: it needs the full stack
 *   (a signed-in Owner), like the other signed-in specs.
 * - The cases below it need only the web app: the player host sits above the router, so a track started on a
 *   public page must survive in-app navigation to another one, the three skins must each be usable and pass axe,
 *   and reduced motion must stop the ticker.
 *
 * The page is never reloaded in these cases (a reload would end any audio): moves between routes are in-app
 * history changes, which the router follows exactly like a clicked link. The player is driven through
 * `window.__srPlayer`, which the dev server build exposes for browser tests (PlayerHost.tsx); the audio is a
 * generated WAV tone served from a route handler, so no media file or bucket is needed.
 */
base.use({ bypassCSP: true });
worldTest.use({ bypassCSP: true });

/** A mono 8 kHz 8-bit WAV of a 440 Hz tone. */
function toneWav(seconds: number): Buffer {
  const rate = 8000;
  const n = rate * seconds;
  const buf = Buffer.alloc(44 + n);
  buf.write('RIFF', 0);
  buf.writeUInt32LE(36 + n, 4);
  buf.write('WAVEfmt ', 8);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(rate, 24);
  buf.writeUInt32LE(rate, 28);
  buf.writeUInt16LE(1, 32);
  buf.writeUInt16LE(8, 34);
  buf.write('data', 36);
  buf.writeUInt32LE(n, 40);
  for (let i = 0; i < n; i++)
    buf[44 + i] = 128 + Math.round(60 * Math.sin((2 * Math.PI * 440 * i) / rate));
  return buf;
}

const TONE = toneWav(40);

/** Serve the tone with byte ranges, the way a presigned bucket URL does. */
async function serveTone(page: Page): Promise<void> {
  await page.route('**/e2e-tone-*.wav', async (route) => {
    const range = /bytes=(\d+)-(\d*)/.exec((await route.request().headerValue('range')) ?? '');
    if (!range) {
      await route.fulfill({
        status: 200,
        body: TONE,
        headers: { 'content-type': 'audio/wav', 'accept-ranges': 'bytes' },
      });
      return;
    }
    const from = Number(range[1]);
    const to = range[2] ? Math.min(Number(range[2]), TONE.length - 1) : TONE.length - 1;
    await route.fulfill({
      status: 206,
      body: TONE.subarray(from, to + 1),
      headers: {
        'content-type': 'audio/wav',
        'accept-ranges': 'bytes',
        'content-range': `bytes ${from}-${to}/${TONE.length}`,
      },
    });
  });
}

const TRACKS = [1, 2, 3].map((n) => ({
  id: `e2e-${n}`,
  title: `Tone ${n}`,
  artist: 'E2E',
  src: `/e2e-tone-${n}.wav`,
}));

interface Handle {
  audio: HTMLAudioElement;
}

async function startPlaying(page: Page): Promise<void> {
  await page.waitForFunction(() => document.readyState === 'complete');
  // The browser lets sound start only after the member has touched the page.
  await page.mouse.click(2, 2);
  await page.evaluate((tracks) => {
    // The store's actions are on `window.__srPlayerActions` in dev builds; the host (and `__srPlayer`, which
    // carries the audio element) loads on demand once the first track is queued.
    (window as unknown as { __srPlayerActions: { play(t: unknown): void } }).__srPlayerActions.play(
      tracks,
    );
  }, TRACKS);
  await page.waitForFunction(() => (window as unknown as { __srPlayer?: Handle }).__srPlayer);
  await expect.poll(() => currentTime(page), { timeout: 15_000 }).toBeGreaterThan(0.4);
}

const currentTime = (page: Page) =>
  page.evaluate(() => (window as unknown as { __srPlayer: Handle }).__srPlayer.audio.currentTime);
const isPaused = (page: Page) =>
  page.evaluate(() => (window as unknown as { __srPlayer: Handle }).__srPlayer.audio.paused);

/** Move to another route without reloading: the router follows history events like a clicked link. */
async function goInApp(page: Page, path: string): Promise<void> {
  await page.evaluate((p) => {
    window.history.pushState({}, '', p);
    window.dispatchEvent(new PopStateEvent('popstate'));
  }, path);
  await expect(page).toHaveURL(new RegExp(`${path.replace(/[/]/g, '\\/')}$`));
}

async function expectAdvancing(page: Page): Promise<void> {
  const before = await currentTime(page);
  await expect.poll(() => currentTime(page), { timeout: 10_000 }).toBeGreaterThan(before + 0.5);
  expect(await isPaused(page)).toBe(false);
}

worldTest('a track started in Cinema keeps playing in Settings', async ({ solo }) => {
  const page = solo.a.page;
  await serveTone(page);
  await page.goto('/cinema');
  await expect(page.getByRole('main')).toBeVisible();
  await startPlaying(page);
  await expect(page.getByRole('region', { name: 'Music player' })).toBeVisible();

  await goInApp(page, '/settings/user/appearance');
  await expect(page.getByRole('heading', { name: 'Appearance' })).toBeVisible();
  // Same page load, same audio element, still advancing, and the player is still on screen.
  await expect(page.getByRole('region', { name: 'Music player' })).toBeVisible();
  await expectAdvancing(page);

  // The skin picker in Settings changes the live player.
  await page.getByRole('combobox', { name: 'Player skin' }).first().click();
  await page.getByRole('option', { name: 'Compact Pill / Dock' }).click();
  await expect(page.locator('[data-skin="pill"].sr-ply')).toBeVisible();
  await expectAdvancing(page);
});

base('playback survives navigation between public routes', async ({ page }) => {
  await serveTone(page);
  await page.goto('/help');
  await startPlaying(page);
  const first = await page.evaluate(() => performance.timeOrigin);

  await goInApp(page, '/login');
  await expect(page.getByRole('region', { name: 'Music player' })).toBeVisible();
  await expectAdvancing(page);
  // No reload happened: the page load is the same one.
  expect(await page.evaluate(() => performance.timeOrigin)).toBe(first);
  expect(await page.locator('audio').count()).toBe(1);

  // The player's own controls and its keys work on this route.
  await page.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect.poll(() => isPaused(page)).toBe(true);
  await page.keyboard.press('Alt+Shift+P');
  await expect.poll(() => isPaused(page)).toBe(false);
  await page.keyboard.press('Alt+Shift+ArrowRight');
  await expect(page.getByText('2/3')).toBeVisible();
  await expect
    .poll(() =>
      page.evaluate(() => (window as unknown as { __srPlayer: Handle }).__srPlayer.audio.src),
    )
    .toContain('e2e-tone-2');
});

base('each skin works and passes axe', async ({ page }) => {
  await serveTone(page);
  await page.goto('/help');
  await startPlaying(page);

  for (const [skin, name] of [
    ['retro', 'Retro MP3 Hardware'],
    ['pill', 'Compact Pill / Dock'],
    ['library', 'Full Library Manager'],
  ] as const) {
    await page
      .getByRole('region', { name: 'Music player' })
      .getByRole('combobox', { name: 'Player skin' })
      .selectOption({ label: name });
    const skinRoot = page.locator(`.sr-ply[data-skin="${skin}"]`);
    await expect(skinRoot).toBeVisible();
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
});

base.describe('reduced motion', () => {
  base.use({ reducedMotion: 'reduce' });
  base('stops the ticker', async ({ page }) => {
    await serveTone(page);
    await page.goto('/help');
    await startPlaying(page);
    await expect(page.locator('.sr-ply-ticker')).toHaveAttribute('data-motion', 'off');
    const animated = await page.evaluate(() =>
      document
        .getAnimations()
        .some((a) => a instanceof CSSAnimation && a.animationName.startsWith('sr-ply')),
    );
    expect(animated).toBe(false);
  });
});

base('the ticker scrolls while playing when motion is allowed', async ({ page }) => {
  await serveTone(page);
  await page.goto('/help');
  await startPlaying(page);
  await expect(page.locator('.sr-ply-ticker')).toHaveAttribute('data-motion', 'on');
});
