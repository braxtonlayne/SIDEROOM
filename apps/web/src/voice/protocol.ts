/**
 * Voice WebSocket protocol (`sr-voice-proto`, docs/specs/lounge-voice-video.md section 4).
 *
 * Discord-voice-gateway-like opcodes. The numbers and payload shapes here are the client's reading of the
 * contract; if the SFU's `sr-voice-proto` differs, this file is the only place that changes (the engine
 * speaks to it through `encode` / `parseFrame`). Assumptions are listed in
 * docs/dev/handoff/feat-p5-voice-client.md.
 *
 * Flow: gateway `VOICE_STATE_UPDATE` (op 4) -> `VOICE_SERVER_UPDATE` {endpoint, token, ice_servers} ->
 * WebSocket `wss://{endpoint}/voice?v=1` -> HELLO -> IDENTIFY -> READY -> SELECT_PROTOCOL {sdp offer}
 * -> SESSION_DESCRIPTION {sdp answer}. The offer is sent after ICE gathering, so no trickle ops exist.
 */
export const VoiceOp = {
  IDENTIFY: 0,
  SELECT_PROTOCOL: 1,
  READY: 2,
  HEARTBEAT: 3,
  SESSION_DESCRIPTION: 4,
  SPEAKING: 5,
  HEARTBEAT_ACK: 6,
  RESUME: 7,
  HELLO: 8,
  RESUMED: 9,
  VIDEO: 12,
  CLIENT_DISCONNECT: 13,
  CLIENT_CONNECT: 18,
} as const;

/** Close codes after which the voice session is gone and a fresh VOICE_SERVER_UPDATE is needed. */
export const VoiceClose = {
  AUTHENTICATION_FAILED: 4004,
  SESSION_NO_LONGER_VALID: 4006,
  SESSION_TIMEOUT: 4009,
  SERVER_NOT_FOUND: 4011,
  DISCONNECTED: 4014,
  SERVER_CRASHED: 4015,
} as const;

export const FATAL_VOICE_CLOSES: ReadonlySet<number> = new Set([
  VoiceClose.AUTHENTICATION_FAILED,
  VoiceClose.SESSION_NO_LONGER_VALID,
  VoiceClose.SESSION_TIMEOUT,
  VoiceClose.SERVER_NOT_FOUND,
  VoiceClose.DISCONNECTED,
]);

/** SPEAKING flag bits. */
export const SPEAKING_VOICE = 1;
export const SPEAKING_PRIORITY = 4;

export interface VoiceFrame {
  op: number;
  d?: unknown;
}

export interface IdentifyData {
  server_id: string;
  user_id: string;
  session_id: string;
  token: string;
}

export interface ReadyData {
  ssrc?: number;
  ice_servers?: { urls: string | string[]; username?: string; credential?: string }[];
}

export interface SessionDescriptionData {
  sdp: string;
  audio_codec?: string;
}

export interface SpeakingData {
  /** Present on server -> client frames. */
  user_id?: string;
  speaking: number;
  ssrc?: number;
  delay?: number;
}

export interface HelloData {
  heartbeat_interval: number;
}

export const encode = (op: number, d?: unknown): string => JSON.stringify({ op, d });

export function parseFrame(raw: unknown): VoiceFrame | null {
  if (typeof raw !== 'string') return null;
  try {
    const f = JSON.parse(raw) as VoiceFrame;
    return typeof f?.op === 'number' ? f : null;
  } catch {
    return null;
  }
}

/** `wss://host/voice?v=1` for a `VOICE_SERVER_UPDATE` endpoint (a bare host, host:port, path, or full URL). */
export function voiceUrl(
  endpoint: string,
  secure: boolean = globalThis.location?.protocol === 'https:',
): string {
  if (/^wss?:\/\//.test(endpoint)) return endpoint;
  const scheme = secure ? 'wss' : 'ws';
  // A leading slash means same-origin through the reverse proxy / dev proxy ("/voice").
  if (endpoint.startsWith('/')) {
    return `${scheme}://${globalThis.location?.host ?? 'localhost'}${endpoint}`;
  }
  return `${scheme}://${endpoint}/voice?v=1`;
}

/**
 * Opus tuning for voice: DTX and in-band FEC on, mono, constrained bitrate (spec section 2).
 * Rewrites (or adds) the `a=fmtp` line of the opus payload type in an offer.
 */
export function tuneOpus(sdp: string, maxBitrate = 64_000): string {
  const m = /a=rtpmap:(\d+) opus\/48000/i.exec(sdp);
  if (!m) return sdp;
  const pt = m[1]!;
  const wanted: Record<string, string> = {
    minptime: '10',
    useinbandfec: '1',
    usedtx: '1',
    stereo: '0',
    maxaveragebitrate: String(maxBitrate),
  };
  const re = new RegExp(`a=fmtp:${pt} ([^\\r\\n]*)`);
  const cur = re.exec(sdp);
  const params = new Map<string, string>();
  if (cur) {
    for (const kv of cur[1]!.split(';')) {
      const [k, v] = kv.split('=');
      if (k) params.set(k.trim(), (v ?? '').trim());
    }
  }
  for (const [k, v] of Object.entries(wanted)) params.set(k, v);
  const line = `a=fmtp:${pt} ${[...params].map(([k, v]) => (v === '' ? k : `${k}=${v}`)).join(';')}`;
  if (cur) return sdp.replace(re, line);
  return sdp.replace(m[0], `${m[0]}\r\n${line}`);
}
