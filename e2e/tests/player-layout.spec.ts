import { expect, test } from '@playwright/test';
import { blocking, describe, runAxe, settle } from '../support/a11y';
import { LONG_ARTIST, LONG_TITLE, measureFit, setSkin, startPlaying } from '../support/playerAudio';
import { openMockedApp } from '../support/playerMock';

/**
 * MUS-PLY-001 layout: each skin sits fully inside the viewport and nothing in it is clipped, with a long title
 * playing, at desktop, phone and the narrowest supported width. Needs no API or database (mocked session).
 */
test.use({ bypassCSP: true });

const SIZES = [
  { width: 1280, height: 800 },
  { width: 360, height: 740 },
  { width: 320, height: 640 },
];

for (const size of SIZES) {
  test(`every skin fits inside a ${size.width}x${size.height} viewport`, async ({ page }) => {
    await page.setViewportSize(size);
    await openMockedApp(page);
    await startPlaying(page);
    for (const skin of ['retro', 'pill', 'library'] as const) {
      await setSkin(page, skin);
      await expect(page.locator('.sr-ply-ticker').first()).toHaveAttribute(
        'title',
        `${LONG_TITLE} — ${LONG_ARTIST}`,
      );
      const fit = await measureFit(page);
      const where = `${skin} at ${size.width}`;
      expect(fit.panel, where).not.toBeNull();
      const p = fit.panel!;
      expect(p.left, where).toBeGreaterThanOrEqual(0);
      expect(p.top, where).toBeGreaterThanOrEqual(0);
      expect(p.right, where).toBeLessThanOrEqual(fit.viewport.width);
      expect(p.bottom, where).toBeLessThanOrEqual(fit.viewport.height);
      expect(fit.scrollOverflow, `${where}: panel scrolls horizontally`).toBeLessThanOrEqual(0);
      expect(fit.escaping, `${where}: boxes past the panel edge`).toEqual([]);
    }
  });
}

test('the title gets the free space in the pill and the library skin', async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await openMockedApp(page);
  await startPlaying(page);
  for (const skin of ['pill', 'library'] as const) {
    await setSkin(page, skin);
    const w = await page
      .locator(`.sr-ply[data-skin="${skin}"] .sr-ply-ticker`)
      .evaluate((e) => e.clientWidth);
    expect(w, skin).toBeGreaterThan(200);
  }
});

test('the dock has the Lounge tokens on its own (no shell around it)', async ({ page }) => {
  await openMockedApp(page);
  await startPlaying(page);
  const color = await page.locator('.sr-ply').evaluate((e) => getComputedStyle(e).color);
  expect(color).not.toBe('rgb(0, 0, 0)');
  const token = await page
    .locator('.sr-ply-dock')
    .evaluate((e) => getComputedStyle(e).getPropertyValue('--ui-accent'));
  expect(token.trim()).not.toBe('');
});

test('each skin passes axe (with contrast) on the Lounge Dark tokens', async ({ page }) => {
  await openMockedApp(page);
  await startPlaying(page);
  for (const skin of ['retro', 'pill', 'library'] as const) {
    await setSkin(page, skin);
    await settle(page);
    const found = await runAxe(page, '.sr-ply-dock');
    expect(blocking(found), describe(found).join('\n')).toEqual([]);
  }
});
