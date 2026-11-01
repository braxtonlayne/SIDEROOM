// ARC-GST-002: the guest's two network calls and nothing else. `joinLobby` is the one REST call of the page (the
// unauthenticated join route); `openGuestSocket` is the guest socket with its single topic `game:<lobby_token>`.
// This file cannot name another topic: `topicOf(token)` is the only string it ever sends as one, and a relay that
// answers anything but its own lobby's frames is closed on (the server drops first; this is the second wall).
import { b64, unb64 } from '@sideroom/embedded-app-sdk';
import {
  GUEST_SOCKET_PATH,
  joinPath,
  topicOf,
  type GuestDown,
  type GuestJoinRequest,
  type GuestJoinResponse,
  type GuestMember,
  type GuestUp,
} from './contract';
import type { GuestLane } from './guestHost';
import { open as openSealed, seal } from './seal';

export type JoinFailure = 'gone' | 'rate_limited' | 'refused' | 'network';

export class JoinError extends Error {
  readonly reason: JoinFailure;
  constructor(reason: JoinFailure) {
    super(reason);
    this.reason = reason;
  }
}

/** `POST /api/v1/play/{token}/join`. No cookie, no credentials, no referrer. */
export async function joinLobby(
  token: string,
  req: GuestJoinRequest,
  f: typeof fetch = fetch,
): Promise<GuestJoinResponse> {
  let res: Response;
  try {
    res = await f(joinPath(token), {
      method: 'POST',
      credentials: 'omit',
      referrerPolicy: 'no-referrer',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(req),
    });
  } catch {
    throw new JoinError('network');
  }
  if (res.status === 429) throw new JoinError('rate_limited');
  if (res.status === 404 || res.status === 410) throw new JoinError('gone');
  if (!res.ok) throw new JoinError('refused');
  const j = (await res.json().catch(() => null)) as Partial<GuestJoinResponse> | null;
  if (
    !j ||
    typeof j.session_token !== 'string' ||
    typeof j.callsign !== 'string' ||
    typeof j.guest_id !== 'string' ||
    typeof j.application_id !== 'string' ||
    !Array.isArray(j.capabilities)
  ) {
    throw new JoinError('refused');
  }
  return {
    session_token: j.session_token,
    callsign: j.callsign,
    guest_id: j.guest_id,
    application_id: j.application_id,
    application_name: typeof j.application_name === 'string' ? j.application_name : '',
    capabilities: j.capabilities.filter((c): c is string => typeof c === 'string'),
    lobby_title: typeof j.lobby_title === 'string' ? j.lobby_title : undefined,
    expires_at: typeof j.expires_at === 'string' ? j.expires_at : '',
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

export interface GuestSocket {
  lane: GuestLane;
  roster(): GuestMember[];
  onRoster(cb: () => void): () => void;
  /** The socket ended: closed by the relay (`403` for a topic that is not ours), expired, or a network loss. */
  onEnd(cb: (code: number) => void): () => void;
  close(): void;
}

export interface OpenOptions {
  url: string;
  sessionToken: string;
  lobbyToken: string;
  key: CryptoKey;
  /** Test seam. */
  makeSocket?: (url: string) => SocketLike;
}

/** `wss://host/gateway/guest` for the page's own origin. */
export function guestSocketUrl(loc: Pick<Location, 'protocol' | 'host'> = window.location): string {
  return `${loc.protocol === 'https:' ? 'wss:' : 'ws:'}//${loc.host}${GUEST_SOCKET_PATH}`;
}

const OPEN = 1;

export function openGuestSocket(o: OpenOptions): GuestSocket {
  const ws = (o.makeSocket ?? ((u: string) => new WebSocket(u) as unknown as SocketLike))(o.url);
  const topic = topicOf(o.lobbyToken);
  let roster: GuestMember[] = [];
  const messageListeners = new Set<(from: string, bytes: Uint8Array) => void>();
  const rosterListeners = new Set<() => void>();
  const endListeners = new Set<(code: number) => void>();
  let ended = false;
  let inChain: Promise<void> = Promise.resolve();
  let outChain: Promise<void> = Promise.resolve();

  const sendFrame = (f: GuestUp) => ws.send(JSON.stringify(f));
  const end = (code: number) => {
    if (ended) return;
    ended = true;
    for (const l of [...endListeners]) l(code);
  };

  ws.onopen = () => sendFrame({ op: 'hello', token: o.sessionToken, topic });
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
      case 'ready':
      case 'roster': {
        const list = f.op === 'ready' ? f.roster : f.members;
        if (!Array.isArray(list)) return;
        roster = list.filter(
          (m): m is GuestMember =>
            typeof m?.id === 'string' && typeof m?.nickname === 'string',
        );
        for (const l of [...rosterListeners]) l();
        return;
      }
      case 'data': {
        // Anything that is not this lobby's topic is not ours to hand to a game.
        if (f.topic !== topic || typeof f.from !== 'string' || typeof f.b !== 'string') return;
        const from = f.from;
        let sealed: Uint8Array;
        try {
          sealed = unb64(f.b);
        } catch {
          return;
        }
        const run = async () => {
          const plain = await openSealed(o.key, o.lobbyToken, sealed);
          if (!plain) return;
          for (const l of [...messageListeners]) l(from, plain);
        };
        inChain = inChain.then(run, run);
        return;
      }
      case 'drop':
        ws.close();
        end(typeof f.code === 'number' ? f.code : 1008);
        return;
    }
  };

  return {
    lane: {
      send(bytes) {
        const run = async () => {
          if (ended || ws.readyState !== OPEN) return false;
          const sealed = await seal(o.key, o.lobbyToken, bytes);
          if (ended || ws.readyState !== OPEN) return false;
          sendFrame({ op: 'data', topic, b: b64(sealed) });
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
    },
    roster: () => roster,
    onRoster(cb) {
      rosterListeners.add(cb);
      return () => rosterListeners.delete(cb);
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
