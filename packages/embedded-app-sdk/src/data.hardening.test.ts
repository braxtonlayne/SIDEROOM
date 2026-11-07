// SEC-AUD-015 (wave 53): a member cannot freeze the shared state with a huge version, cannot write another
// member's writer-scoped keys, and cannot make the host re-broadcast the document in a loop.
import { describe, expect, it } from 'vitest';
import {
  ActivityData,
  DataError,
  LoopbackHub,
  MAX_DOCUMENT_BYTES,
  MAX_LIVE_KEYS,
  SYNC_COOLDOWN_MS,
  type DataTransport,
} from './data';

const enc = new TextEncoder();
const raw = (o: unknown) => enc.encode(JSON.stringify(o));

function room(members: string[], host = members[0]!) {
  const hub = new LoopbackHub(host);
  const clock = { t: 1_000_000 };
  const wires = new Map<string, DataTransport>();
  const docs = Object.fromEntries(
    members.map((m) => {
      const d = new ActivityData({ now: () => clock.t });
      const t = hub.connect(m);
      wires.set(m, t);
      d.attach(t);
      return [m, d];
    }),
  ) as Record<string, ActivityData>;
  return { hub, docs, clock, wires };
}

/** Send raw bytes as `from` to everyone else, as a hostile client could. */
function inject(r: { wires: Map<string, DataTransport> }, from: string, env: unknown) {
  r.wires.get(from)!.send(raw(env));
}

describe('state versions from the wire', () => {
  it('a version of 1e308 is dropped, and the next honest write still lands everywhere', () => {
    const r = room(['host', 'a', 'mal']);
    const { docs } = r;
    inject(r, 'mal', { v: 1, t: 'state', ops: [{ k: 'x', d: 0, ver: 1e308, by: 'mal' }] });
    expect(docs.host!.get('x')).toBeUndefined();
    expect(docs.host!.version).toBe(0);
    docs.a!.set({ x: 1 });
    docs.a!.set({ x: 2 });
    expect(docs.host!.get('x')).toBe(2);
    expect(docs.mal!.get('x')).toBe(2);
  });

  it('rejects fractional, zero, negative and far-ahead versions but takes a near one', () => {
    const r = room(['host', 'mal']);
    const { docs } = r;
    for (const ver of [0.5, 0, -3, 1e15, 5000]) {
      inject(r, 'mal', { v: 1, t: 'state', ops: [{ k: `k${ver}`, d: 1, ver, by: 'mal' }] });
    }
    expect(docs.host!.snapshot()).toEqual({});
    inject(r, 'mal', { v: 1, t: 'state', ops: [{ k: 'ok', d: 1, ver: 3, by: 'mal' }] });
    expect(docs.host!.get('ok')).toBe(1);
  });

  it('a snapshot version above its own floor is dropped', () => {
    const r = room(['host', 'a']);
    const { docs } = r;
    docs.host!.set({ real: 1 });
    // Only the host may send a snapshot; the host is the hub's host id.
    inject(r, 'host', {
      v: 1,
      t: 'snap',
      floor: 1,
      ops: [{ k: 'evil', d: 1, ver: 1e12, by: 'host' }],
    });
    expect(docs.a!.get('evil')).toBeUndefined();
  });
});

