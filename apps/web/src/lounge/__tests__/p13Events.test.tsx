import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';
import { emitDispatch } from '../../store/events';

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
  description: null,
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

const HASH = '0123456789abcdef0123456789abcdef';
const png = (name = 'cover.png', size?: number) => {
  const f = new File([new Uint8Array([137, 80, 78, 71])], name, { type: 'image/png' });
  if (size !== undefined) Object.defineProperty(f, 'size', { value: size });
  return f;
};

describe('Event covers and start notifications (G-93)', () => {
  beforeEach(() => {
    seed('2');
    quiet();
  });

  it('Create Event sends the picked cover as a data URI', async () => {
    const bodies: Array<Record<string, unknown>> = [];
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () => HttpResponse.json([])),
      http.post('*/api/v1/guilds/g1/scheduled-events', async ({ request }) => {
        bodies.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(event());
      }),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    act(() => useLoungeUi.getState().openDialog({ kind: 'createEvent', guildId: 'g1' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Event' });
    await userEvent.type(within(dialog).getByLabelText('Event topic'), 'Movie night');
    const start = within(dialog).getByLabelText('Starts');
    await userEvent.clear(start);
    await userEvent.type(start, '2099-05-05T20:00');
    const end = within(dialog).getByLabelText('Ends (optional)');
    await userEvent.clear(end);
    await userEvent.type(end, '2099-05-05T22:00');
    await userEvent.upload(within(dialog).getByTestId('event-cover-file'), png());
    expect(await within(dialog).findByRole('img', { name: 'Cover image preview' })).toBeVisible();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Event' }));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(String(bodies[0]!.image)).toMatch(/^data:image\/png;base64,/);
  });

  it('refuses a cover that is not JPEG, PNG or GIF or is over 2 MiB, before any request', async () => {
    server.use(http.get('*/api/v1/guilds/g1/scheduled-events', () => HttpResponse.json([])));
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    act(() => useLoungeUi.getState().openDialog({ kind: 'createEvent', guildId: 'g1' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create Event' });
    const input = within(dialog).getByTestId('event-cover-file');
    const user = userEvent.setup({ applyAccept: false });
    await user.upload(input, new File(['x'], 'cover.webp', { type: 'image/webp' }));
    expect(
      await within(dialog).findByText('cover.webp is not a JPEG, PNG or GIF image.'),
    ).toBeVisible();
    await user.upload(input, png('big.png', 3 * 1024 * 1024));
    expect(await within(dialog).findByText('big.png is larger than 2 MiB.')).toBeVisible();
    expect(within(dialog).queryByRole('img', { name: 'Cover image preview' })).toBeNull();
  });

  it('the Events list shows covers and a manager can remove one', async () => {
    const patches: unknown[] = [];
    let image: string | null = HASH;
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () => HttpResponse.json([event({ image })])),
      http.patch('*/api/v1/guilds/g1/scheduled-events/e1', async ({ request }) => {
        patches.push(await request.json());
        image = null;
        return HttpResponse.json(event({ image }));
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: '1 event' }));
    const dialog = await screen.findByRole('dialog', { name: 'Events' });
    const cover = await within(dialog).findByRole('img', { name: 'Cover image of Movie night' });
    expect(cover.getAttribute('src')).toContain(`/cdn/guild-events/e1/${HASH}.png`);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cover' }));
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Remove cover' }));
    await waitFor(() => expect(patches).toEqual([{ image: null }]));
    await waitFor(() =>
      expect(within(dialog).queryByRole('img', { name: 'Cover image of Movie night' })).toBeNull(),
    );
  });

  it('a notification link opens the Events dialog on the event and clears the parameter', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () =>
        HttpResponse.json([event(), event({ id: 'e2', name: 'Park meetup' })]),
      ),
    );
    const router = at('/channels/g1/c1?event=e2');
    const dialog = await screen.findByRole('dialog', { name: 'Events' });
    const marked = (await within(dialog).findByText('Park meetup')).closest('li')!;
    expect(marked).toHaveAttribute('data-focus', 'true');
    await waitFor(() => expect(router.state.location.search).not.toHaveProperty('event'));
  });

  it('tells you in the app when an event you are interested in starts', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/scheduled-events', () =>
        HttpResponse.json([event({ status: 2 })]),
      ),
    );
    at('/channels/g1/c1');
    await screen.findByTestId('channel-sidebar');
    act(() =>
      emitDispatch('SR_SCHEDULED_EVENT_START', {
        guild_id: 'g1',
        guild_scheduled_event_id: 'e1',
        name: 'Movie night',
      }),
    );
    expect(await screen.findByText('Movie night is starting now')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'View event' }));
    expect(await screen.findByRole('dialog', { name: 'Events' })).toBeInTheDocument();
  });
});
