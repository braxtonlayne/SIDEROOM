import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';
import { connectionMessage } from '../shell/Banners';
import { useNotifSettings } from '../shell/notifSettings';
import { useSelfPresence } from '../shell/selfPresence';
import { usePlatformStaff } from '../shell/platformStaff';
import { useSocial } from '../shell/social';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob B' };
const cat: S.User = { id: '3', username: 'cat' };
const dan: S.User = { id: '4', username: 'dan' };
const EVERYONE = String((1 << 10) | (1 << 11) | 1);

function seed() {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useNotifSettings.setState({ byGuild: {} });
  useSocial.getState().reset();
  useSelfPresence.setState({ status: 'online', customText: null, loaded: false });
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob, '3': cat },
    guilds: { g1: { id: 'g1', name: 'Alpha Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels: {
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', position: 0 },
      dm1: { id: 'dm1', type: 1, recipients: [bob], last_message_id: '200' },
    },
    channelIdsByGuild: { g1: ['c1'], '@me': ['dm1'] },
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
    memberIdsByGuild: { g1: ['1'] },
    members: { 'g1:1': { guild_id: 'g1', user_id: '1', roles: [] } },
    presences: { '2': { user_id: '2', status: 'online', activities: [] } },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const rel = (u: S.User, type: number) => ({
  id: u.id,
  type,
  nickname: null,
  since: '2026-01-01T00:00:00Z',
  user: u,
});

const quiet = (rels: unknown[] = [rel(bob, 1), rel(cat, 1), rel(dan, 3)]) =>
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/relationships', () => HttpResponse.json(rels)),
  );

