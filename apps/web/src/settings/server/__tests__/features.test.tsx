import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const starboard = (over: object = {}) => ({
  id: 'lounge.starboard',
  name: 'Starboard',
  description: 'Repost popular messages.',
  section: 'Lounge',
  phase: 11,
  enabled: false,
  default_enabled: false,
  platform_disabled: false,
  settings: { emoji: '*', threshold: 3 },
  schema: {
    fields: [
      { key: 'channel', label: 'Starboard channel', type: 'channel_picker', multi: false },
      { key: 'emoji', label: 'Emoji', type: 'string', max_len: 64, default: '*' },
      { key: 'threshold', label: 'Reactions needed', type: 'int', min: 1, max: 100, default: 3 },
    ],
  },
  ...over,
});

const sticky = {
  id: 'lounge.sticky',
  name: 'Sticky messages',
  description: 'Keep a message at the bottom.',
  section: 'Lounge',
  phase: 11,
  enabled: true,
  default_enabled: false,
  platform_disabled: false,
  settings: { stickies: [] },
  schema: {
    fields: [
      {
        key: 'stickies',
        label: 'Sticky messages',
        type: 'list',
        max_items: 25,
        item: {
          type: 'group',
          fields: [
            { key: 'channel', label: 'Channel', type: 'channel_picker', multi: false },
            { key: 'content', label: 'Message', type: 'string', max_len: 2000 },
            { key: 'every_messages', label: 'Re-post after', type: 'int', default: 5 },
          ],
        },
      },
    ],
  },
};

const tempRoles = (over: object = {}) => ({
  id: 'moderation.temp_roles',
  name: 'Temporary roles',
  description: 'Grant a role for a limited time.',
  section: 'Moderation',
  phase: 11,
  enabled: true,
  default_enabled: false,
  platform_disabled: false,
  settings: {},
  schema: { fields: [] },
  ...over,
});

