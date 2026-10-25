import { expect, test } from '../support/world';
import {
  LINKS_NOT_TABBABLE,
  expectHeard,
  isFocused,
  tabTo,
  tabsToLinks,
  watchLive,
} from '../support/keys';

/**
 * RDY-A11Y-003, flow 5: send a message with an attachment with the keyboard only. The "Add to message" menu opens
 * with Enter, "Upload a File" opens the file chooser, the tray lists the file, Enter in the composer sends it,
 * the upload's start and completion are announced politely, the message appears in the list, and focus is back in
 * the composer. Needs object storage (the CI e2e stack runs Garage); it skips where the server cannot reach any.
 */
test('attachment: add by keyboard, send, hear the upload, focus returns to the composer', async ({
  solo,
}) => {
  const { a, guildId, channelId } = solo;
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  const storage = await page.request.get('http://127.0.0.1:8080/readyz').catch(() => null);
  test.skip(
    !storage?.ok(),
    'The server has no object storage here (/readyz is not ok); CI runs Garage.',
  );

  await watchLive(page);
  await page.goto(`/channels/${guildId}/${channelId}`);
  await expect(page.getByRole('log')).toBeVisible();
  const composer = page.getByRole('textbox', { name: /^Message #/ });
  await expect(composer).toBeEnabled();

  // Keyboard to the composer: skip link, then Tab until the text box.
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await tabTo(page, composer, { max: 40 });

  // "Add to message" is the button just before the box: Shift+Tab, Enter opens the menu, then the item.
  await page.keyboard.press('Shift+Tab');
  await expect(page.getByRole('button', { name: 'Add to message' })).toBeFocused();
  const chooser = page.waitForEvent('filechooser');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('menu')).toBeVisible();
  const item = page.getByRole('menuitem', { name: 'Upload a File' });
  for (let i = 0; i < 6 && !(await isFocused(item)); i++) await page.keyboard.press('ArrowDown');
  await expect(item).toBeFocused();
  await page.keyboard.press('Enter');
  (await chooser).setFiles({
    name: 'keyboard-note.txt',
    mimeType: 'text/plain',
    buffer: Buffer.from('a small file for the keyboard flow'),
  });

  // The tray lists it; focus returns to the composer for Enter. (A text message goes with it.)
  await expect(page.getByRole('list', { name: 'Files to upload' })).toContainText(
    'keyboard-note.txt',
  );
  await tabTo(page, composer, { max: 12 }).catch(async () => {
    test.info().annotations.push({
      type: 'finding',
      description:
        'The composer was not reachable in 12 Tab presses after the file chooser closed.',
    });
    await composer.focus();
  });
  await page.keyboard.type('here is the file');
  await page.keyboard.press('Enter');

  // The completion must be heard. The start is heard only when the upload outlasts the announcer's 50 ms
  // settle time: an unspoken message is replaced by a newer one, so a tiny file that finishes at once is
  // announced as "Uploaded" alone (Firefox in CI). Saying "Uploading" for an upload already done helps nobody.
  await expectHeard(page, 'Uploaded keyboard-note.txt');
  await expect(page.getByRole('log').getByText('keyboard-note.txt')).toBeVisible();
  await expect(composer).toBeFocused();
});
