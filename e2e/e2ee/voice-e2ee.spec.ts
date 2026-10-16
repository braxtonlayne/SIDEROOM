import type { Page } from '@playwright/test';
import { expect, registerUser, test } from '../support/fixtures';
import {
  debugPage,
  inbound,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  VOICE_STACK,
  voiceServer,
} from '../support/voice';

/**
 * VOX-E2E-003 in real browsers: two clients with E2EE on join one voice channel. The MLS group forms over
 * gateway op 101, both show the same call code, their media workers encrypt and decrypt audio and video
 * frames, and the decoders play them. A third client without E2EE (the e2e-only opt-out) joins the same
 * channel and looks at the encoded frames the SFU forwards: every one of them carries the SR frame trailer,
 * so nothing reaches it, or the SFU, in the clear.
 *
 * Needs sr-server with `SR_VOICE__ENABLED=true SR_VOICE__E2EE=true` and this config's web server
 * (`VITE_SR_VOICE_E2EE=1`); set SR_E2E_VOICE=1.
 */

interface FrameCounters {
  encrypted: number;
  decrypted: number;
  sendDropped: number;
  recvDropped: number;
}

interface E2eeSnapshot {
  active: boolean;
  status: string | null;
  callCode: string | null;
  members: number;
  frames: FrameCounters | null;
  workerError: string | null;
}

async function e2ee(page: Page): Promise<E2eeSnapshot> {
  return page.evaluate(() => {
    const get = (globalThis as unknown as { __srE2ee?: () => Record<string, unknown> }).__srE2ee;
    const v = get?.() as
      | {
          active: boolean;
          call: { status: string; callCode: string | null; members: unknown[] } | null;
          frames?: FrameCounters | null;
          workerError?: string | null;
        }
      | undefined;
    return {
      active: v?.active ?? false,
      status: v?.call?.status ?? null,
      callCode: v?.call?.callCode ?? null,
      members: v?.call?.members.length ?? 0,
      frames: v?.frames ?? null,
      workerError: v?.workerError ?? null,
    };
  });
}

/**
 * The observer: E2EE off for this page, and every received encoded frame is inspected through Chromium's
 * `createEncodedStreams` before it reaches the decoder. A frame "looks encrypted" when it ends with the
 * sr-e2ee trailer: `... version (1) | trailer length | 'S' 'R'` (crates/sr-e2ee/src/frame.rs).
 */
async function observeFrames(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('sr.e2e.noVoiceE2ee', '1');
    const w = window as unknown as {
      __frames: { audio: number; video: number; audioSr: number; videoSr: number };
    };
    w.__frames = { audio: 0, video: 0, audioSr: 0, videoSr: 0 };
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(config?: RTCConfiguration) {
        super({ ...(config ?? {}), encodedInsertableStreams: true } as RTCConfiguration);
        const pc = this;
        const pipe = (
          endpoint: { createEncodedStreams?: () => { readable: ReadableStream; writable: WritableStream } },
          kind: string | null,
        ) => {
          const streams = endpoint.createEncodedStreams?.();
          if (!streams) return;
          const inspect = new TransformStream({
            transform(frame: { data: ArrayBuffer }, ctl) {
              if (kind) {
                const b = new Uint8Array(frame.data);
                const n = b.length;
                const sr =
                  n > 17 && b[n - 2] === 0x53 && b[n - 1] === 0x52 && b[n - 4] === 1;
                if (kind === 'audio') {
                  w.__frames.audio++;
                  if (sr) w.__frames.audioSr++;
                } else {
                  w.__frames.video++;
                  if (sr) w.__frames.videoSr++;
                }
              }
              ctl.enqueue(frame);
            },
          });
          void streams.readable.pipeThrough(inspect).pipeTo(streams.writable).catch(() => {});
        };
        pc.addEventListener('track', (ev) => pipe(ev.receiver as never, ev.track.kind));
        // With encoded streams on, Chromium also wants the senders' streams taken; pass them through.
        const addTrack = pc.addTrack.bind(pc);
        pc.addTrack = (track, ...streams) => {
          const sender = addTrack(track, ...streams);
          pipe(sender as never, null);
          return sender;
        };
        const addTransceiver = pc.addTransceiver.bind(pc);
        pc.addTransceiver = (trackOrKind, init) => {
          const t = addTransceiver(trackOrKind, init);
          if (typeof trackOrKind !== 'string' || init?.direction !== 'recvonly')
            pipe(t.sender as never, null);
          return t;
        };
      }
    } as typeof RTCPeerConnection;
  });
}

