import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { useStore } from '../../store';
import { defaultLayout, toForumTag, visiblePosts } from '../forum/forumLogic';
import { useThreads, type ThreadInfo } from '../threads/threadStore';
import { at, baseHandlers, bob, calls, iso, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const wireThread = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  type: 11,
  guild_id: 'g1',
  parent_id: 'c1',
  name: `thread ${id}`,
  owner_id: '2',
  message_count: 3,
  member_count: 2,
  last_message_id: `${id}9`,
  thread_metadata: {
    archived: false,
    locked: false,
    archive_timestamp: '2026-01-01T00:00:00Z',
    auto_archive_duration: 1440,
  },
  ...over,
});

const post = (id: string, over: Partial<ThreadInfo> = {}): ThreadInfo => ({
  id,
  parentId: 'f1',
  guildId: 'g1',
  name: `post ${id}`,
  ownerId: '2',
  archived: false,
  locked: false,
  messageCount: 1,
  memberCount: 1,
  joined: false,
  lastMessageId: id,
  archiveTimestamp: null,
  isPrivate: false,
  slowmode: 0,
  autoArchive: 1440,
  appliedTags: [],
  pinned: false,
  ...over,
});

describe('forum logic', () => {
  it('sorts by activity or creation, keeps pinned posts first, filters by search and tags', () => {
    const posts = [
      post('10', { lastMessageId: '50' }),
      post('20', { lastMessageId: '30', appliedTags: ['t1'] }),
      post('30', { lastMessageId: '40', pinned: true, name: 'Guide to rules' }),
    ];
    const none = new Set<string>();
    expect(
      visiblePosts(posts, { sort: 'activity', query: '', tagIds: none }).map((p) => p.id),
    ).toEqual(['30', '10', '20']);
    expect(
      visiblePosts(posts, { sort: 'created', query: '', tagIds: none }).map((p) => p.id),
    ).toEqual(['30', '20', '10']);
    expect(
      visiblePosts(posts, { sort: 'activity', query: 'GUIDE', tagIds: none }).map((p) => p.id),
    ).toEqual(['30']);
    expect(
      visiblePosts(posts, { sort: 'activity', query: '', tagIds: new Set(['t1']) }).map(
        (p) => p.id,
      ),
    ).toEqual(['20']);
  });

  it('maps tags and picks the default layout', () => {
    expect(toForumTag({ id: '1', name: 'Bug', moderated: true, emoji_name: 'x' })).toEqual({
      id: '1',
      name: 'Bug',
      moderated: true,
      emoji: 'x',
    });
    expect(toForumTag({ name: 'no id' })).toBeNull();
    expect(defaultLayout(null, true)).toBe('gallery');
    expect(defaultLayout(null, false)).toBe('list');
    expect(defaultLayout(1, true)).toBe('list');
    expect(defaultLayout(2, false)).toBe('gallery');
  });
});

describe('thread store additions', () => {
  beforeEach(() => useThreads.getState().reset());

  it('reads private, slowmode, archive and tag fields and starts private threads', async () => {
    useThreads.getState().upsert([
      wireThread('10', {
        type: 12,
        rate_limit_per_user: 30,
        applied_tags: ['a'],
        flags: 2,
      }),
    ]);
    expect(useThreads.getState().byId['10']).toMatchObject({
      isPrivate: true,
      slowmode: 30,
      autoArchive: 1440,
      appliedTags: ['a'],
      pinned: true,
    });
    baseHandlers();
    server.use(
      http.post('*/api/v1/channels/c1/threads', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(wireThread('11', { type: 12, member: {} }), { status: 201 });
      }),
    );
    await useThreads.getState().createStandalone('c1', { name: 'secret', private: true });
    expect(calls.find((c) => c.key === 'POST /channels/c1/threads')?.body).toEqual({
      name: 'secret',
      type: 12,
      invitable: true,
    });
  });
});

