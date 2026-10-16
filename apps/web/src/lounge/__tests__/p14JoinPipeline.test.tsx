// Join pipeline, server guide and security actions in the Lounge (LNG-GLD-030, 031, 033, 034).
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { useLoungeUi } from '../store';
import { useNotifSettings } from '../shell/notifSettings';
import { useGuideFlags } from '../shell/joinPipeline';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const bits = (...n: number[]) => n.reduce((a, b) => a | (1n << BigInt(b)), 0n).toString();

function seed(perms: string = String((1 << 10) | (1 << 11))) {
  resetStore();
  localStorage.clear();
  useLoungeUi.setState(useLoungeUi.getInitialState(), true);
  useNotifSettings.setState({ byGuild: {} });
  useGuideFlags.setState({ enabled: {}, onboarding: {} });
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' }, '9': { id: '9', username: 'owner' } },
    guilds: { g1: { id: 'g1', name: 'Alpha Guild', owner_id: '9' } },
    guildOrder: ['g1'],
    channels: {
      cat: { id: 'cat', type: 4, guild_id: 'g1', name: 'Text Channels', position: 0 },
      c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general', parent_id: 'cat', position: 0 },
      c2: { id: 'c2', type: 0, guild_id: 'g1', name: 'manga', parent_id: 'cat', position: 1 },
    },
    channelIdsByGuild: { g1: ['cat', 'c1', 'c2'] },
    roles: {
      g1: {
        id: 'g1',
        guild_id: 'g1',
        name: '@everyone',
        color: 0,
        position: 0,
        permissions: perms,
      },
    },
    roleIdsByGuild: { g1: ['g1'] },
    memberIdsByGuild: { g1: ['1'] },
    members: { 'g1:1': { guild_id: 'g1', user_id: '1', roles: [] } },
    connection: 'connected',
  });
}

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

const guide = (enabled = true) =>
  http.get('*/api/v1/guilds/g1/sr/guide', () =>
    HttpResponse.json({
      guild_id: 'g1',
      enabled,
      welcome_message: 'Glad you are here',
      todos: [{ id: '1', title: 'Say hello', description: 'Introduce yourself', channel_id: 'c1' }],
      resources: [{ id: '2', title: 'FAQ', content: 'Ask away' }],
    }),
  );

const onboarding = http.get('*/api/v1/guilds/g1/onboarding', () =>
  HttpResponse.json({
    guild_id: 'g1',
    enabled: true,
    default_channel_ids: ['c1'],
    prompts: [
      {
        id: 'p1',
        title: 'What do you read?',
        single_select: true,
        required: true,
        in_onboarding: true,
        options: [
          { id: 'o1', title: 'Manga', channel_ids: ['c2'], role_ids: [] },
          { id: 'o2', title: 'Novels', channel_ids: [], role_ids: [] },
        ],
      },
    ],
  }),
);

describe('Membership screening (LNG-GLD-030)', () => {
  it('asks a pending member to agree to the rules, then offers onboarding', async () => {
    seed();
    const accept = vi.fn();
    const answers = vi.fn();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: true, joined_at: new Date().toISOString(), roles: [] }),
      ),
      http.get('*/api/v1/guilds/g1/member-verification', () =>
        HttpResponse.json({
          enabled: true,
          description: 'Please be nice',
          form_fields: [
            { field_type: 'TERMS', label: 'Rules', values: ['Be kind', 'No spam'], required: true },
          ],
        }),
      ),
      http.put('*/api/v1/guilds/g1/requests/@me', () => {
        accept();
        return HttpResponse.json({});
      }),
      onboarding,
      http.post('*/api/v1/guilds/g1/onboarding-responses', async ({ request }) => {
        answers(await request.json());
        return HttpResponse.json({ channel_ids: ['c1', 'c2'], role_ids: [] });
      }),
    );
    at('/channels/g1/c1');
    const dialog = await screen.findByRole('dialog', { name: 'Welcome to Alpha Guild' });
    expect(within(dialog).getByText('Please be nice')).toBeVisible();
    expect(within(dialog).getByText('No spam')).toBeVisible();
    const complete = within(dialog).getByRole('button', { name: 'Complete' });
    expect(complete).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('checkbox'));
    await userEvent.click(complete);
    await waitFor(() => expect(accept).toHaveBeenCalledTimes(1));

    const ob = await screen.findByRole('dialog', { name: 'Set up your time in Alpha Guild' });
    const next = within(ob).getByRole('button', { name: 'Continue' });
    expect(next).toBeDisabled();
    await userEvent.click(within(ob).getByRole('checkbox', { name: 'Manga' }));
    await userEvent.click(next);
    await waitFor(() => expect(answers).toHaveBeenCalledWith({ option_ids: ['o1'] }));
    const done = await screen.findByRole('dialog', { name: 'Your channels' });
    expect(within(done).getByText('#manga')).toBeVisible();
    expect(localStorage.getItem('sr.onboarded.1.g1')).toBe('1');
  });

  it('does not ask a member who already agreed or whose server has screening off', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      onboarding,
    );
    at('/channels/g1/c1');
    await waitFor(() => expect(useGuideFlags.getState().enabled['g1']).toBe(false));
    expect(screen.queryByRole('dialog', { name: /Welcome to Alpha Guild/ })).toBeNull();
    expect(screen.queryByRole('dialog', { name: /Set up your time/ })).toBeNull();
  });

  it('skipping onboarding is remembered', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: new Date().toISOString(), roles: [] }),
      ),
      onboarding,
    );
    at('/channels/g1/c1');
    const ob = await screen.findByRole('dialog', { name: 'Set up your time in Alpha Guild' });
    await userEvent.click(within(ob).getByRole('button', { name: 'Skip' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: /Set up your time/ })).toBeNull(),
    );
    expect(localStorage.getItem('sr.onboarded.1.g1')).toBe('1');
  });
});

