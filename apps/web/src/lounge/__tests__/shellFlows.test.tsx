import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { resetStore, useStore } from '../../store';
import type * as S from '../../store/types';
import { useLoungeUi } from '../store';
import { useNotifSettings } from '../shell/notifSettings';
import { useSocial } from '../shell/social';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const me: S.User = { id: '1', username: 'me', global_name: 'Me Myself' };
const bob: S.User = { id: '2', username: 'bob' };
const EVERYONE = String((1 << 10) | (1 << 11) | 1);

const role = (gid: string) => ({
  id: gid,
  guild_id: gid,
  name: '@everyone',
  color: 0,
  position: 0,
  permissions: EVERYONE,
});

function seed(ownerId: string) {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useNotifSettings.setState({ byGuild: {} });
  useStore.setState({
    currentUserId: '1',
    users: { '1': me, '2': bob },
    guilds: {
      g1: { id: 'g1', name: 'Alpha Guild', owner_id: ownerId },
      g2: { id: 'g2', name: 'Beta Guild', owner_id: '2' },
    },
    guildOrder: ['g1', 'g2'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 0 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'random', parent_id: 'cat', position: 1 },
      d1: { id: 'd1', type: 0, guild_id: 'g2', name: 'lobby', position: 0 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'c2'], g2: ['d1'] },
    roles: { g1: role('g1'), g2: role('g2') },
    roleIdsByGuild: { g1: ['g1'], g2: ['g2'] },
    memberIdsByGuild: { g1: ['1', '2'], g2: ['1', '2'] },
    members: {
      'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
      'g1:2': { guild_id: 'g1', user_id: '2', roles: [] },
      'g2:1': { guild_id: 'g2', user_id: '1', roles: [] },
      'g2:2': { guild_id: 'g2', user_id: '2', roles: [] },
    },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const settingsBody = (over: Record<string, unknown> = {}) => ({
  muted: false,
  mute_config: null,
  message_notifications: null,
  suppress_everyone: false,
  suppress_roles: false,
  channel_overrides: [],
  ...over,
});

const quiet = () =>
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })),
    http.get('*/api/v1/guilds/:gid/members', () => HttpResponse.json([])),
    http.get('*/api/v1/channels/:cid/messages', () => HttpResponse.json([])),
    http.get('*/api/v1/users/@me/guilds/:gid/settings', () => HttpResponse.json(settingsBody())),
  );

async function openServerMenu(name = /Alpha Guild/) {
  const sidebar = await screen.findByTestId('channel-sidebar');
  await userEvent.click(within(sidebar).getByRole('button', { name }));
}

