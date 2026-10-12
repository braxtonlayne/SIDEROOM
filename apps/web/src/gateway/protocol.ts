// Wire constants mirroring crates/sr-gateway/src/protocol.rs (Discord numbering, docs/05 section 6).
export const Op = {
  DISPATCH: 0,
  HEARTBEAT: 1,
  IDENTIFY: 2,
  PRESENCE_UPDATE: 3,
  VOICE_STATE_UPDATE: 4,
  RESUME: 6,
  RECONNECT: 7,
  REQUEST_GUILD_MEMBERS: 8,
  INVALID_SESSION: 9,
  HELLO: 10,
  HEARTBEAT_ACK: 11,
  WATCH_PARTY_SYNC: 100,
  E2EE_MLS: 101,
  LAZY_REQUEST: 102,
} as const;

export const CloseCode = {
  UNKNOWN_ERROR: 4000,
  DECODE_ERROR: 4002,
  NOT_AUTHENTICATED: 4003,
  AUTHENTICATION_FAILED: 4004,
  INVALID_SEQ: 4007,
  SESSION_TIMED_OUT: 4009,
  INVALID_INTENTS: 4013,
  DISALLOWED_INTENTS: 4014,
} as const;

/** Close codes after which reconnecting cannot succeed without user action. */
export const FATAL_CLOSE_CODES: ReadonlySet<number> = new Set([
  CloseCode.AUTHENTICATION_FAILED,
  4010,
  4011,
  4012,
  CloseCode.INVALID_INTENTS,
  CloseCode.DISALLOWED_INTENTS,
]);

/** Close codes after which the old session can no longer be resumed. */
export const SESSION_LOST_CLOSE_CODES: ReadonlySet<number> = new Set([
  CloseCode.INVALID_SEQ,
  CloseCode.SESSION_TIMED_OUT,
]);

/** Intent bits (Discord positions). */
export const Intents = {
  GUILDS: 1 << 0,
  GUILD_MEMBERS: 1 << 1,
  GUILD_PRESENCES: 1 << 8,
  GUILD_MESSAGES: 1 << 9,
  GUILD_MESSAGE_REACTIONS: 1 << 10,
  GUILD_MESSAGE_TYPING: 1 << 11,
  DIRECT_MESSAGES: 1 << 12,
  DIRECT_MESSAGE_TYPING: 1 << 14,
  MESSAGE_CONTENT: 1 << 15,
} as const;

export interface GatewayPayload {
  op: number;
  d?: unknown;
  s?: number | null;
  t?: string | null;
}