describe('Server guide menu entry and dialog (LNG-GLD-033)', () => {
  it('shows the guide from the server menu when the server has one', async () => {
    seed();
    server.use(
      guide(true),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Server Guide' }));
    const dialog = await screen.findByRole('dialog', { name: 'Alpha Guild guide' });
    expect(await within(dialog).findByText('Glad you are here')).toBeVisible();
    expect(within(dialog).getByText('Introduce yourself')).toBeVisible();
    await userEvent.click(within(dialog).getByRole('button', { name: 'FAQ' }));
    expect(within(dialog).getByText('Ask away')).toBeVisible();
  });
});

describe('Security actions (LNG-GLD-034)', () => {
  it('pauses invites and DMs for a moderator', async () => {
    seed(bits(10, 11, 40));
    const patches: unknown[] = [];
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      http.get('*/api/v1/guilds/g1/sr/community', () =>
        HttpResponse.json({ invites_paused: false, dm_spam_filter: false }),
      ),
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        const body = (await request.json()) as object;
        patches.push(body);
        return HttpResponse.json({ invites_paused: true, dm_spam_filter: false, ...body });
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Security Actions' }));
    const dialog = await screen.findByRole('dialog', { name: 'Security Actions' });
    await userEvent.click(await within(dialog).findByRole('switch', { name: /Pause invites/ }));
    await waitFor(() => expect(patches).toEqual([{ invites_paused: true }]));
    await userEvent.click(within(dialog).getByRole('switch', { name: /Pause direct messages/ }));
    await waitFor(() => expect(patches[1]).toEqual({ dm_spam_filter: true }));
  });
});

describe('Create Server: icon step (LNG-GLD-017)', () => {
  it('keeps a GIF icon as it is and sets it right after the server is created', async () => {
    seed();
    let patch: unknown;
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      http.post('*/api/v1/guilds', () =>
        HttpResponse.json(
          {
            id: 'g5',
            name: 'Fresh',
            owner_id: '1',
            channels: [{ id: 'k2', type: 0, guild_id: 'g5', name: 'general', position: 1 }],
            roles: [],
          },
          { status: 201 },
        ),
      ),
      http.patch('*/api/v1/guilds/g5', async ({ request }) => {
        patch = await request.json();
        return HttpResponse.json({ id: 'g5', name: 'Fresh', owner_id: '1' });
      }),
    );
    const router = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Add a Server' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Create My Own' }));
    await userEvent.click(await screen.findByRole('button', { name: 'skip this question' }));
    await userEvent.upload(
      screen.getByLabelText('Server icon file'),
      new File(['GIF89a'], 'spin.gif', { type: 'image/gif' }),
    );
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Change the icon' })).toBeVisible(),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/channels/g5/k2'));
    expect((patch as { icon: string }).icon).toMatch(/^data:image\/gif;base64,/);
  });
});

describe('Browse Channels and the help menu (LNG-GLD-032, LNG-CHN-018)', () => {
  it('lists the channels a member can open on an onboarding server', async () => {
    seed();
    server.use(
      guide(false),
      onboarding,
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Browse Channels' }));
    const dialog = await screen.findByRole('dialog', { name: 'Browse Channels' });
    expect(within(dialog).getByRole('button', { name: '#general' })).toBeVisible();
    expect(within(dialog).getByRole('button', { name: '#manga' })).toBeVisible();
  });

  it('opens the shortcut sheet from the header help button', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click((await screen.findAllByRole('button', { name: 'Help' }))[0]!);
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Keyboard shortcuts' }));
    expect(await screen.findByRole('dialog', { name: /shortcut/i })).toBeVisible();
  });
});
