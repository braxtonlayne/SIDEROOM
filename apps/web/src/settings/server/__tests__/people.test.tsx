import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  capture,
  http,
  HttpResponse,
  MANAGE,
  memberObj,
  openSettings,
  seed,
  server,
} from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const row = (name: string) => screen.getByRole('row', { name: new RegExp(name) });

describe('Members', () => {
  it('lists and searches members with their roles', async () => {
    seed({ perms: [MANAGE.kick, MANAGE.ban, MANAGE.roles] });
    await openSettings('members');
    expect(await screen.findByText('3 members')).toBeInTheDocument();
    expect(within(row('carol')).getByText('Mod')).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Search members'), 'bo');
    await waitFor(() => expect(screen.getByText('1 member')).toBeInTheDocument());
    expect(screen.queryByText('carol')).toBeNull();
  });

  it('prunes inactive members: previews the count, then removes (F-26 copy)', async () => {
    seed({ perms: [MANAGE.kick] });
    const counts: string[] = [];
    const run = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/prune', ({ request }) => {
        const u = new URL(request.url);
        counts.push(u.search);
        return HttpResponse.json({ pruned: u.searchParams.get('days') === '30' ? 4 : 2 });
      }),
      http.post('*/api/v1/guilds/g1/prune', async ({ request }) => {
        await run.record(request);
        return HttpResponse.json({ pruned: 4 });
      }),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    await userEvent.click(screen.getByRole('button', { name: 'Prune' }));
    const dialog = await screen.findByRole('dialog', { name: 'Prune members' });
    // Until last-seen tracking ships the rule is about messages, not presence.
    expect(dialog).toHaveTextContent(/have not sent a message in 7 days/);
    expect(await within(dialog).findByText('2 members would be removed.')).toBeInTheDocument();
    await userEvent.selectOptions(within(dialog).getByLabelText('Inactive for'), '30');
    expect(await within(dialog).findByText('4 members would be removed.')).toBeInTheDocument();
    expect(counts.at(-1)).toBe('?days=30');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Prune 4 members' }));
    await waitFor(() => expect(run.calls).toHaveLength(1));
    expect(run.calls[0]!.body).toEqual({ days: 30, include_roles: [] });
    expect(await screen.findByText('Pruned 4 members.')).toBeInTheDocument();
  });

  it('hides Prune without Kick Members', async () => {
    seed({ perms: [MANAGE.ban] });
    await openSettings('members');
    await screen.findByText('3 members');
    expect(screen.queryByRole('button', { name: 'Prune' })).toBeNull();
  });

  it('kicks with a reason header; hierarchy and self are blocked', async () => {
    seed({ perms: [MANAGE.kick], position: 3 });
    const cap = capture();
    server.use(
      http.delete('*/api/v1/guilds/g1/members/2', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    // carol (Mod 5) outranks the viewer (3); the viewer cannot kick themselves
    expect(screen.getByRole('button', { name: 'Kick carol' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Kick me' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'Kick bob' }));
    await userEvent.type(screen.getByLabelText('Reason'), 'spam & ads');
    await userEvent.click(
      within(screen.getByRole('dialog', { name: /Kick bob/ })).getByRole('button', {
        name: 'Kick',
      }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(decodeURIComponent(cap.calls[0]!.headers.get('x-audit-log-reason') ?? '')).toBe(
      'spam & ads',
    );
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Kick bob' })).toBeNull());
  });

  it('bans with reason and a message-delete window', async () => {
    seed({ perms: [MANAGE.ban], position: 9 });
    const cap = capture();
    server.use(
      http.put('*/api/v1/guilds/g1/bans/2', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    await userEvent.click(screen.getByRole('button', { name: 'Ban bob' }));
    await userEvent.selectOptions(screen.getByLabelText('Delete message history'), '604800');
    await userEvent.type(screen.getByLabelText('Reason'), 'raid');
    await userEvent.click(
      within(screen.getByRole('dialog', { name: /Ban bob/ })).getByRole('button', { name: 'Ban' }),
    );
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(cap.calls[0]!.body).toEqual({ delete_message_seconds: 604800, reason: 'raid' });
  });

  it('times a member out and shows server errors', async () => {
    seed({ perms: [MANAGE.moderate], position: 9 });
    const cap = capture();
    server.use(
      http.patch('*/api/v1/guilds/g1/members/2', async ({ request }) => {
        await cap.record(request);
        return HttpResponse.json(
          memberObj('2', 'bob', ['low'], { communication_disabled_until: '2099-01-01T00:00:00Z' }),
        );
      }),
      http.patch('*/api/v1/guilds/g1/members/3', () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    await userEvent.click(screen.getByRole('button', { name: 'Timeout bob' }));
    await userEvent.click(screen.getByRole('button', { name: 'Timeout' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    const until = (cap.calls[0]!.body as { communication_disabled_until: string })
      .communication_disabled_until;
    expect(new Date(until).getTime()).toBeGreaterThan(Date.now());
    await userEvent.click(await screen.findByRole('button', { name: 'Timeout carol' }));
    await userEvent.click(screen.getByRole('button', { name: 'Timeout' }));
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });

  it('assigns a role from the roles dialog', async () => {
    seed({ perms: [MANAGE.roles], position: 9 });
    const cap = capture();
    server.use(
      http.put('*/api/v1/guilds/g1/members/2/roles/mod', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    await userEvent.click(screen.getByRole('button', { name: 'Roles bob' }));
    const dialog = screen.getByRole('dialog', { name: /Roles for bob/ });
    expect(within(dialog).getByRole('checkbox', { name: /Admin/ })).toBeDisabled();
    await userEvent.click(within(dialog).getByRole('checkbox', { name: 'Mod' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
  });
});

describe('Bans', () => {
  const bans = [
    { reason: 'spam', user: { id: '20', username: 'spammer' } },
    { reason: null, user: { id: '21', username: 'troll' } },
  ];

  it('lists, searches by reason, and revokes with confirmation', async () => {
    seed({ perms: [MANAGE.ban] });
    const cap = capture();
    server.use(
      http.get('*/api/v1/guilds/g1/bans', () => HttpResponse.json(bans)),
      http.delete('*/api/v1/guilds/g1/bans/20', async ({ request }) => {
        await cap.record(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('bans');
    expect(await screen.findByText('Reason: spam')).toBeInTheDocument();
    expect(screen.getByText('No reason given')).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Search bans'), 'troll');
    expect(screen.queryByText('spammer')).toBeNull();
    await userEvent.clear(screen.getByLabelText('Search bans'));
    await userEvent.click(screen.getByRole('button', { name: 'Revoke ban for spammer' }));
    await userEvent.click(screen.getAllByRole('button', { name: 'Revoke ban' }).at(-1)!);
    await waitFor(() => expect(cap.calls).toHaveLength(1));
  });

  it('shows an empty state and a load error with retry', async () => {
    seed({ perms: [MANAGE.ban] });
    let fail = true;
    server.use(
      http.get('*/api/v1/guilds/g1/bans', () =>
        fail ? HttpResponse.json({ message: 'boom' }, { status: 500 }) : HttpResponse.json([]),
      ),
    );
    await openSettings('bans');
    expect(await screen.findByText('boom')).toBeInTheDocument();
    fail = false;
    await userEvent.click(screen.getByRole('button', { name: 'Try again' }));
    expect(await screen.findByText('No bans')).toBeInTheDocument();
  });
});
