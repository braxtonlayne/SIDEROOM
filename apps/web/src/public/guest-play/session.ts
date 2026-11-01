// ARC-GST-002: the guest's network calls and nothing else. `joinLobby` is the one REST call of the page (the
// unauthenticated join route); `openLobby` is the guest socket with its single topic `game:<lobby_token>`. The same
// socket client seats a signed-in member of the Activity (the link's creator, `activities/guestBridge.ts`), which only
// changes the `hello`. This file cannot name another topic: `topicOf(token)` is the only string it ever sends as one,
// and the relay drops a socket that names any other (`4403`, the relay is the wall; this is the second one).
import { b64, unb64 } from '@sideroom/embedded-app-sdk';
import {
  GUEST_SOCKET_PATH,
  GUEST_SOCKET_VERSION,
  joinPath,
  topicOf,
  type GuestDown,
  type GuestJoined,
  type GuestPeer,
  type GuestUp,
} from './contract';
import { open as openSealed, seal } from './seal';

export type JoinFailure = 'gone' | 'rate_limited' | 'refused' | 'network';

export class JoinError extends Error {
  readonly reason: JoinFailure;
  constructor(reason: JoinFailure) {
    super(reason);
    this.reason = reason;
  }
}

/** `POST /api/v1/play/{token}/join`. No body, no cookie, no referrer: the callsign comes from the server. */
export async function joinLobby(token: string, f: typeof fetch = fetch): Promise<GuestJoined> {
  let res: Response;
  try {
    res = await f(joinPath(token), {
      method: 'POST',
      credentials: 'omit',
      referrerPolicy: 'no-referrer',
    });
  } catch {
    throw new JoinError('network');
  }
  if (res.status === 429) throw new JoinError('rate_limited');
  if (res.status === 404 || res.status === 410) throw new JoinError('gone');
  if (!res.ok) throw new JoinError('refused');
  const j = (await res.json().catch(() => null)) as Partial<GuestJoined> | null;
  if (
    !j ||
    typeof j.guest_token !== 'string' ||
    typeof j.callsign !== 'string' ||
    typeof j.application_id !== 'string'
  ) {
    throw new JoinError('refused');
  }
  return {
    guest_token: j.guest_token,
    callsign: j.callsign,
    expires_at: typeof j.expires_at === 'string' ? j.expires_at : '',
    topic: typeof j.topic === 'string' ? j.topic : topicOf(token),
    socket_path: GUEST_SOCKET_PATH,
    application_id: j.application_id,
    application_name: typeof j.application_name === 'string' ? j.application_name : '',
    ...(Array.isArray(j.capabilities)
      ? { capabilities: j.capabilities.filter((c): c is string => typeof c === 'string') }
      : {}),
  };
}

/** The slice of `WebSocket` the session uses. */
export interface SocketLike {
  readyState: number;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: { code: number }) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  send(data: string): void;
  close(code?: number): void;
}

/** What opening the lobby tells the caller once the relay seated it. */
export interface Seated {
  seat: number;
  role: 'guest' | 'member';
  callsign: string;
  peers: GuestPeer[];
}

/** An opened (and authenticated) payload published by another seat. */
export interface LobbyMessage {
  seat: number;
  role: 'guest' | 'member';
  bytes: Uint8Array;
}

export interface Lobby {
  /** Resolves when the relay answered `ready`; rejects when the socket ended first (`4401`, `4409`, ...). */
  seated: Promise<Seated>;
  peers(): GuestPeer[];
  onPeers(cb: () => void): () => void;
  /** Seal and publish; false when the socket is closed or the message was dropped. */
  publish(bytes: Uint8Array): Promise<boolean>;
  onMessage(cb: (m: LobbyMessage) => void): () => void;
  /** The socket ended: closed by the relay (`4403` for a topic that is not ours), revoked, expired or lost. */
  onEnd(cb: (code: number) => void): () => void;
  close(): void;
}

export interface OpenOptions {
  url: string;
  /** `{guest_token}` for a guest; `{lobby_token}` for a signed-in member of the Activity (cookie + Origin). */
  hello: { guest_token: string } | { lobby_token: string };
  lobbyToken: string;
  key: CryptoKey;
  /** Test seam. */
  makeSocket?: (url: string) => SocketLike;
}

/** `wss://host/gateway/guest?v=1` for the page's own origin. */
export function guestSocketUrl(loc: Pick<Location, 'protocol' | 'host'> = window.location): string {
  return `${loc.protocol === 'https:' ? 'wss:' : 'ws:'}//${loc.host}${GUEST_SOCKET_PATH}?v=${GUEST_SOCKET_VERSION}`;
}

