import { expect, test } from '../support/world';
import {
  LINKS_NOT_TABBABLE,
  focusWithin,
  focusedName,
  isFocused,
  tabTo,
  tabsToLinks,
} from '../support/keys';

/**
 * RDY-A11Y-003, flow 4: a modal, a menu and the quick switcher with the keyboard only. For each: it opens from
 * the keyboard, focus lands inside and stays inside while Tab cycles, Escape closes it, and focus returns to the
 * control that opened it (to the control that had focus, for the shortcut).
 */
test('modal, menu and quick switcher: focus goes in, stays in, and comes back', async ({
  solo,
}) => {
  const { a, guildId, channelId } = solo;
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  await page.goto(`/channels/${guildId}/${channelId}`);
  await expect(page.getByRole('log')).toBeVisible();
  await expect(page.getByRole('main')).toBeVisible();

  // Park keyboard focus on <main> through the skip link; every walk below starts from there.
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main')).toBeFocused();

  // 1. Modal: "Add a Server".
  const add = page.getByRole('button', { name: 'Add a Server' });
  // "Add a Server" is part of the rail's roving list: Shift+Tab to the rail's one stop, then End and Up.
  await tabTo(page, page.getByRole('navigation', { name: 'Servers' }).locator('a[tabindex="0"]'), {
    back: true,
    max: 40,
  });
  await page.keyboard.press('End');
  for (let i = 0; i < 4 && !(await isFocused(add)); i++) await page.keyboard.press('ArrowUp');
  await expect(add).toBeFocused();
  await page.keyboard.press('Enter');
  const dialog = page.getByRole('dialog');
  await expect(dialog).toBeVisible();
  await expect.poll(() => focusWithin(page, dialog)).toBe(true);
  for (let i = 0; i < 12; i++) {
    await page.keyboard.press('Tab');
    expect(
      await focusWithin(page, dialog),
      `Tab ${i + 1} left the dialog: ${await focusedName(page)}`,
    ).toBe(true);
  }
  for (let i = 0; i < 12; i++) {
    await page.keyboard.press('Shift+Tab');
    expect(await focusWithin(page, dialog), `Shift+Tab ${i + 1} left the dialog`).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(dialog).toBeHidden();
  await expect(add).toBeFocused();

  // 2. Menu: the status menu in the user panel.
  const status = page.getByRole('button', { name: /^Set status/ });
  await tabTo(page, status, { max: 16 });
  await page.keyboard.press('Enter');
  const menu = page.getByRole('menu');
  await expect(menu).toBeVisible();
  await expect.poll(() => focusWithin(page, menu)).toBe(true);
  const first = await focusedName(page);
  await page.keyboard.press('ArrowDown');
  expect(await focusWithin(page, menu)).toBe(true);
  expect(await focusedName(page)).not.toBe(first);
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();
  await expect(status).toBeFocused();

  // 3. Quick switcher: Ctrl+K from wherever focus is; Escape returns it there.
  await page.keyboard.press('ControlOrMeta+k');
  const switcher = page.getByRole('dialog');
  await expect(switcher).toBeVisible();
  await expect.poll(() => focusWithin(page, switcher)).toBe(true);
  await page.keyboard.type('gen');
  await expect(switcher.getByText(/general/i).first()).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(switcher).toBeHidden();
  await expect(status).toBeFocused();
});
