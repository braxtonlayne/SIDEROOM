import { screen, waitFor } from '@testing-library/react';
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
