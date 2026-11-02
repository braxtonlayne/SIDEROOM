// The wire protocol between an Activity (an iframe on `https://<application-id>.activities.<host>`) and the
// SIDE ROOM client that hosts it (DEV-ACT-003). Every frame is a plain object posted with `postMessage`; the
// `sr` field is the protocol version. The client answers only the iframe it created, and the Activity only
// trusts its parent window, so neither side needs a secret.
//
// Activity -> client:
//   { sr: 1, kind: 'handshake', application_id }            first message, from `ready()`
//   { sr: 1, kind: 'request', nonce, cmd, args }            one of COMMANDS
// Client -> Activity:
//   { sr: 1, kind: 'ready', config }                         answer to the handshake
//   { sr: 1, kind: 'response', nonce, ok: true, data }       or { ok: false, error: { code, message } }
//   { sr: 1, kind: 'event', event, data }                    after `subscribe`
//
// Commands (args -> result):
//   authorize { scope: ['identify' | 'guilds'][] } -> { code }
//   authenticate { access_token } -> AuthenticateResult
//   getChannel -> ChannelInfo
//   getInstanceConnectedParticipants -> { participants }
//   openExternalLink { url } -> { opened }
//   setActivity { activity: ActivityInput } -> ActivityInput       the person's Rich Presence while the Activity
//     runs (type Playing, this application). `details`, `state` (128 characters each), `timestamps`
//     { start, end } in epoch milliseconds, `party` { id }, `assets` { large_image, large_text, small_image,
//     small_text }: an image is the name of one of the application's Rich Presence assets and is dropped
//     when no asset has that name. Calling it again replaces the activity; `{ activity: null }` clears it.
//     The host clears it when the Activity closes or the instance ends.
//   subscribe { event, args? } / unsubscribe { event }
//   storageSet { key, value } / storageRemove { key } -> {}   ARC-SBX-001, needs the `storage` capability: the
//     Activity runs in an opaque-origin sandbox with no `localStorage`, so the host keeps a small per-member,
//     per-application key-value store (64 keys, 32 KiB a value, 128 KiB in all) and sends it in `ready`
//     (`config.storage`). Reads are local to the SDK; writes go through the bridge.
//   saveFile { name, mime, b } -> { saved }   ARC-SBX-001: hand the person a file (base64, at most 8 MiB, png, jpeg,
//     webp, json or plain text only). A sandbox without `allow-downloads` cannot save one itself, so the host
//     asks the person and downloads it from the app page.
//   shelfObjectUrl { shelf_id, object_hash } -> { url, expires_at }   D-389 (wave 46), needs the `byos_read` capability:
//     a presigned GET for ONE object of a shelf the member can open (their own, a friend's grant, a server shelf),
//     the same per-object URL the Study uses (ten minutes at most). The member is asked first, once per object
//     and frame. Never for a guest (D-397). The URL is on the shelf's storage host, so the game can fetch it only
//     when its reviewed manifest names that host in `connect` (CSP is per host; docs/dev/design/byos-read-activities.md).
//   dataSend { b } -> { sent }          ACT-CH-001, needs the `datachannel` capability: `b` is base64 bytes (an
//     SDK data envelope), sealed by the host page with the room's key and relayed by the SFU to the instance
//
// Events (after `subscribe`; `args.channel_id` is optional and, when given, must be the instance's channel):
//   ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE { participants }
//   VOICE_STATE_UPDATE { voice_state: { mute, deaf, self_mute, self_deaf, suppress }, user, nick, volume, mute,
//     pan: { left, right } }     a member of the instance's voice channel changed their state (Discord's shape)
//   SPEAKING_START / SPEAKING_STOP { channel_id, user_id }
//   DATA_MESSAGE { from, b }          ACT-CH-001: bytes (base64) another member of the instance sent; `from` is
//     authenticated by the room's key, never claimed by the sender
//   HOST_CHANGED { host_id, previous }   D-388 (wave 46): the instance's host left and the platform made
//     `host_id` (the longest-connected member; never a guest) the host. Needs the `host_handoff` capability; without
//     it the instance ends with its host. The SDK subscribes for the data channel itself and moves `sdk.hostId`.
// Voice events exist only for an Activity in a voice channel and only for that channel.

