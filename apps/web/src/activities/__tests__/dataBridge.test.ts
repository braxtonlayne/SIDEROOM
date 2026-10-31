import { describe, expect, it } from 'vitest';
import { SideRoomSDK, type Frame, type SdkEnv } from '@sideroom/embedded-app-sdk';
import { createActivityHost, type ActivityDataBridge, type HostEnv } from '../host';
import type { ActivityInstance } from '../types';

// ACT-CH-001: two Activities (two SDKs, two host pages) share state and inputs through a fake SFU that only
// moves opaque bytes between the members who joined the instance. This is the full SDK <-> postMessage <->
// host <-> data channel path with the SFU and the E2EE worker replaced by a relay.
const ORIGIN = 'https://7.activities.chat.example.org';
const HOST_ORIGIN = 'https://chat.example.org';

const instance = (caps: string[]): ActivityInstance => ({
  id: '9',
  application_id: '7',
  application_name: 'Board',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [],
  capabilities: caps,
});

/** The relay: bytes from a member go to every other member that joined the same instance. */
class FakeSfu {
  readonly wire: Uint8Array[] = [];
  private members = new Map<
    string,
    { instances: Set<string>; cb: Set<(i: string, s: string, b: Uint8Array) => void> }
  >();
  port(user: string): ActivityDataBridge {
    const m = {
      instances: new Set<string>(),
      cb: new Set<(i: string, s: string, b: Uint8Array) => void>(),
    };
    this.members.set(user, m);
    return {
      join: (i) => void m.instances.add(i),
      leave: (i) => void m.instances.delete(i),
      onMessage: (cb) => {
        m.cb.add(cb);
        return () => m.cb.delete(cb);
      },
      send: async (i, bytes) => {
        if (!m.instances.has(i)) return false;
        this.wire.push(bytes);
        for (const [id, o] of this.members) {
          if (id === user || !o.instances.has(i)) continue;
          for (const cb of o.cb) cb(i, user, bytes.slice());
        }
        return true;
      },
    };
  }
}

function participant(sfu: FakeSfu, user: string, caps: string[]) {
  const listeners = new Set<(e: MessageEvent) => void>();
  const hostWindow = {};
  const host = createActivityHost({
    origin: ORIGIN,
    applicationId: '7',
    instance: () => instance(caps),
    channel: () => ({ id: '3', guild_id: '2', name: 'Lobby' }),
    users: () => [],
    oauth: {
      authorizeCode: async () => 'c',
      tokenInfo: async () => {
        throw new Error('x');
      },
    },
    confirm: async () => true,
    openLink: () => undefined,
    post: (f: Frame) =>
      listeners.forEach((l) =>
        l({ data: f, source: hostWindow, origin: HOST_ORIGIN } as MessageEvent),
      ),
    setPresence: () => undefined,
    assets: async () => [],
    selfId: () => user,
    data: sfu.port(user),
  } as HostEnv);
  const env: SdkEnv = {
    parent: { postMessage: (m) => void host.handle(m, ORIGIN) },
    hostOrigin: HOST_ORIGIN,
    hostWindow,
    addEventListener: (_t, l) => listeners.add(l),
    removeEventListener: (_t, l) => listeners.delete(l),
  };
  const sdk = new SideRoomSDK('7', { env });
  return { sdk, host };
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe('sdk.state and sdk.input through the host page', () => {
  it('two Activities exchange state, inputs and an addressed reply; late joiners sync', async () => {
    const sfu = new FakeSfu();
    const a = participant(sfu, '10', ['datachannel']); // the host (started_by 10)
    const b = participant(sfu, '11', ['datachannel']);
    await a.sdk.ready();
    await b.sdk.ready();
    await tick();
    expect(a.sdk.self).toBe('10');
    expect(b.sdk.hostId).toBe('10');
    expect(a.sdk.isHost).toBe(true);

    a.sdk.state.set({ 'host:round': 1, board: 'x' });
    await tick();
    expect(b.sdk.state.get('board')).toBe('x');
    expect(b.sdk.state.get('host:round')).toBe(1);
    b.sdk.state.set({ note: 'hi' });
    await tick();
    expect(a.sdk.state.get('note')).toBe('hi');
    expect(() => b.sdk.state.set({ 'host:round': 2 })).toThrow();

    const got: string[] = [];
    a.sdk.on('input', (e) => got.push(`${e.from}:${new TextDecoder().decode(e.data)}`));
    b.sdk.input.send(new TextEncoder().encode('fold'));
    await tick();
    expect(got).toEqual(['11:fold']);

    const replies: string[] = [];
    b.sdk.on('input', (e) => e.reply && replies.push(new TextDecoder().decode(e.data)));
    a.sdk.input.reply('11', new TextEncoder().encode('A♠ K♦'));
    await tick();
    expect(replies).toEqual(['A♠ K♦']);

    // A third participant joins late and is brought up to date.
    const c = participant(sfu, '12', ['datachannel']);
    await c.sdk.ready();
    await tick();
    await tick();
    expect(c.sdk.state.all()).toEqual({ 'host:round': 1, board: 'x', note: 'hi' });
  });

  it('refuses dataSend without the capability', async () => {
    const sfu = new FakeSfu();
    const a = participant(sfu, '10', []);
    await a.sdk.ready();
    expect(a.sdk.self).toBeNull();
    expect(() => a.sdk.state.set({ a: 1 })).toThrow(/No data channel/);
  });
});
