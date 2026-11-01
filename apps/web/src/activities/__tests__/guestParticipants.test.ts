// ARC-GST-002 (D-397): the guests of a guest link are in the game's participant list, by callsign only.
import { describe, expect, it, vi } from 'vitest';
import type { Frame, Participant } from '@sideroom/embedded-app-sdk';
import { createActivityHost, OPAQUE_ORIGIN, type HostEnv } from '../host';
import type { ActivityInstance } from '../types';

const instance: ActivityInstance = {
  id: '9',
  application_id: '7',
  application_name: 'Land.io',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [{ user_id: '10', joined_at_ms: 1 }],
  capabilities: ['datachannel', 'guest_play'],
};

const guest: Participant = {
  id: 'g3',
  username: 'Amber Otter 42',
  global_name: 'Amber Otter 42',
  avatar: null,
  bot: false,
};

function rig(guests?: () => Participant[]) {
  const posted: Frame[] = [];
  const env = {
    origin: 'https://7.activities.chat.example.org',
    senderOrigin: OPAQUE_ORIGIN,
    applicationId: '7',
    instance: () => instance,
    channel: () => ({ id: '3', guild_id: '2', name: 'Voice' }),
    users: (ids: string[]) =>
      ids.map((id) => ({ id, username: 'ann', global_name: 'Ann', avatar: null })),
    oauth: { authorizeCode: vi.fn(), tokenInfo: vi.fn() },
    confirm: vi.fn(async () => true),
    openLink: vi.fn(),
    post: (f: Frame) => posted.push(f),
    setPresence: vi.fn(),
    assets: async () => [],
    guests,
  } as unknown as HostEnv;
  const host = createActivityHost(env);
  const call = async (cmd: string, args?: unknown) => {
    await host.handle({ sr: 1, kind: 'request', nonce: cmd, cmd, args }, OPAQUE_ORIGIN);
    return posted.find((f) => f.kind === 'response' && f.nonce === cmd) as Extract<
      Frame,
      { kind: 'response' }
    >;
  };
  const hello = () => host.handle({ sr: 1, kind: 'handshake', application_id: '7' }, OPAQUE_ORIGIN);
  return { host, posted, call, hello };
}

describe('guests in the participant list', () => {
  it('adds them after the members, by callsign, with no avatar and no handle beyond it', async () => {
    const r = rig(() => [guest]);
    await r.hello();
    const res = await r.call('getInstanceConnectedParticipants');
    const p = (res.ok ? (res.data as { participants: Participant[] }) : null)!.participants;
    expect(p.map((x) => x.id)).toEqual(['10', 'g3']);
    expect(p[1]).toEqual(guest);
  });

  it('is unchanged without a guest link', async () => {
    const r = rig();
    await r.hello();
    const res = await r.call('getInstanceConnectedParticipants');
    expect(
      (res.ok ? (res.data as { participants: Participant[] }) : null)!.participants,
    ).toHaveLength(1);
  });

  it('tells a subscribed game when the roster changes', async () => {
    const list: Participant[] = [];
    const r = rig(() => list);
    await r.hello();
    await r.call('subscribe', { event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE' });
    list.push(guest);
    r.host.participantsChanged();
    const ev = r.posted.find((f) => f.kind === 'event') as Extract<Frame, { kind: 'event' }>;
    expect((ev.data as { participants: Participant[] }).participants.map((x) => x.id)).toEqual([
      '10',
      'g3',
    ]);
  });
});