describe('Friends page', () => {
  beforeEach(() => {
    seed();
    quiet();
  });

  it('shows online friends first, all friends on the All tab and filters by search', async () => {
    at('/channels/@me');
    const online = await screen.findByRole('list', { name: 'Online' });
    expect(within(online).getByText('Bob B')).toBeInTheDocument();
    expect(within(online).queryByText('cat')).toBeNull();
    expect(screen.getByRole('heading', { name: 'Online — 1' })).toBeInTheDocument();

    await userEvent.click(screen.getByRole('tab', { name: 'All' }));
    const all = await screen.findByRole('list', { name: 'All friends' });
    expect(within(all).getAllByText('cat').length).toBeGreaterThan(0);
    await userEvent.type(screen.getByLabelText('Search friends'), 'ca');
    expect(
      within(screen.getByRole('list', { name: 'All friends' })).queryByText('Bob B'),
    ).toBeNull();
    await userEvent.clear(screen.getByLabelText('Search friends'));
    await userEvent.type(screen.getByLabelText('Search friends'), 'zzz');
    expect(await screen.findByText('No friends match that search')).toBeInTheDocument();
  });

  it('badges Pending with the incoming count and accepts a request', async () => {
    let put: unknown;
    server.use(
      http.put('*/api/v1/users/@me/relationships/4', async ({ request }) => {
        put = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me');
    const tab = await screen.findByRole('tab', { name: /Pending/ });
    expect(tab).toHaveTextContent('1');
    // The sidebar badge counts it too.
    expect(await screen.findByLabelText(/1 pending friend requests/)).toBeInTheDocument();
    await userEvent.click(tab);
    const incoming = await screen.findByRole('list', { name: 'Incoming friend requests' });
    expect(within(incoming).getByText('Incoming Friend Request')).toBeInTheDocument();
    let fetches = 0;
    server.use(
      http.get('*/api/v1/users/@me/relationships', () => {
        fetches += 1;
        return HttpResponse.json([rel(bob, 1), rel(cat, 1), rel(dan, 1)]);
      }),
    );
    await userEvent.click(within(incoming).getByRole('button', { name: 'Accept dan' }));
    await waitFor(() => expect(put).toEqual({ type: 1 }));
    // The gateway tells us it worked (D-183); the page does not re-fetch.
    act(() => useStore.getState().applyDispatch('RELATIONSHIP_ADD', rel(dan, 1)));
    expect(fetches).toBe(0);
    await waitFor(() =>
      expect(screen.getByText(/no pending friend requests/i)).toBeInTheDocument(),
    );
  });

  it('applies RELATIONSHIP_* dispatches live: a request from another session appears without reload', async () => {
    at('/channels/@me');
    const tab = await screen.findByRole('tab', { name: /Pending/ });
    await userEvent.click(tab);
    const incoming = await screen.findByRole('list', { name: 'Incoming friend requests' });
    expect(within(incoming).queryByText('eve')).toBeNull();
    const eve: S.User = { id: '9', username: 'eve' };
    act(() => useStore.getState().applyDispatch('RELATIONSHIP_ADD', rel(eve, 3)));
    expect(await within(incoming).findByText('eve')).toBeInTheDocument();
    expect(await screen.findByLabelText(/2 pending friend requests/)).toBeInTheDocument();
    // A nickname change and a removal also land live.
    act(() =>
      useStore
        .getState()
        .applyDispatch('RELATIONSHIP_UPDATE', { id: '9', type: 3, nickname: 'Evie' }),
    );
    act(() => useStore.getState().applyDispatch('RELATIONSHIP_REMOVE', { id: '9', type: 3 }));
    await waitFor(() => expect(within(incoming).queryByText('eve')).toBeNull());
    expect(await screen.findByLabelText(/1 pending friend requests/)).toBeInTheDocument();
  });

  it('a new friend dispatch updates the All tab and a DM request lands in message requests', async () => {
    at('/channels/@me');
    await screen.findByRole('list', { name: 'Online' });
    act(() =>
      useStore.getState().applyDispatch('CHANNEL_CREATE', {
        id: 'dm9',
        type: 1,
        recipients: [dan],
        is_message_request: true,
      }),
    );
    // Requests stay out of the DM list...
    expect(useStore.getState().channels.dm9).toBeUndefined();
    expect(useSocial.getState().messageRequests.map((d) => d.id)).toEqual(['dm9']);
    // ...until accepted (the server re-sends CHANNEL_CREATE), and CHANNEL_DELETE closes a DM.
    act(() =>
      useStore.getState().applyDispatch('CHANNEL_CREATE', {
        id: 'dm9',
        type: 1,
        recipients: [dan],
        is_message_request: false,
      }),
    );
    expect(useSocial.getState().messageRequests).toEqual([]);
    expect(useStore.getState().channels.dm9).toBeDefined();
    act(() => useStore.getState().applyDispatch('CHANNEL_DELETE', { id: 'dm9', type: 1 }));
    expect(useStore.getState().channels.dm9).toBeUndefined();
    expect(useStore.getState().channelIdsByGuild['@me']).toEqual(['dm1']);
  });

  it('shows the empty pending state', async () => {
    quiet([rel(bob, 1)]);
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('tab', { name: 'Pending' }));
    expect(await screen.findByText('There are no pending friend requests')).toBeInTheDocument();
  });

  it('adds a friend by username and reports a refusal', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/users/@me/relationships', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('tab', { name: 'Add Friend' }));
    await userEvent.type(screen.getByLabelText('Username'), '@zed');
    await userEvent.click(screen.getByRole('button', { name: 'Send Friend Request' }));
    expect(await screen.findByText(/request to zed was sent/)).toBeInTheDocument();
    expect(body).toEqual({ username: 'zed' });

    server.use(
      http.post('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json({ message: 'Unable to add' }, { status: 400 }),
      ),
    );
    await userEvent.type(screen.getByLabelText('Username'), 'nobody');
    await userEvent.click(screen.getByRole('button', { name: 'Send Friend Request' }));
    expect(await screen.findByText(/Double check that the username/)).toBeInTheDocument();
  });

  it('opens a DM from the Message button', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/users/@me/channels', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ id: 'dm1', type: 1, recipients: [bob], last_message_id: '200' });
      }),
    );
    const router = at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: 'Message Bob B' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/dm1'));
    expect(body).toEqual({ recipient_id: '2' });
  });

  it('removes a friend from the row menu and dismisses the info banner for good', async () => {
    let removed = 0;
    server.use(
      http.delete('*/api/v1/users/@me/relationships/2', () => {
        removed += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me');
    await screen.findByRole('list', { name: 'Online' });
    await userEvent.click(screen.getByRole('button', { name: 'More actions for Bob B' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Remove Friend' }));
    await waitFor(() => expect(removed).toBe(1));
    const info = screen.getByText(/blocked or ignored/).closest('[role="status"]') as HTMLElement;
    await userEvent.click(within(info).getByRole('button', { name: 'Dismiss' }));
    expect(screen.queryByText(/blocked or ignored/)).toBeNull();
    expect(useLoungeUi.getState().dismissedBanners['friends-blocked']).toBe(true);
  });

  it('shows friends who are doing something under Active Now', async () => {
    useStore.setState({
      presences: {
        '2': {
          user_id: '2',
          status: 'online',
          activities: [{ type: 3, name: 'Spirited Away' }],
        },
      },
    });
    at('/channels/@me');
    const aside = await screen.findByRole('complementary', { name: 'Active Now' });
    expect(await within(aside).findByText('Watching Spirited Away')).toBeInTheDocument();
  });

  it('says it is quiet when nobody is active', async () => {
    at('/channels/@me');
    const aside = await screen.findByRole('complementary', { name: 'Active Now' });
    expect(within(aside).getByText(/quiet for now/)).toBeInTheDocument();
  });

  it('offers a retry when friends fail to load', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json({ message: 'boom' }, { status: 500 }),
      ),
    );
    at('/channels/@me');
    expect(await screen.findByText(/Could not load your friends/)).toBeInTheDocument();
  });
});

describe('Create DM', () => {
  beforeEach(() => {
    seed();
    quiet([rel(bob, 1), rel(cat, 1)]);
  });

  it('creates a group DM from several friends', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/users/@me/channels', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          id: 'dm9',
          type: 3,
          recipients: [bob, cat],
          last_message_id: '300',
        });
      }),
    );
    const router = at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: 'Create DM' }));
    const dialog = await screen.findByRole('dialog', { name: 'Select Friends' });
    await userEvent.click(await within(dialog).findByRole('checkbox', { name: /Bob B/ }));
    expect(within(dialog).getByRole('button', { name: 'Create DM' })).toBeEnabled();
    await userEvent.click(within(dialog).getByRole('checkbox', { name: /cat/ }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Group DM' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/dm9'));
    expect(body).toEqual({ recipients: ['2', '3'] });
  });

  it('is disabled with nobody picked and shows the empty state without friends', async () => {
    quiet([]);
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: 'Create DM' }));
    const dialog = await screen.findByRole('dialog', { name: 'Select Friends' });
    expect(await within(dialog).findByText('You have no friends to add yet')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Create DM' })).toBeDisabled();
  });
});

