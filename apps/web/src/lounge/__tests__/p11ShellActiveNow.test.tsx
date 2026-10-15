import { screen, within } from '@testing-library/react';
import { useStore } from '../../store';
import { buildActiveCards } from '../shell/activeNowModel';
import type { RelationshipRow } from '../shell/api';
import { toLoungePresence } from '../liveMap';
import { useSocial } from '../shell/social';
import { at, baseHandlers, seedLounge } from './gapsHelpers';
import { http, HttpResponse, server } from '../../test/server';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const row = (id: string, name: string): RelationshipRow => ({
  id,
  type: 1,
  since: '2026-01-01T00:00:00Z',
  user: { id, username: name.toLowerCase(), global_name: name },
});

describe('buildActiveCards', () => {
  const friends = [
    row('2', 'Bob'),
    row('3', 'Cat'),
    row('4', 'Dan'),
    row('5', 'Eve'),
    row('6', 'Fay'),
  ];
  const base = {
    friends,
    guildName: (id: string) => (id === 'g1' ? 'Alpha' : null),
    channelName: (id: string) => (id === 'v1' ? 'Lounge' : null),
  };

  it('picks one card per friend, richest first', () => {
    const cards = buildActiveCards({
      ...base,
      presence: {
        '2': {
          status: 'online',
          activities: [{ kind: 'playing', text: 'Chess', platform: 'steam' }],
        },
        '3': { status: 'online' },
        '4': { status: 'idle' },
        '5': {
          status: 'online',
          activities: [{ kind: 'watching', text: 'Akira', partyId: 'p9' }],
        },
        '6': { status: 'offline' },
      },
      voice: [
        { userId: '3', guildId: 'g1', channelId: 'v1', stream: true },
        { userId: '4', guildId: 'g1', channelId: 'v1', stream: false },
        { userId: '5', guildId: 'g1', channelId: 'v1', stream: false },
        { userId: '6', guildId: 'g1', channelId: 'v1', stream: false },
      ],
    });
    expect(cards.map((c) => [c.row.id, c.kind])).toEqual([
      ['2', 'activity'],
      ['3', 'stream'],
      ['4', 'voice'],
      ['5', 'party'],
      ['6', 'voice'],
    ]);
    const dan = cards.find((c) => c.row.id === '4');
    expect(dan?.kind === 'voice' && dan.others.sort()).toEqual(['3', '5', '6']);
  });

  it('skips offline friends without voice and idle friends without an activity', () => {
    expect(
      buildActiveCards({
        ...base,
        presence: {
          '2': { status: 'offline', activities: [{ kind: 'playing', text: 'x' }] },
          '3': { status: 'online' },
        },
        voice: [],
      }),
    ).toEqual([]);
  });
});

describe('toLoungePresence activities', () => {
  it('reads cinema watching with the title in details and drops expired ones', () => {
    const p = {
      user_id: '2',
      status: 'online' as const,
      activities: [
        {
          type: 3,
          name: 'SIDE ROOM Cinema',
          details: 'Akira',
          state: 'S1 E3 · Tetsuo',
          timestamps: { start: 1000 },
          expires_at_ms: 5000,
        },
        { type: 4, name: 'Custom Status', state: 'brb' },
        { type: 6, name: 'SIDE ROOM Library', details: 'Berserk', state: 'Chapter 12' },
      ],
    };
    const live = toLoungePresence(p, 2000);
    expect(live.activities?.map((a) => [a.kind, a.text, a.details])).toEqual([
      ['watching', 'Akira', 'S1 E3 · Tetsuo'],
      ['reading', 'Berserk', 'Chapter 12'],
    ]);
    expect(live.custom?.text).toBe('brb');
    expect(toLoungePresence(p, 9000).activities?.map((a) => a.text)).toEqual(['Berserk']);
  });
});

describe('LNG-FRD-013 Active Now column', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    useSocial.getState().reset();
    const bob = { id: '2', username: 'bob', global_name: 'Bob' };
    const cat = { id: '3', username: 'cat', global_name: 'Cat' };
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json(
          [row('2', 'Bob'), row('3', 'Cat')].map((r) => ({ ...r, nickname: null })),
        ),
      ),
    );
    useStore.setState((s) => ({
      users: { ...s.users, '2': bob, '3': cat },
      presences: {
        '2': { user_id: '2', status: 'online', activities: [] },
        '3': { user_id: '3', status: 'online', activities: [] },
      },
      voiceStates: {
        g1: {
          '2': { guild_id: 'g1', channel_id: 'c1', user_id: '2', self_stream: true },
          '3': { guild_id: 'g1', channel_id: 'c1', user_id: '3' },
        },
      },
    }));
  });

  it('shows a stream card and a voice card with where they are', async () => {
    at('/channels/@me');
    const aside = await screen.findByRole('complementary', { name: 'Active Now' });
    expect(await within(aside).findByText('Sharing their screen')).toBeInTheDocument();
    expect(within(aside).getByText('LIVE')).toBeInTheDocument();
    expect(within(aside).getByText('In general')).toBeInTheDocument();
    expect(within(aside).getAllByText(/Store Guild/).length).toBeGreaterThan(0);
  });
});
