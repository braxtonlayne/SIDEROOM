import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import {
  buildSearchQuery,
  highlightSegments,
  inviteErrorMessage,
  parseInviteCode,
} from '../inviteFlow';
import { ApiError } from '../../api/client';
import { useLoungeUi } from '../store';

// The gateway is not under test; the store is seeded directly.
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob' };

/** @everyone: VIEW_CHANNEL | SEND_MESSAGES | CREATE_INSTANT_INVITE (no manage bits). */
const EVERYONE = String((1 << 10) | (1 << 11) | 1);

function seed(ownerId: string) {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob },
    guilds: { g1: { id: 'g1', name: 'Store Guild', owner_id: ownerId } },
    guildOrder: ['g1'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
      c1: {
        id: 'c1',
        type: 0,
        guild_id: 'g1',
        name: 'general',
        parent_id: 'cat',
        position: 1,
        last_message_id: null,
      },
    },
    channelIdsByGuild: { g1: ['cat', 'c1'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: EVERYONE,
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    memberIdsByGuild: { g1: ['1', '2'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
    },
    windows: {
      c1: {
        ids: ['100'],
        pending: [],
        hasMoreBefore: false,
        atLatest: true,
        loading: false,
        error: null,
      },
    },
    messages: {
      '100': {
        id: '100',
        channel_id: 'c1',
        author: bob,
        content: 'hello world',
        timestamp: '2026-01-01T00:00:00Z',
      },
    },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const quiet = () =>
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/guilds/:gid/members', () => HttpResponse.json([])),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.post(
      '*/api/v1/channels/:cid/messages/:mid/ack',
      () => new HttpResponse(null, { status: 204 }),
    ),
  );

const signedIn = () =>
  server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })));

const inviteObj = (over: Record<string, unknown> = {}) => ({
  type: 0,
  code: 'abc123',
  guild: { id: 'g2', name: 'Book Club', icon: null },
  channel: { id: 'c9', name: 'general', type: 0 },
  inviter: { id: '2', username: 'bob' },
  uses: 0,
  max_uses: 0,
  max_age: 86400,
  temporary: false,
  created_at: '2026-01-01T00:00:00Z',
  expires_at: null,
  ...over,
});

describe('invite helpers', () => {
  it('extracts a code from pasted links and bare codes', () => {
    expect(parseInviteCode('abc123')).toBe('abc123');
    expect(parseInviteCode('  https://side.room/invite/AbC-123?x=1 ')).toBe('AbC-123');
    expect(parseInviteCode('side.room/join/zzz99')).toBe('zzz99');
    expect(parseInviteCode('https://side.room/zzz99/')).toBe('zzz99');
    expect(parseInviteCode('')).toBeNull();
    expect(parseInviteCode('not a code!')).toBeNull();
  });

  it('maps API errors to user copy', () => {
    const err = (status: number, code: number) => new ApiError(status, 'x', { code });
    expect(inviteErrorMessage(err(404, 10006))).toMatch(/invalid or has expired/);
    expect(inviteErrorMessage(err(403, 40007))).toMatch(/banned/);
    expect(inviteErrorMessage(err(403, 50013))).toMatch(/paused/);
    expect(inviteErrorMessage(new Error('boom'))).toMatch(/Could not reach/);
  });

  it('highlights by UTF-8 byte ranges', () => {
    // "é" is two bytes, so "hello" starts at byte 3.
    expect(highlightSegments('é hello!', [[3, 8]])).toEqual([
      { text: 'é ', hit: false },
      { text: 'hello', hit: true },
      { text: '!', hit: false },
    ]);
  });

  it('resolves from: and in: names to ids', () => {
    const lookups = {
      users: [{ id: '2', names: ['bob', 'Bobby'] }],
      channels: [{ id: 'c1', name: 'general' }],
    };
    expect(buildSearchQuery('hi from:bob in:#general', lookups)).toEqual({
      q: 'hi from:2 in:c1',
    });
    expect(buildSearchQuery('from:77', lookups)).toEqual({ q: 'from:77' });
    expect(buildSearchQuery('from:nobody', lookups)).toHaveProperty('error');
  });
});