describe('Message Requests', () => {
  const request = { id: 'dm7', type: 1, recipients: [dan], is_message_request: true };

  beforeEach(() => {
    seed();
    quiet([rel(bob, 1)]);
    useStore.setState({ messageRequestDms: [request] });
  });

  it('counts requests, lists them and accepts one', async () => {
    let state: unknown;
    server.use(
      http.put('*/api/v1/users/@me/channels/dm7/request', async ({ request: r }) => {
        state = await r.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const router = at('/channels/@me');
    const nav = await screen.findByRole('button', { name: /Message Requests/ });
    await waitFor(() => expect(nav).toHaveTextContent('1'));
    await userEvent.click(nav);
    const list = await screen.findByRole('list', { name: 'Message requests' });
    expect(within(list).getByText('dan')).toBeInTheDocument();
    await userEvent.click(within(list).getByRole('button', { name: 'Accept request from dan' }));
    await waitFor(() => expect(state).toEqual({ state: 'accepted' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/dm7'));
  });

  it('ignores a request and shows the empty state', async () => {
    server.use(
      http.put(
        '*/api/v1/users/@me/channels/dm7/request',
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Message Requests/ }));
    await userEvent.click(await screen.findByRole('button', { name: 'Ignore request from dan' }));
    expect(await screen.findByText('No message requests')).toBeInTheDocument();
  });
});

describe('DM list', () => {
  beforeEach(() => {
    seed();
    quiet([rel(bob, 1)]);
  });

  it('names DMs from the full recipients in READY, without fetching the DM list', async () => {
    let fetched = 0;
    server.use(
      http.get('*/api/v1/users/@me/channels', () => {
        fetched += 1;
        return HttpResponse.json([]);
      }),
    );
    useStore.getState().applyDispatch('READY', {
      user: me,
      guilds: [{ id: 'g1' }],
      private_channels: [
        { id: 'dm1', type: 1, recipients: [bob], last_message_id: '200' },
        { id: 'dm2', type: 1, recipients: [cat], last_message_id: '300' },
        { id: 'dm9', type: 1, recipients: [dan], is_message_request: true },
      ],
    });
    useStore.setState({ connection: 'connected' });
    at('/channels/@me');
    const sidebar = await screen.findByTestId('home-sidebar');
    expect(await within(sidebar).findByRole('link', { name: /^cat/ })).toBeInTheDocument();
    expect(within(sidebar).getByRole('link', { name: /Bob B/ })).toBeInTheDocument();
    // The request stays out of the DM list and fills Message Requests.
    expect(within(sidebar).queryByRole('link', { name: /dan/ })).toBeNull();
    expect(useSocial.getState().messageRequests.map((d) => d.id)).toEqual(['dm9']);
    expect(fetched).toBe(0);
  });

  it('shows a group DM with a member count and an avatar collage', async () => {
    useStore.setState({
      channels: {
        ...useStore.getState().channels,
        dm3: { id: 'dm3', type: 3, recipients: [bob, cat], last_message_id: '400' },
      },
      channelIdsByGuild: { ...useStore.getState().channelIdsByGuild, '@me': ['dm1', 'dm3'] },
    });
    at('/channels/@me');
    const sidebar = await screen.findByTestId('home-sidebar');
    const row = await within(sidebar).findByRole('link', { name: /Bob B, cat/ });
    expect(within(row).getByText('3 Members')).toBeInTheDocument();
    expect(row.querySelector('[role="group"]')).not.toBeNull();
    expect(within(sidebar).getByRole('button', { name: /Leave group/ })).toBeInTheDocument();
    expect(await axeViolations(sidebar)).toEqual([]);
  });

  it('closes a DM with the hover close button', async () => {
    let closed = 0;
    server.use(
      http.delete('*/api/v1/users/@me/channels/dm1', () => {
        closed += 1;
        return HttpResponse.json({ id: 'dm1', type: 1 });
      }),
    );
    at('/channels/@me');
    const sidebar = await screen.findByTestId('home-sidebar');
    await userEvent.click(
      await within(sidebar).findByRole('button', { name: 'Close DM with Bob B' }),
    );
    await waitFor(() => expect(closed).toBe(1));
    await waitFor(() => expect(useStore.getState().channels.dm1).toBeUndefined());
  });

  it('mutes a conversation from its context menu', async () => {
    let body: unknown;
    server.use(
      http.patch('*/api/v1/users/@me/guilds/@me/settings', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          muted: false,
          channel_overrides: [
            { channel_id: 'dm1', muted: true, mute_config: { end_time: '2999-01-01T00:00:00Z' } },
          ],
        });
      }),
    );
    at('/channels/@me');
    const sidebar = await screen.findByTestId('home-sidebar');
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: await within(sidebar).findByRole('link', { name: /Bob B/ }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Mute Conversation' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'For 8 Hours' }));
    await waitFor(() =>
      expect(body).toEqual({
        channel_overrides: { dm1: { muted: true, mute_config: { selected_time_window: 28800 } } },
      }),
    );
    expect(await within(sidebar).findByRole('link', { name: /Bob B.*muted/ })).toBeInTheDocument();
  });
});

describe('status picker', () => {
  beforeEach(() => {
    seed();
    quiet();
  });

  it('sets Do Not Disturb for an hour', async () => {
    let body: unknown;
    server.use(
      http.patch('*/api/v1/users/@me/presence', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ status: 'dnd', custom_status: null });
      }),
    );
    at('/channels/@me');
    await userEvent.click(
      await screen.findByRole('button', { name: /Set status, currently Online/ }),
    );
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Do Not Disturb' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'For 1 Hour' }));
    await waitFor(() => expect(body).toEqual({ status: 'dnd', duration_seconds: 3600 }));
    expect(
      await screen.findByRole('button', { name: /Set status, currently Do Not Disturb/ }),
    ).toBeInTheDocument();
    expect(useStore.getState().presences['1']?.status).toBe('dnd');
  });

  it('shows Invisible to the person but offline in the store', async () => {
    server.use(
      http.patch('*/api/v1/users/@me/presence', () =>
        HttpResponse.json({ status: 'invisible', custom_status: null }),
      ),
    );
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Set status/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Invisible' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Forever' }));
    expect(await screen.findByRole('button', { name: /currently Invisible/ })).toBeInTheDocument();
    expect(useStore.getState().presences['1']?.status).toBe('offline');
  });

  it('sets and shows a custom status', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.patch('*/api/v1/users/@me/presence', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ status: 'online', custom_status: { text: 'Reading manga' } });
      }),
    );
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Set status/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Set Custom Status' }));
    const dialog = await screen.findByRole('dialog', { name: 'Set a custom status' });
    await userEvent.type(within(dialog).getByLabelText("What's happening?"), 'Reading manga');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(body).toEqual({ custom_status: { text: 'Reading manga' } }));
    expect(await screen.findByText('Reading manga')).toBeInTheDocument();
  });
});

