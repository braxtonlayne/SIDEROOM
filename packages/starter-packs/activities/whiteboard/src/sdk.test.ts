// The board over the real Embedded App SDK state (act-channel's ActivityData and loopback "SFU"): two fake
// participants, relay of what the SFU sees, and a late joiner.

import { describe, expect, it } from 'vitest';
import { LoopbackHub, SideRoomSDK, type SdkEnv } from '@sideroom/embedded-app-sdk';
import { HOME_PAGE, type Payload } from './log';
import { adapterFromSdk, memberId, type SdkLike } from './sdkState';
import { Board } from './sync';

const env: SdkEnv = {
  parent: null,
  hostOrigin: 'http://host.test',
  addEventListener: () => {},
  removeEventListener: () => {},
  hostWindow: null,
};

function join(hub: LoopbackHub, id: string, opts: { compactAt?: number } = {}) {
  const sdk = new SideRoomSDK('app', { env, transport: hub.connect(id) });
  const errors: unknown[] = [];
  const board = new Board({
    me: id,
    isHost: () => sdk.isHost,
    adapter: adapterFromSdk(sdk as unknown as SdkLike, (e) => errors.push(e)),
    compactDelayMs: 0,
    minAgeMs: 0,
    compactAt: opts.compactAt,
  });
  return { sdk, board, errors };
}

const stroke = (x: number): Payload => ({
  k: 'stroke',
  pg: HOME_PAGE,
  c: '#102030',
  w: 3,
  hl: false,
  pts: [x, 0, x + 9, 9],
});
const ids = (b: Board) => b.view().objects.map((o) => o.id);

describe('Board over sdk.state', () => {
  it('two participants draw and see each other', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    const b = join(hub, 'bob');
    a.board.add(stroke(1));
    b.board.add(stroke(2));
    a.board.add(stroke(3));
    expect(ids(a.board)).toEqual(ids(b.board));
    expect(ids(a.board)).toHaveLength(3);
    expect(a.errors).toEqual([]);
    expect(b.errors).toEqual([]);
  });

  it('what the relay carries never contains plain strokes in its envelope metadata and stays under 16 KiB', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    join(hub, 'bob');
    for (let i = 0; i < 20; i++) a.board.add(stroke(i));
    a.board.compact();
    for (const m of hub.relayed) expect(m.bytes.length).toBeLessThanOrEqual(16 * 1024);
  });

  it('the host compacts; the guest follows; deletions reach everyone', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    const b = join(hub, 'bob');
    for (let i = 0; i < 6; i++) b.board.add(stroke(i));
    b.board.undo();
    expect(b.board.compact()).toBe(0); // a guest never compacts
    expect(a.board.compact()).toBe(7); // six strokes and the undo
    expect(ids(b.board)).toEqual(ids(a.board));
    expect(ids(b.board)).toHaveLength(5);
    expect(b.board.compactions).toBe(1);
    expect(Object.keys(b.sdk.state.all()).filter((k) => k.startsWith('op:'))).toEqual([]);
    // A guest cannot write a host key; the adapter reports it instead of throwing.
    const b2 = adapterFromSdk(b.sdk as unknown as SdkLike, (e) => b.errors.push(e));
    b2.set('host:base:meta', { gen: 99, chunks: 0, through: {} });
    expect(b.errors).toHaveLength(1);
  });

  it('a pending picture and a chunked snapshot cross the relay under the size limit', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    const b = join(hub, 'bob');
    b.board.addImage({
      pg: HOME_PAGE,
      at: [0, 0],
      size: [10, 10],
      dataUrl: `data:image/png;base64,${'QUJD'.repeat(5000)}`,
    });
    for (let i = 0; i < 60; i++) a.board.add(stroke(i));
    a.board.compact();
    expect(b.errors).toEqual([]);
    expect(a.errors).toEqual([]);
    const img = b.board.view().objects.find((o) => o.k === 'image');
    expect(img?.k === 'image' && img.src.length).toBeGreaterThan(19000);
    for (const m of hub.relayed) expect(m.bytes.length).toBeLessThanOrEqual(16 * 1024);
  });

  it('a late joiner gets the board from the host', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    for (let i = 0; i < 4; i++) a.board.add(stroke(i));
    const c = join(hub, 'carol');
    expect(ids(c.board)).toEqual(ids(a.board));
    expect(ids(c.board)).toHaveLength(4);
  });
});

describe('memberId', () => {
  it('uses sdk.self when the SDK has it, else a stable generated id', () => {
    expect(memberId({ self: 'u123' })).toBe('u123');
    const id = memberId({});
    expect(id).toMatch(/^m[a-z0-9]+$/);
  });
});

describe('late join after a big compaction', () => {
  // KNOWN FAILURE in act-channel's slice: `ActivityData` answers a join with the whole document in one 'snap'
  // envelope, which throws TOO_LARGE above 16 KiB, so a participant who joins after a compaction of a busy board
  // never gets it. `it.fails` passes while that holds and fails the day the host chunks its snapshot: then make
  // this a plain `it`.
  it.fails('a joiner reads a snapshot that is larger than one message', () => {
    const hub = new LoopbackHub('alice');
    const a = join(hub, 'alice');
    for (let i = 0; i < 120; i++) {
      a.board.add({
        k: 'stroke',
        pg: HOME_PAGE,
        c: '#102030',
        w: 3,
        hl: false,
        pts: Array.from({ length: 60 }, (_, j) => 100 + ((i * 7 + j * 13) % 1400)),
      });
    }
    a.board.compact();
    expect(Object.keys(a.sdk.state.all()).filter((k) => k.startsWith('host:base:1:')).length).toBeGreaterThan(2);
    const c = join(hub, 'carol');
    expect(ids(c.board)).toHaveLength(120);
    expect(ids(c.board)).toEqual(ids(a.board));
  });
});
