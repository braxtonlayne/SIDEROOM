// ARC-GST-002 (wave 43, D-385, D-397): the wire contract between the guest runner / the Share link dialog and the
// guest relay that 43-A (`ARC-GST-001`) builds. One module, so the integrator reconciles one file. Everything here
// is plain data and constants; nothing imports the app. Marked "ASSUMED" where 43-A's committed types were not yet
// available when this was written (docs/dev/handoff/feat-p43-guest-runner.md lists them).

/** `POST /api/v1/activity-instances/{id}/guest-links` (a member of the running instance). Body: none. */
export interface GuestLinkCreated {
  lobby_token: string;
  /** ISO-8601 instant after which the token no longer joins. */
  expires_at: string;
}

/** `POST /api/v1/play/{lobby_token}/join` (unauthenticated, rate-limited). */
export interface GuestJoinRequest {
  /** A nickname the lobby may approve; absent or refused: the callsign below is used. */
  nickname?: string;
  /** The callsign the runner drew (ASSUMED: the server may replace it and says so in the answer). */
  callsign: string;
}

/** The answer of the join route (ASSUMED shape). */
export interface GuestJoinResponse {
  /** Short-lived; sent in the guest socket's first frame, never in a URL. */
  session_token: string;
  /** The name the guest plays under in this lobby. */
  callsign: string;
  /** The guest's id inside the lobby (stamped by the relay on everything they send). */
  guest_id: string;
  /** The game the lobby runs and what its reviewed manifest declares. */
  application_id: string;
  application_name: string;
  capabilities: string[];
  /** The lobby title the host chose (D-397: the only server-side name a guest sees). */
  lobby_title?: string;
  expires_at: string;
}

/** The only topic a guest socket has (D-397): `game:<lobby_token>`. */
export const topicOf = (lobbyToken: string): string => `game:${lobbyToken}`;

/** The guest socket (ASSUMED path; `GET` upgrade, same host as the page). */
export const GUEST_SOCKET_PATH = '/gateway/guest';

/** Client -> relay frames of the guest socket (ASSUMED JSON shapes). */
export type GuestUp =
  | { op: 'hello'; token: string; topic: string }
  | { op: 'data'; topic: string; b: string };

/** Relay -> client frames. `from` is stamped by the relay, never claimed by the sender. */
export type GuestDown =
  | { op: 'ready'; roster: GuestMember[] }
  | { op: 'roster'; members: GuestMember[] }
  | { op: 'data'; topic: string; from: string; b: string }
  | { op: 'drop'; code: number; reason?: string };

/** A person in the lobby: the id the relay stamps and the name they play under. Nothing else (D-397). */
export interface GuestMember {
  id: string;
  nickname: string;
  /** A member of the server who is running the game (shown as "host"; no handle, no avatar). */
  host?: boolean;
}

/** Lobby tokens are random url-safe strings; the runner refuses anything else before it asks the server. */
export const LOBBY_TOKEN = /^[A-Za-z0-9_-]{16,128}$/;
/** The ephemeral game key: 32 random bytes, base64url without padding (43 characters). */
export const GAME_KEY = /^[A-Za-z0-9_-]{43}$/;

/** Join route path for a token (the page calls it through `fetch`, same origin). */
export const joinPath = (lobbyToken: string): string =>
  `/api/v1/play/${encodeURIComponent(lobbyToken)}/join`;

/** Member side. */
export const guestLinksPath = (instanceId: string): string =>
  `/activity-instances/${encodeURIComponent(instanceId)}/guest-links`;
export const guestLinkPath = (instanceId: string, lobbyToken: string): string =>
  `${guestLinksPath(instanceId)}/${encodeURIComponent(lobbyToken)}`;
