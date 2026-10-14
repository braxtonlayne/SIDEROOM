/**
 * Voice WebSocket protocol (`sr-voice-proto`, docs/specs/lounge-voice-video.md section 4.3; Rust types in
 * crates/sr-voice-proto/src/wire.rs). Text frames `{"op": n, "d": {...}}`, ids are decimal strings.
 *
 * Flow: gateway `VOICE_STATE_UPDATE` (op 4) -> `VOICE_SERVER_UPDATE` {endpoint (full ws URL), token,
 * ice_servers} -> open the endpoint, send IDENTIFY {token} -> READY -> client SDP_OFFER (after ICE gathering,
 * so no trickle) -> SDP_ANSWER. The server later sends its own SDP_OFFERs to add or remove one sendonly audio
 * line per remote participant (msid stream id = user id); the client answers and is the polite peer on glare.
 */
export const VoiceOp = {
  IDENTIFY: 0,
  READY: 1,
  HEARTBEAT: 2,
  HEARTBEAT_ACK: 3,
  SDP_OFFER: 4,
  SDP_ANSWER: 5,
  ICE_CANDIDATE: 6,
  SPEAKING: 7,
  RESUME: 8,
  RESUMED: 9,
  CLIENT_CONNECT: 10,
  CLIENT_DISCONNECT: 11,
  /** Reserved: video, screen share. */
  VIDEO: 12,
  /** Reserved: egress governor notices. */
  QUALITY: 13,
} as const;

export const VoiceClose = {
  AUTHENTICATION_FAILED: 4004,
  SESSION_INVALID: 4006,
  HEARTBEAT_TIMEOUT: 4009,
  /** More than 200 messages in 10 s: reconnect and RESUME (handled like any other drop). */
  RATE_LIMITED: 4008,
  /** Removed from the call (moderator disconnect or move): do not reconnect. */
  DISCONNECTED: 4014,
  /** SFU draining: resume, else rejoin. */
  SERVER_RESTARTING: 4015,
  /** The same user joined from another tab or device. */
  SESSION_REPLACED: 4016,
  SERVER_FULL: 4017,
} as const;

/** Closes after which only a fresh join through the gateway (new VOICE_SERVER_UPDATE) can help. */
export const REJOIN_VOICE_CLOSES: ReadonlySet<number> = new Set([
  1,
  2,
  3,
  4001,
  4002,
  4003,
  VoiceClose.AUTHENTICATION_FAILED,
  VoiceClose.SESSION_INVALID,
]);

/**
 * Whether the SFU forwards video yet (VOX-SFU-005/006). While false, camera and screen captures stay a local
 * preview: the client does not add video m-lines (the spec says they would be ignored) and does not announce
 * `self_video` / `self_stream` in its voice state, so nobody is shown a "LIVE" tile with nothing in it.
 */
export const VIDEO_PUBLISH = false;

/** SPEAKING bitmask. */
export const SPEAKING_VOICE = 1;
export const SPEAKING_PRIORITY = 4;

export interface VoiceFrame {
  op: number;
  d?: unknown;
}

export interface IdentifyData {
  token: string;
}

export interface ParticipantInfo {
  user_id: string;
  speaking: number;
  muted?: boolean;
  deafened?: boolean;
}

/** READY and RESUMED. */
export interface ReadyData {
  session_id: string;
  resume_token: string;
  heartbeat_interval: number;
  user_id?: string;
  channel_id?: string;
  guild_id?: string;
  participants?: ParticipantInfo[];
}

export interface SdpData {
  sdp: string;
}

export interface SpeakingData {
  /** Present on server -> client frames. */
  user_id?: string;
  speaking: number;
  audio_level?: number;
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

/** The WebSocket URL for a `VOICE_SERVER_UPDATE` endpoint: normally a full ws(s) URL; a bare host is tolerated. */
export function voiceUrl(
  endpoint: string,
  secure: boolean = globalThis.location?.protocol === 'https:',
): string {
  if (/^wss?:\/\//.test(endpoint)) return endpoint;
  const scheme = secure ? 'wss' : 'ws';
  // A leading slash means same-origin behind the reverse proxy / dev proxy.
  if (endpoint.startsWith('/')) {
    return `${scheme}://${globalThis.location?.host ?? 'localhost'}${endpoint}`;
  }
  return `${scheme}://${endpoint}/voice/ws`;
}

/**
 * Opus tuning for voice: DTX and in-band FEC on, mono, constrained bitrate (spec section 2).
 * Rewrites (or adds) the `a=fmtp` line of the opus payload type in an offer.
 */
export function tuneOpus(sdp: string, maxBitrate: number | undefined = 64_000): string {
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
