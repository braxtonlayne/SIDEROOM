import { describe, expect, it } from 'vitest';
import { ActivityData, LoopbackHub, MAX_DATA_BYTES, TOMBSTONE_TTL_MS } from './data';
import { SideRoomSDK, type SdkEnv } from './sdk';

const env: SdkEnv = {
  parent: null,
  hostOrigin: 'https://chat.example.org',
  hostWindow: {},
  addEventListener: () => undefined,
  removeEventListener: () => undefined,
};

describe('late join of a big document', () => {
  it('sends the snapshot in chunks that each fit the data channel', () => {
    const hub = new LoopbackHub('host');
    const host = new SideRoomSDK('7', { env, transport: hub.connect('host') });
    // 60 keys of ~3 KB: about 180 KB, far beyond one 16 KiB envelope.
    for (let i = 0; i < 60; i++) host.state.set({ [`stroke:${i}`]: 'x'.repeat(3000) + i });
    const before = hub.relayed.length;
    const late = new SideRoomSDK('7', { env, transport: hub.connect('late') });
    const snaps = hub.relayed.slice(before).filter((r) => r.from === 'host');
    expect(snaps.length).toBeGreaterThan(8);
    for (const s of snaps) expect(s.bytes.length).toBeLessThanOrEqual(MAX_DATA_BYTES);
    expect(Object.keys(late.state.all()).length).toBe(60);
    expect(late.state.get('stroke:59')).toBe('x'.repeat(3000) + '59');
    expect(late.state.version).toBe(host.state.version);
  });
});

describe('tombstones', () => {
  it('expire, are not sent to a joiner, and the version never goes backwards', () => {
    let t = 1_000;
    const hub = new LoopbackHub('host');
    const hostData = new ActivityData({ now: () => t });
    hostData.attach(hub.connect('host'));
    hostData.set({ a: 1, b: 2 });
    hostData.delete('a'); // version 2 tombstone
    expect(hostData.version).toBe(2);
    t += TOMBSTONE_TTL_MS + 1;
    hostData.compact();
    expect(hostData.snapshot()).toEqual({ b: 2 });
    expect(hostData.version).toBe(2); // floor keeps it
    hostData.set({ c: 3 });
    expect(hostData.version).toBe(3);

    // A joiner gets the live keys plus the version floor, so its next write outranks old tombstones.
    hostData.delete('b'); // tombstone at version 4, still fresh
    const joiner = new SideRoomSDK('7', { env, transport: hub.connect('joiner') });
    expect(joiner.state.all()).toEqual({ c: 3 });
    expect(joiner.state.version).toBe(4);
    joiner.state.set({ b: 'again' });
    expect(hostData.get('b')).toBe('again');
  });
});

describe('identity', () => {
  it('exposes self, hostId and the data layer', () => {
    const hub = new LoopbackHub('host');
    const g = new SideRoomSDK('7', { env, transport: hub.connect('guest') });
    expect(g.self).toBe('guest');
    expect(g.hostId).toBe('host');
    expect(g.isHost).toBe(false);
    expect(g.data.self).toBe('guest');
    expect(new SideRoomSDK('7', { env }).self).toBeNull();
  });
});