const OPEN = 1;

export function openLobby(o: OpenOptions): Lobby {
  const ws = (o.makeSocket ?? ((u: string) => new WebSocket(u) as unknown as SocketLike))(o.url);
  const topic = topicOf(o.lobbyToken);
  let peers: GuestPeer[] = [];
  let me = -1;
  const messageListeners = new Set<(m: LobbyMessage) => void>();
  const peerListeners = new Set<() => void>();
  const endListeners = new Set<(code: number) => void>();
  let ended = false;
  let inChain: Promise<void> = Promise.resolve();
  let outChain: Promise<void> = Promise.resolve();
  let onSeated: (s: Seated) => void = () => undefined;
  let onFailed: (e: Error) => void = () => undefined;
  const seated = new Promise<Seated>((resolve, reject) => {
    onSeated = resolve;
    onFailed = reject;
  });
  // A rejection nobody awaits (the caller used onEnd) must not surface as unhandled.
  seated.catch(() => undefined);

  const sendFrame = (f: GuestUp) => ws.send(JSON.stringify(f));
  const end = (code: number) => {
    if (ended) return;
    ended = true;
    onFailed(new Error(`closed ${code}`));
    for (const l of [...endListeners]) l(code);
  };
  const roleOf = (seat: number): 'guest' | 'member' | null =>
    peers.find((p) => p.seat === seat)?.role ?? null;

  ws.onopen = () => sendFrame({ op: 'hello', ...o.hello });
  ws.onclose = (ev) => end(ev.code);
  ws.onerror = () => end(1006);
  ws.onmessage = (ev) => {
    if (typeof ev.data !== 'string') return;
    let f: GuestDown;
    try {
      f = JSON.parse(ev.data) as GuestDown;
    } catch {
      return;
    }
    switch (f.op) {
      case 'ready': {
        me = f.seat;
        peers = [
          { seat: f.seat, role: f.role, callsign: f.callsign },
          ...(Array.isArray(f.peers) ? f.peers.filter((p) => p.seat !== f.seat) : []),
        ];
        // The topic is implied by the credential; the relay wants it repeated, and acknowledges.
        sendFrame({ op: 'subscribe', topic });
        onSeated({ seat: f.seat, role: f.role, callsign: f.callsign, peers });
        for (const l of [...peerListeners]) l();
        return;
      }
      case 'peer': {
        if (f.event === 'join') peers = [...peers.filter((p) => p.seat !== f.peer.seat), f.peer];
        else peers = peers.filter((p) => p.seat !== f.seat);
        for (const l of [...peerListeners]) l();
        return;
      }
      case 'data': {
        if (typeof f.from !== 'number' || typeof f.data !== 'string' || f.from === me) return;
        const seat = f.from;
        let sealed: Uint8Array;
        try {
          sealed = unb64(f.data);
        } catch {
          return;
        }
        const run = async () => {
          // The role is the relay's word, looked up at delivery time (a peer event precedes its first message).
          const role = roleOf(seat);
          if (!role) return;
          const bytes = await openSealed(o.key, o.lobbyToken, sealed);
          if (!bytes) return;
          for (const l of [...messageListeners]) l({ seat, role, bytes });
        };
        inChain = inChain.then(run, run);
        return;
      }
      case 'closed':
        // The relay follows this with a close frame; the end is reported once, on the socket's close.
        return;
      default:
        return;
    }
  };

  return {
    seated,
    peers: () => peers,
    onPeers(cb) {
      peerListeners.add(cb);
      return () => peerListeners.delete(cb);
    },
    publish(bytes) {
      const run = async () => {
        if (ended || ws.readyState !== OPEN || me < 0) return false;
        const sealed = await seal(o.key, o.lobbyToken, bytes);
        if (ended || ws.readyState !== OPEN) return false;
        sendFrame({ op: 'publish', topic, data: b64(sealed) });
        return true;
      };
      const p = outChain.then(run, run);
      outChain = p.then(
        () => undefined,
        () => undefined,
      );
      return p;
    },
    onMessage(cb) {
      messageListeners.add(cb);
      return () => messageListeners.delete(cb);
    },
    onEnd(cb) {
      endListeners.add(cb);
      return () => endListeners.delete(cb);
    },
    close() {
      try {
        ws.close(1000);
      } catch {
        /* already closed */
      }
      end(1000);
    },
  };
}