test.describe('Voice E2EE (real browsers)', () => {
  test.skip(
    !VOICE_STACK,
    'Needs sr-server with SR_VOICE__ENABLED=true and SR_VOICE__E2EE=true; set SR_E2E_VOICE=1.',
  );

  test('two clients form an MLS group, share a call code and exchange encrypted audio and video', async ({
    browser,
    browserName,
    owner,
    playwright,
  }) => {
    const req = await playwright.request.newContext({ baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173' });
    const bob = await registerUser(req, 'e2eeb');
    const eve = browserName === 'chromium' ? await registerUser(req, 'e2eeobs') : null;
    await req.dispose();

    const a = await mediaContext(browser, browserName, owner);
    const b = await mediaContext(browser, browserName, bob);
    const c = eve ? await mediaContext(browser, browserName, eve) : null;
    for (const [p, label] of [
      [a.page, 'A'],
      [b.page, 'B'],
    ] as const) {
      await trackPeerConnections(p);
      debugPage(p, label);
    }
    if (c) {
      await trackPeerConnections(c.page);
      await observeFrames(c.page);
      debugPage(c.page, 'C');
    }

    const server = await voiceServer(
      a.context,
      [b.context, ...(c ? [c.context] : [])],
      'Secure Voice',
    );

    try {
      await joinVoice(a.page, server);
      await joinVoice(b.page, server);

      // --- The MLS group forms: both secure, two members, the same call code.
      for (const p of [a.page, b.page]) {
        await expect
          .poll(async () => {
            const s = await e2ee(p);
            return `${s.active}/${s.status}/${s.members}/${s.workerError ?? ''}`;
          }, { timeout: 45_000 })
          .toBe('true/secure/2/');
      }
      await expect
        .poll(async () => {
          const [x, y] = [await e2ee(a.page), await e2ee(b.page)];
          return x.callCode !== null && x.callCode === y.callCode;
        })
        .toBe(true);

      // The shield shows that code in the UI on both sides.
      const codes: string[] = [];
      for (const p of [a.page, b.page]) {
        const call = p.getByRole('region', { name: 'Voice connection' });
        await call.getByRole('button', { name: /End-to-end encrypt/ }).first().click();
        const code = p.getByLabel('Call code', { exact: true }).first();
        await expect(code).toBeVisible();
        codes.push((await code.textContent()) ?? '');
        await p.keyboard.press('Escape');
      }
      expect(codes[0]).toMatch(/\d/);
      expect(codes[0]).toBe(codes[1]);

      // --- Audio: frames are encrypted by the sender's worker, decrypted by the receiver's, and decoded.
      for (const p of [a.page, b.page]) {
        await expect
          .poll(async () => (await e2ee(p)).frames?.decrypted ?? 0, { timeout: 30_000 })
          .toBeGreaterThan(50);
        await expect.poll(async () => (await inbound(p)).audioBytes, { timeout: 30_000 }).toBeGreaterThan(0);
        const f = (await e2ee(p)).frames!;
        expect(f.encrypted).toBeGreaterThan(50);
      }

      // --- Video: A turns the (fake) camera on; B decodes frames that went through decryption.
      const before = (await e2ee(b.page)).frames!.decrypted;
      const callA = a.page.getByRole('region', { name: 'Voice connection' });
      await callA.getByRole('button', { name: 'Turn on camera' }).first().click();
      const preview = a.page.getByRole('dialog', { name: /Preview your video/ });
      if (await preview.isVisible().catch(() => false))
        await preview.getByRole('button', { name: 'Turn on camera' }).click();
      await expect
        .poll(async () => (await inbound(b.page)).videoFramesDecoded, { timeout: 45_000 })
        .toBeGreaterThan(10);
      await expect
        .poll(async () => (await e2ee(b.page)).frames!.decrypted - before, { timeout: 30_000 })
        .toBeGreaterThan(100);

      // --- A third party without E2EE: everything the SFU forwards to it is ciphertext.
      if (c) {
        await joinVoice(c.page, server);
        await expect
          .poll(
            () =>
              c.page.evaluate(
                () => (window as unknown as { __frames: { audio: number } }).__frames.audio,
              ),
            { timeout: 30_000 },
          )
          .toBeGreaterThan(50);
        await expect
          .poll(
            () =>
              c.page.evaluate(
                () => (window as unknown as { __frames: { video: number } }).__frames.video,
              ),
            { timeout: 30_000 },
          )
          .toBeGreaterThan(10);
        const seen = await c.page.evaluate(
          () =>
            (window as unknown as {
              __frames: { audio: number; video: number; audioSr: number; videoSr: number };
            }).__frames,
        );
        expect(seen.audioSr, 'every audio frame carries the E2EE trailer').toBe(seen.audio);
        expect(seen.videoSr, 'every video frame carries the E2EE trailer').toBe(seen.video);
        // The encrypted pair keeps talking with the observer in the channel.
        const d = (await e2ee(a.page)).frames!.decrypted;
        await expect.poll(async () => (await e2ee(a.page)).frames!.decrypted - d).toBeGreaterThan(20);
      }
    } finally {
      for (const x of [a, b, c]) await x?.context.close();
    }
  });
});
