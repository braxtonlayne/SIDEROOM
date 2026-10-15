import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { http, HttpResponse, server } from '../../test/server';
import { usePopout } from '../profile/popoutStore';
import { resetProfiles } from '../profile/useProfile';
import { useIgnores } from '../shell/ignores';
import { useSocial } from '../shell/social';
import { useLoungeUi } from '../store';
import { at, baseHandlers, calls, me, record } from './gapsHelpers';

const startDmCall = vi.hoisted(() => vi.fn());
vi.mock('../shell/dmCall', () => ({ startDmCall }));
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const bob: S.User = { id: '2', username: 'bob', global_name: 'Bob' };
const cat: S.User = { id: '3', username: 'cat', global_name: 'Cat' };
const dan: S.User = { id: '4', username: 'dan', global_name: 'Dan' };

const profile = {
  user: { id: '2', username: 'bob', global_name: 'Bob', avatar: null, banner: null },
  user_profile: { bio: 'Trains.', pronouns: 'he/him', banner: null, accent_color: null },
  badges: [{ id: 'staff' }],
  created_at: '2024-03-04T12:00:00Z',
  connected_accounts: [],
  mutual_friends: [{ id: '5', username: 'dana' }],
  mutual_friends_count: 1,
  mutual_guilds: [{ id: 'g1', nick: null, member_since: '2025-01-10T12:00:00Z' }],
};

function seedDms(extra: Record<string, S.Message> = {}, order: string[] = []) {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useSocial.getState().reset();
  useIgnores.getState().reset();
  usePopout.setState({ profile: null, full: null });
  resetProfiles();
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob, '3': cat, '4': dan },
    guilds: { g1: { id: 'g1', name: 'Store Guild', owner_id: '1' } },
    guildOrder: ['g1'],
    channels: {
      dm1: { id: 'dm1', type: 1, recipients: [bob], last_message_id: '200' },
      grp: {
        id: 'grp',
        type: 3,
        name: 'Trio',
        recipients: [bob, cat],
        owner_id: '1',
        last_message_id: '201',
      },
    },
    channelIdsByGuild: { '@me': ['dm1', 'grp'] },
    roles: {},
    roleIdsByGuild: {},
    memberIdsByGuild: {},
    members: {},
    windows: Object.fromEntries(
      ['dm1', 'grp'].map((id) => [
        id,
        {
          ids: order,
          pending: [],
          hasMoreBefore: false,
          atLatest: true,
          loading: false,
          error: null,
        },
      ]),
    ),
    messages: extra,
    presences: { '2': { user_id: '2', status: 'online', activities: [] } },
    connection: 'connected',
  });
}

function handlers() {
  baseHandlers();
  server.use(
    http.get('*/api/v1/users/2/profile', () => HttpResponse.json(profile)),
    http.get('*/api/v1/users/@me/notes/2', () => HttpResponse.json({ note: '' })),
    http.get('*/api/v1/users/@me/relationships', () =>
      HttpResponse.json([
        { id: '2', type: 1, since: '2025-02-03T12:00:00Z', user: { id: '2', username: 'bob' } },
        { id: '3', type: 1, since: '2025-02-03T12:00:00Z', user: { id: '3', username: 'cat' } },
        { id: '4', type: 1, since: '2025-02-03T12:00:00Z', user: { id: '4', username: 'dan' } },
      ]),
    ),
  );
}

