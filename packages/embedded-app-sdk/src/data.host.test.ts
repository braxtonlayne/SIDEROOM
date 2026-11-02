// D-388 (wave 46, ACT-LND-002): the host can change after attach. Only the current host writes `host:*` keys, gets
// inputs and answers `sync`; the successor takes over the host keys it already holds; a peer that learns of the
// change later, or a joiner after it, still ends up with the same document.
import { describe, expect, it } from 'vitest';
import { DataError, LoopbackHub } from './data';
import { SideRoomSDK, type SdkEnv } from './sdk';
import type { Frame } from './protocol';

const env: SdkEnv = {
  parent: null,
  hostOrigin: 'https://chat.example.org',
  hostWindow: {},
  addEventListener: () => undefined,
  removeEventListener: () => undefined,
};

function room(members: string[], host: string) {
  const hub = new LoopbackHub(host);
  const sdks = Object.fromEntries(
    members.map((m) => [m, new SideRoomSDK('7', { env, transport: hub.connect(m) })]),
  ) as Record<string, SideRoomSDK>;
  return { hub, sdks };
}

describe('host handoff in the data layer', () => {
  it('moves host:* writes, inputs and replies to the successor', () => {
    const { hub, sdks } = room(['a', 'b', 'c'], 'a');
    const [a, b, c] = [sdks.a!, sdks.b!, sdks.c!];
    a.state.set({ 'host:round': 1 });
    expect(c.state.get('host:round')).toBe(1);

    // a leaves; the platform names b.
    hub.disconnect('a');
    a.close();
    const events: string[] = [];
    b.on('host', (e) => events.push(`b:${e.host}:${e.previous}:${e.self}`));
    c.on('host', (e) => events.push(`c:${e.host}:${e.previous}:${e.self}`));
    b.data.changeHost('b');
    c.data.changeHost('b');
    expect(events).toEqual(['b:b:a:true', 'c:b:a:false']);
    expect(b.isHost).toBe(true);
    expect(c.hostId).toBe('b');

    // The successor writes host keys; a non-host still cannot.
    b.state.set({ 'host:round': 2 });
    expect(c.state.get('host:round')).toBe(2);
    expect(() => c.state.set({ 'host:round': 9 })).toThrow(DataError);

    // Inputs go to the new host; only it replies.
    const got: string[] = [];
    b.on('input', (e) => got.push(`${e.from}:${e.data[0]}`));
    c.input.send(Uint8Array.of(7));
    expect(got).toEqual(['c:7']);
    expect(() => c.input.reply('b', Uint8Array.of(1))).toThrow(DataError);
  });

  it('ignores a forged host:* write from the old host after the change', () => {
    const { hub, sdks } = room(['a', 'b', 'c'], 'a');
    const [a, b, c] = [sdks.a!, sdks.b!, sdks.c!];
    a.state.set({ 'host:x': 1 });
    b.data.changeHost('b');
    c.data.changeHost('b');
    // `a` is still connected but no longer the host: its host write reaches nobody's document.
    a.state.set({ 'host:x': 99 });
    expect(b.state.get('host:x')).toBe(1);
    expect(c.state.get('host:x')).toBe(1);
    void hub;
  });

  it('a peer that sees the change first, and a joiner after it, still converge', () => {
    const { hub, sdks } = room(['a', 'b', 'c'], 'a');
    const [a, b, c] = [sdks.a!, sdks.b!, sdks.c!];
    a.state.set({ 'host:map': 'm1', score: 3 });
    hub.disconnect('a');
    a.close();
    // c hears of the change before b does: its sync goes unanswered until b is the host.
    c.data.changeHost('b');
    b.data.changeHost('b'); // b re-stamps host:map and broadcasts a snapshot
    expect(c.state.get('host:map')).toBe('m1');
    expect(c.state.version).toBe(b.state.version);

    // A member who joins after the handoff gets the host keys from the new host.
    const d = new SideRoomSDK('7', { env, transport: { ...hub.connect('d'), host: 'b' } });
    expect(d.state.get('host:map')).toBe('m1');
    expect(d.state.get('score')).toBe(3);
    b.state.set({ 'host:map': 'm2' });
    expect(d.state.get('host:map')).toBe('m2');
  });

  it('a repeat or empty change does nothing', () => {
    const { sdks } = room(['a', 'b'], 'a');
    const seen: unknown[] = [];
    sdks.b!.on('host', (e) => seen.push(e));
    sdks.b!.data.changeHost('a');
    sdks.b!.data.changeHost('');
    expect(seen).toEqual([]);
    expect(sdks.b!.hostId).toBe('a');
  });
});

describe('HOST_CHANGED through the bridge', () => {
  it('the SDK subscribes for the data channel and moves hostId and config.host_id', async () => {
    const posted: Frame[] = [];
    let listener: ((e: MessageEvent) => void) | null = null;
    const host = {};
    const bridge: SdkEnv = {
      parent: { postMessage: (m: unknown) => posted.push(m as Frame) },
      hostOrigin: 'https://chat.example.org',
      hostWindow: host,
      addEventListener: (_t, l) => (listener = l),
      removeEventListener: () => (listener = null),
    };
    const send = (data: unknown) =>
      listener?.({ source: host, origin: 'https://chat.example.org', data } as MessageEvent);
    const sdk = new SideRoomSDK('7', { env: bridge, timeoutMs: 1000 });
    const ready = sdk.ready();
    send({
      sr: 1,
      kind: 'ready',
      config: {
        application_id: '7',
        instance_id: '1',
        channel_id: '2',
        guild_id: '3',
        user_id: 'b',
        host_id: 'a',
        capabilities: ['datachannel', 'host_handoff'],
      },
    });
    await ready;
    const subs = posted
      .filter((f) => f.kind === 'request' && f.cmd === 'subscribe')
      .map((f) => (f as { args: { event: string } }).args.event);
    expect(subs).toEqual(expect.arrayContaining(['DATA_MESSAGE', 'HOST_CHANGED']));
    expect(sdk.hostId).toBe('a');
    send({ sr: 1, kind: 'event', event: 'HOST_CHANGED', data: { host_id: 'b', previous: 'a' } });
    expect(sdk.hostId).toBe('b');
    expect(sdk.isHost).toBe(true);
    expect(sdk.config?.host_id).toBe('b');
    sdk.close();
  });
});
