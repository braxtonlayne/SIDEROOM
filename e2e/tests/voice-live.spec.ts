import { readFileSync } from 'node:fs';
import { expect, test, type Page } from '@playwright/test';
import { STATE_FILE, type BootstrapState } from '../global-setup';
import { createInvite } from '../support/cli';
import { openTheDoor } from '../support/register';

/**
 * Voice live test: two Chromium contexts join one server voice channel with Chromium's fake audio device and
 * assert that remote audio actually arrives (RTCPeerConnection inbound-rtp bytes > 0 on both sides).
 *
 * Needs the full voice stack on the server under test: gateway op 4 / VOICE_STATE_UPDATE / VOICE_SERVER_UPDATE
 * and the SFU route `GET /voice/ws` (branch feat/p5-voice-sfu), with `voice.announce_ips` set to 127.0.0.1.
 * Set SR_E2E_VOICE=1 when running against such a server. Until the gateway wiring lands on the integration
 * branch this is skipped, and the client side is covered by unit tests with fake sockets and peer connections.
 *
 * Voice E2EE is on by default (D-249) in sr-server and the web build, so this runs an encrypted call: the bytes
 * counted are ciphertext frames. `e2ee/voice-e2ee.spec.ts` checks the frames themselves.
 */
const VOICE_STACK = process.env.SR_E2E_VOICE === '1';

test.use({
  permissions: ['microphone'],
  launchOptions: {
    args: [
      '--use-fake-device-for-media-stream',
      '--use-fake-ui-for-media-stream',
      // Let two local peers connect without mDNS candidate obfuscation.
      '--disable-features=WebRtcHideLocalIpsWithMdns',
      // The dev SFU announces 127.0.0.1: let Chromium form loopback ICE candidates.
      '--allow-loopback-in-peer-connection',
    ],
  },
});

const state = (): BootstrapState => JSON.parse(readFileSync(STATE_FILE, 'utf8')) as BootstrapState;

/** SR_E2E_VOICE_DEBUG=1 prints console errors and the voice/gateway WebSocket traffic of a page. */
function debugPage(page: Page, label: string) {
  if (process.env.SR_E2E_VOICE_DEBUG !== '1') return;
  page.on('console', (m) => {
    if (m.type() === 'error' || m.type() === 'warning')
      console.log(`[${label}] ${m.type()}: ${m.text()}`);
  });
  page.on('pageerror', (e) => console.log(`[${label}] pageerror: ${e.message}`));
  page.on('websocket', (ws) => {
    console.log(`[${label}] ws open ${ws.url()}`);
    const short = (d: string | Buffer) => String(d).slice(0, 160);
    ws.on('framesent', (f) => console.log(`[${label}] > ${short(f.payload)}`));
    ws.on('framereceived', (f) => console.log(`[${label}] < ${short(f.payload)}`));
    ws.on('close', () => console.log(`[${label}] ws close ${ws.url()}`));
  });
}

/** Record every RTCPeerConnection the page creates so the test can read its stats. */
async function trackPeerConnections(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as { __pcs: RTCPeerConnection[] };
    w.__pcs = [];
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(config?: RTCConfiguration) {
        super(config);
        w.__pcs.push(this);
      }
    } as typeof RTCPeerConnection;
    // The fake device is a beep every second: keep the voice-activity gate open so audio always flows.
    localStorage.setItem(
      'sr.voice.settings',
      JSON.stringify({
        state: { settings: { inputMode: 'vad', autoSensitivity: false, thresholdDb: -100 } },
        version: 1,
      }),
    );
  });
}

/** Bytes of inbound audio RTP received over all of the page's peer connections. */
async function inboundAudioBytes(page: Page): Promise<number> {
  return page.evaluate(async () => {
    const w = window as unknown as { __pcs?: RTCPeerConnection[] };
    let total = 0;
    for (const pc of w.__pcs ?? []) {
      const report = await pc.getStats();
      report.forEach((s: Record<string, unknown>) => {
        if (s.type === 'inbound-rtp' && s.kind === 'audio') total += Number(s.bytesReceived ?? 0);
      });
    }
    return total;
  });
}

