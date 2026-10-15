import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';

// The gateway is not under test; the store is seeded directly.
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob' };

/** @everyone: VIEW_CHANNEL | SEND_MESSAGES | CREATE_EVENTS. */
const EVERYONE = ((1n << 10n) | (1n << 11n) | (1n << 44n)).toString();

function seed(ownerId = '2', everyone = EVERYONE) {
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
      v1: { id: 'v1', type: 2, guild_id: 'g1', name: 'Movie room', position: 2 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'v1'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: everyone,
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
        ids: [],
        pending: [],
        hasMoreBefore: false,
        atLatest: true,
        loading: false,
        error: null,
      },
    },
    messages: {},
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
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
  );

const event = (over: Record<string, unknown> = {}) => ({
  id: 'e1',
  guild_id: 'g1',
  channel_id: 'v1',
  creator_id: '1',
  creator: { id: '1', username: 'me' },
  name: 'Movie night',
  description: 'Bring snacks',
  scheduled_start_time: '2030-01-01T20:00:00+00:00',
  scheduled_end_time: null,
  privacy_level: 2,
  status: 1,
  entity_type: 2,
  entity_id: null,
  entity_metadata: null,
  user_count: 2,
  user_rsvp: false,
  image: null,
  ...over,
});

describe('group DM invites', () => {
  const group = {
    type: 1,
    code: 'grp12345',
    channel: { id: 'dm1', name: 'The crew', type: 3 },
    inviter: { id: '2', username: 'bob' },
    uses: 0,
    max_uses: 0,
    max_age: 86400,
    temporary: false,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: '2026-01-02T00:00:00Z',
    approximate_member_count: 3,
  };

  it('the invite page shows the group, joins it and opens it under Direct Messages', async () => {
    seed();
    quiet();
    server.use(
      http.get('*/api/v1/invites/grp12345', () => HttpResponse.json(group)),
      http.post('*/api/v1/invites/grp12345', () => HttpResponse.json(group)),
    );
    const router = at('/invite/grp12345');
    expect(await screen.findByRole('heading', { name: 'The crew' })).toBeInTheDocument();
    expect(screen.getByText(/invited to join a group chat/)).toBeInTheDocument();
    expect(screen.getByText('3 members')).toBeInTheDocument();
    await userEvent.click(await screen.findByRole('button', { name: 'Accept invite' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/dm1'));
  });

  it('an invite card in chat joins a group without a server', async () => {
    seed();
    quiet();
    server.use(
      http.get('*/api/v1/invites/grp12345', () => HttpResponse.json(group)),
      http.post('*/api/v1/invites/grp12345', () => HttpResponse.json(group)),
    );
    useStore.setState({
      messages: {
        '100': {
          id: '100',
          channel_id: 'c1',
          author: bob,
          content: 'join us http://localhost/invite/grp12345',
          timestamp: '2026-01-01T00:00:00Z',
        },
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
    });
    const router = at('/channels/g1/c1');
    const card = await screen.findByRole('region', { name: 'Invite to The crew' });
    await userEvent.click(within(card).getByRole('button', { name: 'Join' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/dm1'));
  });
});

describe('Events', () => {
  beforeEach(() => {
    seed('2');
    quiet();
  });

  it('lists events from the channel list row and marks you interested', async () => {
    let interested = false;
    const puts: string[] = [];
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () =>
        HttpResponse.json([
          event({ user_rsvp: interested, user_count: interested ? 3 : 2 }),
          event({
            id: 'e2',
            name: 'Park meetup',
            channel_id: null,
            entity_type: 3,
            entity_metadata: { location: 'the park' },
            creator_id: '2',
            description: null,
          }),
        ]),
      ),
      http.put('*/api/v1/guilds/g1/scheduled-events/e1/users/@me', () => {
        interested = true;
        puts.push('put');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    // The Events row at the top of the channel list shows how many there are.
    const row = await screen.findByRole('button', { name: '2 events' });
    await userEvent.click(row);
    const dialog = await screen.findByRole('dialog', { name: 'Events' });
    const list = within(dialog).getByRole('list', { name: 'Upcoming events' });
    expect(within(list).getByText('Movie night')).toBeInTheDocument();
    expect(within(list).getByText('Movie room')).toBeInTheDocument();
    expect(within(list).getByText('the park')).toBeInTheDocument();
    // Own events show Start, Cancel and Delete; someone else's only Interested.
    const mine = within(list).getByText('Movie night').closest('li')!;
    expect(within(mine).getByRole('button', { name: 'Start' })).toBeInTheDocument();
    expect(within(mine).getByRole('button', { name: 'Delete Movie night' })).toBeInTheDocument();
    const theirs = within(list).getByText('Park meetup').closest('li')!;
    expect(within(theirs).queryByRole('button', { name: /Delete/ })).toBeNull();
    expect(within(theirs).queryByRole('button', { name: 'Start' })).toBeNull();

    await userEvent.click(within(mine).getByRole('button', { name: 'Mark interested' }));
    await waitFor(() => expect(puts).toEqual(['put']));
    await waitFor(() =>
      expect(within(list).getAllByRole('button', { name: 'Interested' })[0]).toHaveAttribute(
        'aria-pressed',
        'true',
      ),
    );
  });

  it('Start, End and Delete call the event routes', async () => {
    const calls: string[] = [];
    let status = 1;
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () =>
        HttpResponse.json(calls.includes('delete') ? [] : [event({ status })]),
      ),
      http.patch('*/api/v1/guilds/g1/scheduled-events/e1', async ({ request }) => {
        const body = (await request.json()) as { status: number };
        calls.push(`patch ${body.status}`);
        status = body.status;
        return HttpResponse.json(event({ status }));
      }),
      http.delete('*/api/v1/guilds/g1/scheduled-events/e1', () => {
        calls.push('delete');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: '1 event' }));
    const dialog = await screen.findByRole('dialog', { name: 'Events' });
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Start' }));
    await within(dialog).findByText('Live now');
    await userEvent.click(within(dialog).getByRole('button', { name: 'End' }));
    await waitFor(() => expect(calls).toEqual(['patch 2', 'patch 3']));
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Delete Movie night' }));
    await within(dialog).findByText('No events yet');
    expect(calls.at(-1)).toBe('delete');
  });

  it('Create Event validates, then posts a voice event with the picked channel', async () => {
    const bodies: unknown[] = [];
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/scheduled-events', async ({ request }) => {
        bodies.push(await request.json());
        return HttpResponse.json(event());
      }),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    act(() => useLoungeUi.getState().openDialog({ kind: 'createEvent', guildId: 'g1' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Event' });
    // No topic yet.
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Event' }));
    expect(await within(dialog).findByText('Give the event a topic.')).toBeInTheDocument();
    expect(bodies).toEqual([]);
    // A start time in the past.
    await userEvent.type(within(dialog).getByLabelText('Event topic'), 'Movie night');
    const start = within(dialog).getByLabelText('Starts');
    await userEvent.clear(start);
    await userEvent.type(start, '2001-01-01T10:00');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Event' }));
    expect(
      await within(dialog).findByText('The event has to start in the future.'),
    ).toBeInTheDocument();
    // A good one.
    await userEvent.clear(start);
    await userEvent.type(start, '2099-05-05T20:00');
    const end = within(dialog).getByLabelText('Ends (optional)');
    await userEvent.clear(end);
    await userEvent.type(end, '2099-05-05T22:00');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Event' }));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toMatchObject({
      name: 'Movie night',
      entity_type: 2,
      channel_id: 'v1',
      privacy_level: 2,
    });
    expect((bodies[0] as { scheduled_start_time: string }).scheduled_start_time).toMatch(
      /^2099-05-0[56]T/,
    );
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Create Event' })).toBeNull());
  });

  it('the server menu offers Create Event only to people who may', async () => {
    seed('2', ((1n << 10n) | (1n << 11n)).toString());
    quiet();
    server.use(http.get('*/api/v1/guilds/g1/scheduled-events', () => HttpResponse.json([])));
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Store Guild/ }));
    expect(screen.queryByRole('menuitem', { name: 'Create Event' })).toBeNull();
  });
});
