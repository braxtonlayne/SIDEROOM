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
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

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
  useGuideFlags.setState({ enabled: {}, onboarding: {}, memberFlags: {}, skipped: {} });
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
    // D-245: the completed flag is the memory, and nothing is kept in the browser.
    expect(useGuideFlags.getState().memberFlags['g1']! & 2).toBe(2);
    expect(localStorage.getItem('sr.onboarded.1.g1')).toBeNull();
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

  it('skipping onboarding stays skipped for the visit and writes nothing to the browser', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({
          pending: false,
          joined_at: new Date().toISOString(),
          roles: [],
          flags: 0,
        }),
      ),
      onboarding,
    );
    at('/channels/g1/c1');
    const ob = await screen.findByRole('dialog', { name: 'Set up your time in Alpha Guild' });
    await userEvent.click(within(ob).getByRole('button', { name: 'Skip' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: /Set up your time/ })).toBeNull(),
    );
    expect(useGuideFlags.getState().skipped['g1']).toBe(true);
    expect(localStorage.getItem('sr.onboarded.1.g1')).toBeNull();
    // The questions stay one click away at the top of the channel list.
    expect(screen.getByRole('button', { name: 'Channels & Roles' })).toBeVisible();
  });

  it('does not offer the questions to a new member whose COMPLETED_ONBOARDING flag is set', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({
          pending: false,
          joined_at: new Date().toISOString(),
          roles: [],
          flags: 2,
        }),
      ),
      onboarding,
    );
    at('/channels/g1/c1');
    await waitFor(() => expect(useGuideFlags.getState().memberFlags['g1']).toBe(2));
    expect(screen.queryByRole('dialog', { name: /Set up your time/ })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Channels & Roles' })).toBeNull();
  });

  it('gives a long-standing member without the flag a Channels & Roles entry instead of a modal', async () => {
    seed();
    const answers = vi.fn();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({
          pending: false,
          joined_at: '2020-01-01T00:00:00Z',
          roles: [],
          flags: 0,
        }),
      ),
      onboarding,
      http.post('*/api/v1/guilds/g1/onboarding-responses', async ({ request }) => {
        answers(await request.json());
        return HttpResponse.json({ channel_ids: ['c1'], role_ids: [] });
      }),
    );
    at('/channels/g1/c1');
    const entry = await screen.findByRole('button', { name: 'Channels & Roles' });
    expect(screen.queryByRole('dialog', { name: /Set up your time/ })).toBeNull();
    await userEvent.click(entry);
    const ob = await screen.findByRole('dialog', { name: 'Set up your time in Alpha Guild' });
    await userEvent.click(within(ob).getByRole('checkbox', { name: 'Novels' }));
    await userEvent.click(within(ob).getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(answers).toHaveBeenCalledWith({ option_ids: ['o2'] }));
    await userEvent.click(await screen.findByRole('button', { name: 'Done' }));
    // The flag is set, so the entry goes away.
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Channels & Roles' })).toBeNull(),
    );
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

