import type { Browser, BrowserContext, Page } from '@playwright/test';
import { apiPost, loginApi, type SeededUser } from './fixtures';
import { PUBLIC_URL } from './cli';

/** SR_E2E_VOICE=1: the server under test runs the SFU (`SR_VOICE__ENABLED=true`). */
export const VOICE_STACK = process.env.SR_E2E_VOICE === '1';

/**
 * A logged-in context that may use the microphone and camera. Chromium and WebKit take the permission
 * grant; Firefox does not know these permission names and gets them from its prefs (see the config).
 */
export async function mediaContext(
  browser: Browser,
  browserName: string,
  user: SeededUser,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({
    baseURL: PUBLIC_URL,
    ...(browserName === 'firefox' ? {} : { permissions: ['microphone', 'camera'] }),
  });
  await loginApi(context.request, user);
  const page = await context.newPage();
  return { context, page };
}

/**
 * Record every RTCPeerConnection the page creates (so the test can read its stats) and keep the voice
 * activity gate open: the fake microphone is a beep, so audio must always flow.
 */
export async function trackPeerConnections(page: Page) {
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
    localStorage.setItem(
      'sr.voice.settings',
      JSON.stringify({
        state: { settings: { inputMode: 'vad', autoSensitivity: false, thresholdDb: -100 } },
        version: 1,
      }),
    );
  });
}

export interface InboundStats {
  audioBytes: number;
  videoBytes: number;
  videoFramesDecoded: number;
}

/** Inbound RTP over all of the page's peer connections. */
export async function inbound(page: Page): Promise<InboundStats> {
  return page.evaluate(async () => {
    const w = window as unknown as { __pcs?: RTCPeerConnection[] };
    const out = { audioBytes: 0, videoBytes: 0, videoFramesDecoded: 0 };
    for (const pc of w.__pcs ?? []) {
      if (pc.connectionState === 'closed') continue;
      const report = await pc.getStats();
      report.forEach((s: Record<string, unknown>) => {
        if (s.type !== 'inbound-rtp') return;
        if (s.kind === 'audio') out.audioBytes += Number(s.bytesReceived ?? 0);
        if (s.kind === 'video') {
          out.videoBytes += Number(s.bytesReceived ?? 0);
          out.videoFramesDecoded += Number(s.framesDecoded ?? 0);
        }
      });
    }
    return out;
  });
}

/** SR_E2E_VOICE_DEBUG=1 prints console errors and the gateway/voice WebSocket traffic of a page. */
export function debugPage(page: Page, label: string) {
  if (process.env.SR_E2E_VOICE_DEBUG !== '1') return;
  page.on('console', (m) => {
    if (m.type() === 'error' || m.type() === 'warning')
      console.log(`[${label}] ${m.type()}: ${m.text()}`);
  });
  page.on('pageerror', (e) => console.log(`[${label}] pageerror: ${e.message}`));
  page.on('websocket', (ws) => {
    console.log(`[${label}] ws open ${ws.url()}`);
    const short = (d: string | Buffer) => String(d).slice(0, 200);
    ws.on('framesent', (f) => console.log(`[${label}] > ${short(f.payload)}`));
    ws.on('framereceived', (f) => console.log(`[${label}] < ${short(f.payload)}`));
  });
}

export interface VoiceServer {
  guildId: string;
  textChannelId: string;
  voiceChannelId: string;
  voiceName: string;
}

/** A new server owned by `ownerCtx`'s account, with a voice channel, joined by every other context. */
export async function voiceServer(
  ownerCtx: BrowserContext,
  members: BrowserContext[],
  voiceName = 'Secure Voice',
): Promise<VoiceServer> {
  const guild = await apiPost<{ id: string; channels: { id: string; type: number }[] }>(
    ownerCtx.request,
    '/guilds',
    { name: `Voice ${Date.now().toString(36)}` },
  );
  const text = guild.channels.find((c) => c.type === 0);
  if (!text) throw new Error('new server has no text channel');
  const voice = await apiPost<{ id: string }>(ownerCtx.request, `/guilds/${guild.id}/channels`, {
    name: voiceName,
    type: 2,
  });
  const invite = await apiPost<{ code: string }>(
    ownerCtx.request,
    `/channels/${text.id}/invites`,
    {},
  );
  for (const m of members) await apiPost(m.request, `/invites/${invite.code}`);
  return { guildId: guild.id, textChannelId: text.id, voiceChannelId: voice.id, voiceName };
}

/** Open the server and click into its voice channel; resolves once the call bar says connected. */
export async function joinVoice(page: Page, s: VoiceServer) {
  await page.goto(`/channels/${s.guildId}/${s.textChannelId}`);
  await page
    .getByTestId('channel-sidebar')
    .getByRole('button', { name: new RegExp(s.voiceName) })
    .first()
    .click();
  await page
    .getByRole('region', { name: 'Voice connection' })
    .getByText('Voice Connected')
    .waitFor({ timeout: 45_000 });
}
