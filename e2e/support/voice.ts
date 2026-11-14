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

export interface InboundAudioStream {
  /** The `inbound-rtp` stats id (stable for an SSRC), to tell a stream that appeared later from the ones before. */
  id: string;
  bytes: number;
  mimeType: string;
  channels: number;
  /** The negotiated `a=fmtp` line of the codec (the music payload type says `stereo=1`, the microphone's does not). */
  fmtp?: string;
}

/** The inbound audio streams of all of the page's peer connections: bytes, and the codec they negotiated. */
export async function inboundAudioStreams(page: Page): Promise<InboundAudioStream[]> {
  return page.evaluate(async () => {
    const w = window as unknown as { __pcs?: RTCPeerConnection[] };
    const out: {
      id: string;
      bytes: number;
      mimeType: string;
      channels: number;
      fmtp: string;
    }[] = [];
    for (const pc of w.__pcs ?? []) {
      const report = await pc.getStats();
      const codecs = new Map<string, Record<string, unknown>>();
      report.forEach((s: Record<string, unknown>) => {
        if (s.type === 'codec') codecs.set(String(s.id), s);
      });
      report.forEach((s: Record<string, unknown>) => {
        if (s.type !== 'inbound-rtp' || s.kind !== 'audio') return;
        const codec = codecs.get(String(s.codecId));
        out.push({
          id: String(s.id),
          bytes: Number(s.bytesReceived ?? 0),
          mimeType: String(codec?.mimeType ?? ''),
          channels: Number(codec?.channels ?? 0),
          fmtp: String(codec?.sdpFmtpLine ?? ''),
        });
      });
    }
    return out;
  });
}

/**
 * MUS-JB-001: the page plays a generated stereo tone (440 Hz left, 660 Hz right) as its music track, a second
 * audio sender under the same frame encryptor as the microphone. Needs the web build with VITE_SR_E2E=1 (it
 * exposes the voice engine as `__srVoiceEngine`), a channel that allows music and SPEAK for the member.
 *
 * `through: 'engine'` (the default, what the soak does) hands the stream to the engine directly: enough while
 * the connection lasts, but the controller never hears of it, so a rejoin (a new engine) loses it.
 * `through: 'controller'` publishes it the way the jukebox does (`voiceController().publishMusic`), so the
 * controller keeps it across a rejoin (D-724). The controller is reached with a dynamic import of its module
 * from the Vite dev server (the one module instance the app uses): this needs the dev server, not a built bundle.
 */
export async function playTone(
  page: Page,
  opts: { through?: 'engine' | 'controller' } = {},
): Promise<void> {
  await page.evaluate(async (through) => {
    const engine = (
      globalThis as unknown as {
        __srVoiceEngine?: { publishMusic(stream: MediaStream, kbps?: number): void };
      }
    ).__srVoiceEngine;
    if (!engine) throw new Error('no voice engine handle: run the web build with VITE_SR_E2E=1');
    const ctx = new AudioContext();
    const left = ctx.createOscillator();
    const right = ctx.createOscillator();
    left.frequency.value = 440;
    right.frequency.value = 660;
    const merger = ctx.createChannelMerger(2);
    left.connect(merger, 0, 0);
    right.connect(merger, 0, 1);
    const dest = ctx.createMediaStreamDestination();
    dest.channelCount = 2;
    dest.channelCountMode = 'explicit';
    merger.connect(dest);
    left.start();
    right.start();
    void ctx.resume();
    if (through === 'controller') {
      const url = '/src/voice/controller.ts';
      const mod = (await import(/* @vite-ignore */ url)) as {
        voiceController(): { publishMusic(stream: MediaStream): void; musicPublished: boolean };
      };
      const controller = mod.voiceController();
      controller.publishMusic(dest.stream);
      if (!controller.musicPublished) throw new Error('the controller did not take the music');
    } else {
      engine.publishMusic(dest.stream, 256);
    }
  }, opts.through ?? 'engine');
}

/**
 * Whether an inbound stream is Opus with two channels. Chromium reports two channels for every Opus line (the
 * rtpmap says `opus/48000/2`), the microphone's too: `isMusicStream` tells the music line from the microphone.
 */