describe('server menu: mute, notifications, leave', () => {
  // The shell is a lazy route; load it once so a cold transform does not eat the first test's budget.
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);
  beforeEach(() => {
    quiet();
  });

  it('mutes the server for an hour and dims it on the rail', async () => {
    seed('2');
    let patch: Record<string, unknown> = {};
    server.use(
      http.patch('*/api/v1/users/@me/guilds/g1/settings', async ({ request }) => {
        patch = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(
          settingsBody({ muted: true, mute_config: { end_time: '2999-01-01T00:00:00Z' } }),
        );
      }),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Mute Server' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'For 1 Hour' }));
    await waitFor(() =>
      expect(patch).toEqual({ muted: true, mute_config: { selected_time_window: 3600 } }),
    );
    const rail = screen.getByRole('navigation', { name: 'Servers' });
    await waitFor(() =>
      expect(
        within(rail)
          .getByRole('link', { name: /Alpha Guild/ })
          .closest('[data-muted]'),
      ).not.toBeNull(),
    );
    // The menu now offers Unmute.
    await openServerMenu();
    expect(await screen.findByRole('menuitem', { name: 'Unmute Server' })).toBeInTheDocument();
  });

  it('saves the notification level from Notification Settings', async () => {
    seed('2');
    const patches: Record<string, unknown>[] = [];
    server.use(
      http.patch('*/api/v1/users/@me/guilds/g1/settings', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        patches.push(body);
        return HttpResponse.json(settingsBody({ message_notifications: 1 }));
      }),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Notification Settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Notification Settings' });
    await userEvent.click(await within(dialog).findByRole('radio', { name: 'Only @mentions' }));
    await waitFor(() => expect(patches).toEqual([{ message_notifications: 1 }]));
    await waitFor(() =>
      expect(within(dialog).getByRole('radio', { name: 'Only @mentions' })).toBeChecked(),
    );
    await userEvent.click(within(dialog).getByRole('switch', { name: /Suppress @everyone/ }));
    await waitFor(() => expect(patches[1]).toEqual({ suppress_everyone: true }));
  });

  it('channel notification settings send a channel override', async () => {
    seed('2');
    let body: Record<string, unknown> = {};
    server.use(
      http.patch('*/api/v1/users/@me/guilds/g1/settings', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json(settingsBody());
      }),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(sidebar).getByRole('link', { name: 'random' }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Notification Settings' }));
    const dialog = await screen.findByRole('dialog', { name: /Notification Settings: #random/ });
    await userEvent.click(await within(dialog).findByRole('radio', { name: 'Nothing' }));
    await waitFor(() =>
      expect(body).toEqual({ channel_overrides: { c2: { message_notifications: 2 } } }),
    );
  });

  it('offers highlight and mobile push toggles and lists channel overrides with a reset', async () => {
    seed('2');
    const patches: Record<string, unknown>[] = [];
    server.use(
      http.get('*/api/v1/users/@me/guilds/g1/settings', () =>
        HttpResponse.json(
          settingsBody({
            mobile_push: true,
            channel_overrides: [{ channel_id: 'c2', muted: false, message_notifications: 2 }],
          }),
        ),
      ),
      http.patch('*/api/v1/users/@me/guilds/g1/settings', async ({ request }) => {
        patches.push((await request.json()) as Record<string, unknown>);
        return HttpResponse.json(settingsBody({ mobile_push: true, suppress_highlights: true }));
      }),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Notification Settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Notification Settings' });
    await userEvent.click(
      await within(dialog).findByRole('switch', { name: 'Suppress highlights' }),
    );
    await waitFor(() => expect(patches[0]).toEqual({ suppress_highlights: true }));
    await userEvent.click(
      within(dialog).getByRole('switch', { name: 'Mobile push notifications' }),
    );
    await waitFor(() => expect(patches[1]).toEqual({ mobile_push: false }));
  });

  it('leaving asks for confirmation, calls the API and goes Home', async () => {
    seed('2');
    let left = 0;
    server.use(
      http.delete('*/api/v1/users/@me/guilds/g1', () => {
        left += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const router = at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Leave Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Leave Alpha Guild' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Leave Server' }));
    await waitFor(() => expect(left).toBe(1));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/@me'));
    expect(useStore.getState().guilds.g1).toBeUndefined();
  });

  it('shows the failure copy when leaving is refused', async () => {
    seed('2');
    server.use(
      http.delete('*/api/v1/users/@me/guilds/g1', () =>
        HttpResponse.json({ message: 'Transfer ownership first' }, { status: 400 }),
      ),
    );
    at('/channels/g1/c1');
    await openServerMenu();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Leave Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Leave Alpha Guild' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Leave Server' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent('Transfer ownership first');
    expect(useStore.getState().guilds.g1).toBeDefined();
  });

  it('the owner is told to transfer ownership instead (rail context menu)', async () => {
    seed('1');
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(rail).getByRole('link', { name: /Alpha Guild/ }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Leave Server' }));
    expect(await screen.findByText(/Transfer ownership to another member/)).toBeInTheDocument();
  });
});

describe('channel list management', () => {
  beforeEach(() => {
    quiet();
    seed('1');
  });

  it('deletes a channel from its context menu after confirming', async () => {
    let deleted = 0;
    server.use(
      http.delete('*/api/v1/channels/c2', () => {
        deleted += 1;
        return HttpResponse.json({ id: 'c2', type: 0 });
      }),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(sidebar).getByRole('link', { name: 'random' }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Delete Channel' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delete Channel' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Delete Channel' }));
    await waitFor(() => expect(deleted).toBe(1));
    await waitFor(() => expect(within(sidebar).queryByRole('link', { name: 'random' })).toBeNull());
  });

  it('moves a channel up from the context menu (keyboard alternative to dragging)', async () => {
    let body: unknown;
    server.use(
      http.patch('*/api/v1/guilds/g1/channels', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(sidebar).getByRole('link', { name: 'random' }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Move Up' }));
    await waitFor(() =>
      expect(body).toEqual(
        expect.arrayContaining([
          { id: 'c2', position: 0, parent_id: 'cat' },
          { id: 'c1', position: 1, parent_id: 'cat' },
        ]),
      ),
    );
    const names = within(sidebar)
      .getAllByRole('link')
      .map((l) => l.getAttribute('aria-label'));
    expect(names).toEqual(['random', 'general']);
  });

  it('drag and drop reorders channels and rolls back when the server refuses', async () => {
    server.use(
      http.patch('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const row = (n: string) => within(sidebar).getByRole('link', { name: n }).closest('li')!;
    const data = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(row('general'), { dataTransfer: data });
    // jsdom has no layout, so the pointer lands in the lower half: "after random".
    fireEvent.dragOver(row('random'), { dataTransfer: data });
    expect(row('random').getAttribute('data-drop')).toBe('after');
    fireEvent.drop(row('random'), { dataTransfer: data });
    expect(await screen.findByText(/Could not move that channel/)).toBeInTheDocument();
    const names = within(sidebar)
      .getAllByRole('link')
      .map((l) => l.getAttribute('aria-label'));
    expect(names).toEqual(['general', 'random']);
  });

  it('opens the Edit Channel modal and saves the name', async () => {
    let patched: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          {
            id: 'cat',
            type: 4,
            guild_id: 'g1',
            name: 'Text Channels',
            position: 0,
            parent_id: null,
            nsfw: false,
            rate_limit_per_user: 0,
            topic: null,
            permission_overwrites: [],
          },
          {
            id: 'c1',
            type: 0,
            guild_id: 'g1',
            name: 'general',
            position: 0,
            parent_id: 'cat',
            nsfw: false,
            rate_limit_per_user: 0,
            topic: null,
            permission_overwrites: [],
          },
        ]),
      ),
      http.patch('*/api/v1/channels/c1', async ({ request }) => {
        patched = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ id: 'c1', type: 0, name: patched.name });
      }),
    );
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const row = within(sidebar).getByRole('link', { name: 'general' }).closest('li')!;
    await userEvent.click(within(row).getByRole('button', { name: 'Edit Channel' }));
    const dialog = await screen.findByRole('dialog', { name: 'Edit Channel' });
    const name = await within(dialog).findByLabelText('Channel name');
    await userEvent.clear(name);
    await userEvent.type(name, 'lobby');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save Changes' }));
    await waitFor(() => expect(patched).toEqual({ name: 'lobby' }));
    expect(within(dialog).getByRole('tab', { name: 'Permissions' })).toBeInTheDocument();
  });
});

describe('server rail drag and drop', () => {
  beforeEach(() => {
    quiet();
    seed('1');
  });

  const item = (rail: HTMLElement, name: RegExp) =>
    within(rail).getByRole('link', { name }).closest('.lng-rail-item')!;

  it('dropping a server on another makes a folder that persists', async () => {
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    const data = { setData: vi.fn(), effectAllowed: '', dropEffect: '' };
    fireEvent.dragStart(item(rail, /Beta Guild/), { dataTransfer: data });
    fireEvent.dragOver(item(rail, /Alpha Guild/), { dataTransfer: data });
    expect(item(rail, /Alpha Guild/).getAttribute('data-drop')).toBe('into');
    fireEvent.drop(item(rail, /Alpha Guild/), { dataTransfer: data });
    const folder = await within(rail).findByRole('button', { name: /folder/ });
    expect(folder).toHaveAttribute('aria-expanded', 'false');
    const layout = useLoungeUi.getState().railLayout;
    expect(layout).toHaveLength(1);
    expect(layout![0]).toMatchObject({ kind: 'folder', guildIds: ['g1', 'g2'] });
    await userEvent.click(folder);
    expect(within(rail).getByRole('link', { name: /Alpha Guild/ })).toBeInTheDocument();
    expect(within(rail).getByRole('link', { name: /Beta Guild/ })).toBeInTheDocument();
  });

  it('keyboard: Move Down in the context menu reorders servers', async () => {
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(rail).getByRole('link', { name: /Alpha Guild/ }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Move Down' }));
    const links = within(rail)
      .getAllByRole('link')
      .map((l) => l.getAttribute('aria-label'));
    expect(links.indexOf('Beta Guild')).toBeLessThan(links.indexOf('Alpha Guild'));
  });

  it('edits a folder from Folder Settings and can ungroup it', async () => {
    useLoungeUi.setState({
      railLayout: [
        { kind: 'folder', id: 'f1', name: null, color: '#5865f2', guildIds: ['g1', 'g2'] },
      ],
    });
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(rail).getByRole('button', { name: /folder/ }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Folder Settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Folder Settings' });
    await userEvent.type(within(dialog).getByLabelText('Folder name'), 'Games');
    await userEvent.click(within(dialog).getByRole('radio', { name: '#3ba55d' }));
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(useLoungeUi.getState().railLayout![0]).toMatchObject({
      name: 'Games',
      color: '#3ba55d',
    });
    expect(await within(rail).findByRole('button', { name: /Games folder/ })).toBeInTheDocument();

    await userEvent.pointer({
      keys: '[MouseRight]',
      target: within(rail).getByRole('button', { name: /Games folder/ }),
    });
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Ungroup Folder' }));
    await waitFor(() => expect(within(rail).queryByRole('button', { name: /folder/ })).toBeNull());
    expect(within(rail).getByRole('link', { name: /Alpha Guild/ })).toBeInTheDocument();
  });
});

describe('accessibility', () => {
  it('server sidebar, rail and the open dialogs have no axe violations', async () => {
    quiet();
    seed('1');
    at('/channels/g1/c1');
    const sidebar = await screen.findByTestId('channel-sidebar');
    const rail = screen.getByRole('navigation', { name: 'Servers' });
    expect(await axeViolations(sidebar)).toEqual([]);
    expect(await axeViolations(rail)).toEqual([]);
    await userEvent.click(within(sidebar).getByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Notification Settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Notification Settings' });
    await within(dialog).findByRole('radio', { name: 'Nothing' });
    expect(await axeViolations(dialog)).toEqual([]);
  });

  it('the folder settings dialog has no axe violations', async () => {
    quiet();
    seed('1');
    useLoungeUi.setState({
      railLayout: [
        { kind: 'folder', id: 'f1', name: 'Games', color: '#5865f2', guildIds: ['g1', 'g2'] },
      ],
    });
    at('/channels/g1/c1');
    useLoungeUi.getState().openDialog({ kind: 'folder', folderId: 'f1' });
    const dialog = await screen.findByRole('dialog', { name: 'Folder Settings' });
    expect(await axeViolations(dialog)).toEqual([]);
  });
});

describe('Create a Server wizard', () => {
  beforeEach(() => {
    quiet();
    seed('1');
  });

  const templates = [
    { id: 'gaming', name: 'Gaming', description: 'Squads, clips and voice lobbies' },
    { id: 'friends', name: 'Friends', description: 'A hangout for your close circle' },
  ];

  it('lists the templates the server offers and creates from one in a single POST /guilds', async () => {
    let posted: unknown;
    server.use(
      http.get('*/api/v1/guilds/templates/builtin', () => HttpResponse.json(templates)),
      http.post('*/api/v1/guilds', async ({ request }) => {
        posted = await request.json();
        return HttpResponse.json(
          {
            id: 'g5',
            name: 'Raid Night',
            owner_id: '1',
            channels: [{ id: 'k2', type: 0, guild_id: 'g5', name: 'general', position: 0 }],
            roles: [],
          },
          { status: 201 },
        );
      }),
    );
    const router = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a Server' });
    const list = await within(dialog).findByRole('list', { name: 'Templates' });
    expect(within(list).getByText('Squads, clips and voice lobbies')).toBeInTheDocument();
    expect(await axeViolations(dialog)).toEqual([]);
    await userEvent.click(within(dialog).getByRole('button', { name: /Gaming/ }));
    await userEvent.click(await screen.findByRole('button', { name: 'For me and my friends' }));
    expect(screen.getByText(/Gaming/, { selector: 'strong' })).toBeInTheDocument();
    const name = screen.getByLabelText('Server name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Raid Night');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g5/k2'));
    // One request: the server builds the layout, the client no longer replays channels.
    expect(posted).toEqual({ name: 'Raid Night', template: 'gaming' });
  });

  it('creates a plain server without a template field, and copes with the list failing', async () => {
    let posted: unknown;
    server.use(
      http.get('*/api/v1/guilds/templates/builtin', () => new HttpResponse(null, { status: 500 })),
      http.post('*/api/v1/guilds', async ({ request }) => {
        posted = await request.json();
        return HttpResponse.json(
          { id: 'g6', name: 'Mine', owner_id: '1', channels: [], roles: [] },
          { status: 201 },
        );
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a Server' });
    expect(await within(dialog).findByText(/Templates are unavailable/)).toBeInTheDocument();
    expect(within(dialog).queryByRole('list', { name: 'Templates' })).toBeNull();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create My Own' }));
    await userEvent.click(await screen.findByRole('button', { name: 'skip this question' }));
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(posted).toEqual({ name: "Me Myself's server" }));
  });

  it('goes back through the steps and lists the invite examples on Join', async () => {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Create My Own' }));
    expect(
      screen.getByRole('dialog', { name: 'Tell us more about your server' }),
    ).toBeInTheDocument();
    await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Back' }));
    await userEvent.click(screen.getByRole('button', { name: 'Join a Server' }));
    expect(screen.getByText('hTKzmak')).toBeInTheDocument();
    await userEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Back' }));
    expect(screen.getByRole('dialog', { name: 'Create a Server' })).toBeInTheDocument();
  });

  it('the wizard steps have no axe violations', async () => {
    server.use(
      http.get('*/api/v1/guilds/templates/builtin', () =>
        HttpResponse.json([
          { id: 'study', name: 'Study Group', description: 'Notes and quiet rooms' },
        ]),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a Server' });
    await within(dialog).findByRole('list', { name: 'Templates' });
    expect(await axeViolations(dialog)).toEqual([]);
    await userEvent.click(within(dialog).getByRole('button', { name: /Study Group/ }));
    expect(await axeViolations(screen.getByRole('dialog'))).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'skip this question' }));
    expect(await axeViolations(screen.getByRole('dialog'))).toEqual([]);
  });
});

describe('Create Channel types and private picker', () => {
  beforeEach(() => {
    quiet();
    seed('1');
  });

  const capture = () => {
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/guilds/g1/channels', async ({ request }) => {
        const b = (await request.json()) as Record<string, unknown>;
        bodies.push(b);
        return HttpResponse.json({ id: 'n1', guild_id: 'g1', ...b }, { status: 201 });
      }),
    );
    return bodies;
  };

  async function openCreate() {
    useLoungeUi.getState().openDialog({ kind: 'createChannel', guildId: 'g1' });
    return screen.findByRole('dialog', { name: 'Create Channel' });
  }

  it.each([
    ['Announcement', 5, 'big-news'],
    ['Forum', 15, 'help-desk'],
    ['Stage', 13, 'Town Hall'],
    ['Media', 16, 'art-wall'],
  ])('creates a %s channel with type %i', async (label, type, expected) => {
    const bodies = capture();
    at('/channels/g1/c1');
    const dialog = await openCreate();
    await userEvent.click(within(dialog).getByRole('radio', { name: new RegExp(label) }));
    await userEvent.type(within(dialog).getByLabelText('Channel name'), expected);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Channel' }));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]).toMatchObject({ type, name: expected });
  });

  it('shows the slug a text channel name becomes', async () => {
    at('/channels/g1/c1');
    const dialog = await openCreate();
    await userEvent.type(within(dialog).getByLabelText('Channel name'), 'Movie Night!');
    expect(within(dialog).getByText('#movie-night')).toBeInTheDocument();
  });

  it('a private channel can include picked roles and members', async () => {
    useStore.setState({
      roles: {
        g1: {
          id: 'g1',
          guild_id: 'g1',
          name: '@everyone',
          color: 0,
          position: 0,
          permissions: EVERYONE,
        },
        r1: { id: 'r1', guild_id: 'g1', name: 'Mods', color: 0, position: 1, permissions: '0' },
      },
      roleIdsByGuild: { g1: ['g1', 'r1'] },
    });
    const bodies = capture();
    at('/channels/g1/c1');
    const dialog = await openCreate();
    await userEvent.type(within(dialog).getByLabelText('Channel name'), 'staff');
    await userEvent.click(within(dialog).getByRole('switch', { name: /Private Channel/ }));
    await userEvent.click(within(dialog).getByRole('checkbox', { name: 'Mods' }));
    await userEvent.click(within(dialog).getByRole('checkbox', { name: /bob/ }));
    expect(await axeViolations(dialog)).toEqual([]);
    await userEvent.click(within(dialog).getByRole('button', { name: 'Create Channel' }));
    await waitFor(() => expect(bodies).toHaveLength(1));
    expect(bodies[0]!.permission_overwrites).toEqual([
      { id: 'g1', type: 0, allow: '0', deny: '1024' },
      { id: '1', type: 1, allow: '1024', deny: '0' },
      { id: 'r1', type: 0, allow: '1024', deny: '0' },
      { id: '2', type: 1, allow: '1024', deny: '0' },
    ]);
  });
});

describe('member actions', () => {
  beforeEach(() => {
    quiet();
    seed('1');
    useStore.setState({
      roles: {
        g1: {
          id: 'g1',
          guild_id: 'g1',
          name: '@everyone',
          color: 0,
          position: 0,
          permissions: EVERYONE,
        },
        r1: { id: 'r1', guild_id: 'g1', name: 'Mods', color: 0, position: 1, permissions: '0' },
      },
      roleIdsByGuild: { g1: ['g1', 'r1'] },
    });
  });

  async function rightClickBob() {
    const members = await screen.findByRole('list', { name: 'Members' });
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: await within(members).findByRole('button', { name: /^bob,/ }),
    });
  }

  it('kicks a member after a reason and removes them from the list', async () => {
    let reason: string | null = null;
    server.use(
      http.delete('*/api/v1/guilds/g1/members/2', ({ request }) => {
        reason = request.headers.get('x-audit-log-reason');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Kick bob' }));
    const dialog = await screen.findByRole('dialog', { name: 'Kick bob' });
    await userEvent.type(within(dialog).getByLabelText('Reason (optional)'), 'spam');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Kick' }));
    await waitFor(() => expect(reason).toBe('spam'));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Kick bob' })).toBeNull());
    expect(useStore.getState().memberIdsByGuild.g1).toEqual(['1']);
  });

  it('bans with a history window', async () => {
    let body: unknown;
    server.use(
      http.put('*/api/v1/guilds/g1/bans/2', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Ban bob' }));
    const dialog = await screen.findByRole('dialog', { name: 'Ban bob' });
    await userEvent.selectOptions(
      within(dialog).getByLabelText('Delete message history'),
      'Previous 24 hours',
    );
    await userEvent.type(within(dialog).getByLabelText('Reason (optional)'), 'raid');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Ban' }));
    await waitFor(() => expect(body).toEqual({ delete_message_seconds: 86400, reason: 'raid' }));
  });

  it('times a member out for an hour and shows the refusal copy on 403', async () => {
    let until = '';
    server.use(
      http.patch('*/api/v1/guilds/g1/members/2', async ({ request }) => {
        until = ((await request.json()) as { communication_disabled_until: string })
          .communication_disabled_until;
        return HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 });
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Timeout' }));
    const dialog = await screen.findByRole('dialog', { name: 'Timeout bob' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Timeout' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(/do not have permission/);
    const ms = Date.parse(until) - Date.now();
    expect(ms).toBeGreaterThan(3500 * 1000);
    expect(ms).toBeLessThan(3700 * 1000);
    expect(await axeViolations(dialog)).toEqual([]);
  });

  it('toggles a role from the Roles submenu', async () => {
    let put = 0;
    server.use(
      http.put('*/api/v1/guilds/g1/members/2/roles/r1', () => {
        put += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Roles' }));
    await userEvent.click(await screen.findByRole('menuitemcheckbox', { name: 'Mods' }));
    await waitFor(() => expect(put).toBe(1));
    await waitFor(() => expect(useStore.getState().members['g1:2']?.roles).toEqual(['r1']));
  });

  it('offers no moderation to people without the permissions, or on themselves', async () => {
    seed('2');
    at('/channels/g1/c1');
    await rightClickBob();
    expect(await screen.findByRole('menuitem', { name: 'Copy User ID' })).toBeInTheDocument();
    expect(screen.queryByRole('menuitem', { name: /^Kick/ })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: /^Ban/ })).toBeNull();
    expect(screen.queryByRole('menuitem', { name: 'Timeout' })).toBeNull();
  });
});

describe('invite friends', () => {
  beforeEach(() => {
    quiet();
    seed('1');
    useSocial.getState().reset();
  });

  it('sends each friend their own invite as a DM, with the server defaults (D-182)', async () => {
    const calls: string[] = [];
    let inviteBody: unknown;
    let message: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 1, since: '2026-01-01T00:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
      http.post('*/api/v1/channels/c1/invites', async ({ request }) => {
        calls.push('invite');
        inviteBody = await request.json();
        return HttpResponse.json({
          code: 'abc123',
          guild: { id: 'g1', name: 'Alpha Guild' },
          channel: { id: 'c1', name: 'general', type: 0 },
          uses: 0,
          max_uses: 1,
          max_age: 86400,
          created_at: '2026-01-01T00:00:00Z',
        });
      }),
      http.post('*/api/v1/users/@me/channels', () => {
        calls.push('dm');
        return HttpResponse.json({
          id: 'dm1',
          type: 1,
          recipients: [{ id: '2', username: 'bob' }],
        });
      }),
      http.post('*/api/v1/channels/dm1/messages', async ({ request }) => {
        calls.push('message');
        message = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({
          id: '500',
          channel_id: 'dm1',
          author: { id: '1', username: 'me' },
          content: message.content,
          timestamp: '2026-01-01T00:00:00Z',
        });
      }),
    );
    at('/channels/g1/c1');
    useLoungeUi.getState().openDialog({ kind: 'invite', guildId: 'g1' });
    const dialog = await screen.findByRole('dialog', { name: /Invite people to/ });
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Invite bob' }));
    await waitFor(() =>
      expect(within(dialog).getByRole('button', { name: 'Invite bob' })).toHaveTextContent('Sent'),
    );
    expect(calls).toEqual(['invite', 'dm', 'message']);
    // In-platform invite: no explicit limits and not external, so the server defaults apply.
    expect(inviteBody).toEqual({});
    expect(String(message.content)).toMatch(/\/invite\/abc123$/);
    expect(await axeViolations(dialog)).toEqual([]);
  });

  it('shows Retry when sending fails', async () => {
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 1, since: '2026-01-01T00:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
      http.post('*/api/v1/channels/c1/invites', () =>
        HttpResponse.json({ message: 'no' }, { status: 403 }),
      ),
    );
    at('/channels/g1/c1');
    useLoungeUi.getState().openDialog({ kind: 'invite', guildId: 'g1' });
    const dialog = await screen.findByRole('dialog', { name: /Invite people to/ });
    await userEvent.click(await within(dialog).findByRole('button', { name: 'Invite bob' }));
    expect(await within(dialog).findByText('Could not send the invite.')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Invite bob' })).toHaveTextContent('Retry');
  });
});

describe('server welcome screen', () => {
  beforeEach(() => {
    quiet();
    seed('1');
    useStore.setState({
      guilds: {
        g1: { id: 'g1', name: 'Alpha Guild', owner_id: '1' },
        g3: { id: 'g3', name: 'Empty Hall', owner_id: '1' },
      },
      guildOrder: ['g1', 'g3'],
      channelIdsByGuild: { g1: ['cat', 'c1', 'c2'], g2: ['d1'], g3: [] },
      roles: {
        g1: {
          id: 'g1',
          guild_id: 'g1',
          name: '@everyone',
          color: 0,
          position: 0,
          permissions: EVERYONE,
        },
        g3: {
          id: 'g3',
          guild_id: 'g3',
          name: '@everyone',
          color: 0,
          position: 0,
          permissions: EVERYONE,
        },
      },
      roleIdsByGuild: { g1: ['g1'], g3: ['g3'] },
      memberIdsByGuild: { g1: ['1', '2'], g3: ['1'] },
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g3:1': { guild_id: 'g3', user_id: '1', roles: [] },
      },
    });
  });

  it('greets a server with no channels and opens the invite dialog from its checklist', async () => {
    at('/channels/g3');
    const welcome = await screen.findByRole('region', { name: 'Welcome to Empty Hall' });
    const cards = within(welcome).getByRole('list', { name: 'Getting started' });
    expect(within(cards).getByRole('button', { name: /Send your first message/ })).toBeDisabled();
    await userEvent.click(within(cards).getByRole('button', { name: /Invite your friends/ }));
    expect(
      await screen.findByRole('dialog', { name: /Invite people to Empty Hall/ }),
    ).toBeInTheDocument();
    expect(useLoungeUi.getState().welcomeDone['g3:invite']).toBe(true);
    expect(await axeViolations(welcome)).toEqual([]);
  });
});

describe('person menu entries', () => {
  beforeEach(() => {
    quiet();
    seed('1');
    useSocial.getState().reset();
  });

  async function rightClickBob() {
    const members = await screen.findByRole('list', { name: 'Members' });
    await userEvent.pointer({
      keys: '[MouseRight]',
      target: await within(members).findByRole('button', { name: /^bob,/ }),
    });
  }

  it('saves a private note', async () => {
    let put: unknown;
    server.use(
      http.get('*/api/v1/users/@me/notes/2', () =>
        HttpResponse.json({ message: 'none' }, { status: 404 }),
      ),
      http.put('*/api/v1/users/@me/notes/2', async ({ request }) => {
        put = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Add Note' }));
    const dialog = await screen.findByRole('dialog', { name: 'Note for bob' });
    await userEvent.type(await within(dialog).findByLabelText('Note'), 'met at the club');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(put).toEqual({ note: 'met at the club' }));
  });

  it('changes a nickname and shows it in the member list', async () => {
    let patch: unknown;
    server.use(
      http.patch('*/api/v1/guilds/g1/members/2', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({});
      }),
    );
    at('/channels/g1/c1');
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Change Nickname' }));
    const dialog = await screen.findByRole('dialog', { name: 'Change Nickname' });
    await userEvent.type(within(dialog).getByLabelText('Nickname'), 'Bobby');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(patch).toEqual({ nick: 'Bobby' }));
    const members = screen.getByRole('list', { name: 'Members' });
    expect(await within(members).findByRole('button', { name: /^Bobby,/ })).toBeInTheDocument();
  });

  it('invites a member to another server by DM and blocks from the menu', async () => {
    const calls: string[] = [];
    server.use(
      http.get('*/api/v1/users/@me/relationships', () =>
        HttpResponse.json([
          { id: '2', type: 1, since: '2026-01-01T00:00:00Z', user: { id: '2', username: 'bob' } },
        ]),
      ),
      http.post('*/api/v1/channels/d1/invites', () => {
        calls.push('invite');
        return HttpResponse.json({
          code: 'zzz999',
          guild: { id: 'g2', name: 'Beta Guild' },
          channel: { id: 'd1', name: 'lobby', type: 0 },
          uses: 0,
          max_uses: 1,
          max_age: 86400,
          created_at: '2026-01-01T00:00:00Z',
        });
      }),
      http.post('*/api/v1/users/@me/channels', () => {
        calls.push('dm');
        return HttpResponse.json({
          id: 'dm5',
          type: 1,
          recipients: [{ id: '2', username: 'bob' }],
        });
      }),
      http.post('*/api/v1/channels/dm5/messages', () => {
        calls.push('message');
        return HttpResponse.json({
          id: '9',
          channel_id: 'dm5',
          author: { id: '1', username: 'me' },
          content: 'x',
          timestamp: '2026-01-01T00:00:00Z',
        });
      }),
      http.put('*/api/v1/users/@me/relationships/2', () => {
        calls.push('block');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    at('/channels/g1/c1');
    await screen.findByRole('list', { name: 'Members' });
    await waitFor(() => expect(useSocial.getState().status).toBe('ready'));
    await rightClickBob();
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Invite to Server' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Beta Guild' }));
    await waitFor(() => expect(calls).toEqual(['invite', 'dm', 'message']));

    await rightClickBob();
    expect(await screen.findByRole('menuitem', { name: 'Remove Friend' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('menuitem', { name: 'Block' }));
    await waitFor(() => expect(calls).toContain('block'));
  });
});