describe('connection banner', () => {
  it('maps gateway and network state to copy', () => {
    expect(connectionMessage('connected', true)).toBeNull();
    expect(connectionMessage('idle', true)).toBeNull();
    expect(connectionMessage('connecting', true)?.text).toMatch(/Connecting/);
    expect(connectionMessage('reconnecting', true)?.text).toMatch(/Reconnecting/);
    expect(connectionMessage('closed', true)?.tone).toBe('warn');
    expect(connectionMessage('connected', false)?.text).toMatch(/offline/);
  });

  it('shows Reconnecting in the shell and clears when connected', async () => {
    seed();
    quiet();
    useStore.setState({ connection: 'reconnecting' });
    at('/channels/@me');
    expect(await screen.findByText('Reconnecting…')).toBeInTheDocument();
    useStore.setState({ connection: 'connected' });
    await waitFor(() => expect(screen.queryByText('Reconnecting…')).toBeNull());
  });
});

describe('notices bar', () => {
  it('asks unverified accounts to verify and can resend', async () => {
    seed();
    quiet();
    let resent = 0;
    server.use(
      http.get('*/api/v1/users/@me', () =>
        HttpResponse.json({ id: '1', username: 'me', verified: false }),
      ),
      http.post('*/api/v1/sr/auth/verify-email/resend', () => {
        resent += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me');
    const bar = await screen.findByRole('region', { name: 'Notices' });
    await userEvent.click(within(bar).getByRole('button', { name: 'Resend link' }));
    expect(await within(bar).findByText('Check your inbox for the link.')).toBeInTheDocument();
    expect(resent).toBe(1);
    await userEvent.click(within(bar).getByRole('button', { name: 'Dismiss notice' }));
    expect(screen.queryByRole('region', { name: 'Notices' })).toBeNull();
  });
});

describe('Home accessibility', () => {
  it('friends page, message requests and the create DM dialog have no axe violations', async () => {
    seed();
    quiet([rel(bob, 1), rel(dan, 3)]);
    useStore.setState({
      messageRequestDms: [{ id: 'dm7', type: 1, recipients: [dan], is_message_request: true }],
    });
    at('/channels/@me');
    const list = await screen.findByRole('list', { name: 'Online' });
    const section = list.closest('section')!;
    expect(await axeViolations(section)).toEqual([]);
    await userEvent.click(screen.getByRole('tab', { name: /Pending/ }));
    await screen.findByRole('list', { name: 'Incoming friend requests' });
    expect(await axeViolations(section)).toEqual([]);
    await userEvent.click(screen.getByRole('tab', { name: 'Add Friend' }));
    expect(await axeViolations(section)).toEqual([]);
    await userEvent.click(await screen.findByRole('button', { name: /Message Requests/ }));
    const mr = await screen.findByRole('region', { name: 'Message Requests' });
    await within(mr).findByRole('list', { name: 'Message requests' });
    expect(await axeViolations(mr)).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Create DM' }));
    const dialog = await screen.findByRole('dialog', { name: 'Select Friends' });
    await within(dialog).findByRole('checkbox', { name: /Bob B/ });
    expect(await axeViolations(dialog)).toEqual([]);
  });
});

describe('platform admin entry points (ADM-STF-010, ADM-STF-011)', () => {
  beforeEach(() => {
    seed();
    quiet();
    usePlatformStaff.setState({ staff: false, loaded: false });
  });

  it('shows Platform Admin in the user menu to staff and opens /admin', async () => {
    server.use(
      http.get('*/api/v1/admin/me', () =>
        HttpResponse.json({ is_owner: false, platform_permissions: '1', user_id: '1' }),
      ),
    );
    const router = at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Set status/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Platform Admin' }));
    await waitFor(() => expect(router.state.location.pathname).toMatch(/^\/admin/));
  });

  it('does not show it to people who are not staff', async () => {
    server.use(
      http.get('*/api/v1/admin/me', () =>
        HttpResponse.json({ message: 'Missing permissions' }, { status: 403 }),
      ),
    );
    at('/channels/@me');
    await userEvent.click(await screen.findByRole('button', { name: /Set status/ }));
    await screen.findByRole('menuitem', { name: 'Invisible' });
    await waitFor(() => expect(usePlatformStaff.getState().loaded).toBe(true));
    expect(screen.queryByRole('menuitem', { name: 'Platform Admin' })).toBeNull();
  });

  it('frames the Staff Server in the rail and offers Platform Admin in its server menu', async () => {
    server.use(
      http.get('*/api/v1/admin/me', () =>
        HttpResponse.json({ is_owner: false, platform_permissions: '1', user_id: '1' }),
      ),
    );
    useStore.setState({
      guilds: { g1: { id: 'g1', name: 'Staff Server', owner_id: '1', is_staff_guild: true } },
    });
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: /servers/i });
    expect(await within(rail).findByText('STAFF')).toBeInTheDocument();
    await userEvent.click(await screen.findByRole('button', { name: 'Staff Server' }));
    expect(await screen.findByRole('menuitem', { name: 'Platform Admin' })).toBeInTheDocument();
  });
});
