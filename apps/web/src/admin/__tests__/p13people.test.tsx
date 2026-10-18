// ADM-PPL-001/002/004, ADM-GRAPH-004, ADM-COM-002: the staff routes added in phase 13.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  account,
  asStaff,
  detail,
  http,
  HttpResponse,
  openAdmin,
  readJson,
  server,
} from './helpers';

const SLOW = { timeout: 10_000 };

async function openDrawer(
  perms: Parameters<typeof asStaff>[0]['perms'],
  over: Record<string, unknown> = {},
) {
  asStaff({ perms });
  server.use(
    http.get('*/api/v1/admin/accounts', () =>
      HttpResponse.json({ items: [account()], next_after: null }),
    ),
    http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail(over))),
  );
  openAdmin('accounts');
  const user = userEvent.setup();
  await user.click(await screen.findByRole('button', { name: 'View alice' }, SLOW));
  await screen.findByText('Active sessions', {}, SLOW);
  return user;
}

describe('accounts search by Discord id and IP (ADM-PPL-001)', () => {
  it('sends the typed text as discord_id or ip when chosen', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    const queries: string[] = [];
    server.use(
      http.get('*/api/v1/admin/accounts', ({ request }) => {
        queries.push(new URL(request.url).search);
        return HttpResponse.json({ items: [account()], next_after: null });
      }),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await screen.findByRole('button', { name: 'View alice' }, SLOW);
    await user.click(screen.getByRole('combobox', { name: 'Search by' }));
    await user.click(await screen.findByRole('option', { name: 'Discord id' }));
    await user.type(screen.getByLabelText('Search accounts'), '1234567890');
    await user.click(screen.getByRole('button', { name: 'Search' }));
    await waitFor(() =>
      expect(queries.some((q) => q.includes('discord_id=1234567890'))).toBe(true),
    );
    await user.click(screen.getByRole('combobox', { name: 'Search by' }));
    await user.click(await screen.findByRole('option', { name: 'IP address' }));
    await user.click(screen.getByRole('button', { name: 'Search' }));
    await waitFor(() => expect(queries.some((q) => q.includes('ip=1234567890'))).toBe(true));
  });
});