describe('logged-in invite acceptance', () => {
  beforeEach(() => {
    seed('2');
    quiet();
    signedIn();
  });

  const preview = () =>
    server.use(
      http.get('*/api/v1/invites/abc123', () =>
        HttpResponse.json({
          ...inviteObj(),
          approximate_member_count: 12,
          approximate_presence_count: 3,
        }),
      ),
    );

  it('shows the server and an Accept button, joins, then opens the guild', async () => {
    preview();
    let accepted = 0;
    server.use(
      http.post('*/api/v1/invites/abc123', () => {
        accepted += 1;
        return HttpResponse.json(inviteObj());
      }),
      http.get('*/api/v1/guilds/g2', () =>
        HttpResponse.json({ id: 'g2', name: 'Book Club', owner_id: '2' }),
      ),
      http.get('*/api/v1/guilds/g2/channels', () =>
        HttpResponse.json([{ id: 'c9', type: 0, guild_id: 'g2', name: 'general', position: 0 }]),
      ),
      http.get('*/api/v1/guilds/g2/roles', () => HttpResponse.json([])),
    );
    const router = at('/invite/abc123');
    expect(await screen.findByRole('heading', { name: 'Book Club' })).toBeInTheDocument();
    expect(screen.getByText(/12 members/)).toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Log in to accept' })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Accept invite' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g2/c9'));
    expect(accepted).toBe(1);
    expect(useStore.getState().guilds.g2?.name).toBe('Book Club');
  });

  it.each([
    [404, { code: 10006, message: 'Unknown Invite' }, /invalid or has expired/],
    [403, { code: 40007, message: 'banned' }, /banned/],
    [403, { code: 50013, message: 'paused' }, /paused/],
  ])('shows the failure copy for %i', async (status, body, text) => {
    preview();
    server.use(http.post('*/api/v1/invites/abc123', () => HttpResponse.json(body, { status })));
    const router = at('/invite/abc123');
    await userEvent.click(await screen.findByRole('button', { name: 'Accept invite' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(text);
    expect(router.state.location.pathname).toBe('/invite/abc123');
  });

  it('an expired invite shows the generic message and no buttons', async () => {
    server.use(
      http.get('*/api/v1/invites/abc123', () =>
        HttpResponse.json({ code: 10006 }, { status: 404 }),
      ),
    );
    at('/invite/abc123');
    expect(await screen.findByText('This invite is invalid or has expired.')).toBeInTheDocument();
    expect(screen.queryByRole('button')).toBeNull();
  });
});

describe('Add a Server', () => {
  beforeEach(() => {
    seed('1');
    quiet();
  });

  it('Create My Own posts /guilds and opens the new server', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/guilds', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(
          {
            id: 'g5',
            name: 'Fresh',
            owner_id: '1',
            channels: [
              { id: 'k1', type: 4, guild_id: 'g5', name: 'Text Channels', position: 0 },
              { id: 'k2', type: 0, guild_id: 'g5', name: 'general', position: 1, parent_id: 'k1' },
            ],
            roles: [],
          },
          { status: 201 },
        );
      }),
    );
    const router = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Create My Own' }));
    await userEvent.click(await screen.findByRole('button', { name: 'skip this question' }));
    const name = screen.getByLabelText('Server name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Fresh');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g5/k2'));
    expect(body).toEqual({ name: 'Fresh' });
  });

  it('Join a Server accepts a pasted invite link', async () => {
    let hit = '';
    server.use(
      http.post('*/api/v1/invites/:code', ({ params }) => {
        hit = String(params.code);
        return HttpResponse.json(inviteObj({ code: hit }));
      }),
      http.get('*/api/v1/guilds/g2', () =>
        HttpResponse.json({ id: 'g2', name: 'Book Club', owner_id: '2' }),
      ),
      http.get('*/api/v1/guilds/g2/channels', () =>
        HttpResponse.json([{ id: 'c9', type: 0, guild_id: 'g2', name: 'general', position: 0 }]),
      ),
      http.get('*/api/v1/guilds/g2/roles', () => HttpResponse.json([])),
    );
    const router = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Join a Server' }));
    await userEvent.type(
      screen.getByLabelText('Invite link or code'),
      'http://localhost/invite/abc123',
    );
    await userEvent.click(screen.getByRole('button', { name: 'Join Server' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g2/c9'));
    expect(hit).toBe('abc123');
  });

  it('shows the banned message when the join is refused', async () => {
    server.use(
      http.post('*/api/v1/invites/:code', () =>
        HttpResponse.json({ code: 40007, message: 'banned' }, { status: 403 }),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Join a Server' }));
    await userEvent.type(screen.getByLabelText('Invite link or code'), 'abc123');
    await userEvent.click(screen.getByRole('button', { name: 'Join Server' }));
    expect(await screen.findByText(/banned from this server/)).toBeInTheDocument();
  });
});

describe('Create Channel / Category', () => {
  beforeEach(() => quiet());

  async function openServerMenu() {
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.click(within(sidebar).getByRole('button', { name: /Store Guild/ }));
  }

  it('creates a text channel in the guild from the server menu', async () => {
    seed('1');
    let body: Record<string, unknown> = {};
    server.use(
      http.post('*/api/v1/guilds/g1/channels', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          { id: 'c2', type: 0, guild_id: 'g1', name: body.name, position: 2 },
          { status: 201 },
        );
      }),
    );
    const router = at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Channel' }));
    await userEvent.type(screen.getByLabelText('Channel name'), 'Book Chat');
    await userEvent.click(screen.getByRole('button', { name: 'Create Channel' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/c2'));
    expect(body).toEqual({ name: 'book-chat', type: 0 });
    expect(useStore.getState().channels.c2?.name).toBe('book-chat');
  });

  it('private voice channel sends a VIEW_CHANNEL overwrite for @everyone', async () => {
    seed('1');
    let body: Record<string, unknown> = {};
    server.use(
      http.post('*/api/v1/guilds/g1/channels', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          { id: 'v1', type: 2, guild_id: 'g1', name: 'Lounge' },
          { status: 201 },
        );
      }),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Channel' }));
    await userEvent.click(screen.getByRole('radio', { name: /Voice/ }));
    await userEvent.type(screen.getByLabelText('Channel name'), 'Lounge');
    await userEvent.click(screen.getByRole('switch', { name: /Private Channel/ }));
    await userEvent.click(screen.getByRole('button', { name: 'Create Channel' }));
    await waitFor(() => expect(body.name).toBe('Lounge'));
    expect(body.type).toBe(2);
    expect(body.permission_overwrites).toEqual([
      { id: 'g1', type: 0, allow: '0', deny: '1024' },
      { id: '1', type: 1, allow: '1024', deny: '0' },
    ]);
  });

  it('creates a category and a channel inside a category', async () => {
    seed('1');
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/guilds/g1/channels', async ({ request }) => {
        const b = (await request.json()) as Record<string, unknown>;
        bodies.push(b);
        return HttpResponse.json(
          { id: `n${bodies.length}`, type: b.type, guild_id: 'g1', name: b.name },
          { status: 201 },
        );
      }),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Category' }));
    expect(screen.queryByRole('radio')).toBeNull();
    await userEvent.type(screen.getByLabelText('Category name'), 'Games');
    await userEvent.click(screen.getByRole('button', { name: 'Create Category' }));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toEqual({ name: 'Games', type: 4 });

    await userEvent.click(
      await screen.findByRole('button', { name: 'Create Channel in Text Channels' }),
    );
    await userEvent.type(screen.getByLabelText('Channel name'), 'ideas');
    await userEvent.click(screen.getByRole('button', { name: 'Create Channel' }));
    await waitFor(() => expect(bodies).toHaveLength(2));
    expect(bodies[1]).toEqual({ name: 'ideas', type: 0, parent_id: 'cat' });
  });

  it('hides every create control without MANAGE_CHANNELS', async () => {
    seed('2');
    at('/channels/g1/c1');
    await openServerMenu();
    expect(await screen.findByRole('menuitem', { name: 'Invite to Server' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: 'Create Channel' })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Create Category' })).toBeNull();
    expect(screen.queryByRole('button', { name: /Create Channel in/ })).toBeNull();
  });

  it('shows the permission error when the server refuses', async () => {
    seed('1');
    server.use(
      http.post('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Create Channel' }));
    await userEvent.type(screen.getByLabelText('Channel name'), 'nope');
    await userEvent.click(screen.getByRole('button', { name: 'Create Channel' }));
    expect(await screen.findByText(/do not have permission/)).toBeInTheDocument();
  });
});

describe('Invite people', () => {
  beforeEach(() => {
    seed('1');
    quiet();
  });

  it('creates an invite with the chosen options, copies the link, lists and revokes', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true });
    let body: unknown;
    let revoked = '';
    server.use(
      http.get('*/api/v1/guilds/g1/invites', () =>
        HttpResponse.json([
          inviteObj({
            code: 'old111',
            uses: 2,
            max_uses: 5,
            guild: { id: 'g1', name: 'Store Guild' },
          }),
        ]),
      ),
      http.post('*/api/v1/channels/c1/invites', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(inviteObj({ code: 'new222', max_uses: 10, max_age: 3600 }));
      }),
      http.delete('*/api/v1/invites/:code', ({ params }) => {
        revoked = String(params.code);
        return HttpResponse.json(inviteObj({ code: revoked }));
      }),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.click(within(sidebar).getByRole('button', { name: /Store Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Invite to Server' }));
    const dialog = await screen.findByRole('dialog', { name: /Invite people to Store Guild/ });
    expect(await within(dialog).findByText('old111')).toBeInTheDocument();

    await userEvent.selectOptions(within(dialog).getByLabelText('Expire after'), '1 hour');
    await userEvent.selectOptions(within(dialog).getByLabelText('Max number of uses'), '10 uses');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Generate invite link' }));
    const link = (await within(dialog).findByLabelText('Invite link')) as HTMLInputElement;
    expect(link.value).toMatch(/\/invite\/new222$/);
    expect(body).toEqual({ max_age: 3600, max_uses: 10 });

    await userEvent.click(within(dialog).getByRole('button', { name: 'Copy' }));
    expect(writeText).toHaveBeenCalledWith(link.value);
    expect(await within(dialog).findByRole('button', { name: 'Copied!' })).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole('button', { name: 'Revoke invite old111' }));
    await waitFor(() => expect(within(dialog).queryByText('old111')).toBeNull());
    expect(revoked).toBe('old111');
  });

  it('a member without MANAGE_GUILD can create but does not get the invite list', async () => {
    seed('2');
    let listed = false;
    server.use(
      http.get('*/api/v1/guilds/g1/invites', () => {
        listed = true;
        return HttpResponse.json([]);
      }),
      http.post('*/api/v1/channels/c1/invites', () =>
        HttpResponse.json(inviteObj({ code: 'mine33' })),
      ),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.click(within(sidebar).getByRole('button', { name: /Store Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Invite to Server' }));
    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).queryByRole('region', { name: 'Active invites' })).toBeNull();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Generate invite link' }));
    expect(await within(dialog).findByLabelText('Invite link')).toBeInTheDocument();
    expect(listed).toBe(false);
  });

  it('opens from the rail context menu too', async () => {
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    const link = within(rail).getByRole('link', { name: /Store Guild/ });
    server.use(http.get('*/api/v1/guilds/g1/invites', () => HttpResponse.json([])));
    await userEvent.pointer({ keys: '[MouseRight]', target: link });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Invite People' }));
    expect(
      await screen.findByRole('dialog', { name: /Invite people to Store Guild/ }),
    ).toBeInTheDocument();
  });
});

describe('message search', () => {
  beforeEach(() => {
    seed('1');
    quiet();
  });

  const hit = {
    id: '100',
    channel_id: 'c1',
    guild_id: 'g1',
    author: { id: '2', username: 'bob' },
    content: 'hello world',
    timestamp: '2026-01-01T00:00:00Z',
    hit: true,
    sr_snippet: { text: 'hello world', highlights: [[6, 11]] },
  };

  it('searches the guild, highlights the snippet and jumps to the message', async () => {
    let q = '';
    server.use(
      http.get('*/api/v1/guilds/g1/messages/search', ({ request }) => {
        q = new URL(request.url).searchParams.get('q') ?? '';
        return HttpResponse.json({
          total_results: 1,
          messages: [[hit]],
          doing_deep_historical_index: false,
        });
      }),
    );
    const router = at('/channels/g1/c1');
    const box = await screen.findByRole('searchbox', { name: 'Search #general' });
    await userEvent.type(box, 'world from:bob in:#general{Enter}');
    const panel = await screen.findByRole('region', { name: 'Search results' });
    expect(await within(panel).findByText('1 result')).toBeInTheDocument();
    expect(q).toBe('world from:2 in:c1');
    const mark = within(panel).getByText('world');
    expect(mark.tagName).toBe('MARK');
    await userEvent.click(within(panel).getByRole('button', { name: /bob/ }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g1/c1/100'));
    expect(screen.queryByRole('region', { name: 'Search results' })).toBeNull();
  });

  it('inserts filter chips and reports an unknown from: name without calling the API', async () => {
    let called = false;
    server.use(
      http.get('*/api/v1/guilds/g1/messages/search', () => {
        called = true;
        return HttpResponse.json({
          total_results: 0,
          messages: [],
          doing_deep_historical_index: false,
        });
      }),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('searchbox', { name: 'Search #general' });
    await userEvent.type(box, 'x from:ghost{Enter}');
    const panel = await screen.findByRole('region', { name: 'Search results' });
    expect(await within(panel).findByRole('alert')).toHaveTextContent(/No one called "ghost"/);
    expect(called).toBe(false);
    await userEvent.click(within(panel).getByRole('button', { name: 'has:link' }));
    expect(box).toHaveValue('x from:ghost has:link');
  });

  it('shows an empty state and a failure message', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/messages/search', () =>
        HttpResponse.json({ total_results: 0, messages: [], doing_deep_historical_index: false }),
      ),
    );
    at('/channels/g1/c1');
    const box = await screen.findByRole('searchbox', { name: 'Search #general' });
    await userEvent.type(box, 'zzz{Enter}');
    expect(await screen.findByText('No results')).toBeInTheDocument();
    server.use(
      http.get('*/api/v1/guilds/g1/messages/search', () =>
        HttpResponse.json({ code: 500 }, { status: 500 }),
      ),
    );
    await userEvent.clear(box);
    await userEvent.type(box, 'boom{Enter}');
    expect(await screen.findByText('Search failed. Try again.')).toBeInTheDocument();
  });

  it('a jump to a message that is not loaded fetches the page around it', async () => {
    let around = '';
    server.use(
      http.get('*/api/v1/channels/c1/messages', ({ request }) => {
        around = new URL(request.url).searchParams.get('around') ?? '';
        return HttpResponse.json([
          { ...hit, id: '51', content: 'older one' },
          { ...hit, id: '50', content: 'far away target', hit: undefined },
        ]);
      }),
    );
    at('/channels/g1/c1/50');
    expect(await screen.findByText('far away target')).toBeInTheDocument();
    expect(around).toBe('50');
    expect(useStore.getState().windows.c1?.atLatest).toBe(false);
  });
});

describe('history after READY', () => {
  it('reloads the open channel when READY wipes the cached history', async () => {
    seed('1');
    quiet();
    useStore.setState({ windows: {}, messages: {} });
    let fetches = 0;
    server.use(
      http.get('*/api/v1/channels/c1/messages', () => {
        fetches += 1;
        return HttpResponse.json([
          {
            id: '100',
            channel_id: 'c1',
            author: { id: '2', username: 'bob' },
            content: 'kept history',
            timestamp: '2026-01-01T00:00:00Z',
          },
        ]);
      }),
    );
    at('/channels/g1/c1');
    expect(await screen.findByText('kept history')).toBeInTheDocument();
    act(() =>
      useStore
        .getState()
        .applyDispatch('READY', { user: me, guilds: [{ id: 'g1' }], private_channels: [] }),
    );
    await waitFor(() => expect(fetches).toBe(2));
    expect(await screen.findByText('kept history')).toBeInTheDocument();
  });
});
