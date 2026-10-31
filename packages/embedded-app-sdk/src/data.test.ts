import { describe, expect, it } from 'vitest';
import { DataError, LoopbackHub, MAX_DATA_BYTES } from './data';
import { SideRoomSDK, type SdkEnv } from './sdk';

const env: SdkEnv = {
  parent: null,
  hostOrigin: 'https://chat.example.org',
  hostWindow: {},
  addEventListener: () => undefined,
  removeEventListener: () => undefined,
};

function room(members: string[], host = members[0] ?? 'h') {
  const hub = new LoopbackHub(host);
  const sdks = Object.fromEntries(
    members.map((m) => [m, new SideRoomSDK('7', { env, transport: hub.connect(m) })]),
  ) as Record<string, SideRoomSDK>;
  return { hub, sdks };
}
const bytes = (...n: number[]) => Uint8Array.from(n);

describe('sdk.state with two participants', () => {
  it('shares a key both ways and fires state events', () => {
    const { sdks } = room(['host', 'guest']);
    const host = sdks.host!;
    const guest = sdks.guest!;
    const seen: string[] = [];
    guest.on('state', (e) => e.changes.forEach((c) => seen.push(`${c.key}=${String(c.value)}`)));
    host.state.set({ board: 'a' });
    expect(guest.state.get('board')).toBe('a');
    guest.state.set({ move: 1 });
    expect(host.state.get('move')).toBe(1);
    expect(seen).toEqual(['board=a', 'move=1']);
    expect(host.state.version).toBe(guest.state.version);
  });

  it('resolves concurrent writes last-writer-wins, the same on both sides', () => {
    const { hub, sdks } = room(['host', 'guest']);
    hub.manual = true;
    sdks.host!.state.set({ k: 'from-host' });
    sdks.guest!.state.set({ k: 'from-guest' });
    hub.flush();
    // equal versions: the larger author id wins ("host" > "guest")
    expect(sdks.host!.state.get('k')).toBe('from-host');
    expect(sdks.guest!.state.get('k')).toBe('from-host');
    hub.manual = false;
    // a later write has a higher version and wins whoever makes it
    sdks.guest!.state.set({ k: 'later' });
    expect(sdks.host!.state.get('k')).toBe('later');
  });

  it('deletes with a tombstone that a stale set cannot resurrect', () => {
    const { hub, sdks } = room(['host', 'guest']);
    sdks.host!.state.set({ k: 1 });
    hub.manual = true;
    sdks.guest!.state.set({ k: 'stale-ish' }); // version 2
    sdks.host!.state.delete('k'); // version 2 too, host id wins the tie
    hub.flush();
    expect(sdks.host!.state.get('k')).toBeUndefined();
    expect(sdks.guest!.state.get('k')).toBeUndefined();
  });

  it('keeps host:* keys for the host only', () => {
    const { sdks } = room(['host', 'guest', 'other']);
    expect(() => sdks.guest!.state.set({ 'host:turn': 'guest' })).toThrow(DataError);
    sdks.host!.state.set({ 'host:turn': 'guest' });
    expect(sdks.other!.state.get('host:turn')).toBe('guest');
  });

  it('drops a forged host:* write and a forged author on the receiving side', () => {
    const { hub, sdks } = room(['host', 'guest']);
    const evil = hub.connect('evil');
    const enc = (o: unknown) => new TextEncoder().encode(JSON.stringify(o));
    evil.send(enc({ v: 1, t: 'state', ops: [{ k: 'host:turn', d: 'evil', ver: 99, by: 'host' }] }));
    evil.send(enc({ v: 1, t: 'state', ops: [{ k: 'host:turn', d: 'evil', ver: 99, by: 'evil' }] }));
    evil.send(enc({ v: 1, t: 'snap', to: 'guest', ops: [{ k: 'x', d: 1, ver: 1, by: 'evil' }] }));
    expect(sdks.guest!.state.get('host:turn')).toBeUndefined();
    expect(sdks.guest!.state.get('x')).toBeUndefined();
    expect(sdks.host!.state.version).toBe(0);
  });

  it('brings a late joiner up to date from the host', () => {
    const { hub, sdks } = room(['host', 'guest']);
    sdks.host!.state.set({ a: 1, 'host:round': 3 });
    sdks.guest!.state.set({ b: 2 });
    sdks.host!.state.delete('a');
    const late = new SideRoomSDK('7', { env, transport: hub.connect('late') });
    expect(late.state.all()).toEqual({ 'host:round': 3, b: 2 });
    expect(late.state.version).toBe(sdks.host!.state.version);
    // the late joiner then takes part normally
    late.state.set({ c: 3 });
    expect(sdks.host!.state.get('c')).toBe(3);
  });

  it('refuses an oversized change and leaves the document alone', () => {
    const { sdks } = room(['host', 'guest']);
    expect(() => sdks.host!.state.set({ big: 'x'.repeat(MAX_DATA_BYTES) })).toThrow(/limit/);
    expect(sdks.host!.state.get('big')).toBeUndefined();
  });
});

describe('sdk.input', () => {
  it('sends guest input to the host only', () => {
    const { sdks } = room(['host', 'g1', 'g2']);
    const got: string[] = [];
    sdks.host!.on('input', (e) => got.push(`${e.from}:${Array.from(e.data).join(',')}:${e.reply}`));
    sdks.g2!.on('input', () => got.push('g2 saw it'));
    sdks.g1!.input.send(bytes(1, 2, 3));
    expect(got).toEqual(['g1:1,2,3:false']);
  });

  it('delivers a host reply to the addressed member and nobody else', () => {
    const { hub, sdks } = room(['host', 'g1', 'g2']);
    const before = hub.relayed.length;
    const g1: number[][] = [];
    const g2: number[][] = [];
    sdks.g1!.on('input', (e) => g1.push([...e.data]));
    sdks.g2!.on('input', (e) => g2.push([...e.data]));
    sdks.host!.input.reply('g1', bytes(9, 8));
    expect(g1).toEqual([[9, 8]]);
    expect(g2).toEqual([]);
    // the relay still carried the bytes to everyone (addressing is inside the payload)
    expect(hub.relayed.length - before).toBe(1);
  });

  it('ignores a reply that did not come from the host, and lets only the host reply', () => {
    const { hub, sdks } = room(['host', 'g1', 'g2']);
    const got: number[][] = [];
    sdks.g1!.on('input', (e) => got.push([...e.data]));
    expect(() => sdks.g2!.input.reply('g1', bytes(1))).toThrow(DataError);
    const evil = hub.connect('evil');
    evil.send(new TextEncoder().encode(JSON.stringify({ v: 1, t: 'reply', to: 'g1', b: 'AQ==' })));
    expect(got).toEqual([]);
  });

  it('a host sending to itself gets the event locally', () => {
    const { sdks } = room(['host', 'g1']);
    const got: boolean[] = [];
    sdks.host!.on('input', (e) => got.push(e.reply));
    sdks.host!.input.send(bytes(1));
    expect(got).toEqual([false]);
  });
});

describe('without a transport', () => {
  it('fails with NO_TRANSPORT', () => {
    const sdk = new SideRoomSDK('7', { env });
    expect(() => sdk.state.set({ a: 1 })).toThrow(/No data channel/);
  });
});
