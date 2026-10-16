import { render, screen, waitFor } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { http, HttpResponse, server } from '../../test/server';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import { buildLiveSnapshot, type SnapshotInput } from '../live';
import { toLoungeMessage } from '../liveMap';
import { drafts } from '../drafts';
import { useLoungeUi } from '../store';
import type * as S from '../../store/types';

// The gateway is not under test here; the store is seeded directly.
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob' };

const message = (id: string, extra: Partial<S.Message> = {}): S.Message => ({
  id,
  channel_id: 'c1',
  author: bob,
  content: `text ${id}`,
  timestamp: '2026-01-01T00:00:00Z',
  ...extra,
});

function input(extra: Partial<SnapshotInput> = {}): SnapshotInput {
  return {
    currentUserId: '1',
    users: { '1': me, '2': bob },
    guilds: { g1: { id: 'g1', name: 'One', owner_id: '2' } },
    guildOrder: ['g1'],
    channels: {
      c1: {
        id: 'c1',
        type: 0,
        guild_id: 'g1',
        name: 'general',
        position: 0,
        last_message_id: '300',
      },
      c2: {
        id: 'c2',
        type: 0,
        guild_id: 'g1',
        name: 'off-topic',
        position: 1,
        last_message_id: null,
      },
    },
    channelIdsByGuild: { g1: ['c1', 'c2'] },
    // @everyone: VIEW_CHANNEL (1 << 10) and SEND_MESSAGES (1 << 11).
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: '3072',
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    members: { 'g1:1': { guild_id: 'g1', user_id: '1', roles: [] } },
    memberIdsByGuild: { g1: ['1'] },
    presences: { '2': { user_id: '2', status: 'dnd', activities: [] } },
    readStates: { c1: { channel_id: 'c1', last_message_id: '200', mention_count: 2 } },
    ...extra,
  };
}

describe('buildLiveSnapshot', () => {
  it('maps guilds, channels and unread state from the store', () => {
    const snap = buildLiveSnapshot(input());
    expect(snap.me.displayName).toBe('Me Myself');
    expect(snap.rail).toEqual([{ kind: 'guild', guildId: 'g1' }]);
    const [general, offTopic] = snap.channelsByGuild.g1!;
    expect(general).toMatchObject({
      name: 'general',
      unread: true,
      mentionCount: 2,
      canSend: true,
    });
    expect(general!.lastReadMessageId).toBe('200');
    // No last message means nothing to read, whatever the read state says.
    expect(offTopic).toMatchObject({ name: 'off-topic', unread: false });
    expect(snap.guilds[0]).toMatchObject({ id: 'g1', unread: true, mentionCount: 2 });
  });

  it('derives guild permissions from @everyone and the viewer roles', () => {
    const snap = buildLiveSnapshot(input());
    expect(snap.guilds[0]!.permissions).toEqual({
      manageGuild: false,
      manageChannels: false,
      createInvite: false,
      moderate: false,
      manageMessages: false,
      mentionEveryone: false,
      pinMessages: false,
      createEvents: false,
      manageEvents: false,
      serverSettings: false,
    });
    // Without SEND_MESSAGES (1 << 11) in the viewer's base permissions the composer is locked.
    const locked = buildLiveSnapshot(
      input({
        roles: {
          g1: {
            id: 'g1',
            guild_id: 'g1',
            name: '@everyone',
            color: 0,
            position: 0,
            permissions: '1024',
          },
        },
      }),
    );
    expect(locked.channelsByGuild.g1![0]!.canSend).toBe(false);
  });

  it('gives the guild owner every permission and an open composer', () => {
    const snap = buildLiveSnapshot(
      input({ guilds: { g1: { id: 'g1', name: 'One', owner_id: '1' } } }),
    );
    expect(snap.guilds[0]!.permissions.manageGuild).toBe(true);
    expect(snap.channelsByGuild.g1![0]!.canSend).toBe(true);
  });

  it('keeps the composer enabled until the viewer member object is loaded', () => {
    const snap = buildLiveSnapshot(input({ members: {}, memberIdsByGuild: {} }));
    expect(snap.channelsByGuild.g1![0]!.canSend).toBe(true);
  });

  it('lists direct messages, newest first, with snowflake-derived times', () => {
    const snap = buildLiveSnapshot(
      input({
        channels: {
          d1: {
            id: '1000000000000000000',
            type: 1,
            name: null,
            last_message_id: '1000000000000000000',
            recipients: [bob],
          },
          d2: {
            id: '2000000000000000000',
            type: 3,
            name: 'group',
            last_message_id: null,
            recipients: [bob],
          },
        },
        channelIdsByGuild: { '@me': ['d1', 'd2'] },
        readStates: {},
      }),
    );
    expect(snap.dms.map((d) => d.id)).toEqual(['1000000000000000000', '2000000000000000000']);
    expect(snap.dms[0]).toMatchObject({ recipientIds: ['2'], unread: true });
    // 1000000000000000000 encodes 2022-07 (snowflake epoch 2015-01-01).
    expect(snap.dms[0]!.lastMessageAt).toMatch(/^2022-/);
    expect(snap.dms[1]!.lastMessageAt).toBe('');
  });
});

