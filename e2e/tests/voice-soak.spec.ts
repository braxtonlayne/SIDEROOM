import type { Page } from '@playwright/test';
import { expect, registerUser, test, type SeededUser } from '../support/fixtures';
import {
  inbound,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  voiceServer,
} from '../support/voice';

/**
 * RDY-VOX-002: voice soak. N Chromium contexts (default 10) sit in one E2EE voice channel for D minutes (default
 * 10). Each member, on its own schedule, mutes and unmutes (repeatedly), turns the camera on and off once, and
 * reconnects once (its voice WebSocket is closed from the client; the engine must resume). Nothing else touches the
 * call. At the end the test asserts, per member:
 *
 *   - the call is still E2EE `secure` with all N members and the voice bar says connected;
 *   - no frame-transform failure: the media worker never reported an error, no peer connection went `failed`
 *     during the run, and the dropped-frame ratio (key changes at joins and reconnects drop a few frames) stays
 *     under SOAK_MAX_DROP_RATIO (default 2 %), with no drops at all in the closing steady-state window;
 *   - no SFU restart: peers that did NOT reconnect never reopened their voice socket (a restarted SFU would drop
 *     every member), and a reconnecting member opens at most two extra sockets. The workflow additionally checks
 *     that the sr-server process is the one it started and has not logged a panic;
 *   - a tone is heard at the end: inbound audio bytes and decrypted frames both grow over a closing window.
 *
 * Inputs (environment; voice-soak.yml maps its dispatch inputs to them): SOAK_COUNT, SOAK_MINUTES (may be
 * fractional), SOAK_MAX_DROP_RATIO, SOAK_CAMERA ("0" skips the camera on/off step). Owner (2026-10-24): the soak
 * stays on the standard runner, and the camera step is the first thing to drop if the runner struggles. Needs the voice stack with E2EE: SR_E2E_VOICE=1 SR_E2E_SOAK=1 against an
 * sr-server with SR_VOICE__ENABLED=true. A summary is attached to the report and printed.
 */
const SOAK = process.env.SR_E2E_VOICE === '1' && process.env.SR_E2E_SOAK === '1';
const COUNT = Math.max(2, Number(process.env.SOAK_COUNT ?? 10));
const MINUTES = Math.max(0.25, Number(process.env.SOAK_MINUTES ?? 10));
const MAX_DROP = Number(process.env.SOAK_MAX_DROP_RATIO ?? 0.02);
/** The first thing to drop on a struggling runner (owner, 2026-10-24): SOAK_CAMERA=0. */
const CAMERA = process.env.SOAK_CAMERA !== '0';
const DURATION_MS = MINUTES * 60_000;
const TAIL_MS = Math.min(20_000, DURATION_MS * 0.25); // quiet steady state at the end

test.use({
  permissions: ['microphone', 'camera'],
  launchOptions: {
    args: [
      '--use-fake-device-for-media-stream',
      '--use-fake-ui-for-media-stream',
      '--disable-features=WebRtcHideLocalIpsWithMdns',
      '--allow-loopback-in-peer-connection',
    ],
  },
});

interface FrameCounters {
  encrypted: number;
  decrypted: number;
  sendDropped: number;
  recvDropped: number;
}

interface Snapshot {
  active: boolean;
  status: string | null;
  members: number;
  frames: FrameCounters | null;
  workerError: string | null;
  failedPeers: number;
  voiceSockets: number;
}

/** Counts the page's /voice/ws sockets (an SFU restart would reopen every member's). */
async function trackVoiceSockets(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as { __voiceSockets: WebSocket[] };
    w.__voiceSockets = [];
    const Native = window.WebSocket;
    window.WebSocket = class extends Native {
      constructor(url: string | URL, protocols?: string | string[]) {
        super(url, protocols);
        if (/\/voice\/ws/.test(String(url))) w.__voiceSockets.push(this);
      }
    } as typeof WebSocket;
  });
}