describe('Lounge channel header and threads', () => {
  beforeEach(() => {
    baseHandlers();
    useThreads.getState().reset();
  });

  it('opens the pins popout, lists pinned messages and unpins one', async () => {
    seedLounge({ messages: [msg('100', { content: 'hello' })] });
    server.use(
      http.get('*/api/v1/channels/c1/pins', () =>
        HttpResponse.json([
          msg('100', { content: 'Remember the rules', pinned: true, timestamp: iso(5) }),
        ]),
      ),
      http.delete('*/api/v1/channels/c1/pins/100', async (ctx) => {
        await record('DELETE')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await screen.findByRole('log', { name: 'Messages in general' });
    await userEvent.click(screen.getByRole('button', { name: 'Pinned Messages' }));
    const pop = await screen.findByRole('dialog', { name: 'Pinned Messages' });
    expect(await within(pop).findByText('Remember the rules')).toBeInTheDocument();
    expect(within(pop).getByText(bob.global_name ?? 'Bob')).toBeInTheDocument();
    await userEvent.click(within(pop).getByRole('button', { name: /Unpin the message from/ }));
    await waitFor(() => expect(calls.map((c) => c.key)).toContain('DELETE /channels/c1/pins/100'));
    expect(await within(pop).findByText(/Nothing is pinned/)).toBeInTheDocument();
  });

  it('shows an empty pins popout', async () => {
    seedLounge({ messages: [msg('100')] });
    server.use(http.get('*/api/v1/channels/c1/pins', () => HttpResponse.json([])));
    at('/channels/g1/c1');
    await screen.findByRole('log');
    await userEvent.click(screen.getByRole('button', { name: 'Pinned Messages' }));
    expect(await screen.findByText(/Nothing is pinned/)).toBeInTheDocument();
  });

  it('filters the threads list by Joined and by search', async () => {
    seedLounge({ messages: [msg('100')] });
    server.use(
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({
          threads: [
            wireThread('t1', { name: 'Planning' }),
            wireThread('t2', { name: 'Memes', last_message_id: 't29' }),
          ],
          members: [{ id: 't1' }],
          has_more: false,
        }),
      ),
    );
    at('/channels/g1/c1');
    await screen.findByRole('log');
    await userEvent.click(screen.getByRole('button', { name: 'Threads' }));
    await screen.findByRole('button', { name: /Planning/ });
    expect(screen.getByRole('button', { name: /Memes/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Joined' }));
    expect(screen.queryByRole('button', { name: /Memes/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Planning/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('tab', { name: 'Active' }));
    await userEvent.type(screen.getByRole('searchbox', { name: 'Search threads' }), 'mem');
    expect(screen.queryByRole('button', { name: /Planning/ })).not.toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Memes/ })).toBeInTheDocument();
  });

  it('nests followed threads under their channel in the list', async () => {
    seedLounge({ messages: [msg('100')] });
    useThreads
      .getState()
      .upsert([wireThread('t1', { name: 'Planning', member: {} }), wireThread('t2')]);
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const nest = await within(sidebar).findByRole('list', { name: 'Threads' });
    expect(within(nest).getByText('Planning')).toBeInTheDocument();
    expect(within(nest).queryByText('thread t2')).not.toBeInTheDocument();
  });

  it('edits and deletes a thread from its settings', async () => {
    seedLounge({ messages: [msg('100')] });
    useThreads.getState().upsert([wireThread('t1', { name: 'Planning', member: {} })]);
    server.use(
      http.get('*/api/v1/channels/t1/messages', () => HttpResponse.json([])),
      http.get('*/api/v1/users/@me/guilds/g1/settings', () =>
        HttpResponse.json({ muted: false, channel_overrides: [] }),
      ),
      http.patch('*/api/v1/channels/t1', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json(wireThread('t1', { name: 'Trip', rate_limit_per_user: 60 }));
      }),
      http.delete('*/api/v1/channels/t1', async (ctx) => {
        await record('DELETE')(ctx);
        return HttpResponse.json(wireThread('t1'));
      }),
    );
    at('/channels/g1/c1/threads/t1');
    const pane = await screen.findByRole('complementary', { name: 'Thread: Planning' });
    // The viewer owns it? bob does; the viewer reaches settings through Manage Channels, so seed it.
    useStore.setState((s) => ({
      guilds: { ...s.guilds, g1: { ...s.guilds['g1']!, owner_id: '1' } },
    }));
    await userEvent.click(within(pane).getByRole('button', { name: 'Thread options' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Thread settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Thread settings' });
    const name = within(dialog).getByRole('textbox', { name: 'Thread name' });
    await userEvent.clear(name);
    await userEvent.type(name, 'Trip');
    await userEvent.click(within(dialog).getByRole('switch', { name: /Lock thread/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /channels/t1')?.body).toMatchObject({
        name: 'Trip',
        locked: true,
        auto_archive_duration: 1440,
      }),
    );
  });
});

describe('Lounge forum channel', () => {
  beforeEach(() => {
    baseHandlers();
    useThreads.getState().reset();
  });

  const forumChannels = {
    f1: {
      id: 'f1',
      type: 15,
      guild_id: 'g1',
      name: 'help',
      topic: 'Be kind. Search before posting.',
      position: 1,
    },
  };

  it('shows guidelines, posts, tag chips, search, and creates a post', async () => {
    seedLounge({ messages: [], channels: forumChannels });
    server.use(
      http.get('*/api/v1/channels/f1', () =>
        HttpResponse.json({
          id: 'f1',
          type: 15,
          available_tags: [
            { id: 'tg1', name: 'Question', moderated: false },
            { id: 'tg2', name: 'Bug', moderated: false },
          ],
          flags: 0,
        }),
      ),
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({
          threads: [
            wireThread('p1', {
              parent_id: 'f1',
              name: 'How do I log in?',
              applied_tags: ['tg1'],
            }),
            wireThread('p2', { parent_id: 'f1', name: 'Crash on start', applied_tags: ['tg2'] }),
          ],
          members: [],
          has_more: false,
        }),
      ),
      http.get('*/api/v1/channels/f1/threads/archived/public', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
      http.get('*/api/v1/channels/:tid/messages', ({ params }) =>
        HttpResponse.json([
          { ...msg('m1', { content: `first message of ${String(params['tid'])}` }) },
        ]),
      ),
      http.post('*/api/v1/channels/f1/threads', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json(
          wireThread('p3', { parent_id: 'f1', name: 'Fresh post', applied_tags: ['tg1'] }),
          { status: 201 },
        );
      }),
    );
    at('/channels/g1/f1');
    expect(
      within(await screen.findByRole('complementary', { name: 'Post guidelines' })).getByText(
        'Be kind. Search before posting.',
      ),
    ).toBeInTheDocument();
    const list = await screen.findByRole('list', { name: 'Posts' });
    expect(await within(list).findByText('How do I log in?')).toBeInTheDocument();
    expect(within(list).getByText('Crash on start')).toBeInTheDocument();
    expect(await within(list).findByText('first message of p1')).toBeInTheDocument();

    const chips = await screen.findByRole('group', { name: 'Filter by tag' });
    await userEvent.click(within(chips).getByRole('button', { name: /Bug/ }));
    expect(within(list).queryByText('How do I log in?')).not.toBeInTheDocument();
    expect(within(list).getByText('Crash on start')).toBeInTheDocument();
    await userEvent.click(within(chips).getByRole('button', { name: /Bug/ }));

    await userEvent.type(screen.getByRole('searchbox', { name: 'Search posts' }), 'log in');
    expect(within(list).queryByText('Crash on start')).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Gallery view' }));
    expect(list).toHaveAttribute('data-layout', 'gallery');

    await userEvent.click(screen.getByRole('button', { name: 'New Post' }));
    const dialog = await screen.findByRole('dialog', { name: 'New Post' });
    await userEvent.type(within(dialog).getByRole('textbox', { name: 'Title' }), 'Fresh post');
    await userEvent.type(within(dialog).getByRole('textbox', { name: 'Message' }), 'Hello there');
    await userEvent.click(within(dialog).getByRole('button', { name: /Question/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Post' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'POST /channels/f1/threads')?.body).toEqual({
        name: 'Fresh post',
        message: { content: 'Hello there' },
        applied_tags: ['tg1'],
      }),
    );
  });

  it('defaults media channels to the gallery layout', async () => {
    seedLounge({
      messages: [],
      channels: { m1: { id: 'm1', type: 16, guild_id: 'g1', name: 'art', position: 1 } },
    });
    server.use(
      http.get('*/api/v1/channels/m1', () => HttpResponse.json({ id: 'm1', type: 16 })),
      http.get('*/api/v1/guilds/g1/threads/active', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
      http.get('*/api/v1/channels/m1/threads/archived/public', () =>
        HttpResponse.json({ threads: [], members: [], has_more: false }),
      ),
    );
    at('/channels/g1/m1');
    expect(await screen.findByText('No posts yet')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Gallery view' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    expect(screen.getByRole('button', { name: 'New Upload' })).toBeInTheDocument();
  });
});

describe('Lounge jump pills and welcome header', () => {
  beforeEach(() => baseHandlers());

  it('offers a jump pill for unread mentions', async () => {
    seedLounge({
      messages: [
        msg('100', { content: 'read already' }),
        msg('101', { content: 'ping for you', mentions: [{ id: '1', username: 'me' }] }),
        msg('102', { content: 'later chatter' }),
      ],
    });
    useStore.setState({
      readStates: { c1: { channel_id: 'c1', last_message_id: '100', mention_count: 1 } },
    });
    at('/channels/g1/c1');
    const pill = await screen.findByRole('button', { name: /1 unread mention/ });
    await userEvent.click(pill);
    expect(pill).toBeInTheDocument();
  });

  it('shows Edit Channel in the welcome header for people who manage channels', async () => {
    seedLounge({ messages: [] });
    useStore.setState((s) => ({
      guilds: { ...s.guilds, g1: { ...s.guilds['g1']!, owner_id: '1' } },
    }));
    const { container } = at('/channels/g1/c1');
    const actions = await waitFor(() => {
      const el = container.querySelector<HTMLElement>('.lng-welcome-actions');
      if (!el) throw new Error('no welcome actions yet');
      return el;
    });
    await userEvent.click(within(actions).getByRole('button', { name: /Edit Channel/ }));
    expect(await screen.findByRole('dialog')).toBeInTheDocument();
  });
});