describe('DM header, welcome and panel', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    seedDms();
    handlers();
    startDmCall.mockReset();
  });

  it('LNG-DM-004: header has identity, calls, pins, add friends, panel toggle and search', async () => {
    const { container } = at('/channels/@me/dm1');
    const section = await screen.findByRole('region', { name: 'Bob' });
    const header = within(section).getByRole('banner');
    await userEvent.click(within(header).getByRole('button', { name: 'Start Voice Call' }));
    expect(startDmCall).toHaveBeenLastCalledWith('dm1', 'Bob', false);
    await userEvent.click(within(header).getByRole('button', { name: 'Start Video Call' }));
    expect(startDmCall).toHaveBeenLastCalledWith('dm1', 'Bob', true);
    expect(within(header).getByRole('button', { name: /pinned/i })).toBeInTheDocument();
    expect(within(header).getByRole('button', { name: 'User Profile' })).toHaveAttribute(
      'aria-pressed',
    );
    expect(within(header).getByRole('searchbox', { name: 'Search Bob' })).toBeInTheDocument();
    await userEvent.click(within(header).getByRole('button', { name: 'Open profile of Bob' }));
    expect(await screen.findByRole('dialog', { name: 'User profile' })).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('LNG-DM-004: adding friends to a 1:1 DM makes a group and opens it', async () => {
    server.use(
      http.put('*/api/v1/channels/dm1/recipients/3', async (ctx) => {
        await record('PUT')(ctx);
        return HttpResponse.json({ id: 'grp2', type: 3, recipients: [bob, cat] });
      }),
      http.put('*/api/v1/channels/grp2/recipients/4', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { router } = at('/channels/@me/dm1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add Friends to DM' }));
    const dlg = await screen.findByRole('dialog', { name: 'Add Friends to DM' });
    // Bob is already in the DM, so he is not offered again.
    expect(await within(dlg).findByRole('checkbox', { name: /cat/i })).toBeInTheDocument();
    expect(within(dlg).queryByRole('checkbox', { name: /bob/i })).toBeNull();
    await userEvent.click(within(dlg).getByRole('checkbox', { name: /cat/i }));
    await userEvent.click(within(dlg).getByRole('checkbox', { name: /dan/i }));
    await userEvent.click(within(dlg).getByRole('button', { name: 'Add to DM' }));
    await waitFor(() =>
      expect(calls.map((c) => c.key)).toEqual([
        'PUT /channels/dm1/recipients/3',
        'PUT /channels/grp2/recipients/4',
      ]),
    );
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me/grp2'));
  });

  it('LNG-DM-005: the welcome header names the person, shared servers and relationship buttons', async () => {
    server.use(
      http.delete('*/api/v1/users/@me/relationships/2', async (ctx) => {
        await record('DELETE')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
      http.put('*/api/v1/users/@me/relationships/2', async (ctx) => {
        await record('PUT')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me/dm1');
    const log = await screen.findByRole('log');
    expect(
      await within(log).findByText(
        'This is the beginning of your direct message history with @bob.',
      ),
    ).toBeInTheDocument();
    expect(await within(log).findByText(/1 mutual server: Store Guild/)).toBeInTheDocument();
    await userEvent.click(await within(log).findByRole('button', { name: 'Remove Friend' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'DELETE /users/@me/relationships/2')).toBe(true),
    );
    await userEvent.click(within(log).getByRole('button', { name: 'Block' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PUT /users/@me/relationships/2')?.body).toEqual({
        type: 2,
      }),
    );
  });

  it('LNG-DM-006: the right panel summarises the profile', async () => {
    at('/channels/@me/dm1');
    const panel = await screen.findByLabelText('Bob profile');
    expect(await within(panel).findByText('Trains.')).toBeInTheDocument();
    expect(within(panel).getByText(/bob • he\/him/)).toBeInTheDocument();
    expect(within(panel).getByRole('img', { name: 'SIDE ROOM Staff' })).toBeInTheDocument();
    expect(within(panel).getByText(/member since/i)).toBeInTheDocument();
    expect(within(panel).getByText('Mar 4, 2024')).toBeInTheDocument();
    expect(
      within(panel).getByRole('button', { name: /1 Mutual Server • 1 Mutual Friend/ }),
    ).toBeInTheDocument();
    expect(within(panel).getByRole('button', { name: 'More actions for Bob' })).toBeInTheDocument();
    await userEvent.click(within(panel).getByRole('button', { name: 'View Full Profile' }));
    expect(await screen.findByRole('dialog', { name: 'Bob' })).toBeInTheDocument();
  });
});

describe('LNG-DM-007 group DM panel', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    seedDms();
    handlers();
  });

  it('lists members with the owner marked, and the owner can remove one', async () => {
    server.use(
      http.delete('*/api/v1/channels/grp/recipients/3', async (ctx) => {
        await record('DELETE')(ctx);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/@me/grp');
    const panel = await screen.findByLabelText('Group members');
    expect(await within(panel).findByText('Members — 3')).toBeInTheDocument();
    expect(within(panel).getByRole('img', { name: 'Owner' })).toBeInTheDocument();
    await userEvent.click(within(panel).getByRole('button', { name: 'Remove Cat from the group' }));
    await waitFor(() =>
      expect(calls.some((c) => c.key === 'DELETE /channels/grp/recipients/3')).toBe(true),
    );
    expect(
      within(panel).queryByRole('button', { name: 'Remove Me Myself from the group' }),
    ).toBeNull();
  });

  it('renames the group', async () => {
    server.use(
      http.patch('*/api/v1/users/@me/channels/grp', async (ctx) => {
        await record('PATCH')(ctx);
        return HttpResponse.json({ id: 'grp', name: 'Quartet' });
      }),
    );
    at('/channels/@me/grp');
    const panel = await screen.findByLabelText('Group members');
    await userEvent.click(within(panel).getByRole('button', { name: 'Group settings' }));
    const dlg = await screen.findByRole('dialog', { name: 'Group Settings' });
    const name = within(dlg).getByLabelText('Group name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Quartet');
    await userEvent.click(within(dlg).getByRole('button', { name: 'Save' }));
    await waitFor(() =>
      expect(calls.find((c) => c.key === 'PATCH /users/@me/channels/grp')?.body).toEqual({
        name: 'Quartet',
      }),
    );
    await waitFor(() => expect(useStore.getState().channels.grp?.name).toBe('Quartet'));
  });

  it('offers an invite link to the owner and reports an unavailable route', async () => {
    server.use(
      http.post('*/api/v1/channels/grp/invites', () => HttpResponse.json({ code: 'abc123' })),
    );
    at('/channels/@me/grp');
    const panel = await screen.findByLabelText('Group members');
    await userEvent.click(await within(panel).findByRole('button', { name: 'Create invite link' }));
    const link = await within(panel).findByLabelText('Group invite link');
    expect((link as HTMLInputElement).value).toMatch(/\/invite\/abc123$/);
  });

  it('shows the not-available note when the server has no group invites', async () => {
    server.use(
      http.post('*/api/v1/channels/grp/invites', () =>
        HttpResponse.json({ message: 'Not a guild channel' }, { status: 404 }),
      ),
    );
    at('/channels/@me/grp');
    const panel = await screen.findByLabelText('Group members');
    await userEvent.click(await within(panel).findByRole('button', { name: 'Create invite link' }));
    expect(await within(panel).findByRole('alert')).toHaveTextContent(/not available yet/);
  });

  it('hides the invite link from members who do not own the group', async () => {
    useStore.setState((s) => ({
      channels: { ...s.channels, grp: { ...s.channels.grp!, owner_id: '2' } },
    }));
    at('/channels/@me/grp');
    const panel = await screen.findByLabelText('Group members');
    await within(panel).findByText('Members — 3');
    expect(within(panel).queryByRole('button', { name: 'Create invite link' })).toBeNull();
    expect(within(panel).queryByRole('button', { name: /Remove .* from the group/ })).toBeNull();
  });
});

describe('LNG-DM-009 blocked messages in a DM', () => {
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);

  it("collapses a blocked person's messages and shows them on request", async () => {
    const m: S.Message = {
      id: '300',
      channel_id: 'dm1',
      author: bob,
      content: 'you should not see this',
      timestamp: new Date().toISOString(),
    };
    seedDms({ '300': m }, ['300']);
    handlers();
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 2, since: '2025-02-03T12:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
    );
    at('/channels/@me/dm1');
    const log = await screen.findByRole('log');
    expect(await within(log).findByText('1 blocked message')).toBeInTheDocument();
    expect(within(log).queryByText('you should not see this')).toBeNull();
    await userEvent.click(within(log).getByRole('button', { name: 'Show message' }));
    expect(await within(log).findByText('you should not see this')).toBeInTheDocument();
  });
});
