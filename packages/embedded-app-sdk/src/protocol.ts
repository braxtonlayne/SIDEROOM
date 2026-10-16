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

export const PROTOCOL_VERSION = 1;

/** Commands the client understands. The set mirrors the core of Discord's Embedded App SDK. */
export const COMMANDS = [
  'authorize',
  'authenticate',
  'getChannel',
  'getInstanceConnectedParticipants',
  'openExternalLink',
  'subscribe',
  'unsubscribe',
] as const;
export type Command = (typeof COMMANDS)[number];

/** Events a client can push after `subscribe`. */
export const EVENTS = ['ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE'] as const;
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