export function isStereoOpus(s: InboundAudioStream): boolean {
  return /opus/i.test(s.mimeType) && s.channels === 2;
}

/** The music line: Opus whose negotiated fmtp says `stereo=1` (the microphone's payload type does not). */
export function isMusicStream(s: InboundAudioStream): boolean {
  return isStereoOpus(s) && /(^|;)stereo=1(;|$)/.test(s.fmtp ?? '');
}

/**
 * The music check of the soak and of the live spec (D-724), as a function: `null` when a stream that was not in
 * `before` (the streams listed before the music started) is Opus with two channels and its bytes grew over the
 * closing window (`tail`: the same streams' bytes at the start and at the end of it; a stream missing from `tail`
 * counts from 0); otherwise one sentence that says what is missing.
 */
export function musicVerdict(
  before: InboundAudioStream[],
  after: InboundAudioStream[],
  tail: { id: string; bytesBefore: number; bytesAfter: number }[],
): string | null {
  const known = new Set(before.map((s) => s.id));
  const added = after.filter((s) => !known.has(s.id));
  if (added.length === 0) return 'no new audio stream';
  const opus = added.filter((s) => /opus/i.test(s.mimeType));
  if (opus.length === 0) {
    return `the new stream is not Opus (${added.map((s) => s.mimeType || 'unknown').join(', ')})`;
  }
  const stereo = opus.filter((s) => s.channels === 2);
  if (stereo.length === 0) {
    return opus.every((s) => s.channels === 1)
      ? 'the new stream is mono'
      : `the new stream has ${opus.map((s) => s.channels).join(', ')} channels, not 2`;
  }
  const grew = stereo.some((s) => {
    const t = tail.find((x) => x.id === s.id);
    return (t ? t.bytesAfter - t.bytesBefore : s.bytes) > 0;
  });
  return grew ? null : 'the stereo stream stopped: 0 bytes in the closing window';
}

/** Record every voice socket (`/voice/ws`) the page opens, so a test can close one (`closeVoiceSockets`). */
export async function trackVoiceSockets(page: Page) {
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

/** How many voice sockets the page has opened so far (needs `trackVoiceSockets`). */
export async function voiceSocketCount(page: Page): Promise<number> {
  return page.evaluate(
    () => (window as unknown as { __voiceSockets?: WebSocket[] }).__voiceSockets?.length ?? 0,
  );
}

/**
 * Make the engine see its open voice sockets close with `code`: a `close` event with that code is dispatched on
 * each (needs `trackVoiceSockets`), so the engine's own handler runs. A real client-side `close(code)` cannot do
 * this: the browser reports the code the server answers with, never the one sent, so the engine would only resume.
 * Returns how many sockets were open; the engine closes them itself when it gives up on the session.
 */
export async function failVoiceSockets(page: Page, code: number): Promise<number> {
  return page.evaluate((code) => {
    const w = window as unknown as { __voiceSockets?: WebSocket[] };
    let n = 0;
    for (const s of w.__voiceSockets ?? [])
      if (s.readyState === WebSocket.OPEN) {
        s.dispatchEvent(new CloseEvent('close', { code, reason: 'e2e', wasClean: true }));
        n++;
      }
    return n;
  }, code);
}

/** How many RTCPeerConnections the page has created so far (needs `trackPeerConnections`). */
export async function peerConnectionCount(page: Page): Promise<number> {
  return page.evaluate(
    () => (window as unknown as { __pcs?: RTCPeerConnection[] }).__pcs?.length ?? 0,
  );
}

/** The ICE ufrags in the local description of the page's newest open peer connection (they change on an ICE restart). */
export async function iceUfrags(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const pcs = (window as unknown as { __pcs?: RTCPeerConnection[] }).__pcs ?? [];
    const pc = [...pcs].reverse().find((p) => p.connectionState !== 'closed');
    const sdp = pc?.localDescription?.sdp ?? '';
    return [...new Set([...sdp.matchAll(/^a=ice-ufrag:(\S+)/gm)].map((m) => m[1] as string))];
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