describe('Server settings > Features', () => {
  it('is hidden without Manage Server', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([])));
    await openSettings('channels');
    expect(screen.queryByRole('button', { name: 'Features' })).toBeNull();
  });

  it('lists the modules grouped by section and switches one on', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () =>
        HttpResponse.json([starboard(), tempRoles({ enabled: false })]),
      ),
      http.patch('*/api/v1/guilds/g1/sr/features/lounge.starboard', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(starboard({ enabled: true }));
      }),
    );
    await openSettings('features');
    expect(await screen.findByRole('heading', { name: 'Starboard' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Lounge' })).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'Moderation' })).toBeInTheDocument();
    const sw = screen.getByRole('switch', { name: 'Enable Starboard' });
    expect(sw).not.toBeChecked();
    await userEvent.click(sw);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ enabled: true });
    await waitFor(() =>
      expect(screen.getByRole('switch', { name: 'Enable Starboard' })).toBeChecked(),
    );
  });

  it('edits settings with the server channels as choices and saves only the overrides', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([starboard()])),
      http.patch('*/api/v1/guilds/g1/sr/features/lounge.starboard', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(
          starboard({ settings: { emoji: '*', threshold: 5, channel: 'c2' } }),
        );
      }),
    );
    await openSettings('features');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Show settings for Starboard' }),
    );
    const save = screen.getByRole('button', { name: 'Save Starboard' });
    expect(save).toBeDisabled();
    await userEvent.click(screen.getByRole('combobox', { name: 'Starboard channel' }));
    await userEvent.click(await screen.findByRole('option', { name: 'random' }));
    const threshold = screen.getByLabelText(/Reactions needed/);
    await userEvent.clear(threshold);
    await userEvent.type(threshold, '5');
    expect(save).toBeEnabled();
    await userEvent.click(save);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ settings: { channel: 'c2', threshold: 5 } });
    await screen.findByText('Saved.');
  });

  it('edits a list of groups as rows', async () => {
    seed({ perms: [MANAGE.guild] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([sticky])),
      http.patch('*/api/v1/guilds/g1/sr/features/lounge.sticky', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({ ...sticky, settings: { stickies: [] } });
      }),
    );
    await openSettings('features');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Show settings for Sticky messages' }),
    );
    const add = screen.getByRole('button', { name: 'Add to Sticky messages' });
    await userEvent.click(add);
    await userEvent.click(screen.getByRole('button', { name: 'Remove Sticky messages 1' }));
    expect(screen.queryByRole('group', { name: 'Sticky messages 1' })).toBeNull();
    await userEvent.click(add);
    const row = await screen.findByRole('group', { name: 'Sticky messages 1' });
    await userEvent.type(within(row).getByLabelText('Message'), 'Read the rules');
    await userEvent.click(screen.getByRole('button', { name: 'Save Sticky messages' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    // The new row is seeded with the schema defaults; the unset channel is not sent.
    expect(cap.calls[0]!.body).toEqual({
      settings: { stickies: [{ content: 'Read the rules', every_messages: 5 }] },
    });
  });

  it('warns when the platform switched a module off', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () =>
        HttpResponse.json([starboard({ platform_disabled: true })]),
      ),
    );
    await openSettings('features');
    expect(await screen.findByText(/switched this feature off for every server/)).toBeVisible();
  });

  it('shows the server error when a save is refused', async () => {
    seed({ perms: [MANAGE.guild] });
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([starboard()])),
      http.patch('*/api/v1/guilds/g1/sr/features/lounge.starboard', () =>
        HttpResponse.json({ code: 50035, message: 'Invalid Form Body' }, { status: 400 }),
      ),
    );
    await openSettings('features');
    await userEvent.click(await screen.findByRole('switch', { name: 'Enable Starboard' }));
    expect(await screen.findByText(/Invalid Form Body/)).toBeInTheDocument();
  });

  it('lists, gives and ends temporary roles', async () => {
    seed({ perms: [MANAGE.guild, MANAGE.roles] });
    const give = capture();
    const end = capture();
    let grants = [{ user_id: '2', role_id: 'low', expires_at: Date.UTC(2030, 0, 2, 12) }];
    server.use(
      http.get('*/api/v1/guilds/g1/sr/features', () => HttpResponse.json([tempRoles()])),
      http.get('*/api/v1/guilds/g1/sr/temp-roles', () => HttpResponse.json(grants)),
      http.post('*/api/v1/guilds/g1/sr/temp-roles', async ({ request }) => {
        await give.record(request);
        grants = [
          ...grants,
          { user_id: '3', role_id: 'mod', expires_at: Date.UTC(2030, 0, 3, 12) },
        ];
        return HttpResponse.json(grants[1], { status: 201 });
      }),
      http.delete('*/api/v1/guilds/g1/sr/temp-roles/2/low', async ({ request }) => {
        await end.record(request);
        grants = grants.filter((g) => g.user_id !== '2');
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('features');
    const panel = await screen.findByRole('region', { name: 'Temporary roles' });
    expect(await within(panel).findByText(/bob has Member until/)).toBeInTheDocument();

    await userEvent.click(within(panel).getByRole('combobox', { name: 'Member' }));
    await userEvent.click(await screen.findByRole('option', { name: 'carol' }));
    await userEvent.click(within(panel).getByRole('combobox', { name: 'Role' }));
    await userEvent.click(await screen.findByRole('option', { name: 'Mod' }));
    await userEvent.click(within(panel).getByRole('button', { name: 'Give role' }));
    await waitFor(() => expect(give.calls).toHaveLength(1));
    expect(give.calls[0]!.body).toEqual({
      user_id: '3',
      role_id: 'mod',
      duration_seconds: 86400,
    });
    expect(await within(panel).findByText(/carol has Mod until/)).toBeInTheDocument();

    await userEvent.click(within(panel).getByRole('button', { name: 'End Member for bob' }));
    await waitFor(() => expect(end.calls).toHaveLength(1));
    await waitFor(() => expect(within(panel).queryByText(/bob has Member/)).toBeNull());
  });
});
