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
 * RDY-A11Y-003, flow 6: a voice call with the keyboard only. Join from the channel list with Enter, mute and
 * unmute with Space, leave with Enter; each step is announced by the polite live region ("Joined voice channel
 * General", "Microphone muted", "Left the voice channel"). A dropped call (assertive) is covered by the unit tests
 * of `a11y/voiceAnnounce.ts`; it is not forced here.
 *
 * Needs the voice stack (the `live` CI job: SR_VOICE__ENABLED and SR_E2E_VOICE=1) and Chromium's fake audio
 * device; set SR_E2E_VOICE=1 against a server that has voice on.
 */
test.skip(
  process.env.SR_E2E_VOICE !== '1',
  'Needs the voice stack of the live CI job; set SR_E2E_VOICE=1.',
);

test.use({
  permissions: ['microphone'],
  launchOptions: {
    args: [
      '--use-fake-device-for-media-stream',
      '--use-fake-ui-for-media-stream',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
      '--allow-loopback-in-peer-connection',
    ],
  },
});

test('voice: join, mute, unmute and leave by keyboard, each one announced', async ({
  solo,
  browserName,
}) => {
  test.skip(browserName !== 'chromium', 'Fake media flags are Chromium only.');
  const { a, guildId, channelId } = solo;
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  await watchLive(page);
  await page.goto(`/channels/${guildId}/${channelId}`);
  await expect(page.getByRole('log')).toBeVisible();
  await page.keyboard.press('Tab');
  await page.keyboard.press('Enter');
  await expect(page.getByRole('main')).toBeFocused();

  // The channel list is one tab stop; the voice channel is a button in it, reached with the arrows.
  const channels = page.getByRole('navigation', { name: /channels/i });
  await tabTo(page, channels.locator('[tabindex="0"]').first(), { back: true, max: 30 });
  const voice = channels.getByRole('button', { name: /voice channel/ });
  for (let i = 0; i < 12 && !(await isFocused(voice)); i++) await page.keyboard.press('ArrowDown');
  await expect(voice).toBeFocused();
  await page.keyboard.press('Enter');
  await expectHeard(page, /Joined voice channel/);
  await expect(voice).toBeFocused();

  // Mute and unmute from the user panel.
  const mute = page.getByRole('button', { name: 'Mute', exact: true });
  await tabTo(page, mute, { max: 30 });
  await page.keyboard.press('Space');
  await expectHeard(page, 'Microphone muted');
  await expect(mute).toBeFocused();
  await page.keyboard.press('Space');
  await expectHeard(page, 'Microphone unmuted');

  // Leave.
  const leave = page.getByRole('button', { name: 'Disconnect' }).first();
  await tabTo(page, leave, { max: 30 }).catch(() => tabTo(page, leave, { max: 30, back: true }));
  await page.keyboard.press('Enter');
  await expectHeard(page, 'Left the voice channel');
});
