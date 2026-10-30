import { expect, test } from '../support/world';

/**
 * UTL-IMG-001: edit a pasted image in the composer and send it. A PNG is pasted into the message box, the tray's
 * "Edit" opens the image studio, the image is padded to a square and captioned, the edited file replaces the
 * pending attachment, and the sent message carries the EDITED picture (a square, named .png) and not the pasted one.
 * Needs object storage like the other attachment cases (it skips where /readyz is not ok); Chromium only, because
 * the paste is a synthetic ClipboardEvent with a DataTransfer.
 *
 * Local run (ports 8082 and 5175 belong to this agent in wave 32):
 *   SR_E2E_BASE_URL=http://localhost:5175 SR_E2E_SERVER_PORT=8082 pnpm --filter e2e exec playwright test studio --project=chromium
 */
test.skip(
  ({ browserName }) => browserName !== 'chromium',
  'Synthetic clipboard paste: Chromium only.',
);

test('paste an image in the composer, edit it in the studio, send the edited image', async ({
  solo,
}) => {
  const { a, guildId, channelId } = solo;
  const page = a.page;
  const storage = await page.request.get('http://127.0.0.1:8080/readyz').catch(() => null);
  test.skip(
    !storage?.ok(),
    'The server has no object storage here (/readyz is not ok); CI runs Garage.',
  );

  await page.goto(`/channels/${guildId}/${channelId}`);
  const composer = page.getByRole('textbox', { name: /^Message #/ });
  await expect(composer).toBeEnabled();

  // Paste a 240 x 120 PNG (wider than tall) into the message box.
  await composer.focus();
  await page.evaluate(async () => {
    const canvas = document.createElement('canvas');
    canvas.width = 240;
    canvas.height = 120;
    const ctx = canvas.getContext('2d')!;
    ctx.fillStyle = '#3366cc';
    ctx.fillRect(0, 0, 240, 120);
    ctx.fillStyle = '#ffcc00';
    ctx.fillRect(20, 20, 60, 60);
    const blob = await new Promise<Blob>((r) => canvas.toBlob((b) => r(b!), 'image/png'));
    const data = new DataTransfer();
    data.items.add(new File([blob], 'pasted.png', { type: 'image/png' }));
    document.activeElement!.dispatchEvent(
      new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
    );
  });
  const tray = page.getByRole('list', { name: 'Files to upload' });
  await expect(tray).toContainText('pasted.png');

  // Edit it: pad to a square, add a caption, export, use the edited image.
  await tray.getByRole('button', { name: 'Edit pasted.png in the image studio' }).click();
  const studio = page.getByRole('dialog', { name: 'Image Studio' });
  await expect(studio.getByRole('img', { name: 'Image preview, 240 by 120 pixels' })).toBeVisible();
  await studio.getByRole('button', { name: 'Pad', exact: true }).click();
  await studio.getByRole('button', { name: 'Pad to a square' }).click();
  await expect(studio.getByRole('img', { name: 'Image preview, 240 by 240 pixels' })).toBeVisible();
  await studio.getByRole('button', { name: 'Caption', exact: true }).click();
  await studio.getByLabel('Top text').fill('edited in the studio');
  await studio.getByRole('button', { name: 'Add the caption' }).click();
  await studio.getByRole('button', { name: 'Export', exact: true }).click();
  await studio.getByRole('button', { name: 'Use the edited image' }).click();
  await expect(studio).toBeHidden();
  await expect(tray).toContainText('pasted.png');

  // Send. The message shows an image, and the stored file is the square one.
  await composer.fill('edited');
  await composer.press('Enter');
  const log = page.getByRole('log');
  const img = log.locator('img[src*="pasted"]').first();
  await expect(img).toBeVisible({ timeout: 30_000 });
  const size = await img.evaluate(async (el) => {
    const i = el as HTMLImageElement;
    await i.decode();
    return [i.naturalWidth, i.naturalHeight];
  });
  expect(size).toEqual([240, 240]);
});

test('"Open in studio" in the media viewer works on a copy and leaves the message alone', async ({
  solo,
}) => {
  const { a, guildId, channelId } = solo;
  const page = a.page;
  const storage = await page.request.get('http://127.0.0.1:8080/readyz').catch(() => null);
  test.skip(
    !storage?.ok(),
    'The server has no object storage here (/readyz is not ok); CI runs Garage.',
  );

  await page.goto(`/channels/${guildId}/${channelId}`);
  const composer = page.getByRole('textbox', { name: /^Message #/ });
  await expect(composer).toBeEnabled();
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Add to message' }).click();
  await page.getByRole('menuitem', { name: 'Upload a File' }).click();
  const png = await page.evaluate(async () => {
    const c = document.createElement('canvas');
    c.width = 64;
    c.height = 32;
    c.getContext('2d')!.fillRect(0, 0, 64, 32);
    const b = await new Promise<Blob>((r) => c.toBlob((x) => r(x!), 'image/png'));
    return Array.from(new Uint8Array(await b.arrayBuffer()));
  });
  (await chooser).setFiles({
    name: 'original.png',
    mimeType: 'image/png',
    buffer: Buffer.from(png),
  });
  await composer.fill('original');
  await composer.press('Enter');
  const log = page.getByRole('log');
  await log
    .getByRole('button', { name: /original\.png/ })
    .first()
    .click();
  await page.getByTestId('open-in-studio').click();
  const studio = page.getByRole('dialog', { name: 'Image Studio' });
  await expect(studio.getByRole('img', { name: 'Image preview, 64 by 32 pixels' })).toBeVisible();
  await studio.getByRole('button', { name: 'Rotate right' }).click();
  await expect(studio.getByRole('img', { name: 'Image preview, 32 by 64 pixels' })).toBeVisible();
  await expect(
    studio.getByText('The studio works on a copy. The original message is not changed.'),
  ).toBeVisible();
  // Escape closes the studio, then the media viewer it was opened from.
  await page.keyboard.press('Escape');
  await expect(studio).toBeHidden();
  await page.keyboard.press('Escape');
  // The message still has exactly the one attachment, unchanged. An image shows its name as its accessible
  // name (alt text), not as visible text, so the check uses the same button the viewer was opened from.
  const attachment = log.getByRole('button', { name: /original\.png/ });
  await expect(attachment).toHaveCount(1);
  await expect(attachment).toBeVisible();
});
