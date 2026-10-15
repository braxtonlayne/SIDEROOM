import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { overwriteApplies } from '../sections/Channels';
import { bits, capture, http, HttpResponse, MANAGE, openSettings, seed, server } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const ch = (id: string, name: string, type: number, extra: object = {}) => ({
  id,
  type,
  guild_id: 'g1',
  name,
  topic: null,
  position: 0,
  parent_id: null,
  nsfw: false,
  rate_limit_per_user: 0,
  permission_overwrites: [],
  ...extra,
});

const channels = (over: Record<string, object> = {}) => [
  ch('cat', 'Text', 4, {
    permission_overwrites: [{ id: 'g1', type: 0, allow: '0', deny: bits(11) }],
    ...over.cat,
  }),
  ch('c1', 'general', 0, { parent_id: 'cat', position: 0, ...over.c1 }),
  ch('c2', 'random', 0, { parent_id: 'cat', position: 1, ...over.c2 }),
];

describe('Channels', () => {
  it('shows the tree and reorders siblings with contiguous positions', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.patch('*/api/v1/guilds/g1/channels', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('channels');
    expect(await screen.findByRole('button', { name: '# general' })).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Move general down' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual([
      { id: 'c2', position: 0 },
      { id: 'c1', position: 1 },
    ]);
    expect(screen.getByRole('button', { name: 'Move general up' })).toBeDisabled();
  });

  it('renames a channel and sends only the change; validates empty names', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.patch('*/api/v1/channels/c1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(ch('c1', 'lobby', 0));
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    const name = await screen.findByLabelText('Channel name');
    await userEvent.clear(name);
    expect(screen.getByText('Name must be between 1 and 100 characters.')).toBeInTheDocument();
    await userEvent.type(name, 'lobby');
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ name: 'lobby' });
  });

  it('surfaces a delete failure', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.delete('*/api/v1/channels/c2', () =>
        HttpResponse.json({ message: 'nope' }, { status: 403 }),
      ),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# random' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete channel' }));
    await userEvent.click(screen.getAllByRole('button', { name: 'Delete channel' }).at(-1)!);
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });

  it('edits a role overwrite tri-state and saves allow/deny', async () => {
    seed({ perms: [MANAGE.channels, MANAGE.roles, 10, 11] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json(
          channels({
            c1: { permission_overwrites: [{ id: 'low', type: 0, allow: bits(11), deny: '0' }] },
          }),
        ),
      ),
      http.put('*/api/v1/channels/c1/permissions/low', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Permissions' }));
    expect(await screen.findByRole('radio', { name: 'Send Messages: Allow' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await userEvent.click(screen.getByRole('radio', { name: 'View Channels: Deny' }));
    await userEvent.click(screen.getByRole('button', { name: 'Save changes' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ type: 0, allow: bits(11), deny: bits(10) });
  }, 30000);

  it('is read-only without Manage Roles, and syncs with the category', async () => {
    seed({ perms: [MANAGE.channels] });
    server.use(http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())));
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Permissions' }));
    expect(await screen.findByText(/need the Manage Roles permission/)).toBeInTheDocument();
    expect(screen.getByRole('switch', { name: /Sync permissions with category/ })).toBeDisabled();
  });

  it('sync with category sends lock_permissions', async () => {
    seed({ perms: [MANAGE.channels, MANAGE.roles] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.patch('*/api/v1/channels/c1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(ch('c1', 'general', 0));
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Permissions' }));
    await userEvent.click(
      await screen.findByRole('switch', { name: /Sync permissions with category/ }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ lock_permissions: true });
  });
});

describe('Channel editor extra tabs (LNG-CHN-017)', () => {
  const invite = (code: string) => ({
    code,
    guild: { id: 'g1', name: 'Book Club' },
    channel: { id: 'c1', name: 'general', type: 0 },
    inviter: { id: '3', username: 'carol' },
    uses: 2,
    max_uses: 5,
    max_age: 0,
    created_at: '2026-01-02T00:00:00Z',
    expires_at: null,
  });
  const hook = (id: string, name: string, channel: string) => ({
    id,
    type: 1,
    guild_id: 'g1',
    channel_id: channel,
    name,
    avatar: null,
    user: { id: '3', username: 'carol' },
  });

  async function openEditor(name = '# general') {
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name }));
  }

  it('lists, creates and revokes the channel invites', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/channels/c1/invites', () => HttpResponse.json([invite('abc123')])),
      http.post('*/api/v1/channels/c1/invites', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(invite('new999'));
      }),
      http.delete('*/api/v1/invites/abc123', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(invite('abc123'));
      }),
    );
    await openEditor();
    await userEvent.click(await screen.findByRole('tab', { name: 'Invites' }));
    const list = await screen.findByRole('list', { name: 'Invites to this channel' });
    expect(within(list).getByText('abc123')).toBeInTheDocument();
    expect(within(list).getByText(/2 \/ 5 uses/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Create invite' }));
    expect(await within(list).findByText('new999')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Revoke invite abc123' }));
    await waitFor(() => expect(within(list).queryByText('abc123')).not.toBeInTheDocument());
    expect(cap.calls.map((c) => c.method)).toEqual(['POST', 'DELETE']);
  });

  it('shows only this channel webhooks, creates one and shows its URL once', async () => {
    seed({ perms: [MANAGE.channels, MANAGE.webhooks] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.get('*/api/v1/guilds/g1/webhooks', () =>
        HttpResponse.json([hook('w1', 'Deploy bot', 'c1'), hook('w2', 'Elsewhere', 'c2')]),
      ),
      http.post('*/api/v1/channels/c1/webhooks', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json({
          ...hook('w3', 'Build bot', 'c1'),
          url: '/api/v1/webhooks/w3/tok',
        });
      }),
      http.delete('*/api/v1/webhooks/w1', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openEditor();
    await userEvent.click(await screen.findByRole('tab', { name: 'Integrations' }));
    const list = await screen.findByRole('list', { name: 'Webhooks in this channel' });
    expect(within(list).getByText('Deploy bot')).toBeInTheDocument();
    expect(within(list).queryByText('Elsewhere')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Create webhook' }));
    await userEvent.type(await screen.findByLabelText('Webhook name'), 'Build bot');
    await userEvent.click(screen.getByRole('button', { name: 'Create' }));
    expect(await screen.findByText(/\/api\/v1\/webhooks\/w3\/tok/)).toBeInTheDocument();
    expect(cap.calls[0]!.body).toEqual({ name: 'Build bot' });
    await userEvent.click(screen.getByRole('button', { name: 'Delete webhook Deploy bot' }));
    await userEvent.click(screen.getAllByRole('button', { name: 'Delete webhook' }).at(-1)!);
    await waitFor(() => expect(within(list).queryByText('Deploy bot')).not.toBeInTheDocument());
  });

  it('saves the thread defaults from the Features tab and hides the tabs for categories', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () => HttpResponse.json(channels())),
      http.patch('*/api/v1/channels/c1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(ch('c1', 'general', 0));
      }),
    );
    await openEditor();
    await userEvent.click(await screen.findByRole('tab', { name: 'Features' }));
    await userEvent.selectOptions(
      await screen.findByLabelText('Hide threads after inactivity'),
      '4320',
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ default_auto_archive_duration: 4320 });
    await userEvent.selectOptions(screen.getByLabelText('Slowmode for new threads'), '30');
    await waitFor(() => expect(cap.calls).toHaveLength(2));
    expect(cap.calls[1]!.body).toEqual({ default_thread_rate_limit_per_user: 30 });
    await userEvent.click(screen.getByRole('button', { name: 'Text' }));
    await waitFor(() => expect(screen.queryByRole('tab', { name: 'Features' })).toBeNull());
    expect(screen.getByRole('tab', { name: 'Permissions' })).toBeInTheDocument();
  });
});