describe('Security actions (LNG-GLD-034, J-02)', () => {
  const memberMock = http.get('*/api/v1/users/@me/guilds/g1/member', () =>
    HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
  );

  async function openDialog() {
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Security Actions' }));
    return screen.findByRole('dialog', { name: 'Security Actions' });
  }

  it('pauses invites and DMs for 24 hours, and shows the pause in the channel list', async () => {
    seed(bits(10, 11, 40));
    const puts: { invites_disabled_until: string | null; dms_disabled_until: string | null }[] = [];
    server.use(
      guide(false),
      memberMock,
      http.get('*/api/v1/guilds/g1/sr/community', () =>
        HttpResponse.json({ invites_paused: false, raid_auto_pause_invites: true }),
      ),
      http.get('*/api/v1/guilds/g1', () =>
        HttpResponse.json({
          id: 'g1',
          name: 'Alpha Guild',
          incidents_data: {
            invites_disabled_until: null,
            dms_disabled_until: null,
            raid_detected_at: null,
          },
        }),
      ),
      http.put('*/api/v1/guilds/g1/incident-actions', async ({ request }) => {
        const body = (await request.json()) as (typeof puts)[number];
        puts.push(body);
        return HttpResponse.json({ raid_detected_at: null, ...body });
      }),
    );
    const dialog = await openDialog();
    await userEvent.click(
      await within(dialog).findByRole('switch', { name: /Pause invites for 24 hours/ }),
    );
    await waitFor(() => expect(puts).toHaveLength(1));
    const first = puts[0]!;
    expect(first.dms_disabled_until).toBeNull();
    const hours = (Date.parse(first.invites_disabled_until!) - Date.now()) / 3_600_000;
    expect(hours).toBeGreaterThan(23);
    expect(hours).toBeLessThanOrEqual(24);
    // The guild object carries it now, so the channel list says so.
    await waitFor(() =>
      expect(useStore.getState().guilds['g1']?.incidents_data?.invites_disabled_until).toBe(
        first.invites_disabled_until,
      ),
    );
    expect(await screen.findByText(/Invites paused until/)).toBeVisible();

    // The DM pause keeps the invite pause's time and writes its own.
    await userEvent.click(
      within(dialog).getByRole('switch', { name: /Pause direct messages for 24 hours/ }),
    );
    await waitFor(() => expect(puts).toHaveLength(2));
    expect(puts[1]!.invites_disabled_until).toBe(first.invites_disabled_until);
    expect(puts[1]!.dms_disabled_until).not.toBeNull();

    // Turning one off clears only it.
    await userEvent.click(
      within(dialog).getByRole('switch', { name: /Pause invites for 24 hours/ }),
    );
    await waitFor(() => expect(puts).toHaveLength(3));
    expect(puts[2]!.invites_disabled_until).toBeNull();
    expect(puts[2]!.dms_disabled_until).toBe(puts[1]!.dms_disabled_until);
    await waitFor(() => expect(screen.queryByText(/Invites paused until/)).toBeNull());
  });

  it('keeps the indefinite pause as its own switch and the raid alert switch', async () => {
    seed(bits(10, 11, 40));
    const patches: unknown[] = [];
    server.use(
      guide(false),
      memberMock,
      http.get('*/api/v1/guilds/g1/sr/community', () =>
        HttpResponse.json({ invites_paused: false, raid_auto_pause_invites: true }),
      ),
      http.get('*/api/v1/guilds/g1', () => HttpResponse.json({ id: 'g1', name: 'Alpha Guild' })),
      http.patch('*/api/v1/guilds/g1/sr/community', async ({ request }) => {
        const body = (await request.json()) as object;
        patches.push(body);
        return HttpResponse.json({ invites_paused: true, raid_auto_pause_invites: true, ...body });
      }),
    );
    const dialog = await openDialog();
    await userEvent.click(
      await within(dialog).findByRole('switch', { name: /Pause invites until I turn it back on/ }),
    );
    await waitFor(() => expect(patches).toEqual([{ invites_paused: true }]));
    await userEvent.click(
      within(dialog).getByRole('switch', { name: /Pause invites automatically during a raid/ }),
    );
    await waitFor(() => expect(patches[1]).toEqual({ raid_auto_pause_invites: false }));
  });

  it('shows no pause line once the pause time has passed', async () => {
    seed(bits(10, 11, 40));
    useStore.setState((s) => ({
      guilds: {
        g1: {
          ...s.guilds['g1']!,
          incidents_data: {
            invites_disabled_until: new Date(Date.now() - 60_000).toISOString(),
            dms_disabled_until: null,
            raid_detected_at: null,
          },
        },
      },
    }));
    server.use(guide(false), memberMock);
    at('/channels/g1/c1');
    await screen.findByRole('button', { name: /Alpha Guild/ });
    expect(screen.queryByText(/Invites paused until/)).toBeNull();
  });
});