describe('account drawer additions (ADM-PPL-002, ADM-GRAPH-004)', () => {
  it('reads sessions and servers only on request', async () => {
    const hits = { sessions: 0, guilds: 0 };
    server.use(
      http.get('*/api/v1/admin/accounts/100/sessions', () => {
        hits.sessions += 1;
        return HttpResponse.json([
          {
            id: '1',
            created_at_ms: 1_700_000_000_000,
            last_seen_at_ms: 1_700_000_100_000,
            expires_at_ms: 1_800_000_000_000,
            ip: '203.0.113.9',
            user_agent: 'Firefox',
            device_label: 'Work laptop',
          },
        ]);
      }),
      http.get('*/api/v1/admin/accounts/100/guilds', () => {
        hits.guilds += 1;
        return HttpResponse.json([
          {
            id: '9',
            name: 'Garden',
            member_count: 12,
            joined_at_ms: 1_700_000_000_000,
            owner: true,
          },
        ]);
      }),
    );
    const user = await openDrawer(['STAFF_BASIC', 'MANAGE_ACCOUNTS']);
    expect(hits).toEqual({ sessions: 0, guilds: 0 });
    await user.click(screen.getByRole('button', { name: 'Show sessions' }));
    expect(await screen.findByText('Work laptop', {}, SLOW)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Show servers' }));
    expect(await screen.findByText('Garden', {}, SLOW)).toBeInTheDocument();
    expect(screen.getByText('Owner')).toBeInTheDocument();
    expect(hits).toEqual({ sessions: 1, guilds: 1 });
  });

  it('freezes invites with a reason and shows the unfreeze button afterwards', async () => {
    let frozen = false;
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/accounts/100/credits', () =>
        HttpResponse.json({ balance: 3, entries: [] }),
      ),
      http.get('*/api/v1/admin/accounts/100/invite-tree', () =>
        HttpResponse.json({
          account: { id: '100', username: 'alice' },
          ancestors: [],
          stats: { descendants: 0, banned_descendants: 0, open: 0 },
        }),
      ),
      http.post('*/api/v1/admin/accounts/100/freeze-invites', async ({ request }) => {
        sent = await readJson(request);
        frozen = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = await openDrawer(['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES']);
    // After the freeze the drawer re-reads the account, which now reports it frozen.
    server.use(
      http.get('*/api/v1/admin/accounts/100', () =>
        HttpResponse.json(detail({ invites_frozen: frozen })),
      ),
    );
    await user.click(await screen.findByRole('button', { name: 'Freeze invites' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: /Freeze invites for @alice/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'invite farming');
    await user.click(within(dialog).getByRole('button', { name: 'Freeze invites' }));
    await waitFor(() => expect(sent).toEqual({ reason: 'invite farming' }));
    expect(
      await screen.findByRole('button', { name: 'Unfreeze invites' }, SLOW),
    ).toBeInTheDocument();
  });

  it('unlinks an account from its Person', async () => {
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/accounts/100/unlink', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ person_id: '777' });
      }),
    );
    const user = await openDrawer(['STAFF_BASIC', 'MANAGE_ACCOUNTS']);
    await user.click(screen.getByRole('button', { name: 'Unlink from Person' }));
    const dialog = await screen.findByRole('dialog', { name: /Unlink @alice/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'wrongly merged');
    await user.click(within(dialog).getByRole('button', { name: 'Unlink from Person' }));
    await waitFor(() => expect(sent).toEqual({ reason: 'wrongly merged' }));
    expect(await screen.findByText(/now belongs to Person 777/, {}, SLOW)).toBeInTheDocument();
  });

  it('shows the 409 message when the Person is banned', async () => {
    server.use(
      http.post('*/api/v1/admin/accounts/100/unlink', () =>
        HttpResponse.json({ message: 'The Person is banned.' }, { status: 409 }),
      ),
    );
    const user = await openDrawer(['STAFF_BASIC', 'MANAGE_ACCOUNTS']);
    await user.click(screen.getByRole('button', { name: 'Unlink from Person' }));
    const dialog = await screen.findByRole('dialog', { name: /Unlink @alice/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Unlink from Person' }));
    expect(await within(dialog).findByText('The Person is banned.')).toBeInTheDocument();
  });
});

describe('Person record additions (ADM-PPL-004)', () => {
  const person = (over: Record<string, unknown> = {}) => ({
    id: '500',
    accounts: [account()],
    ban: null,
    notes: 'watch this one',
    suspicion_level: 2,
    ...over,
  });

  async function openPerson() {
    server.use(
      http.get('*/api/v1/admin/persons/500', () => HttpResponse.json(person())),
      http.get('*/api/v1/admin/persons/500/suspicion-log', () =>
        HttpResponse.json([
          {
            id: '1',
            created_at_ms: 1_700_000_000_000,
            delta: 2,
            level_after: 2,
            reason: 'invited a banned account',
            actor_id: null,
            source_account_id: '9',
          },
        ]),
      ),
    );
    const user = await openDrawer(['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES']);
    await user.click(screen.getByRole('button', { name: 'Open person record' }));
    await screen.findByRole('list', { name: 'Account chain' }, SLOW);
    return user;
  }

  it('lists the suspicion log, edits the notes and links an account', async () => {
    let notes: unknown;
    let link: unknown;
    server.use(
      http.put('*/api/v1/admin/persons/500/notes', async ({ request }) => {
        notes = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
      http.post('*/api/v1/admin/persons/500/link', async ({ request }) => {
        link = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = await openPerson();
    expect(
      await screen.findByText('invited a banned account', { exact: false }),
    ).toBeInTheDocument();
    expect(screen.getByText('+2 (level now 2)')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Edit notes' }));
    const nd = await screen.findByRole('dialog', { name: 'Edit notes' });
    const box = within(nd).getByLabelText('Notes');
    await user.clear(box);
    await user.type(box, 'confirmed duplicate');
    await user.click(within(nd).getByRole('button', { name: 'Save notes' }));
    await waitFor(() => expect(notes).toEqual({ notes: 'confirmed duplicate' }));
    expect(await screen.findByText('Notes saved.')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: 'Link an account' }));
    const ld = await screen.findByRole('dialog', { name: 'Link an account' });
    const confirm = within(ld).getByRole('button', { name: 'Link account' });
    expect(confirm).toBeDisabled();
    await user.type(within(ld).getByLabelText('Account id'), '321');
    await user.type(within(ld).getByLabelText(/Reason/), 'same person');
    await user.click(confirm);
    await waitFor(() => expect(link).toEqual({ account_id: '321', reason: 'same person' }));
    expect(await screen.findByText('Account linked.')).toBeInTheDocument();
  });
});

describe('moving a Discord identity (K-20, D-262)', () => {
  const alice = account();
  const bob = account({ id: '101', username: 'alice2' });
  const detailWith = (identities: unknown[]) => ({
    id: '500',
    accounts: [alice, bob],
    ban: null,
    notes: '',
    suspicion_level: 0,
    identities,
  });
  const live = {
    id: '900',
    account_id: '100',
    provider: 'discord',
    provider_user_id: '31337',
    linked_at_ms: 1_700_000_000_000,
    unlinked_at_ms: null,
    moved_to_account_id: null,
  };

  async function openPerson(perms: Parameters<typeof asStaff>[0]['perms'], identities: unknown[]) {
    asStaff({ perms });
    server.use(
      http.get('*/api/v1/admin/accounts', () =>
        HttpResponse.json({ items: [alice], next_after: null }),
      ),
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
      http.get('*/api/v1/admin/persons/500', () => HttpResponse.json(detailWith(identities))),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }, SLOW));
    await screen.findByText('Active sessions', {}, SLOW);
    await user.click(screen.getByRole('button', { name: 'Open person record' }));
    await screen.findByRole('list', { name: 'Account chain' }, SLOW);
    return user;
  }

  it('moves the identity to another account of the chain with a reason', async () => {
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/persons/500/identities/900/move', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ to_account_id: '101', linked_target: false, emailed: true });
      }),
    );
    const user = await openPerson(['STAFF_BASIC', 'MANAGE_ACCOUNTS'], [live]);
    expect(screen.getByText('Discord 31337 on @alice')).toBeInTheDocument();
    await user.click(
      screen.getByRole('button', { name: 'Move the Discord identity 31337 to another account' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Move to account' });
    const confirm = within(dialog).getByRole('button', { name: 'Move identity' });
    // The first other account of the chain is preselected; the reason is required.
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Reason/), 'lost the old login');
    await user.click(confirm);
    await waitFor(() =>
      expect(sent).toEqual({ to_account_id: '101', reason: 'lost the old login' }),
    );
    expect(
      await screen.findByText('The identity was moved. The old account was messaged and emailed.'),
    ).toBeInTheDocument();
  });

  it('can move it to an account outside the chain by id and shows moved rows as history', async () => {
    let sent: unknown;
    server.use(
      http.post('*/api/v1/admin/persons/500/identities/900/move', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ to_account_id: '777', linked_target: true, emailed: false });
      }),
    );
    const user = await openPerson(
      ['STAFF_BASIC', 'MANAGE_ACCOUNTS'],
      [
        live,
        {
          ...live,
          id: '800',
          provider_user_id: '555',
          unlinked_at_ms: 1_700_000_500_000,
          moved_to_account_id: '101',
        },
      ],
    );
    // The moved row is history: it names where it went and has no action.
    expect(screen.getByText(/Moved to @alice2 on/)).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: 'Move the Discord identity 555 to another account' }),
    ).toBeNull();
    await user.click(
      screen.getByRole('button', { name: 'Move the Discord identity 31337 to another account' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Move to account' });
    await user.click(within(dialog).getByRole('combobox', { name: 'Move to account' }));
    await user.click(await screen.findByRole('option', { name: 'Another account (enter its id)' }));
    await user.type(within(dialog).getByLabelText('Account id'), '777');
    await user.type(within(dialog).getByLabelText(/Reason/), 'new account');
    await user.click(within(dialog).getByRole('button', { name: 'Move identity' }));
    await waitFor(() => expect(sent).toEqual({ to_account_id: '777', reason: 'new account' }));
    expect(
      await screen.findByText('The identity was moved. The old account was messaged.'),
    ).toBeInTheDocument();
  });

  it('shows a refusal from the server', async () => {
    const user = await openPerson(['STAFF_BASIC', 'MANAGE_ACCOUNTS'], [live]);
    server.use(
      http.post('*/api/v1/admin/persons/500/identities/900/move', () =>
        HttpResponse.json({ message: 'the Person is banned; lift the ban first' }, { status: 409 }),
      ),
    );
    await user.click(
      screen.getByRole('button', { name: 'Move the Discord identity 31337 to another account' }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Move to account' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Move identity' }));
    expect(
      await within(dialog).findByText('the Person is banned; lift the ban first'),
    ).toBeInTheDocument();
  });
});

