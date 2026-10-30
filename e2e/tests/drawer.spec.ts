import { expect, test } from '@playwright/test';
import { blocking, describe, runAxe, settle } from '../support/a11y';
import { goInApp } from '../support/playerAudio';
import { openMockedApp } from '../support/playerMock';

/**
 * UTL-SHL-001: the utility drawer, gadgets and pins in a real browser (all three engines). The session is mocked
 * (every API call answers with an empty success, `/users/@me` with an id), the page is a signed-in app route
 * (`/preview-host`), so the case needs no API or database. Moves between routes are in-app history changes, which
 * the router follows exactly like a clicked link, so the page is not reloaded until the case asks for it.
 */
test.use({ bypassCSP: true });

const drawer = (page: import('@playwright/test').Page) =>
  page.getByRole('dialog', { name: 'Drawer', exact: true });
const gadget = (page: import('@playwright/test').Page) =>
  page.getByRole('dialog', { name: 'Drawer guide', exact: true });

/** The gate is a lazy chunk, so the key may arrive before its listener exists: press until the drawer shows. */
async function openDrawerByKey(page: import('@playwright/test').Page) {
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawer(page)).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
}

async function openGuide(page: import('@playwright/test').Page) {
  await openDrawerByKey(page);
  await page.keyboard.type('>guide');
  await expect(drawer(page).getByRole('tab', { name: 'Tools', selected: true })).toBeVisible();
  await page.keyboard.press('Enter');
}

test('Mod+K opens the drawer, a tool pops out, survives navigation and a reload; Escape leaves it alone', async ({
  page,
}) => {
  await openMockedApp(page);
  await openGuide(page);
  await drawer(page).getByRole('button', { name: 'Pop out' }).click();
  await expect(gadget(page)).toBeVisible();
  await expect(drawer(page)).toBeHidden();

  // Navigate inside the app: the gadget stays, the same element (the host is not remounted).
  await gadget(page).evaluate((el) => el.setAttribute('data-probe', 'same'));
  await goInApp(page, '/preview-elsewhere');
  await expect(gadget(page)).toHaveAttribute('data-probe', 'same');

  // Escape closes the drawer, never the gadget.
  await page.keyboard.press('ControlOrMeta+k');
  await expect(drawer(page)).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(drawer(page)).toBeHidden();
  await expect(gadget(page)).toBeVisible();

  // Moving it with the keyboard is remembered across a reload (per account).
  const box = await gadget(page).boundingBox();
  await gadget(page).getByRole('toolbar').focus();
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await expect
    .poll(async () => (await gadget(page).boundingBox())?.y ?? 0)
    .toBeGreaterThan((box?.y ?? 0) + 20);
  const moved = await gadget(page).boundingBox();
  await page.reload();
  await expect(gadget(page)).toBeVisible();
  const again = await gadget(page).boundingBox();
  expect(Math.abs((again?.y ?? 0) - (moved?.y ?? 0))).toBeLessThanOrEqual(1);

  await gadget(page).getByRole('button', { name: 'Close Drawer guide' }).click();
  await expect(gadget(page)).toBeHidden();
});

test('a tool pinned to the rail opens in a panel and unpins', async ({ page }) => {
  await openMockedApp(page);
  await openGuide(page);
  await drawer(page).getByRole('button', { name: 'Pin to the rail' }).click();
  await page.keyboard.press('Escape');
  await expect(drawer(page)).toBeHidden();
  const rail = page.getByRole('toolbar', { name: 'Pinned tools' });
  await expect(rail).toBeVisible();
  await goInApp(page, '/preview-elsewhere');
  await expect(rail).toBeVisible();
  await rail.getByRole('button', { name: 'Drawer guide' }).click();
  const panel = page.getByRole('region', { name: 'Drawer guide' });
  await expect(panel).toBeVisible();
  await panel.getByRole('button', { name: 'Unpin from the rail' }).click();
  await expect(rail).toBeHidden();
});

test('the drawer, a gadget and the rail have no serious accessibility findings', async ({
  page,
}) => {
  await openMockedApp(page);
  await openGuide(page);
  await settle(page);
  expect(describe(blocking(await runAxe(page, '[role="dialog"]')))).toEqual([]);
  await drawer(page).getByRole('button', { name: 'Pin to the rail' }).click();
  await drawer(page).getByRole('button', { name: 'Pop out' }).click();
  await expect(gadget(page)).toBeVisible();
  await settle(page);
  expect(describe(blocking(await runAxe(page, '.sr-tl-gadgets')))).toEqual([]);
  expect(describe(blocking(await runAxe(page, '.sr-tl-rail-wrap')))).toEqual([]);
});

test('the drawer fits a phone and respects reduced motion', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await page.setViewportSize({ width: 360, height: 740 });
  await openMockedApp(page);
  await openGuide(page);
  await drawer(page).getByRole('button', { name: 'Pop out' }).click();
  const box = await gadget(page).boundingBox();
  expect(box).not.toBeNull();
  expect(box!.x).toBeGreaterThanOrEqual(0);
  expect(box!.y).toBeGreaterThanOrEqual(0);
  expect(box!.x + box!.width).toBeLessThanOrEqual(360);
  expect(box!.y + box!.height).toBeLessThanOrEqual(740);
  const transition = await gadget(page)
    .getByRole('button', { name: 'Close Drawer guide' })
    .evaluate((e) => getComputedStyle(e).transitionDuration);
  // The app's reduced-motion rule shortens transitions to (almost) nothing.
  expect(Number.parseFloat(transition)).toBeLessThan(0.01);
});
