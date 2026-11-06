import { expect, test } from '../support/world';

/**
 * UTL-AUD-001: the shared audio trimmer in a real browser (Chromium: it records WebM Opus, which the voice path
 * takes since FU-7, and it has the fake microphone from the config).
 *
 *   1. drawer tool: open a generated 12 s WAV, cut it to the first 3 s and export a WAV of that length.
 *   2. voice message: record a moment, trim it, and send it; the message shows as a voice message.
 *   3. soundboard: upload the 12 s WAV; the same tool cuts it to the five second limit and the upload goes through.
 *
 * The WAV is written here (no downloaded files). The Owner's one-person server is the `solo` fixture's.
 */
test.use({ bypassCSP: true });
test.describe.configure({ timeout: 90_000 });

/** A mono 16-bit PCM WAV of `seconds` of a 440 Hz tone at 8 kHz. */
function toneWav(seconds: number): Buffer {
  const rate = 8000;
  const frames = seconds * rate;
  const b = Buffer.alloc(44 + frames * 2);
  b.write('RIFF', 0);
  b.writeUInt32LE(36 + frames * 2, 4);
  b.write('WAVEfmt ', 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(rate, 24);
  b.writeUInt32LE(rate * 2, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write('data', 36);
  b.writeUInt32LE(frames * 2, 40);
  for (let i = 0; i < frames; i++) {
    b.writeInt16LE(Math.round(8000 * Math.sin((2 * Math.PI * 440 * i) / rate)), 44 + i * 2);
  }
  return b;
}

test.beforeEach(({ browserName }) => {
  test.skip(browserName !== 'chromium', 'Chromium records WebM Opus and has the fake microphone.');
});

test('the drawer tool cuts a WAV to the chosen part and exports it', async ({ solo }) => {
  const page = solo.a.page;
  await page.goto(`/channels/${solo.guildId}/${solo.channelId}`);
  await expect(page.getByRole('log')).toBeVisible();
  const drawer = page.getByRole('dialog', { name: 'Drawer', exact: true });
  await expect(async () => {
    await page.keyboard.press('ControlOrMeta+k');
    await expect(drawer).toBeVisible({ timeout: 1000 });
  }).toPass({ timeout: 20_000 });
  await page.keyboard.type('>audio');
  await page.keyboard.press('Enter');

  const tool = drawer.getByRole('group', { name: 'Audio trimmer and normalizer' });
  await tool
    .getByLabel('Choose an audio file')
    .setInputFiles({ name: 'tone.wav', mimeType: 'audio/wav', buffer: toneWav(12) });
  await expect(tool.getByTestId('audio-range')).toContainText('0.00 s to 12.00 s');
  await tool.getByLabel('End').fill('3');
  await expect(tool.getByTestId('audio-range')).toContainText('3.00 s kept');

  const download = page.waitForEvent('download');
  await tool.getByRole('button', { name: 'Export WAV' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('tone-trimmed.wav');
  const path = await file.path();
  const { readFileSync } = await import('node:fs');
  const bytes = readFileSync(path);
  expect(bytes.subarray(0, 4).toString()).toBe('RIFF');
  // 3 s of 16-bit mono PCM at the 48 kHz the tool decodes to, plus the 44 byte header.
  expect(bytes.byteLength).toBe(44 + 3 * 48000 * 2);
});

test('a recorded voice message is trimmed and sent', async ({ solo }) => {
  const page = solo.a.page;
  await page.goto(`/channels/${solo.guildId}/${solo.channelId}`);
  await expect(page.getByRole('textbox', { name: /^Message #/ })).toBeEnabled();
  await page.getByRole('button', { name: 'Record a voice message' }).click();
  const recorder = page.getByRole('group', { name: 'Voice message', exact: true });
  await expect(recorder.getByRole('timer')).toBeVisible();
  await page.waitForTimeout(2500);
  await recorder.getByRole('button', { name: 'Stop' }).click();
  await recorder.getByRole('button', { name: 'Trim' }).click();

  const tool = recorder.getByRole('group', { name: 'Audio trimmer and normalizer' });
  await expect(tool.getByTestId('audio-range')).toBeVisible();
  await tool.getByLabel('End').fill('1');
  await tool.getByRole('button', { name: 'Use this' }).click();
  await expect(recorder.getByLabel('Voice message preview')).toBeVisible();

  // The trimmed part goes up as WebM Opus (the voice path takes it since FU-7). The upload slot is requested first:
  // that request names the file, so it proves the format even where object storage is not reachable from the
  // browser (a bare local stack uses memory storage); with real storage (CI's Garage) the message then shows.
  const slot = page.waitForRequest(
    (r) => r.method() === 'POST' && /\/channels\/\d+\/attachments$/.test(r.url()),
  );
  await recorder.getByRole('button', { name: 'Send' }).click();
  const file = (await slot).postDataJSON().files[0];
  expect(file.filename).toMatch(/-trimmed\.webm$/);
  expect(file.content_type).toMatch(/^audio\/webm/);
  if (process.env.SR_E2E_LIVE === '1') {
    await expect(page.getByRole('log').getByRole('group', { name: 'Voice message' })).toBeVisible();
  }
});

test('the soundboard upload trims a long sound with the shared tool and uploads the part', async ({
  solo,
}) => {
  const page = solo.a.page;
  await page.goto(`/settings/server/${solo.guildId}/emoji`);
  await page.getByRole('tab', { name: 'Soundboard' }).click();
  await page.getByRole('button', { name: 'Upload a sound' }).click();
  const dialog = page.getByRole('dialog', { name: 'Upload a sound' });
  await dialog
    .getByLabel('Sound file')
    .setInputFiles({ name: 'long tone.wav', mimeType: 'audio/wav', buffer: toneWav(12) });
  const tool = dialog.getByRole('group', { name: 'Audio trimmer and normalizer' });
  await expect(tool.getByText(/12\.0 seconds long and the limit is 5/)).toBeVisible();
  // The part is capped at the soundboard's five seconds.
  await tool.getByLabel('End').fill('9');
  await expect(tool.getByTestId('audio-range')).toContainText('4.00 s to 9.00 s');
  await expect(dialog.getByRole('button', { name: 'Upload' })).toBeDisabled();
  await tool.getByRole('button', { name: 'Use this' }).click();
  await expect(dialog.getByText('Ready to upload: long tone-trimmed.wav')).toBeVisible();

  const posted = page.waitForResponse(
    (r) =>
      r.request().method() === 'POST' &&
      r.url().endsWith(`/guilds/${solo.guildId}/soundboard-sounds`),
  );
  await dialog.getByRole('button', { name: 'Upload' }).click();
  const res = await posted;
  expect(res.status()).toBe(201);
  expect((await res.request().postDataJSON()).sound).toMatch(/^data:audio\/wav;base64,/);
  await expect(dialog).toBeHidden();
});
