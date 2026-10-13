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

  it('builds a template server: guild first, then its categories and channels', async () => {
    const posted: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/guilds', () =>
        HttpResponse.json(
          {
            id: 'g5',
            name: 'Raid Night',
            owner_id: '1',
            channels: [{ id: 'k2', type: 0, guild_id: 'g5', name: 'general', position: 0 }],
            roles: [],
          },
          { status: 201 },
        ),
      ),
      http.post('*/api/v1/guilds/g5/channels', async ({ request }) => {
        const body = (await request.json()) as Record<string, unknown>;
        posted.push(body);
        return HttpResponse.json(
          {
            id: `n${posted.length}`,
            guild_id: 'g5',
            position: posted.length,
            ...body,
          },
          { status: 201 },
        );
      }),
    );
    const router = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a Server' });
    await userEvent.click(within(dialog).getByRole('button', { name: /Gaming/ }));
    await userEvent.click(await screen.findByRole('button', { name: 'For me and my friends' }));
    expect(screen.getByText(/Gaming/, { selector: 'strong' })).toBeInTheDocument();
    const name = screen.getByLabelText('Server name');
    await userEvent.clear(name);
    await userEvent.type(name, 'Raid Night');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g5/k2'));
    expect(posted.map((p) => [p.name, p.type, p.parent_id ?? null])).toEqual([
      ['Games', 4, null],
      ['game-chat', 0, 'n1'],
      ['clips-and-highlights', 0, 'n1'],
      ['Squad 1', 2, 'n1'],
    ]);
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
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    const dialog = await screen.findByRole('dialog', { name: 'Create a Server' });
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