test.describe('Voice live', () => {
  test.skip(
    !VOICE_STACK,
    'Needs the voice stack (gateway voice states + SFU /voice/ws from feat/p5-voice-sfu); set SR_E2E_VOICE=1 against a server that has it.',
  );

  test("two users in one voice channel receive each other's audio", async ({
    browser,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'Fake media flags are Chromium only.');
    const stamp = Date.now().toString(36);
    const serverName = `Voice ${stamp}`;
    const { owner } = state();

    const ctxA = await browser.newContext({ permissions: ['microphone'] });
    const a = await ctxA.newPage();
    const ctxB = await browser.newContext({ permissions: ['microphone'] });
    const b = await ctxB.newPage();
    await trackPeerConnections(a);
    await trackPeerConnections(b);
    debugPage(a, 'A');
    debugPage(b, 'B');

    // --- A: sign in, create a server and a voice channel, make an invite.
    await a.goto('/login');
    await a.getByLabel('Username or email').fill(owner.username);
    await a.getByLabel('Password', { exact: true }).fill(owner.password);
    await a.getByRole('button', { name: 'Step inside' }).click();
    await expect(a).toHaveURL(/\/channels/);
    await a.getByRole('button', { name: 'Add a Server' }).click();
    await a.getByRole('button', { name: 'Create My Own' }).click();
    await a.getByRole('button', { name: 'skip this question' }).click();
    await a.getByLabel('Server name').fill(serverName);
    await a.getByRole('button', { name: 'Create', exact: true }).click();
    await expect(a.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();

    await a.getByTestId('channel-sidebar').getByRole('button', { name: serverName }).click();
    await a.getByRole('menuitem', { name: 'Create Channel' }).click();
    const create = a.getByRole('dialog', { name: 'Create Channel' });
    await create.getByRole('radio', { name: /Voice/ }).check();
    await create.getByLabel('Channel name').fill('Lounge Voice');
    await create.getByRole('button', { name: 'Create Channel' }).click();
    await expect(
      a.getByTestId('channel-sidebar').getByRole('button', { name: /Lounge Voice/ }),
    ).toBeVisible();

    await a.getByTestId('channel-sidebar').getByRole('button', { name: serverName }).click();
    await a.getByRole('menuitem', { name: 'Invite to Server' }).click();
    const dialog = a.getByRole('dialog', { name: /Invite people to/ });
    await dialog.getByRole('button', { name: 'Copy link' }).click();
    const link = await dialog.getByLabel('Invite link').inputValue();
    await a.keyboard.press('Escape');

    // --- B: register and accept the invite.
    const friend = {
      username: `voiceb_${stamp}`,
      email: `voiceb_${stamp}@example.test`,
      password: 'Friend-Passw0rd-9!',
    };
    await b.goto('/');
    await b.locator('#invite-code').fill(createInvite(`voice ${stamp}`));
    await b.getByRole('button', { name: 'Check invitation' }).click();
    await b.getByLabel('Email').fill(friend.email);
    await b.getByLabel('Username').fill(friend.username);
    await b.getByLabel('Password', { exact: true }).fill(friend.password);
    await b.getByLabel('Confirm password').fill(friend.password);
    await b.getByLabel(/house notes/).check();
    await openTheDoor(b);
    await b.goto(new URL(link).pathname);
    await b.getByRole('button', { name: 'Accept invite' }).click();
    await expect(b.getByRole('heading', { level: 1, name: 'general' })).toBeVisible();

    // --- Both join the voice channel.
    for (const page of [a, b]) {
      await page
        .getByTestId('channel-sidebar')
        .getByRole('button', { name: /Lounge Voice/ })
        .click();
      await expect(page.getByRole('region', { name: 'Voice connection' })).toContainText(
        'Voice Connected',
        { timeout: 30_000 },
      );
    }

    // Each sees the other listed in the channel.
    await expect(
      a.getByRole('list', { name: 'In Lounge Voice' }).getByRole('listitem'),
    ).toHaveCount(2);

    // The SFU forwards audio: inbound audio bytes grow on both ends.
    for (const page of [a, b]) {
      await expect.poll(() => inboundAudioBytes(page), { timeout: 30_000 }).toBeGreaterThan(0);
    }

    // Disconnecting leaves the room.
    await a
      .getByRole('toolbar', { name: 'Call controls' })
      .getByRole('button', { name: 'Disconnect' })
      .click();
    await expect(a.getByRole('region', { name: 'Voice connection' })).toHaveCount(0);

    await ctxA.close();
    await ctxB.close();
  });
});