describe('Forum settings', () => {
  it('edits tags, sort and layout on the Features tab of a forum channel', async () => {
    seed({ perms: [MANAGE.channels] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/channels', () =>
        HttpResponse.json([
          ch('cat', 'Text', 4),
          ch('c1', 'general', 15, {
            parent_id: 'cat',
            available_tags: [{ id: 't1', name: 'Question', moderated: false }],
          }),
        ]),
      ),
      http.patch('*/api/v1/channels/c1', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(ch('c1', 'general', 15));
      }),
    );
    await openSettings('channels');
    await userEvent.click(await screen.findByRole('button', { name: '# general' }));
    await userEvent.click(await screen.findByRole('tab', { name: 'Features' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Add tag' }));
    await userEvent.type(screen.getByLabelText('Tag 2 name'), 'Bug');
    await userEvent.selectOptions(screen.getByLabelText('Default sort order'), '1');
    await userEvent.selectOptions(screen.getByLabelText('Default layout'), '2');
    await userEvent.click(screen.getByRole('button', { name: 'Save forum settings' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({
      available_tags: [{ id: 't1', name: 'Question', moderated: false }, { name: 'Bug' }],
      default_reaction_emoji: null,
      default_sort_order: 1,
      default_forum_layout: 2,
    });
  });
});

describe('overwriteApplies', () => {
  it('hides voice bits on text channels and text bits on voice channels', () => {
    const text = ch('a', 'a', 0) as never;
    const voice = ch('b', 'b', 2) as never;
    expect(overwriteApplies('SEND_MESSAGES', text)).toBe(true);
    expect(overwriteApplies('SPEAK', text)).toBe(false);
    expect(overwriteApplies('SPEAK', voice)).toBe(true);
    expect(overwriteApplies('SEND_MESSAGES', voice)).toBe(false);
    expect(overwriteApplies('VIEW_CHANNEL', voice)).toBe(true);
    expect(overwriteApplies('BAN_MEMBERS', text)).toBe(false);
  });
});
