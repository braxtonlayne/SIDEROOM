// ARC-GST-002 (wave 43, D-385, D-397): the wire contract between the guest runner / the Share link dialog and the
// guest relay that 43-A (`ARC-GST-001`) built. The Rust side is `sr_activities::guest` (types, routes) and
// `sr_gateway::guest` (the socket frames), read from branch feat/p43-guest-relay at 898d4de6 / f2eaddbb. One
// module, so the integrator reconciles one file. Everything here is plain data and constants; nothing imports
// the app. "ASSUMED" marks what 43-A's contract does not carry yet (see the handoff).

/** `POST /api/v1/activity-instances/{id}/guest-links` (a member of the running instance), 201. No body. */
export interface GuestLinkCreated {
  /** 64 lowercase hex digits, shown once. */
  lobby_token: string;
  /** RFC 3339, UTC. */
  expires_at: string;
}

/** `POST /api/v1/play/{lobby_token}/join` (unauthenticated, rate-limited), 200. No body: guests never choose a name. */
export interface GuestJoined {
  /** `srg_` plus 64 hex digits; sent as the `hello` of the guest socket, accepted by nothing else. */
  guest_token: string;
  /** The random callsign the lobby shows for this guest. */
  callsign: string;
  expires_at: string;
  /** `game:<lobby_token>`. */
  topic: string;
  /** `/gateway/guest`. */
  socket_path: string;
  application_id: string;
  application_name: string;
  /**
   * ASSUMED (not in 43-A's `GuestJoined` yet): what the game's reviewed manifest declares, so the runner can refuse
   * `ingame_voice`. Absent: the runner treats the game as `guest_play` (the server only issues links for such
   * games) and the `ingame_voice` refusal cannot fire client-side. The integrator asks 43-A to add it.
   */
  capabilities?: string[];
}

/** The only topic a guest socket has (D-397): `game:<lobby_token>`. */
export const topicOf = (lobbyToken: string): string => `game:${lobbyToken}`;

export const GUEST_SOCKET_PATH = '/gateway/guest';
export const GUEST_SOCKET_VERSION = 1;

/** Close codes of the guest socket (`sr_gateway::guest`): 4403 is the relay's "403/Drop". */
export const CLOSE = {
  badFrame: 4400,
  unauthorized: 4401,
  forbidden: 4403,
  full: 4409,
  gone: 4410,
  tooLarge: 4413,
} as const;

/** Client -> relay frames. A guest says `guest_token`; a signed-in member of the Activity says `lobby_token`. */
export type GuestUp =
  | { op: 'hello'; guest_token: string }
  | { op: 'hello'; lobby_token: string }
  | { op: 'subscribe'; topic: string }
  | { op: 'publish'; topic: string; data: string }
  | { op: 'ping' };

export interface GuestPeer {
  seat: number;
  role: 'guest' | 'member';
  callsign: string;
}

/** Relay -> client frames. `from` is a seat number stamped by the relay, never claimed by the sender. */
export type GuestDown =
  | {
      op: 'ready';
      seat: number;
      role: 'guest' | 'member';
      callsign: string;
      key_id: string;
      epoch: number;
      scope: { data: boolean; media: boolean };
      peers: GuestPeer[];
    }
  | { op: 'subscribed'; topic: string }
  | { op: 'pong' }
  | { op: 'data'; from: number; data: string }
  | { op: 'peer'; event: 'join'; peer: GuestPeer }
  | { op: 'peer'; event: 'leave'; seat: number }
  | { op: 'closed'; reason: 'revoked' | 'ended' };

/** Lobby tokens are 64 lowercase hex digits; the runner refuses anything else before it asks the server. */
export const LOBBY_TOKEN = /^[0-9a-f]{64}$/;
/** The ephemeral game key: 32 random bytes, base64url without padding (43 characters). */
export const GAME_KEY = /^[A-Za-z0-9_-]{43}$/;

/** Join route path for a token (the page calls it through `fetch`, same origin). */
export const joinPath = (lobbyToken: string): string =>
  `/api/v1/play/${encodeURIComponent(lobbyToken)}/join`;

/** Member side (paths relative to `/api/v1`, as `apiFetch` takes them). */
export const guestLinksPath = (instanceId: string): string =>
  `/activity-instances/${encodeURIComponent(instanceId)}/guest-links`;
export const guestLinkPath = (instanceId: string, lobbyToken: string): string =>
  `${guestLinksPath(instanceId)}/${encodeURIComponent(lobbyToken)}`;

/**
 * A person in the lobby as a game sees them (the runner's own shape, built from the relay's peers): an id the game
 * can tell messages apart by and the callsign the relay assigned. Nothing else (D-397).
 */
export interface GuestMember {
  /** `g<seat>` for a guest, `m<seat>` for a member seat (the game's host speaks under their room id, see envelope). */
  id: string;
  nickname: string;
  /** A member of the server who is running the game. No handle, no avatar. */
  host?: boolean;
}