async function snapshot(page: Page): Promise<Snapshot> {
  return page.evaluate(() => {
    const g = globalThis as unknown as {
      __srE2ee?: () => {
        active: boolean;
        call: { status: string; members: unknown[] } | null;
        frames?: FrameCounters | null;
        workerError?: string | null;
      };
      __pcs?: RTCPeerConnection[];
      __voiceSockets?: WebSocket[];
    };
    const v = g.__srE2ee?.();
    return {
      active: v?.active ?? false,
      status: v?.call?.status ?? null,
      members: v?.call?.members.length ?? 0,
      frames: v?.frames ?? null,
      workerError: v?.workerError ?? null,
      failedPeers: (g.__pcs ?? []).filter((pc) => pc.connectionState === 'failed').length,
      voiceSockets: g.__voiceSockets?.length ?? 0,
    };
  });
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

test.describe('Voice soak', () => {
  test.skip(
    !SOAK,
    'Dispatch-only soak (voice-soak.yml); needs the voice stack, set SR_E2E_VOICE=1 SR_E2E_SOAK=1.',
  );
  test.setTimeout(DURATION_MS + COUNT * 120_000 + 10 * 60_000);

  test(`${COUNT} members, ${MINUTES} min: mute, ${CAMERA ? 'camera, ' : ''}one reconnect each, E2EE on`, async ({
    browser,
    browserName,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Fake media flags are Chromium only.');
    const req = await playwright.request.newContext({
      baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173',
    });
    const users: SeededUser[] = [];
    for (let i = 0; i < COUNT; i++) users.push(await registerUser(req, `soak${i}`));
    await req.dispose();

    const members: Awaited<ReturnType<typeof mediaContext>>[] = [];
    for (const u of users) {
      const m = await mediaContext(browser, browserName, u);
      await trackPeerConnections(m.page);
      await trackVoiceSockets(m.page);
      members.push(m);
    }
    const failures: string[] = [];
    const reconnected = new Set<number>();
    const server = await voiceServer(
      members[0]!.context,
      members.slice(1).map((m) => m.context),
      'Soak Voice',
    );

    try {
      for (const m of members) await joinVoice(m.page, server);
      for (const [i, m] of members.entries()) {
        await expect
          .poll(async () => (await snapshot(m.page)).members, {
            message: `member ${i} sees all ${COUNT} in the MLS group`,
            timeout: 120_000,
          })
          .toBe(COUNT);
        await expect
          .poll(async () => (await snapshot(m.page)).status, { timeout: 60_000 })
          .toBe('secure');
      }

      // ---- The soak. Everything is scheduled as fractions of the duration so a short run exercises the same code.
      const t0 = Date.now();
      const at = (fraction: number) => sleep(Math.max(0, t0 + DURATION_MS * fraction - Date.now()));
      const controls = (p: Page) => p.getByRole('toolbar', { name: 'Call controls' });

      async function member(i: number) {
        const p = members[i]!.page;
        const lane = i / COUNT; // spreads the members over the run
        // Mute / unmute, repeatedly while the run lasts (the tail stays unmuted).
        const cycle = async (fraction: number) => {
          await at(fraction);
          await controls(p).getByRole('button', { name: 'Mute', exact: true }).first().click();
          await sleep(3000);
          await controls(p).getByRole('button', { name: 'Unmute', exact: true }).first().click();
        };
        // Camera on, a few seconds, off.
        const camera = async (fraction: number) => {
          await at(fraction);
          await controls(p).getByRole('button', { name: 'Turn on camera' }).first().click();
          const preview = p.getByRole('dialog', { name: /Preview your video/ });
          if (await preview.isVisible().catch(() => false))
            await preview.getByRole('button', { name: 'Turn on camera' }).click();
          await expect(
            controls(p).getByRole('button', { name: 'Turn off camera' }).first(),
          ).toBeVisible({
            timeout: 30_000,
          });
          await sleep(5000);
          await controls(p).getByRole('button', { name: 'Turn off camera' }).first().click();
        };
        // One reconnect: the client closes its voice socket; the engine must resume without a new join.
        const reconnect = async (fraction: number) => {
          await at(fraction);
          reconnected.add(i);
          await p.evaluate(() => {
            const w = window as unknown as { __voiceSockets: WebSocket[] };
            for (const s of w.__voiceSockets)
              if (s.readyState === WebSocket.OPEN) s.close(3999, 'soak');
          });
          await expect(
            p.getByRole('region', { name: 'Voice connection' }).getByText('Voice Connected'),
          ).toBeVisible({ timeout: 60_000 });
        };
        await cycle(0.1 + 0.15 * lane);
        if (CAMERA) await camera(0.28 + 0.2 * lane);
        await reconnect(0.52 + 0.2 * lane);
        // More mute cycles until the tail starts.
        for (let f = 0.78; f < 1 - TAIL_MS / DURATION_MS - 0.05; f += 0.12)
          await cycle(f + 0.02 * lane);
      }

      // Watch for failures while the members act.
      let watching = true;
      const watcher = (async () => {
        while (watching) {
          for (const [i, m] of members.entries()) {
            const s = await snapshot(m.page).catch(() => null);
            if (!s) continue;
            if (s.workerError) failures.push(`member ${i}: media worker error ${s.workerError}`);
            if (s.failedPeers > 0) failures.push(`member ${i}: a peer connection failed`);
          }
          await sleep(10_000);
        }
      })();

      await Promise.all([...members.keys()].map((i) => member(i)));
      await at(1 - TAIL_MS / DURATION_MS); // the closing steady-state window starts here
      const before = await Promise.all(
        members.map(async (m) => ({ snap: await snapshot(m.page), inb: await inbound(m.page) })),
      );
      await at(1);
      watching = false;
      await watcher;

      // ---- Verdicts.
      const summary: unknown[] = [];
      for (const [i, m] of members.entries()) {
        const now = await snapshot(m.page);
        const inb = await inbound(m.page);
        const b = before[i]!;
        const f = now.frames;
        const bf = b.snap.frames;
        const recvRatio =
          f && f.decrypted + f.recvDropped > 0 ? f.recvDropped / (f.decrypted + f.recvDropped) : 0;
        const sendRatio =
          f && f.encrypted + f.sendDropped > 0 ? f.sendDropped / (f.encrypted + f.sendDropped) : 0;
        summary.push({
          member: i,
          status: now.status,
          members: now.members,
          frames: f,
          recvDropRatio: Number(recvRatio.toFixed(4)),
          sendDropRatio: Number(sendRatio.toFixed(4)),
          voiceSockets: now.voiceSockets,
          reconnected: reconnected.has(i),
          audioBytesInTail: inb.audioBytes - b.inb.audioBytes,
          decryptedInTail: f && bf ? f.decrypted - bf.decrypted : null,
          droppedInTail:
            f && bf ? f.recvDropped + f.sendDropped - bf.recvDropped - bf.sendDropped : null,
        });
        expect(now.status, `member ${i} is still secure`).toBe('secure');
        expect(now.members, `member ${i} still sees everyone`).toBe(COUNT);
        await expect(
          m.page.getByRole('region', { name: 'Voice connection' }).getByText('Voice Connected'),
        ).toBeVisible();
        // No SFU restart: only a member that reconnected itself opened another socket.
        if (reconnected.has(i))
          expect(now.voiceSockets, `member ${i} reconnect sockets`).toBeLessThanOrEqual(3);
        else expect(now.voiceSockets, `member ${i} never reconnected, so no new socket`).toBe(1);
        // No frame-transform failures.
        expect(now.workerError, `member ${i} media worker`).toBeNull();
        expect(recvRatio, `member ${i} receive drop ratio`).toBeLessThan(MAX_DROP);
        expect(sendRatio, `member ${i} send drop ratio`).toBeLessThan(MAX_DROP);
        expect(
          f && bf ? f.recvDropped + f.sendDropped - bf.recvDropped - bf.sendDropped : 0,
          `member ${i} drops in the steady tail`,
        ).toBe(0);
        // The tone: audio is still arriving and being decrypted.
        expect(
          inb.audioBytes - b.inb.audioBytes,
          `member ${i} hears the tone at the end`,
        ).toBeGreaterThan(0);
        expect(
          f && bf ? f.decrypted - bf.decrypted : 0,
          `member ${i} decrypts at the end`,
        ).toBeGreaterThan(0);
      }
      const text = JSON.stringify(
        { count: COUNT, minutes: MINUTES, maxDropRatio: MAX_DROP, failures, summary },
        null,
        2,
      );
      console.log(text);
      await test
        .info()
        .attach('soak-summary.json', { body: text, contentType: 'application/json' });
      expect(failures, 'failures seen while the soak ran').toEqual([]);
    } finally {
      for (const m of members) await m.context.close();
    }
  });
});