export const PROTOCOL_VERSION = 1;

/** Commands the client understands. The set mirrors the core of Discord's Embedded App SDK. */
export const COMMANDS = [
  'authorize',
  'authenticate',
  'getChannel',
  'getInstanceConnectedParticipants',
  'openExternalLink',
  'setActivity',
  'subscribe',
  'unsubscribe',
  'dataSend',
  'storageSet',
  'storageRemove',
  'saveFile',
  'shelfObjectUrl',
] as const;
export type Command = (typeof COMMANDS)[number];

/** Events a client can push after `subscribe`. */
export const EVENTS = [
  'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
  'VOICE_STATE_UPDATE',
  'SPEAKING_START',
  'SPEAKING_STOP',
  'DATA_MESSAGE',
  'HOST_CHANGED',
] as const;
export type EventName = (typeof EVENTS)[number];

/** What the client tells an Activity once the handshake succeeds. */
export interface ReadyConfig {
  application_id: string;
  instance_id: string;
  channel_id: string;
  guild_id: string;
  /** The signed-in member (ACT-CH-001). */
  user_id?: string;
  /** The instance's host: the only writer of `host:*` keys (the launcher, or the successor after HOST_CHANGED). */
  host_id?: string;
  /** What the reviewed bundle declared in `sideroom.json` (`datachannel`, `storage`). */
  capabilities?: string[];
  /**
   * ARC-GST-002 (D-397): true when the game runs for a guest on `/play/<token>`. A guest bridge has no channel,
   * server or member data: `channel_id`/`guild_id` are placeholders, `user_id` is the lobby id the relay stamps and
   * `callsign` the name they play under.
   */
  guest?: boolean;
  callsign?: string;
  /** The `storage` capability: every key the member has stored for this application (ARC-SBX-001). */
  storage?: Record<string, string>;
  /**
   * The instance's own credential for writes through the application's URL mappings (D-399): the sandboxed
   * frame's requests carry `Origin: null`, which the proxy accepts only with this token in
   * [`PROXY_TOKEN_HEADER`]. `sdk.mappedFetch` sends it. Absent when the client could not mint one.
   */
  proxy_token?: string;
}

/** Request header of the proxy token on writes under a URL mapping (D-399). */
export const PROXY_TOKEN_HEADER = 'X-SR-Activity-Token';

/** The capability a bundle declares to use `storageSet` / `storageRemove` and receive `config.storage`. */
export const STORAGE_CAPABILITY = 'storage';
/** Limits of the bridge key-value store; the host enforces them, the SDK mirrors them for early errors. */
export const STORAGE_LIMITS = {
  keys: 64,
  keyChars: 128,
  valueChars: 32 * 1024,
  totalChars: 128 * 1024,
} as const;
/** Most bytes `saveFile` takes, and the types it hands over. */
export const SAVE_FILE_MAX_BYTES = 8 * 1024 * 1024;
export const SAVE_FILE_TYPES = [
  'image/png',
  'image/jpeg',
  'image/webp',
  'application/json',
  'text/plain',
] as const;

export interface UserInfo {
  id: string;
  username: string | null;
  global_name: string | null;
  avatar: string | null;
  /** An absolute, possibly short-lived image URL (SIDE ROOM extension; there is no avatar hash). */
  avatar_url?: string | null;
}

export interface ChannelInfo {
  id: string;
  guild_id: string;
  name: string;
}

export interface Participant extends UserInfo {
  bot?: boolean;
}

export interface AuthenticateResult {
  access_token: string;
  user: UserInfo | null;
  scopes: string[];
  expires: string;
  application: { id: string; name: string };
}

export type Frame =
  | { sr: 1; kind: 'handshake'; application_id: string }
  | { sr: 1; kind: 'request'; nonce: string; cmd: string; args?: unknown }
  | { sr: 1; kind: 'ready'; config: ReadyConfig }
  | { sr: 1; kind: 'response'; nonce: string; ok: true; data: unknown }
  | { sr: 1; kind: 'response'; nonce: string; ok: false; error: RpcErrorBody }
  | { sr: 1; kind: 'event'; event: string; data: unknown };

