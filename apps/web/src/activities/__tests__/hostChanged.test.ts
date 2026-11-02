// D-388 (wave 46, ACT-LND-002): when the server moves an instance's host, the bridge tells a subscribed Activity
// HOST_CHANGED { host_id, previous } exactly once per change, and `ready` names the current host.
import { describe, expect, it } from 'vitest';
import type { Frame } from '@sideroom/embedded-app-sdk';
import { createActivityHost, type HostEnv } from '../host';
import { hostOf, type ActivityInstance } from '../types';

const ORIGIN = 'https://7.activities.chat.example.org';

function rig(start: Partial<ActivityInstance> = {}) {
  let inst: ActivityInstance = {
    id: '9',
    application_id: '7',
    application_name: 'Land.io',
    guild_id: '2',
    channel_id: '3',
    channel_type: 'voice',
    development: false,
    started_by: '10',
    created_at_ms: 0,
    participants: [
      { user_id: '10', joined_at_ms: 1 },
      { user_id: '11', joined_at_ms: 2 },
    ],
    capabilities: ['datachannel', 'host_handoff'],
    ...start,
  };
  const posted: Frame[] = [];
  const env: HostEnv = {
    origin: ORIGIN,
    applicationId: '7',
    instance: () => inst,
    channel: () => ({ id: '3', guild_id: '2', name: 'Lobby' }),
    users: () => [],
    oauth: { authorizeCode: async () => 'x', tokenInfo: async () => ({}) as never },
    confirm: async () => true,
    openLink: () => undefined,
    post: (f) => posted.push(f),
    setPresence: () => undefined,
    assets: async () => [],
  };
  const host = createActivityHost(env);
  return {
    host,
    posted,
    set: (patch: Partial<ActivityInstance>) => (inst = { ...inst, ...patch }),
    send: (data: unknown) => host.handle(data, ORIGIN),
    events: () => posted.filter((f): f is Extract<Frame, { kind: 'event' }> => f.kind === 'event'),
  };
}

const handshake = { sr: 1, kind: 'handshake', application_id: '7' };

describe('HOST_CHANGED from the client bridge', () => {
  it('ready names the current host, the starter when the server sends no host_id', async () => {
    const r = rig();
    await r.send(handshake);
    expect((r.posted[0] as Extract<Frame, { kind: 'ready' }>).config.host_id).toBe('10');
    const moved = rig({ host_id: '11' });
    await moved.send(handshake);
    expect((moved.posted[0] as Extract<Frame, { kind: 'ready' }>).config.host_id).toBe('11');
  });

  it('tells a subscriber once per change, with the previous host', async () => {
    const r = rig();
    await r.send(handshake);
    await r.send({
      sr: 1,
      kind: 'request',
      nonce: 'n1',
      cmd: 'subscribe',
      args: { event: 'HOST_CHANGED' },
    });
    r.host.hostChanged();
    expect(r.events()).toEqual([]);
    r.set({ host_id: '11', participants: [{ user_id: '11', joined_at_ms: 2 }] });
    r.host.participantsChanged();
    r.host.hostChanged();
    const ev = r.events().filter((e) => e.event === 'HOST_CHANGED');
    expect(ev).toHaveLength(1);
    expect(ev[0]!.data).toEqual({ host_id: '11', previous: '10' });
  });

  it('says nothing before the handshake or to a frame that did not subscribe', async () => {
    const r = rig();
    r.set({ host_id: '11' });
    r.host.hostChanged();
    await r.send(handshake);
    r.set({ host_id: '12' });
    r.host.hostChanged();
    expect(r.events()).toEqual([]);
  });

  it('hostOf falls back to the starter', () => {
    expect(hostOf({ started_by: '1' } as ActivityInstance)).toBe('1');
    expect(hostOf({ started_by: '1', host_id: '2' } as ActivityInstance)).toBe('2');
  });
});