describe('writer-scoped keys', () => {
  it('a member cannot write or overwrite another member key; the owner and the host can delete', () => {
    const r = room(['host', 'v', 'mal']);
    const { docs } = r;
    for (const d of Object.values(docs)) d.scopeKeys('op:');
    docs.v!.set({ 'op:v:1': { stroke: 1 } });
    expect(docs.host!.get('op:v:1')).toEqual({ stroke: 1 });

    // Forged over the wire: overwrite with a higher version, and a fresh key under the victim's name.
    inject(r, 'mal', {
      v: 1,
      t: 'state',
      ops: [
        { k: 'op:v:1', d: { forged: true }, ver: 2, by: 'mal' },
        { k: 'op:v:2', d: { forged: true }, ver: 2, by: 'mal' },
        { k: 'op:v:1', ver: 3, by: 'mal' },
      ],
    });
    expect(docs.host!.get('op:v:1')).toEqual({ stroke: 1 });
    expect(docs.v!.get('op:v:2')).toBeUndefined();

    // The local API refuses it too.
    expect(() => docs.mal!.set({ 'op:v:9': 1 })).toThrow(DataError);
    expect(() => docs.mal!.delete('op:v:1')).toThrow(DataError);

    // The host compacts by deleting an author's keys; the author can delete their own.
    docs.host!.delete('op:v:1');
    expect(docs.v!.get('op:v:1')).toBeUndefined();
    docs.mal!.set({ 'op:mal:1': 1 });
    docs.mal!.delete('op:mal:1');
    expect(docs.host!.get('op:mal:1')).toBeUndefined();
  });

  it('keys outside the scoped prefix are unaffected', () => {
    const { docs } = room(['host', 'a']);
    for (const d of Object.values(docs)) d.scopeKeys('op:');
    docs.a!.set({ 'board:title': 'x', 'op:a:1': 1 });
    expect(docs.host!.get('board:title')).toBe('x');
  });
});

describe('sync flood', () => {
  it('the host answers one member once per cooldown', () => {
    const r = room(['host', 'a']);
    const { hub, docs, clock } = r;
    docs.host!.set({ k: 1 });
    const count = () =>
      hub.relayed.filter(
        (r) => r.from === 'host' && new TextDecoder().decode(r.bytes).includes('"snap"'),
      ).length;
    // The join itself already cost one answer; let the cooldown pass, then flood.
    clock.t += SYNC_COOLDOWN_MS + 1;
    const before = count();
    for (let i = 0; i < 20; i++) inject(r, 'a', { v: 1, t: 'sync' });
    expect(count() - before).toBe(1);
    clock.t += SYNC_COOLDOWN_MS + 1;
    inject(r, 'a', { v: 1, t: 'sync' });
    expect(count() - before).toBe(2);
  });
});

// SEC-AUD-035 (wave 54): the shared document has a key and size budget, so one member cannot grow every
// participant's copy (and every late join) without bound.
describe('the document budget', () => {
  it('refuses new keys past MAX_LIVE_KEYS, locally and from the wire, but existing keys still change', () => {
    const r = room(['host', 'a', 'mal']);
    const { docs } = r;
    // Fill to the cap in envelopes small enough for the data channel.
    let n = 0;
    while (n < MAX_LIVE_KEYS) {
      const patch: Record<string, number> = {};
      for (let i = 0; i < 200 && n < MAX_LIVE_KEYS; i++, n++) patch[`f${n}`] = 0;
      docs.host!.set(patch);
    }
    expect(Object.keys(docs.a!.snapshot()).length).toBe(MAX_LIVE_KEYS);
    expect(() => docs.a!.set({ one: 1 })).toThrow(DataError);
    const ver = docs.host!.version + 1;
    inject(r, 'mal', { v: 1, t: 'state', ops: [{ k: 'extra', d: 1, ver, by: 'mal' }] });
    expect(docs.host!.get('extra')).toBeUndefined();
    expect(docs.a!.get('extra')).toBeUndefined();
    // Changing and deleting what is there still works, and a deletion makes room.
    docs.a!.set({ f0: 5 });
    expect(docs.host!.get('f0')).toBe(5);
    docs.a!.delete('f1');
    docs.a!.set({ fresh: 1 });
    expect(docs.host!.get('fresh')).toBe(1);
  });

  it('refuses growth past MAX_DOCUMENT_BYTES', () => {
    const r = room(['host', 'a']);
    const { docs } = r;
    const chunk = 'x'.repeat(15_000);
    const per = JSON.stringify(chunk).length;
    const fit = Math.floor(MAX_DOCUMENT_BYTES / per);
    for (let i = 0; i < fit; i++) docs.host!.set({ [`c${i}`]: chunk });
    expect(() => docs.a!.set({ last: chunk })).toThrow(DataError);
    expect(docs.host!.get('last')).toBeUndefined();
    // Shrinking a value is always allowed.
    docs.a!.set({ c0: 'small' });
    expect(docs.host!.get('c0')).toBe('small');
  });
});