export interface RpcErrorBody {
  code: string;
  message: string;
}

/** Error codes of a failed command. */
export const RPC_ERRORS = {
  unknownCommand: 'UNKNOWN_COMMAND',
  invalidArguments: 'INVALID_ARGUMENTS',
  notAuthorized: 'NOT_AUTHORIZED',
  denied: 'USER_DENIED',
  failed: 'FAILED',
  rateLimited: 'RATE_LIMITED',
} as const;

/** Parse an unknown `MessageEvent.data` into a [`Frame`], or `null` when it is not ours. */
export function parseFrame(data: unknown): Frame | null {
  if (typeof data !== 'object' || data === null) return null;
  const f = data as Record<string, unknown>;
  if (f.sr !== PROTOCOL_VERSION || typeof f.kind !== 'string') return null;
  switch (f.kind) {
    case 'handshake':
      return typeof f.application_id === 'string' ? (f as unknown as Frame) : null;
    case 'request':
      return typeof f.nonce === 'string' && typeof f.cmd === 'string'
        ? (f as unknown as Frame)
        : null;
    case 'ready':
      return typeof f.config === 'object' && f.config !== null ? (f as unknown as Frame) : null;
    case 'response':
      return typeof f.nonce === 'string' && typeof f.ok === 'boolean'
        ? (f as unknown as Frame)
        : null;
    case 'event':
      return typeof f.event === 'string' ? (f as unknown as Frame) : null;
    default:
      return null;
  }
}

/** Arguments of `subscribe`; `channel_id` must be the instance's channel when given. */
export interface SubscribeArgs {
  channel_id?: string;
}

/** The Rich Presence an Activity sets with `setActivity`. */
export interface ActivityInput {
  details?: string;
  state?: string;
  timestamps?: { start?: number; end?: number };
  party?: { id?: string };
  assets?: {
    large_image?: string;
    large_text?: string;
    small_image?: string;
    small_text?: string;
  };
}

/** `VOICE_STATE_UPDATE` data (Discord's shape). */
export interface VoiceStateUpdate {
  voice_state: {
    mute: boolean;
    deaf: boolean;
    self_mute: boolean;
    self_deaf: boolean;
    suppress: boolean;
  };
  user: Participant;
  nick: string | null;
  volume: number;
  mute: boolean;
  pan: { left: number; right: number };
}

/** `SPEAKING_START` / `SPEAKING_STOP` data. */
export interface SpeakingUpdate {
  channel_id: string;
  user_id: string;
}

/** `HOST_CHANGED` data (D-388): the new host and the one who left. */
export interface HostChanged {
  host_id: string;
  previous: string;
}

/** The capability a bundle declares to keep running when its host leaves (D-388). */
export const HOST_HANDOFF_CAPABILITY = 'host_handoff';

/** The capability a bundle declares to use `shelfObjectUrl` (D-389). */
export const BYOS_READ_CAPABILITY = 'byos_read';

/** `shelfObjectUrl` arguments: a shelf (mount) id and an object hash from that shelf's catalog. */
export interface ShelfObjectRef {
  shelf_id: string;
  object_hash: string;
}

/** `shelfObjectUrl` result: a presigned GET for one object and when it stops working (ISO 8601). */
export interface ShelfObjectUrl {
  url: string;
  expires_at: string;
}

/** Whether `r` names a shelf object: a snowflake id and a lowercase hex SHA-256. */
export function isShelfObjectRef(r: unknown): r is ShelfObjectRef {
  if (typeof r !== 'object' || r === null) return false;
  const o = r as Record<string, unknown>;
  return (
    typeof o.shelf_id === 'string' &&
    /^[0-9]{1,20}$/.test(o.shelf_id) &&
    typeof o.object_hash === 'string' &&
    /^[0-9a-f]{64}$/.test(o.object_hash)
  );
}