describe('Per-server DM switch (J-01, LNG-GLD-025)', () => {
  const privacy = (overrides: Record<string, boolean> = {}) => ({
    friend_requests_everyone: true,
    friend_requests_friends_of_friends: true,
    friend_requests_server_members: true,
    allow_server_member_dms: true,
    message_requests: true,
    sensitive_media: { friends: 'show', others: 'blur', servers: 'show' },
    dm_spam_filter: 'non_friends',
    server_dm_overrides: overrides,
  });

  it('writes the override from the server menu and clears it again', async () => {
    seed();
    let state = privacy();
    const patches: unknown[] = [];
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      http.get('*/api/v1/users/@me/privacy', () => HttpResponse.json(state)),
      http.patch('*/api/v1/users/@me/privacy', async ({ request }) => {
        const body = (await request.json()) as {
          server_dm_overrides: Record<string, boolean | null>;
        };
        patches.push(body);
        const next = { ...state.server_dm_overrides };
        for (const [id, v] of Object.entries(body.server_dm_overrides)) {
          if (v === null) delete next[id];
          else next[id] = v;
        }
        state = privacy(next);
        return HttpResponse.json(state);
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Privacy Settings' }));
    const dialog = await screen.findByRole('dialog', { name: 'Privacy Settings' });
    const toggle = await within(dialog).findByRole('switch', {
      name: /Allow direct messages from server members/,
    });
    expect(toggle).toBeChecked();
    await userEvent.click(toggle);
    await waitFor(() => expect(patches).toEqual([{ server_dm_overrides: { g1: false } }]));
    await waitFor(() => expect(toggle).not.toBeChecked());
    await userEvent.click(toggle);
    await waitFor(() => expect(patches[1]).toEqual({ server_dm_overrides: { g1: null } }));
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it('is also in the per-server profile dialog and shows an existing override', async () => {
    seed();
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      http.get('*/api/v1/users/@me/privacy', () => HttpResponse.json(privacy({ g1: false }))),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: /Alpha Guild/ }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Edit Per-server Profile' }));
    const dialog = await screen.findByRole('dialog', { name: /profile/i });
    const toggle = await within(dialog).findByRole('switch', {
      name: /Allow direct messages from server members/,
    });
    expect(toggle).not.toBeChecked();
  });
});

describe('Create Server: icon step (LNG-GLD-017)', () => {
  it('keeps a GIF icon as it is and sends it with the create call (J-06)', async () => {
    seed();
    let created: unknown;
    let patched = false;
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
      http.post('*/api/v1/guilds', async ({ request }) => {
        created = await request.json();
        return HttpResponse.json(
          {
            id: 'g5',
            name: 'Fresh',
            owner_id: '1',
            channels: [{ id: 'k2', type: 0, guild_id: 'g5', name: 'general', position: 1 }],
            roles: [],
          },
          { status: 201 },
        );
      }),
      http.patch('*/api/v1/guilds/g5', () => {
        patched = true;
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
    expect((created as { icon: string }).icon).toMatch(/^data:image\/gif;base64,/);
    // The icon no longer needs a second request.
    expect(patched).toBe(false);
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
    expect(await screen.findByRole('menuitem', { name: 'Help' })).toBeVisible(); // D-270: always offered
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Keyboard shortcuts' }));
    expect(await screen.findByRole('dialog', { name: /shortcut/i })).toBeVisible();
  });
});

describe('Server header banner and rail icon (LNG-GLD-019)', () => {
  it('draws the banner behind the server name and the icon in the rail', async () => {
    seed();
    useStore.setState((s) => ({
      guilds: { g1: { ...s.guilds['g1']!, icon: 'iconhash', banner: 'bannerhash' } },
    }));
    server.use(
      guide(false),
      http.get('*/api/v1/users/@me/guilds/g1/member', () =>
        HttpResponse.json({ pending: false, joined_at: '2020-01-01T00:00:00Z', roles: [] }),
      ),
    );
    at('/channels/g1/c1');
    const header = await screen.findByRole('button', {
      name: /Alpha Guild server menu|Alpha Guild/,
    });
    await waitFor(() => expect(header).toHaveAttribute('data-banner'));
    expect(header.style.backgroundImage).toContain('/cdn/icons/g1/bannerhash');
    expect(document.querySelector('img[src*="/cdn/icons/g1/iconhash"]')).not.toBeNull();
  });
});
