// ARC-GST-002: the member bridge between the room's data channel and a guest link's lobby.
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  decodeLobby,
  encodeDirect,
  encodeRelayed,
  unwrapFromRoom,
  wrapForRoom,
} from '../../public/guest-play/envelope';
import type { GuestPeer } from '../../public/guest-play/contract';
import type { Lobby, LobbyMessage } from '../../public/guest-play/session';
import { generateGameKey } from '../../public/guest-play/link';
import type { ActivityDataBridge } from '../host';
import { bridgeFor, guestAwareData, startBridge, stopBridge } from '../guestBridge';

const TOKEN = 'ab12'.repeat(16);
const INST = '9';

function fakeRoom() {
  const listeners = new Set<(i: string, s: string, b: Uint8Array) => void>();
  const sent: [string, number[]][] = [];
  const room: ActivityDataBridge = {
    join: vi.fn(),
    leave: vi.fn(),
    send: vi.fn(async (i, b) => {
      sent.push([i, Array.from(b)]);
      return true;
    }),
    onMessage: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
  return {
    room,
    sent,
    deliver: (i: string, s: string, b: Uint8Array) => listeners.forEach((l) => l(i, s, b)),
  };
}

function fakeLobby() {
  const msgs = new Set<(m: LobbyMessage) => void>();
  const peerCbs = new Set<() => void>();
  let peers: GuestPeer[] = [{ seat: 1, role: 'member', callsign: 'Me' }];
  const published: Uint8Array[] = [];
  const lobby: Lobby = {
    seated: Promise.resolve({ seat: 1, role: 'member', callsign: 'Me', peers }),
    peers: () => peers,
    onPeers: (cb) => {
      peerCbs.add(cb);
      return () => peerCbs.delete(cb);
    },
    publish: async (b) => {
      published.push(b);
      return true;
    },
    onMessage: (cb) => {
      msgs.add(cb);
      return () => msgs.delete(cb);
    },
    onEnd: () => () => undefined,
    close: vi.fn(),
  };
  return {
    lobby,
    published,
    say: (m: LobbyMessage) => msgs.forEach((l) => l(m)),
    join: (p: GuestPeer) => {
      peers = [...peers, p];
      peerCbs.forEach((l) => l());
    },
  };
}

const settle = () => new Promise((r) => setTimeout(r, 10));

function bridge() {
  const room = fakeRoom();
  const l = fakeLobby();
  const open = vi.fn(() => l.lobby);
  const b = startBridge({
    instanceId: INST,
    lobbyToken: TOKEN,
    gameKey: generateGameKey(),
    hostId: '555',
    selfId: '777',
    room: room.room,
    open: open as never,
  });
  return { room, l, open, b };
}

afterEach(() => stopBridge(INST));

describe('the guest bridge of the link’s creator', () => {
  it('seats the member by the lobby token (cookie session), not a guest token, and names the host', async () => {
    const { l, open } = bridge();
    await settle();
    expect((open.mock.calls[0] as unknown as [{ hello: unknown }])[0].hello).toEqual({
      lobby_token: TOKEN,
    });
    const first = decodeLobby(1, 'member', l.published[0]!);
    expect(first).toEqual({ kind: 'sys', host: '555' });
  });

  it('tells a guest who sits down later who the host is', async () => {
    const { l } = bridge();
    await settle();
    l.join({ seat: 2, role: 'guest', callsign: 'Amber Otter 42' });
    await settle();
    expect(l.published.map((p) => decodeLobby(1, 'member', p)?.kind)).toEqual(['sys', 'sys']);
  });

  it('relays a room member’s message to the lobby under their user id, and only for its own instance', async () => {
    const { room, l } = bridge();
    await settle();
    l.published.length = 0;
    room.deliver(INST, '4242', new Uint8Array([1, 2]));
    room.deliver('other-instance', '4242', new Uint8Array([3]));
    expect(l.published).toHaveLength(1);
    expect(decodeLobby(1, 'member', l.published[0]!)).toMatchObject({ kind: 'data', from: '4242' });
  });

  it('does not send a guest’s relayed message back into the lobby', async () => {
    const { room, l } = bridge();
    await settle();
    l.published.length = 0;
    room.deliver(INST, '777', wrapForRoom('g3', new Uint8Array([9])));
    expect(l.published).toHaveLength(0);
  });

  it('puts a guest’s message into the room for the other members and into this member’s own frame', async () => {
    const { room, l } = bridge();
    await settle();
    const seen: [string, string, number[]][] = [];
    const data = guestAwareData(room.room);
    data.onMessage((i, from, b) => seen.push([i, from, Array.from(b)]));
    l.say({ seat: 3, role: 'guest', bytes: encodeDirect(new Uint8Array([5, 6])) });
    expect(seen).toEqual([[INST, 'g3', [5, 6]]]);
    expect(room.sent).toHaveLength(1);
    const back = unwrapFromRoom(new Uint8Array(room.sent[0]![1]));
    expect(back?.from).toBe('g3');
    expect(Array.from(back!.bytes)).toEqual([5, 6]);
  });

  it('never re-broadcasts what a member seat published (a guest cannot make a member say things)', async () => {
    const { room, l } = bridge();
    await settle();
    l.say({ seat: 2, role: 'member', bytes: encodeRelayed('1', new Uint8Array([1])) });
    expect(room.sent).toHaveLength(0);
  });
});

describe('guestAwareData', () => {
  it('sends to the room and to the guests, with the member’s own id', async () => {
    const { room, l } = bridge();
    await settle();
    l.published.length = 0;
    const data = guestAwareData(room.room);
    expect(await data.send(INST, new Uint8Array([8]))).toBe(true);
    expect(room.sent).toEqual([[INST, [8]]]);
    expect(decodeLobby(1, 'member', l.published[0]!)).toMatchObject({ kind: 'data', from: '777' });
  });

  it('shows another member’s client a guest under its guest id, and everything else unchanged', () => {
    const room = fakeRoom();
    const data = guestAwareData(room.room);
    const seen: [string, number[]][] = [];
    data.onMessage((_i, from, b) => seen.push([from, Array.from(b)]));
    room.deliver(INST, '4242', new Uint8Array([1]));
    room.deliver(INST, '4242', wrapForRoom('g3', new Uint8Array([2])));
    expect(seen).toEqual([
      ['4242', [1]],
      ['g3', [2]],
    ]);
  });

  it('is a plain pass-through when no link was made, and stops listening when unsubscribed', async () => {
    const room = fakeRoom();
    const data = guestAwareData(room.room);
    expect(bridgeFor(INST)).toBeUndefined();
    expect(await data.send(INST, new Uint8Array([1]))).toBe(true);
    const seen: string[] = [];
    const off = data.onMessage((_i, from) => seen.push(from));
    off();
    room.deliver(INST, '1', new Uint8Array([1]));
    expect(seen).toEqual([]);
  });

  it('stopBridge closes the lobby seat', async () => {
    const { l } = bridge();
    await settle();
    stopBridge(INST);
    expect(l.lobby.close).toHaveBeenCalled();
    expect(bridgeFor(INST)).toBeUndefined();
  });
});

describe('host handoff through the guest bridge (D-388)', () => {
  it('tells the lobby the new host once, and ignores a repeat', async () => {
    const { l, b } = bridge();
    await settle();
    l.published.length = 0;
    b.setHost('556');
    b.setHost('556');
    b.setHost('');
    expect(l.published.map((p) => decodeLobby(1, 'member', p))).toEqual([
      { kind: 'sys', host: '556' },
    ]);
    // A guest who sits down later is told the new host.
    l.published.length = 0;
    l.join({ seat: 2, role: 'guest', callsign: 'Amber Otter 42' });
    await settle();
    expect(decodeLobby(1, 'member', l.published[0]!)).toEqual({ kind: 'sys', host: '556' });
  });
});
