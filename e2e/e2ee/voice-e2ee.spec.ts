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
 * The observer: E2EE off for this page, and every received encoded frame is inspected in an
 * `RTCRtpScriptTransform` worker before it reaches the decoder. A frame "looks encrypted" when it ends with
 * the sr-e2ee trailer: `... version (1) | trailer length | 'S' 'R'` (crates/sr-e2ee/src/frame.rs).
 */
async function observeFrames(page: Page) {
  await page.addInitScript(() => {
    localStorage.setItem('sr.e2e.noVoiceE2ee', '1');
    const w = window as unknown as {
      __frames: { audio: number; video: number; audioSr: number; videoSr: number; tracks: string[] };
    };
    w.__frames = { audio: 0, video: 0, audioSr: 0, videoSr: 0, tracks: [] };
    const Transform = (window as unknown as { RTCRtpScriptTransform?: new (w: Worker, o: unknown) => unknown })
      .RTCRtpScriptTransform;
    if (!Transform) return;
    const source = `
      const counts = { audio: 0, video: 0, audioSr: 0, videoSr: 0 };
      onrtctransform = (ev) => {
        const kind = ev.transformer.options.kind;
        ev.transformer.readable
          .pipeThrough(new TransformStream({
            transform(frame, ctl) {
              const b = new Uint8Array(frame.data);
              const n = b.length;
              const sr = n > 17 && b[n - 2] === 0x53 && b[n - 1] === 0x52 && b[n - 4] === 1;
              counts[kind]++;
              if (sr) counts[kind + 'Sr']++;
              ctl.enqueue(frame);
            },
          }))
          .pipeTo(ev.transformer.writable)
          .catch(() => {});
      };
      setInterval(() => postMessage(counts), 250);
    `;
    const worker = new Worker(URL.createObjectURL(new Blob([source], { type: 'text/javascript' })));
    worker.onmessage = (e: MessageEvent) => Object.assign(w.__frames, e.data);
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(config?: RTCConfiguration) {
        super(config);
        this.addEventListener('track', (ev) => {
          w.__frames.tracks.push(ev.track.kind);
          (ev.receiver as unknown as { transform: unknown }).transform = new Transform(worker, {
            kind: ev.track.kind,
          });
        });
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

      // --- A third party without E2EE joins (it watches the frames, see the end); the pair stays secure.
      if (c) {
        await joinVoice(c.page, server);
        for (const p of [a.page, b.page])
          await expect.poll(async () => (await e2ee(p)).status).toBe('secure');
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

      // --- The third party: everything the SFU forwarded to it is ciphertext.
      if (c) {
        await expect
          .poll(
            () =>
              c.page.evaluate(
                () => (window as unknown as { __frames: { audio: number } }).__frames.audio,
              ),
            { timeout: 30_000 },
          )
          .toBeGreaterThan(50);
        if (process.env.SR_E2E_VOICE_DEBUG === '1') {
          await c.page.waitForTimeout(5000);
          console.log(
            'observer',
            JSON.stringify(await c.page.evaluate(() => (window as unknown as { __frames: unknown }).__frames)),
            JSON.stringify(await inbound(c.page)),
          );
        }
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