describe('toLoungeMessage', () => {
  it('maps optimistic state, reply target and reactions', () => {
    const out = toLoungeMessage(
      message('pending:x', {
        local: { status: 'failed', nonce: 'x' },
        message_reference: { message_id: '9' },
        reactions: [{ emoji: { name: '👍' }, count: 2, me: true }],
      }),
    );
    expect(out).toMatchObject({
      channelId: 'c1',
      failed: true,
      pending: false,
      replyToId: '9',
      reactions: [{ emoji: '👍', count: 2, me: true }],
      author: { displayName: 'bob' },
    });
  });

  it('shows a webhook as its name and avatar, without a profile (DEV-BOT-006)', () => {
    const out = toLoungeMessage(
      message('w1', {
        webhook_id: '77',
        author: {
          id: '77',
          username: 'Build Bot',
          global_name: 'Build Bot',
          bot: true,
          avatar_url: '/cdn/external/t/u',
        },
      }),
    );
    expect(out.author).toMatchObject({
      id: '77',
      displayName: 'Build Bot',
      bot: true,
      webhook: true,
      avatarUrl: '/cdn/external/t/u',
    });
    expect(toLoungeMessage(message('m1', {})).author.webhook).toBeUndefined();
  });
});

describe('drafts', () => {
  beforeEach(() => {
    drafts.clear();
    localStorage.clear();
  });

  it('keeps one draft per channel across a reload', () => {
    drafts.set('c1', 'half a thought');
    drafts.set('c2', 'other');
    drafts.delete('c2');
    expect(drafts.get('c1')).toBe('half a thought');
    expect(JSON.parse(localStorage.getItem('sr.lounge.drafts') ?? '{}')).toEqual({
      c1: 'half a thought',
    });
    // A reload re-reads storage: simulate by dropping the in-memory copy.
    drafts.clear();
    localStorage.setItem('sr.lounge.drafts', JSON.stringify({ c1: 'after reload' }));
    expect(drafts.get('c1')).toBe('after reload');
  });

  it('empty text removes the draft', () => {
    drafts.set('c1', 'x');
    drafts.set('c1', '');
    expect(drafts.get('c1')).toBeUndefined();
  });
});

describe('store ack throttle (Q128)', () => {
  beforeEach(() => resetStore());

  it('posts the newest read message at most once per second, then trails', async () => {
    const posted: string[] = [];
    server.use(
      http.post('*/api/v1/channels/:cid/messages/:mid/ack', ({ params }) => {
        posted.push(String(params.mid));
        return new HttpResponse(null, { status: 204 });
      }),
    );
    useStore.setState({
      windows: {
        c1: {
          ids: ['100', '200'],
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
      },
    });
    const { ackChannel } = useStore.getState();
    ackChannel('c1');
    await waitFor(() => expect(posted).toEqual(['200']));

    // A newer message arrives inside the throttle window: nothing is sent yet.
    useStore.setState((s) => ({
      windows: { c1: { ...s.windows.c1!, ids: ['100', '200', '300'] } },
    }));
    useStore.getState().ackChannel('c1');
    expect(posted).toEqual(['200']);
    await waitFor(() => expect(posted).toEqual(['200', '300']), { timeout: 2500 });
  });
});

describe('live shell', () => {
  beforeEach(() => {
    resetStore();
    localStorage.clear();
    useLoungeUi.setState(useLoungeUi.getInitialState(), true);
    useStore.setState({
      currentUserId: '1',
      users: { '1': me, '2': bob },
      guilds: { g1: { id: 'g1', name: 'Store Guild', owner_id: '1' } },
      guildOrder: ['g1'],
      channels: {
        c1: {
          id: 'c1',
          type: 0,
          guild_id: 'g1',
          name: 'general',
          position: 0,
          last_message_id: '200',
        },
      },
      channelIdsByGuild: { g1: ['c1'] },
      memberIdsByGuild: { g1: ['1', '2'] },
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
      },
      windows: {
        c1: {
          ids: ['100', '200'],
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
      },
      messages: {
        '100': message('100', { content: 'earlier words' }),
        '200': message('200', { content: 'hello from the store' }),
      },
      connection: 'connected',
    });
  });

  it('renders channels and messages from the store, with no mock provider', async () => {
    server.use(
      http.post(
        '*/api/v1/channels/:cid/messages/:mid/ack',
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    const router = createAppRouter(createMemoryHistory({ initialEntries: ['/channels/g1/c1'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole('heading', { level: 1, name: 'general' })).toBeInTheDocument();
    expect(screen.getByText('hello from the store')).toBeInTheDocument();
    expect(screen.getAllByText('Store Guild').length).toBeGreaterThan(0);
    expect(screen.getByRole('textbox', { name: 'Message #general' })).toBeInTheDocument();
  });
});

describe('store in-flight loads', () => {
  beforeEach(() => resetStore());

  it('shares one member request between concurrent callers', async () => {
    let hits = 0;
    server.use(
      http.get('*/api/v1/guilds/:gid/members', () => {
        hits += 1;
        return HttpResponse.json([]);
      }),
    );
    const { loadMembers } = useStore.getState();
    await Promise.all([loadMembers('g9'), loadMembers('g9'), loadMembers('g9')]);
    expect(hits).toBe(1);
  });
});
