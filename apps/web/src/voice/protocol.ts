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
  /** Camera and screen share: publish, subscribe, published, layer (see VideoIn / VideoOut below). */
  VIDEO: 12,
  /** Egress governor notices (client contract below; the SFU does not send them yet, VOX-SFU-008). */
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
 * Whether captures are published to the SFU (it forwards video since VOX-SFU-005/006). While false, camera and screen captures stay a local
 * preview: the client does not add video m-lines (the spec says they would be ignored) and does not announce
 * `self_video` / `self_stream` in its voice state, so nobody is shown a "LIVE" tile with nothing in it.
 */
export const VIDEO_PUBLISH = true;

/** Music bitrate bounds in kbit/s (MUS-JB-001): the channel setting `voice.music_bitrate_kbps`. */
export const MUSIC_MIN_KBPS = 32;
export const MUSIC_DEFAULT_KBPS = 256;
export const MUSIC_MAX_KBPS = 320;

export const clampMusicKbps = (kbps: number | undefined): number =>
  Math.min(MUSIC_MAX_KBPS, Math.max(MUSIC_MIN_KBPS, Math.round(kbps ?? MUSIC_DEFAULT_KBPS)));

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

/**
 * QUALITY (op 13), server -> client: the egress governor stepped this room down (`reduced: true`) or back up.
 * Contract proposed by the client for VOX-SFU-008; ignored by older servers.
 */
export interface QualityData {
  reduced: boolean;
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
 * Opus tuning (spec section 2): the `voice` profile is DTX and in-band FEC on, mono, constrained bitrate. The
 * `music` profile (MUS-JB-001, used while a music line is attached to the connection) is stereo with
 * `sprop-stereo`, DTX off, in-band FEC on and the music bitrate as the ceiling. Both lines share one Opus
 * payload type, so the profile applies to the whole connection while music plays; each sender's real cap is set
 * per sender with `setParameters` (the microphone stays at the channel bitrate).
 * Rewrites (or adds) the `a=fmtp` line of the opus payload type in an offer.
 */
export function tuneOpus(
  sdp: string,
  maxBitrate: number | undefined = 64_000,
  profile: 'voice' | 'music' = 'voice',
): string {
  const m = /a=rtpmap:(\d+) opus\/48000/i.exec(sdp);
  if (!m) return sdp;
  const pt = m[1]!;
  const wanted: Record<string, string> =
    profile === 'music'
      ? {
          minptime: '10',
          useinbandfec: '1',
          usedtx: '0',
          stereo: '1',
          'sprop-stereo': '1',
          maxaveragebitrate: String(maxBitrate),
        }
      : {
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

// ---- video (op 12), crates/sr-voice-proto/src/video.rs ----

/** What a published or forwarded track carries. */
export type VideoSource = 'camera' | 'screen' | 'screen_audio' | 'music';
export type VideoLayer = 'low' | 'mid' | 'high';

/** C->S: the m-line `mid` carries `source` (or stops carrying it with `active: false`). */
export interface VideoPublishOut {
  t: 'publish';
  mid: string;
  source: VideoSource;
  active: boolean;
  width: number;
  height: number;
  fps: number;
}

/** What a subscriber wants from one source of one participant. */
export interface VideoHint {
  user_id: string;
  /** `music` (MUS-JB-001) only uses `enabled`: a listener mutes someone's music track separately. */
  source: 'camera' | 'screen' | 'music';
  enabled?: boolean;
  layer?: VideoLayer;
  viewport?: { width: number; height: number };
}

export interface VideoSubscribeOut {
  t: 'subscribe';
  subscriptions: VideoHint[];
}

/** S->C: someone started or stopped publishing a source. */
export interface VideoPublishedIn {
  t: 'published';
  user_id: string;
  source: VideoSource;
  active: boolean;
  width?: number;
  height?: number;
  fps?: number;
}

/** S->C: the layer now sent for a source changed. */
export interface VideoLayerIn {
  t: 'layer';
  user_id: string;
  source: VideoSource;
  layer: VideoLayer;
  reason: 'hint' | 'bandwidth' | 'governor';
}

export type VideoIn = VideoPublishedIn | VideoLayerIn;

/**
 * The SFU names each forwarded line's msid stream: `<user>` is the microphone, `<user>-camera` the camera,
 * `<user>-screen` the screen and its audio (one stream group, so the browser plays them in sync),
 * `<user>-music` the stereo music track (MUS-JB-001).
 */
export function parseStreamId(id: string): {
  userId: string;
  kind: 'mic' | 'camera' | 'screen' | 'music';
} {
  if (id.endsWith('-music')) return { userId: id.slice(0, -'-music'.length), kind: 'music' };
  if (id.endsWith('-camera')) return { userId: id.slice(0, -'-camera'.length), kind: 'camera' };
  if (id.endsWith('-screen')) return { userId: id.slice(0, -'-screen'.length), kind: 'screen' };
  return { userId: id, kind: 'mic' };
}
