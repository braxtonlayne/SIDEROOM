// ARC-GST-002: the guest's view of the lobby, shaped for the bridge host: a data lane that carries the envelope
// (`envelope.ts`), a roster of lobby callsigns (nothing else about anyone, D-397) and the id of the game's host.
import { decodeLobby, encodeDirect } from './envelope';
import type { GuestMember, GuestPeer } from './contract';
import type { GuestLane } from './guestHost';
import type { Lobby } from './session';

export interface GuestLaneKit {
  lane: GuestLane;
  roster(): GuestMember[];
  /** The id a message from the game's host carries: told by the member bridge, else the first member seat. */
  hostId(): string | null;
  /** D-388: the member bridge named a different host (the old one left a `host_handoff` game). */
  onHost(cb: (host: string, previous: string | null) => void): () => void;
  /** Resolves (true) once a member bridge named the host, or (false) after `ms`. */
  hostKnown(ms: number): Promise<boolean>;
}

export const guestId = (seat: number) => `g${seat}`;
export const memberSeatId = (seat: number) => `m${seat}`;

export const memberOf = (p: GuestPeer): GuestMember =>
  p.role === 'member'
    ? { id: memberSeatId(p.seat), nickname: p.callsign, host: true }
    : { id: guestId(p.seat), nickname: p.callsign };

export function guestLane(lobby: Lobby): GuestLaneKit {
  let host: string | null = null;
  const waiters = new Set<() => void>();
  const hostListeners = new Set<(host: string, previous: string | null) => void>();
  const listeners = new Set<(from: string, bytes: Uint8Array) => void>();
  lobby.onMessage((m) => {
    const d = decodeLobby(m.seat, m.role, m.bytes);
    if (!d) return;
    if (d.kind === 'sys') {
      const previous = host;
      host = d.host;
      if (previous !== null && previous !== host)
        for (const l of [...hostListeners]) l(host, previous);
      for (const w of [...waiters]) w();
      return;
    }
    for (const l of [...listeners]) l(d.from, d.bytes);
  });
  return {
    lane: {
      send: (bytes) => lobby.publish(encodeDirect(bytes)),
      onMessage(cb) {
        listeners.add(cb);
        return () => listeners.delete(cb);
      },
    },
    roster: () => lobby.peers().map(memberOf),
    hostId: () => {
      const m = lobby.peers().find((p) => p.role === 'member');
      return host ?? (m ? memberSeatId(m.seat) : null);
    },
    onHost(cb) {
      hostListeners.add(cb);
      return () => hostListeners.delete(cb);
    },
    hostKnown(ms) {
      if (host) return Promise.resolve(true);
      return new Promise((resolve) => {
        const done = (v: boolean) => {
          waiters.delete(ok);
          clearTimeout(timer);
          resolve(v);
        };
        const ok = () => done(true);
        const timer = setTimeout(() => done(false), ms);
        waiters.add(ok);
      });
    },
  };
}
