// ARC-GST-002 (wave 43, D-397): the member side of a guest link. The member who made the link seats themselves in the
// lobby (the guest socket accepts a member of the Activity by session cookie) and bridges two worlds that must never
// meet any other way: the room's end-to-end encrypted data channel (members) and the lobby's game-key sealed lane
// (guests). Only the game's own data crosses; nothing of the call, the channel or the server does (the relay is
// unkeyed and the guest socket cannot name another topic).
//
//   room -> lobby   a member's game message, wrapped with their user id (`encodeRelayed`)
//   lobby -> room   a guest's game message, wrapped with its guest id (`wrapForRoom`) so every member's client can
//                   tell it from a member's, and handed to this member's own frame directly
//   control         the game's host id, so guests can tell which messages are the host's (`encodeSys`)
//
// One bridge per instance per browser tab. Its key lives in this tab's sessionStorage only.
import type { ActivityDataBridge } from './host';
import {
  decodeLobby,
  encodeRelayed,
  encodeSys,
  unwrapFromRoom,
  wrapForRoom,
} from '../public/guest-play/envelope';
import { guestSocketUrl, openLobby, type Lobby } from '../public/guest-play/session';
import { importGameKey } from '../public/guest-play/seal';

export interface BridgeOptions {
  instanceId: string;
  lobbyToken: string;
  gameKey: string;
  /** The user id the game knows as its host (`instance.started_by`). */
  hostId: string;
  /** This member's own user id (their frame's messages go to the lobby under it). */
  selfId: string;
  room: ActivityDataBridge;
  /** Test seam. */
  open?: typeof openLobby;
}

type GuestListener = (instance: string, from: string, bytes: Uint8Array) => void;

/** Frames listening for guest messages, whenever a bridge starts (a frame subscribes before the link exists). */
const guestListeners = new Set<GuestListener>();

export class GuestBridge {
  private lobby: Lobby | null = null;
  private offRoom: (() => void) | null = null;
  private stopped = false;
  readonly instanceId: string;
  readonly lobbyToken: string;

  constructor(private readonly o: BridgeOptions) {
    this.instanceId = o.instanceId;
    this.lobbyToken = o.lobbyToken;
  }

  /** Seat in the lobby and start relaying. Resolves once the relay seated us. */
  async start(): Promise<void> {
    const key = await importGameKey(this.o.gameKey);
    if (this.stopped) return;
    const lobby = (this.o.open ?? openLobby)({
      url: guestSocketUrl(),
      hello: { lobby_token: this.o.lobbyToken },
      lobbyToken: this.o.lobbyToken,
      key,
    });
    this.lobby = lobby;
    await lobby.seated;
    const announce = () => void lobby.publish(encodeSys({ host: this.o.hostId }));
    announce();
    // A guest who sits down later learns the host too.
    let known = new Set(lobby.peers().map((p) => p.seat));
    lobby.onPeers(() => {
      const now = lobby.peers();
      if (now.some((p) => !known.has(p.seat) && p.role === 'guest')) announce();
      known = new Set(now.map((p) => p.seat));
    });
    // room -> lobby. A message that is itself a relayed guest message is not sent back to the lobby.
    this.offRoom = this.o.room.onMessage((inst, sender, bytes) => {
      if (inst !== this.o.instanceId || unwrapFromRoom(bytes)) return;
      void lobby.publish(encodeRelayed(sender, bytes));
    });
    // lobby -> room. Only guests' own data; a member seat's payload is never re-broadcast.
    lobby.onMessage((m) => {
      if (m.role !== 'guest') return;
      const d = decodeLobby(m.seat, m.role, m.bytes);
      if (!d || d.kind !== 'data') return;
      for (const l of [...guestListeners]) l(this.o.instanceId, d.from, d.bytes);
      void this.o.room.send(this.o.instanceId, wrapForRoom(d.from, d.bytes));
    });
  }

  /** This member's own frame sent `bytes`: guests get them too. */
  fromLocal(bytes: Uint8Array): void {
    void this.lobby?.publish(encodeRelayed(this.o.selfId, bytes));
  }

  /** The relay ended the lobby (revoked, expired, ended). */
  onEnd(cb: (code: number) => void): () => void {
    return this.lobby ? this.lobby.onEnd(cb) : () => undefined;
  }

  stop(): void {
    this.stopped = true;
    this.offRoom?.();
    this.offRoom = null;
    this.lobby?.close();
    this.lobby = null;
  }
}

// ---- the bridges of this tab ------------------------------------------------------------------------------------

const bridges = new Map<string, GuestBridge>();

export const bridgeFor = (instanceId: string): GuestBridge | undefined => bridges.get(instanceId);

export function startBridge(o: BridgeOptions): GuestBridge {
  bridges.get(o.instanceId)?.stop();
  const b = new GuestBridge(o);
  bridges.set(o.instanceId, b);
  void b.start().catch(() => {
    // The relay refused the seat (the link was revoked, the instance ended): nothing to bridge.
    if (bridges.get(o.instanceId) === b) bridges.delete(o.instanceId);
    b.stop();
  });
  return b;
}

export function stopBridge(instanceId: string): void {
  bridges.get(instanceId)?.stop();
  bridges.delete(instanceId);
}

/**
 * The data bridge an Activity frame gets: the room's channel, plus, when this member bridges a guest link, the guest
 * messages (already tagged `g<seat>`) and their own sends going to the guests. Messages from other members' bridges
 * (a guest's, wrapped for the room) are unwrapped here so every member's game sees the guest under its id.
 */
export function guestAwareData(room: ActivityDataBridge): ActivityDataBridge {
  return {
    join: (i) => room.join(i),
    leave: (i) => room.leave(i),
    async send(instance, bytes) {
      const sent = await room.send(instance, bytes);
      bridgeFor(instance)?.fromLocal(bytes);
      return sent;
    },
    onMessage(cb) {
      const offRoom = room.onMessage((inst, sender, bytes) => {
        const wrapped = unwrapFromRoom(bytes);
        if (wrapped) cb(inst, wrapped.from, wrapped.bytes);
        else cb(inst, sender, bytes);
      });
      // This member's own bridge hands their frame the guests' messages directly (the room never echoes to the sender).
      guestListeners.add(cb);
      return () => {
        offRoom();
        guestListeners.delete(cb);
      };
    },
  };
}
