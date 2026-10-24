import type { Page } from '@playwright/test';
import { expect, registerUser, test } from '../support/fixtures';
import {
  debugPage,
  inbound,
  joinVoice,
  mediaContext,
  trackPeerConnections,
  voiceServer,
} from '../support/voice';

/**
 * RDY-VOX-001: voice through a TURN relay. Two Chromium contexts join one voice channel with
 * `iceTransportPolicy: 'relay'` forced on every RTCPeerConnection, so the only candidates the browser may use are
 * relay candidates allocated on coturn. The test then reads `getStats()` and requires that
 *
 *   - the selected candidate pair (transport.selectedCandidatePairId) has a local candidate of type `relay`,
 *   - every local candidate the browser gathered is a relay (nothing leaked past the policy),
 *   - the ICE server list the server handed out holds a `turn:` URL with REST credentials
 *     (username `<expiry>:<user id>`, the `voice.turn.secret` path), and
 *   - audio still arrives (inbound bytes grow) and the E2EE call is secure.
 *
 * Needs the stack of e2e.yml's "Voice through TURN relay" step: sr-server with SR_VOICE__ENABLED=true,
 * SR_VOICE__TURN__SECRET and SR_VOICE__TURN__URLS pointing at the coturn that renders
 * deploy/compose/coturn/turnserver.prod.conf.example (deploy/compose/coturn/render-test-conf.sh), and
 * SR_VOICE__ANNOUNCE_IPS set to the address coturn may relay to. Set SR_E2E_VOICE=1 and SR_E2E_RELAY=1.
 *
 * Run against a server without coturn the call cannot connect and the test fails at the join: that is the point,
 * it proves nothing about the relay unless coturn is really there. Chromium only.
 */
const RELAY = process.env.SR_E2E_VOICE === '1' && process.env.SR_E2E_RELAY === '1';

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

/** Every RTCPeerConnection of the page is built with `iceTransportPolicy: 'relay'`. */
async function forceRelay(page: Page) {
  await page.addInitScript(() => {
    const Native = window.RTCPeerConnection;
    window.RTCPeerConnection = class extends Native {
      constructor(config?: RTCConfiguration) {
        super({ ...(config ?? {}), iceTransportPolicy: 'relay' });
      }
    } as typeof RTCPeerConnection;
  });
}

interface RelayEvidence {
  /** candidateType of the local candidate of the selected pair, per peer connection with a selected pair. */
  selected: string[];
  /** candidateType of every local candidate that was gathered. */
  gathered: string[];
  /** relayProtocol of the selected local candidates. */
  protocols: string[];
  /** The `urls` and `username` of each configured TURN server. */
  turn: { urls: string[]; username: string }[];
  policies: string[];
}

async function relayEvidence(page: Page): Promise<RelayEvidence> {
  return page.evaluate(async () => {
    const w = window as unknown as { __pcs?: RTCPeerConnection[] };
    const out: RelayEvidence = {
      selected: [],
      gathered: [],
      protocols: [],
      turn: [],
      policies: [],
    };
    for (const pc of w.__pcs ?? []) {
      if (pc.connectionState === 'closed') continue;
      const cfg = pc.getConfiguration();
      out.policies.push(String(cfg.iceTransportPolicy));
      for (const s of cfg.iceServers ?? []) {
        const urls = Array.isArray(s.urls) ? s.urls : [s.urls];
        if (urls.some((u) => /^turns?:/.test(u)))
          out.turn.push({ urls, username: String(s.username ?? '') });
      }
      const stats = await pc.getStats();
      const byId = new Map<string, Record<string, unknown>>();
      stats.forEach((s: Record<string, unknown>) => byId.set(String(s.id), s));
      byId.forEach((s) => {
        if (s.type === 'local-candidate') out.gathered.push(String(s.candidateType));
      });
      byId.forEach((t) => {
        if (t.type !== 'transport' || !t.selectedCandidatePairId) return;
        const pair = byId.get(String(t.selectedCandidatePairId));
        const local = pair ? byId.get(String(pair.localCandidateId)) : undefined;
        if (local) {
          out.selected.push(String(local.candidateType));
          out.protocols.push(String(local.relayProtocol ?? ''));
        }
      });
    }
    return out;
  });
}

test.describe('Voice through a TURN relay', () => {
  test.skip(
    !RELAY,
    'Needs coturn rendered from turnserver.prod.conf.example and the SFU; set SR_E2E_VOICE=1 SR_E2E_RELAY=1.',
  );

  test('the selected candidate pair is a relay and audio flows through it', async ({
    browser,
    browserName,
    playwright,
  }) => {
    test.skip(browserName !== 'chromium', 'Fake media flags are Chromium only.');
    const req = await playwright.request.newContext({
      baseURL: process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173',
    });
    const alice = await registerUser(req, 'relaya');
    const bob = await registerUser(req, 'relayb');
    await req.dispose();

    const a = await mediaContext(browser, browserName, alice);
    const b = await mediaContext(browser, browserName, bob);
    for (const [p, label] of [
      [a.page, 'A'],
      [b.page, 'B'],
    ] as const) {
      await trackPeerConnections(p);
      await forceRelay(p);
      debugPage(p, label);
    }
    const server = await voiceServer(a.context, [b.context], 'Relay Voice');

    try {
      await joinVoice(a.page, server);
      await joinVoice(b.page, server);

      for (const [p, who] of [
        [a.page, 'A'],
        [b.page, 'B'],
      ] as const) {
        // Audio arrives over the relay.
        await expect
          .poll(async () => (await inbound(p)).audioBytes, {
            message: `${who} receives audio`,
            timeout: 45_000,
          })
          .toBeGreaterThan(0);

        const ev = await relayEvidence(p);
        test.info().annotations.push({ type: `relay-${who}`, description: JSON.stringify(ev) });
        expect(ev.policies, `${who}: the policy reached the peer connection`).toContain('relay');
        expect(ev.selected.length, `${who}: a candidate pair is selected`).toBeGreaterThan(0);
        expect(
          ev.selected.every((t) => t === 'relay'),
          `${who}: selected local candidate types ${ev.selected}`,
        ).toBe(true);
        expect(ev.gathered.length, `${who}: candidates were gathered`).toBeGreaterThan(0);
        expect(
          ev.gathered.every((t) => t === 'relay'),
          `${who}: gathered local candidate types ${ev.gathered}`,
        ).toBe(true);
        // The REST credential path: the server hands out `<expiry unix seconds>:<user id>`.
        expect(ev.turn.length, `${who}: the server handed out a TURN server`).toBeGreaterThan(0);
        expect(ev.turn[0]!.username).toMatch(/^\d{9,}:/);
      }
    } finally {
      await a.context.close();
      await b.context.close();
    }
  });
});
