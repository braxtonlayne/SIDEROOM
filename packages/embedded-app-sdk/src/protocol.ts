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
//
// Events (after `subscribe`; `args.channel_id` is optional and, when given, must be the instance's channel):
//   ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE { participants }
//   VOICE_STATE_UPDATE { voice_state: { mute, deaf, self_mute, self_deaf, suppress }, user, nick, volume, mute,
//     pan: { left, right } }     a member of the instance's voice channel changed their state (Discord's shape)
//   SPEAKING_START / SPEAKING_STOP { channel_id, user_id }
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
] as const;
export type Command = (typeof COMMANDS)[number];

/** Events a client can push after `subscribe`. */
export const EVENTS = [
  'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
  'VOICE_STATE_UPDATE',
  'SPEAKING_START',
  'SPEAKING_STOP',
] as const;
export type EventName = (typeof EVENTS)[number];

/** What the client tells an Activity once the handshake succeeds. */
export interface ReadyConfig {
  application_id: string;
  instance_id: string;
  channel_id: string;
  guild_id: string;
}

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
