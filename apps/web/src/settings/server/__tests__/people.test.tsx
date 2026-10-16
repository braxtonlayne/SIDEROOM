import { render, screen, waitFor, within } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../../router';
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

  it('asks the server for the search text and the role filter, and shows member details', async () => {
    seed({ perms: [MANAGE.kick, MANAGE.ban, MANAGE.roles] });
    const urls: string[] = [];
    server.events.on('request:start', ({ request }) => {
      if (request.url.includes('/guilds/g1/members?')) urls.push(new URL(request.url).search);
    });
    await openSettings('members');
    expect(await screen.findByText('3 members')).toBeInTheDocument();
    await userEvent.selectOptions(screen.getByLabelText('Filter by role'), 'mod');
    await waitFor(() => expect(screen.getByText('1 member')).toBeInTheDocument());
    expect(urls.at(-1)).toContain('role_id=mod');
    expect(within(row('carol')).getByText('Mod')).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText('Search members'), 'zzz');
    expect(await screen.findByText('No members match your search.')).toBeInTheDocument();
    expect(urls.at(-1)).toContain('query=zzz');

    await userEvent.clear(screen.getByLabelText('Search members'));
    await userEvent.selectOptions(screen.getByLabelText('Filter by role'), '');
    await screen.findByText('3 members');
    await userEvent.click(screen.getByRole('button', { name: 'Details for carol' }));
    const dialog = await screen.findByRole('dialog', { name: 'carol' });
    expect(within(dialog).getByText('Not timed out')).toBeInTheDocument();
    expect(within(dialog).getByText('3')).toBeInTheDocument();
    expect(within(dialog).getByText('Mod')).toBeInTheDocument();
  });

  it('prunes inactive members: previews the count, then removes (Discord wording, F-26)', async () => {
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
    // Last-seen tracking shipped (F-02), so the rule is about presence, as on Discord.
    expect(dialog).toHaveTextContent(/have not been online for 7 days/);
    expect(dialog).not.toHaveTextContent(/sent a message/);
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

  it('bulk-kicks the selected members, skipping those it cannot moderate', async () => {
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
    await userEvent.click(screen.getByRole('checkbox', { name: 'Select bob' }));
    await userEvent.click(screen.getByRole('checkbox', { name: 'Select carol' }));
    const bar = screen.getByRole('toolbar', { name: 'Bulk actions' });
    expect(within(bar).getByText('2 members selected')).toBeInTheDocument();
    expect(within(bar).queryByRole('button', { name: 'Add role' })).toBeNull();
    await userEvent.click(within(bar).getByRole('button', { name: 'Kick' }));
    const dialog = await screen.findByRole('dialog', { name: /Kick: 1 member/ });
    await userEvent.type(within(dialog).getByLabelText('Reason'), 'cleanup');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Kick' }));
    await waitFor(() => expect(cap.calls).toHaveLength(1));
    expect(await screen.findByText('Done for 1 member.')).toBeInTheDocument();
    expect(screen.queryByRole('checkbox', { name: 'Select bob' })).toBeNull();
  });

  it('bulk-adds a role and keeps the failed members selected', async () => {
    seed({ perms: [MANAGE.roles], position: 9 });
    const puts: string[] = [];
    server.use(
      http.put('*/api/v1/guilds/g1/members/:uid/roles/:rid', ({ params }) => {
        puts.push(`${String(params.uid)}:${String(params.rid)}`);
        return params.uid === '3'
          ? HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 })
          : new HttpResponse(null, { status: 204 });
      }),
    );
    await openSettings('members');
    await screen.findByText('3 members');
    await userEvent.click(
      screen.getByRole('checkbox', { name: 'Select all members on this page' }),
    );
    await userEvent.click(screen.getByRole('button', { name: 'Add role' }));
    const dialog = await screen.findByRole('dialog', { name: /Add role: 3 members/ });
    await userEvent.selectOptions(within(dialog).getByLabelText('Role'), 'low');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Add role' }));
    expect(await screen.findByText(/Done for 2, failed for 1/)).toBeInTheDocument();
    expect(puts.sort()).toEqual(['1:low', '2:low', '3:low']);
    expect(screen.getByText('1 member selected')).toBeInTheDocument();
    expect(screen.getByRole('checkbox', { name: 'Select carol' })).toBeChecked();
  });

  it('sorts the loaded members by when they joined', async () => {
    seed({ perms: [MANAGE.kick] });
    const joined = (id: string, name: string, at: string) =>
      memberObj(id, name, [], { joined_at: at });
    await openSettings('members');
    server.use(
      http.get('*/api/v1/guilds/g1/members', () =>
        HttpResponse.json([
          joined('1', 'me', '2026-03-01T00:00:00Z'),
          joined('2', 'bob', '2026-01-01T00:00:00Z'),
          joined('3', 'carol', '2026-02-01T00:00:00Z'),
        ]),
      ),
    );
    // Changing the role filter makes the page load again, now from the handler above.
    await userEvent.selectOptions(screen.getByLabelText('Filter by role'), 'mod');
    await screen.findByText('3 members');
    const names = () =>
      screen.getAllByRole('button', { name: /^Details for / }).map((b) => b.textContent);
    await userEvent.selectOptions(screen.getByLabelText('Sort by'), 'oldest');
    expect(names()).toEqual(['bob', 'carol', 'me']);
    await userEvent.selectOptions(screen.getByLabelText('Sort by'), 'newest');
    expect(names()).toEqual(['me', 'carol', 'bob']);
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

describe('Members: join source and signals (H-37, LNG-SSET-027)', () => {
  it('shows how members joined and their signals to managers, and filters on unusual activity', async () => {
    seed({ perms: [MANAGE.guild, MANAGE.kick] });
    const urls: string[] = [];
    server.events.on('request:start', ({ request }) => {
      if (request.url.includes('/guilds/g1/members?')) urls.push(new URL(request.url).search);
    });
    server.use(
      http.get('*/api/v1/guilds/:gid/members', ({ request }) => {
        const flagged = new URL(request.url).searchParams.get('signals') === 'true';
        const all = [
          memberObj('1', 'me', ['mine'], { sr_join: { source: 'creator' }, sr_signals: [] }),
          memberObj('2', 'bob', ['low'], {
            sr_join: { source: 'invite', invite_code: 'abc123', inviter_id: '1' },
            sr_signals: [],
          }),
          memberObj('3', 'carol', ['mod'], {
            sr_join: { source: 'directory' },
            sr_signals: ['new_account', 'rejoined'],
          }),
        ];
        return HttpResponse.json(flagged ? all.filter((m) => m.sr_signals.length) : all);
      }),
    );
    // Not openSettings: its base handlers would answer the member list ahead of this one.
    server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
    render(
      <RouterProvider
        router={createAppRouter(
          createMemoryHistory({ initialEntries: ['/settings/server/g1/members'] }),
        )}
      />,
    );
    expect(await screen.findByText('3 members', {}, { timeout: 10_000 })).toBeInTheDocument();
    expect(within(row('bob')).getByText('Invite abc123')).toBeInTheDocument();
    expect(within(row('carol')).getByText('Server Directory')).toBeInTheDocument();
    expect(within(row('carol')).getByText('New account')).toBeInTheDocument();
    expect(within(row('carol')).getByText('Rejoined')).toBeInTheDocument();
    expect(screen.getByText('Created the server')).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText('Unusual activity only'));
    await waitFor(() => expect(screen.getByText('1 member')).toBeInTheDocument());
    expect(urls.at(-1)).toContain('signals=true');
    expect(screen.queryByText('bob')).toBeNull();
  });

  it('leaves the columns and the filter out without MANAGE_GUILD', async () => {
    seed({ perms: [MANAGE.kick] });
    await openSettings('members');
    expect(await screen.findByText('3 members')).toBeInTheDocument();
    expect(screen.queryByRole('columnheader', { name: 'Joined via' })).toBeNull();
    expect(screen.queryByLabelText('Unusual activity only')).toBeNull();
  });
});