describe('Servers admin (ADM-COM-002)', () => {
  const guild = (over: Record<string, unknown> = {}) => ({
    id: '9',
    name: 'Garden',
    owner_id: '100',
    owner_username: 'alice',
    member_count: 12,
    channel_count: 4,
    created_at_ms: 1_700_000_000_000,
    is_staff_guild: false,
    deleted: false,
    community_enabled: true,
    ...over,
  });

  async function openServer(over: Record<string, unknown> = {}) {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_GUILDS_GLOBAL'] });
    server.use(
      http.get('*/api/v1/admin/guilds', () =>
        HttpResponse.json({ items: [guild(over)], next_after: null }),
      ),
      http.get('*/api/v1/admin/guilds/9', () => HttpResponse.json(guild(over))),
    );
    openAdmin('servers');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View Garden' }, SLOW));
    await screen.findByText('Community features', {}, SLOW);
    return user;
  }

  it('transfers ownership with a reason', async () => {
    let sent: unknown;
    const user = await openServer();
    server.use(
      http.post('*/api/v1/admin/guilds/9/transfer-ownership', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(guild({ owner_id: '200', owner_username: 'bob' }));
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Transfer ownership' }));
    const dialog = await screen.findByRole('dialog', { name: /Transfer Garden/ });
    await user.type(within(dialog).getByLabelText('New owner account id'), '200');
    await user.type(within(dialog).getByLabelText(/Reason/), 'owner left');
    await user.click(within(dialog).getByRole('button', { name: 'Transfer ownership' }));
    await waitFor(() => expect(sent).toEqual({ new_owner_id: '200', reason: 'owner left' }));
    expect(await screen.findByText('Ownership transferred.')).toBeInTheDocument();
  });

  it('deletes only after the name is typed and a reason is given', async () => {
    let sent: unknown;
    const user = await openServer();
    server.use(
      http.delete('*/api/v1/admin/guilds/9', async ({ request }) => {
        sent = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Delete server' }));
    const dialog = await screen.findByRole('dialog', { name: /Delete Garden/ });
    const go = within(dialog).getByRole('button', { name: 'Delete server' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'abuse reports');
    expect(go).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Type "Garden" to confirm'), 'Garden');
    await user.click(go);
    await waitFor(() => expect(sent).toEqual({ reason: 'abuse reports' }));
    expect(await screen.findByText('Server deleted.')).toBeInTheDocument();
  });

  it('disables the actions for the Staff Server', async () => {
    await openServer({ is_staff_guild: true });
    expect(screen.getByRole('button', { name: 'Transfer ownership' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Delete server' })).toBeDisabled();
  });

  it('is hidden from staff without MANAGE_GUILDS_GLOBAL', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts', () => HttpResponse.json({ items: [], next_after: null })),
    );
    openAdmin('accounts');
    await screen.findByRole('navigation', { name: /admin/i }, SLOW);
    expect(screen.queryByRole('button', { name: 'Servers' })).toBeNull();
  });
});
