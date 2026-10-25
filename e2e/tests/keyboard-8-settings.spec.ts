import { expect, test } from '../support/world';
import {
  LINKS_NOT_TABBABLE,
  isFocused,
  tabTo,
  tabsToLinks,
  textHistory,
  watchText,
} from '../support/keys';

/**
 * RDY-A11Y-003, flow 8: Settings with the keyboard only. Open them from the user panel, move to a section with the
 * arrow keys, change a value, save it ("Settings saved" is announced by the toast's status region) and close with
 * Escape; focus ends up back in the Lounge. The change is reverted before the end (the Owner's account is shared).
 */
test('settings: open, change a value, hear "Settings saved", close with Escape', async ({
  solo,
}) => {
  const { a, guildId, channelId } = solo;
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  await watchText(page, '[role="status"][aria-label="Notifications"]', '__toasts');
  await page.goto(`/channels/${guildId}/${channelId}`);
  await expect(page.getByRole('log')).toBeVisible();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main')).toBeFocused();

  // Open: the "User Settings" button of the user panel sits before <main>.
  const open = page.getByRole('button', { name: 'User Settings' });
  await tabTo(page, open, { back: true, max: 20 });
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/settings\/user\//);
  const settingsRoot = page.getByRole('main');
  await expect(settingsRoot).toBeVisible();

  // The section list is one tab stop; arrows move, Enter opens. Walk to Accessibility.
  const accessibility = page.getByRole('button', { name: 'Accessibility', exact: true });
  const navStop = page.getByRole('navigation').locator('button[tabindex="0"]').first();
  await tabTo(page, navStop, { max: 12 });
  for (let i = 0; i < 25 && !(await isFocused(accessibility)); i++)
    await page.keyboard.press('ArrowDown');
  await expect(accessibility).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/settings\/user\/accessibility/);
  await expect(page.getByRole('heading', { name: 'Accessibility', level: 1 })).toBeVisible();

  // Change a value with Space, save with Enter, and hear it.
  const underline = page.getByRole('switch', { name: /Always underline links/ });
  await tabTo(page, underline, { max: 40 });
  const before = await underline.isChecked();
  await page.keyboard.press('Space');
  expect(await underline.isChecked()).toBe(!before);
  const save = page.getByRole('button', { name: /^Save/ });
  await tabTo(page, save, { max: 12 });
  await page.keyboard.press('Enter');
  await expect
    .poll(() => textHistory(page, '__toasts'))
    .toContainEqual(expect.stringContaining('Settings saved'));

  // Put it back (and save again), so the shared Owner account is as it was.
  await tabTo(page, underline, { max: 40, back: true });
  await page.keyboard.press('Space');
  expect(await underline.isChecked()).toBe(before);
  await tabTo(page, save, { max: 12 });
  await page.keyboard.press('Enter');
  await expect(save).toBeHidden();

  // Escape closes Settings and hands focus back to the Lounge.
  await page.keyboard.press('Escape');
  await expect(page).toHaveURL(new RegExp(`/channels/${guildId}/${channelId}`));
  await expect(page.getByRole('log')).toBeVisible();
  // Focus is back on the control that opened Settings, not lost to <body>.
  await expect(open).toBeFocused();
});
