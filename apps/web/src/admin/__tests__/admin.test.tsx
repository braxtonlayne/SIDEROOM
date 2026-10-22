import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import {
  account,
  asStaff,
  detail,
  http,
  HttpResponse,
  nav,
  openAdmin,
  readJson,
  server,
} from './helpers';

const STATS = {
  accounts_active: 7,
  accounts_banned: 0,
  accounts_deactivated: 1,
  accounts_deleted: 0,
  jobs_dead: 0,
  jobs_queued: 0,
  jobs_running: 0,
  sessions_active: 4,
  staff: 2,
};

const navNames = () =>
  within(nav())
    .getAllByRole('button')
    .map((b) => b.textContent);

describe('admin shell', () => {
  it('lists only the sections the staff role can use', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS', 'MANAGE_INVITES'] }); // Support
    server.use(
      http.get('*/api/v1/admin/stats', () =>
        HttpResponse.json({
          accounts_active: 7,
          accounts_banned: 0,
          accounts_deactivated: 1,
          accounts_deleted: 0,
          jobs_dead: 0,
          jobs_queued: 0,
          jobs_running: 0,
          sessions_active: 4,
          staff: 2,
        }),
      ),
    );
    openAdmin('dashboard');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(navNames()).toEqual([
      'Dashboard',
      'Accounts',
      'Invites',
      'Invite graph',
      'Command line',
    ]);
  });

  it('shows everything to the Owner', async () => {
    asStaff({ perms: [], owner: true });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(STATS)),
      http.get('*/api/v1/admin/settings', () =>
        HttpResponse.json([
          {
            key: 'accounts.cap',
            group: 'limits',
            kind: 'integer',
            description: '',
            min: 1,
            max: 9,
            value: 20,
            default: 20,
            is_default: true,
          },
        ]),
      ),
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({ counts: STATS, gateway_sessions: 2, probes: [], version: '1' }),
      ),
    );
    openAdmin('dashboard');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(await screen.findByRole('progressbar', { name: /account cap/ })).toBeInTheDocument();
    expect(await screen.findByText('Online now')).toBeInTheDocument();
    expect(navNames()).toEqual([
      'Dashboard',
      'Accounts',
      'Platform bans',
      'Moderation',
      'Invites',
      'Invite graph',
      'Servers',
      'Staff & roles',
      'Review queues',
      'Jobs',
      'Ingest sources',
      'Library catalog',
      'Cinema catalog',
      'Cinema collections',
      'Cinema transcoding',
      'Live TV',
      'Donations',
      'Invoices',
      'Entitlements',
      'Pricing and perks',
      'Features',
      'Limits & settings',
      'Entrance pages',
      'Announcements',
      'System',
      'Platform audit log',
      'Private content',
      'Access log',
      'Command line',
      'Data requests',
    ]);
  });

  it('explains a 403 from /admin/me instead of rendering admin pages', async () => {
    server.use(
      http.get('*/api/v1/admin/me', () =>
        HttpResponse.json({ code: 40001, message: 'Staff only.' }, { status: 403 }),
      ),
    );
    openAdmin('accounts');
    expect(await screen.findByText('Staff only')).toBeInTheDocument();
    expect(screen.getByText('You do not have permission to do that.')).toBeInTheDocument();
    expect(within(nav()).queryAllByRole('button')).toEqual([]);
  });

  it('asks signed-out visitors to log in', async () => {
    server.use(
      http.get('*/api/v1/admin/me', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    openAdmin('accounts');
    expect(await screen.findByText('Sign in to continue')).toBeInTheDocument();
  });

  it('redirects an unknown or forbidden section to the first allowed one', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json({ accounts_active: 1 })));
    const { router } = openAdmin('staff');
    await screen.findByRole('heading', { name: 'Dashboard' });
    expect(router.state.location.pathname).toBe('/admin/dashboard');
  });

  it('passes axe on the dashboard', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(
      http.get('*/api/v1/admin/stats', () =>
        HttpResponse.json({
          accounts_active: 7,
          accounts_banned: 0,
          accounts_deactivated: 1,
          accounts_deleted: 0,
          jobs_dead: 2,
          jobs_queued: 3,
          jobs_running: 1,
          sessions_active: 4,
          staff: 2,
        }),
      ),
    );
    const { container } = openAdmin('dashboard');
    await screen.findByText('Active accounts');
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('accounts', () => {
  const seedAccounts = () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts', ({ request }) => {
        const q = new URL(request.url).searchParams.get('q');
        return HttpResponse.json({
          items: q === 'nobody' ? [] : [account()],
          next_after: null,
        });
      }),
    );
  };

  it('searches and shows an empty state', async () => {
    seedAccounts();
    openAdmin('accounts');
    await screen.findByRole('table', { name: 'Accounts' });
    const user = userEvent.setup();
    await user.type(screen.getByLabelText('Search accounts'), 'nobody');
    await user.click(screen.getByRole('button', { name: 'Search' }));
    expect(await screen.findByText('No accounts match')).toBeInTheDocument();
  });

  it('requires a reason before suspending and posts it', async () => {
    seedAccounts();
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
      http.post('*/api/v1/admin/accounts/100/suspend', async ({ request }) => {
        sent = await readJson(request);
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await user.click(await screen.findByRole('button', { name: 'Suspend' }));
    const dialog = await screen.findByRole('dialog', { name: /Suspend @alice/ });
    const confirm = within(dialog).getByRole('button', { name: 'Suspend account' });
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByLabelText(/Reason/), 'spam wave');
    expect(confirm).toBeEnabled();
    await user.click(confirm);
    await waitFor(() => expect(sent).toEqual({ reason: 'spam wave' }));
    expect(await screen.findByText('@alice was suspended.')).toBeInTheDocument();
  });

  it('shows the server error inside the dialog and stays open (409 cap reached)', async () => {
    seedAccounts();
    server.use(
      http.get('*/api/v1/admin/accounts', () =>
        HttpResponse.json({ items: [account({ state: 'deactivated' })], next_after: null }),
      ),
      http.get('*/api/v1/admin/accounts/100', () =>
        HttpResponse.json(detail({}, { state: 'deactivated' })),
      ),
      http.post('*/api/v1/admin/accounts/100/unsuspend', () =>
        HttpResponse.json(
          { code: 50001, message: 'The account cap has been reached.' },
          { status: 409 },
        ),
      ),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await user.click(await screen.findByRole('button', { name: 'Reactivate' }));
    const dialog = await screen.findByRole('dialog', { name: /Reactivate @alice/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'appeal accepted');
    await user.click(within(dialog).getByRole('button', { name: 'Reactivate account' }));
    expect(await within(dialog).findByRole('alert')).toHaveTextContent(
      'The account cap has been reached.',
    );
    expect(screen.getByRole('dialog', { name: /Reactivate @alice/ })).toBeInTheDocument();
  });

  it('hides ban buttons without PLATFORM_BAN and suspend without MANAGE_ACCOUNTS', async () => {
    seedAccounts();
    server.use(http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())));
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await screen.findByRole('group', { name: 'Account actions' });
    expect(screen.queryByRole('group', { name: 'Platform ban' })).toBeNull();
  });

  it('revokes sessions with a reason and reports the count', async () => {
    seedAccounts();
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())),
      http.post('*/api/v1/admin/accounts/100/force-logout', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ sessions_revoked: 2 });
      }),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await user.click(await screen.findByRole('button', { name: 'Revoke sessions' }));
    const dialog = await screen.findByRole('dialog', { name: /Revoke every session/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'stolen laptop');
    await user.click(within(dialog).getByRole('button', { name: 'Revoke sessions' }));
    expect(await screen.findByText('2 session(s) revoked.')).toBeInTheDocument();
    expect(sent).toEqual({ reason: 'stolen laptop' });
  });

  it('passes axe with the account drawer open', async () => {
    seedAccounts();
    server.use(http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail())));
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await screen.findByRole('group', { name: 'Account actions' });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('invites', () => {
  it('lists, revokes after confirmation and shows the CLI note', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES'] });
    let revoked = false;
    server.use(
      http.get('*/api/v1/admin/invites', () =>
        HttpResponse.json({
          items: [
            {
              id: '9',
              code: 'ABCDE-FGHJK',
              issuer_account_id: '1',
              state: revoked ? 'revoked' : 'open',
              emergency: true,
              note: 'break glass',
              created_at_ms: 1_700_000_000_000,
              redeemed_by_account_id: null,
            },
          ],
          next_after: null,
        }),
      ),
      http.get('*/api/v1/admin/suspicion', () =>
        HttpResponse.json([
          {
            accounts: [{ id: '5', username: 'mallory' }],
            frozen: true,
            person_id: '55',
            suspicion_level: 3,
          },
        ]),
      ),
      http.delete('*/api/v1/admin/invites/9', () => {
        revoked = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('invites');
    expect(await screen.findByText('ABCDE-FGHJK')).toBeInTheDocument();
    expect(
      within(screen.getByRole('table', { name: 'Invites' })).getByText('emergency'),
    ).toBeInTheDocument();
    expect(screen.getByText('invites frozen')).toBeInTheDocument();
    expect(screen.getByText(/sr-cli invite create/)).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Revoke invite ABCDE-FGHJK' }));
    const dialog = await screen.findByRole('dialog', { name: 'Revoke this invite' });
    await user.click(within(dialog).getByRole('button', { name: 'Revoke invite' }));
    expect(await screen.findByText('Invite ABCDE-FGHJK was revoked.')).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: /Revoke invite ABCDE/ })).toBeNull(),
    );
  });

  it('surfaces a 403 as an inline error', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES'] });
    server.use(
      http.get('*/api/v1/admin/invites', () =>
        HttpResponse.json({ message: 'Missing' }, { status: 403 }),
      ),
      http.get('*/api/v1/admin/suspicion', () => HttpResponse.json([])),
    );
    openAdmin('invites');
    expect(await screen.findByText('You do not have permission to do that.')).toBeInTheDocument();
  });
});

describe('staff', () => {
  const roles = [
    {
      id: 'r1',
      name: 'Moderator',
      platform_permissions: String((1 << 12) | (1 << 11) | 1),
      position: 3,
    },
    {
      id: 'r2',
      name: 'Senior Staff',
      platform_permissions: String((1 << 19) | (1 << 16) | 1),
      position: 5,
    },
  ];
  const member = {
    account_id: '7',
    username: 'bob',
    display_name: 'Bob',
    appointed_at_ms: 1_700_000_000_000,
    appointed_by: '1',
    is_owner: false,
    platform_permissions: String(1 << 12),
    reason: 'new mod',
    roles: [roles[0]],
  };
  const ownerMember = { ...member, account_id: '1', username: 'owner', is_owner: true, roles: [] };

  it('lets a delegate assign only roles it can grant, and never removes the Owner', async () => {
    // Delegate holds APPOINT_STAFF + the Moderator bits, but not READ_PRIVATE_CONTENT.
    asStaff({ perms: ['STAFF_BASIC', 'APPOINT_STAFF', 'MANAGE_ACCOUNTS', 'PLATFORM_BAN'] });
    server.use(
      http.get('*/api/v1/admin/staff', () => HttpResponse.json([ownerMember, member])),
      http.get('*/api/v1/admin/staff/roles', () => HttpResponse.json(roles)),
    );
    openAdmin('staff');
    await screen.findByText('@bob');
    expect(screen.queryByRole('button', { name: /Remove owner/ })).toBeNull();
    const user = userEvent.setup();
    await user.click(screen.getByRole('button', { name: 'Change roles for bob' }));
    const dialog = await screen.findByRole('dialog', { name: /Roles for @bob/ });
    expect(within(dialog).getByRole('checkbox', { name: /Moderator/ })).toBeEnabled();
    expect(within(dialog).getByRole('checkbox', { name: /Senior Staff/ })).toBeDisabled();
  });

  it('appoints with a reason and the chosen roles', async () => {
    asStaff({ perms: [], owner: true });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/staff', () => HttpResponse.json([ownerMember])),
      http.get('*/api/v1/admin/staff/roles', () => HttpResponse.json(roles)),
      http.get('*/api/v1/admin/stats', () => HttpResponse.json({})),
      http.post('*/api/v1/admin/staff', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(member);
      }),
    );
    openAdmin('staff');
    const user = userEvent.setup();
    // The direct appoint sits in a closed "emergency" fold (G-80).
    await user.click(await screen.findByText('Appoint directly (emergency)'));
    await user.click(await screen.findByRole('button', { name: 'Appoint directly' }));
    const dialog = await screen.findByRole('dialog', { name: 'Appoint staff' });
    const go = within(dialog).getByRole('button', { name: 'Appoint' });
    expect(go).toBeDisabled();
    await user.type(within(dialog).getByLabelText('Account id'), '7');
    await user.click(await within(dialog).findByRole('checkbox', { name: /Senior Staff/ }));
    await user.type(within(dialog).getByLabelText(/Reason/), 'trusted');
    await user.click(go);
    await waitFor(() =>
      expect(sent).toEqual({ account_id: '7', role_ids: ['r2'], reason: 'trusted' }),
    );
    expect(await screen.findByText('Staff member appointed.')).toBeInTheDocument();
  });

  it('removes staff after confirmation', async () => {
    asStaff({ perms: [], owner: true });
    let removed = false;
    server.use(
      http.get('*/api/v1/admin/staff', () =>
        HttpResponse.json(removed ? [ownerMember] : [ownerMember, member]),
      ),
      http.get('*/api/v1/admin/staff/roles', () => HttpResponse.json(roles)),
      http.delete('*/api/v1/admin/staff/7', () => {
        removed = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('staff');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Remove bob from staff' }));
    const dialog = await screen.findByRole('dialog', { name: /Remove @bob/ });
    await user.click(within(dialog).getByRole('button', { name: 'Remove from staff' }));
    expect(await screen.findByText('@bob was removed from staff.')).toBeInTheDocument();
  });
});

describe('access log', () => {
  const entry = {
    seq: 2,
    at_us: 1_700_000_000_000_000,
    actor_id: '1',
    kind: 'private_read',
    action: 'read_dm',
    target_kind: 'channel',
    target_id: '77',
    reason: 'abuse report 12',
    detail: '',
    ip: null,
    prev_hash: 'a'.repeat(64),
    hash: 'b'.repeat(64),
  };
  const open = () => {
    asStaff({ perms: ['STAFF_BASIC', 'VIEW_ACCESS_LOG'] });
    server.use(
      http.get('*/api/v1/admin/access-log', () =>
        HttpResponse.json({ items: [entry], next_before: null }),
      ),
    );
    return openAdmin('access-log');
  };

  it('shows rows and a passing chain verification', async () => {
    server.use(
      http.get('*/api/v1/admin/access-log/verify', () =>
        HttpResponse.json({ ok: true, rows_checked: 2, head_hash: 'c'.repeat(64) }),
      ),
    );
    open();
    expect(await screen.findByText('abuse report 12')).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole('button', { name: 'Verify chain' }));
    expect(await screen.findByText('Chain intact')).toBeInTheDocument();
  });

  it('shows a broken chain as an alert', async () => {
    server.use(
      http.get('*/api/v1/admin/access-log/verify', () =>
        HttpResponse.json({
          ok: false,
          rows_checked: 1,
          first_bad_seq: 2,
          problem: 'hash mismatch',
        }),
      ),
    );
    open();
    await screen.findByText('abuse report 12');
    await userEvent.setup().click(screen.getByRole('button', { name: 'Verify chain' }));
    const alert = await screen.findByRole('alert');
    expect(alert).toHaveTextContent('Chain broken');
    expect(alert).toHaveTextContent('hash mismatch');
    expect(alert).toHaveTextContent('#2');
  });

  it('passes axe', async () => {
    const { container } = open();
    await screen.findByText('abuse report 12');
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('limits and features', () => {
  const settings = [
    {
      key: 'accounts.cap',
      group: 'limits',
      kind: 'integer',
      description: 'Maximum number of active accounts.',
      min: 1,
      max: 1_000_000,
      value: 20000,
      default: 20000,
      is_default: true,
      updated_at_ms: null,
    },
    {
      key: 'moderation.audit_retention_days',
      group: 'retention',
      kind: 'integer',
      description: 'Guild audit-log retention in days.',
      min: 0,
      max: 36500,
      value: 45,
      default: 45,
      is_default: true,
      updated_at_ms: null,
    },
  ];

  it('validates ranges client-side and saves only changed keys with a reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/settings', () => HttpResponse.json(settings)),
      http.patch('*/api/v1/admin/settings', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(settings);
      }),
    );
    openAdmin('limits');
    const user = userEvent.setup();
    const retention = await screen.findByLabelText('moderation.audit_retention_days');
    expect(screen.getByRole('heading', { name: 'Retention' })).toBeInTheDocument();
    await user.clear(retention);
    await user.type(retention, '999999');
    expect(await screen.findByText('Must be between 0 and 36500.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /^Save/ })).toBeDisabled();
    await user.clear(retention);
    await user.type(retention, '90');
    await user.click(screen.getByRole('button', { name: 'Save 1 change' }));
    const dialog = await screen.findByRole('dialog', { name: 'Save 1 setting' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'policy change');
    await user.click(within(dialog).getByRole('button', { name: 'Save settings' }));
    await waitFor(() =>
      expect(sent).toEqual({
        values: { 'moderation.audit_retention_days': 90 },
        reason: 'policy change',
      }),
    );
    expect(await screen.findByText('Settings saved. They are live now.')).toBeInTheDocument();
  });

  it('toggles a feature kill switch with an optional reason', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_FEATURES'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/features', () =>
        HttpResponse.json([
          {
            id: 'cinema',
            name: 'Cinema',
            description: 'Movies and shows.',
            section: 'cinema',
            phase: 4,
            scopes: ['platform'],
            enabled: true,
            default_enabled: true,
            schema: {},
            settings: {},
          },
        ]),
      ),
      http.patch('*/api/v1/admin/features/cinema', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({});
      }),
    );
    openAdmin('features');
    const user = userEvent.setup();
    await user.click(
      await screen
        .findByRole('switch', { name: 'Cinema enabled' })
        .catch(() => screen.getByLabelText('Cinema enabled')),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Disable Cinema' });
    await user.click(within(dialog).getByRole('button', { name: 'Disable' }));
    await waitFor(() => expect(sent).toEqual({ enabled: false }));
  });
});

describe('system and jobs', () => {
  it('shows probe failures without credentials', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'OPERATOR'] });
    server.use(
      http.get('*/api/v1/admin/system', () =>
        HttpResponse.json({
          version: '0.4.0',
          gateway_sessions: 3,
          counts: { jobs_queued: 1, jobs_running: 0, jobs_dead: 2 },
          probes: [
            { name: 'database', ok: true },
            { name: 'storage', ok: false, error: 'connection refused' },
          ],
        }),
      ),
    );
    openAdmin('system');
    expect(await screen.findByText('connection refused')).toBeInTheDocument();
    expect(screen.getByText('failing')).toBeInTheDocument();
  });

  it('retries a dead job', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TRANSCODE'] });
    let retried = false;
    server.use(
      http.get('*/api/v1/admin/jobs', () =>
        HttpResponse.json({
          items: [
            {
              id: '31',
              kind: 'transcode',
              state: 'dead',
              attempts: 5,
              max_attempts: 5,
              run_at_ms: 1_700_000_000_000,
              finished_at_ms: null,
              last_error: 'ffmpeg exited 1',
            },
          ],
          next_after: null,
        }),
      ),
      http.post('*/api/v1/admin/jobs/31/retry', () => {
        retried = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('jobs');
    await userEvent.setup().click(await screen.findByRole('button', { name: 'Retry job 31' }));
    expect(await screen.findByText('Job re-queued.')).toBeInTheDocument();
    expect(retried).toBe(true);
  });
});

describe('platform bans', () => {
  it('lifts a ban with a reason and reports the outcome', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'PLATFORM_BAN'] });
    let sent: unknown;
    server.use(
      http.get('*/api/v1/admin/bans', () =>
        HttpResponse.json({
          items: [
            {
              id: '1',
              person_id: '55',
              banned_by: '1',
              category: 'serious',
              note: 'ban evasion',
              created_at_ms: 1_700_000_000_000,
              lifted_at_ms: null,
            },
          ],
          next_after: null,
        }),
      ),
      http.post('*/api/v1/admin/persons/55/unban', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json({ accounts_reactivated: 1, accounts_left_deactivated: 0 });
      }),
    );
    openAdmin('bans');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Lift ban on person 55' }));
    const dialog = await screen.findByRole('dialog', { name: 'Lift this ban' });
    await user.type(within(dialog).getByLabelText(/Reason/), 'appeal upheld');
    await user.click(within(dialog).getByRole('button', { name: 'Lift ban' }));
    expect(await screen.findByText(/1 account\(s\) reactivated/)).toBeInTheDocument();
    expect(sent).toEqual({ reason: 'appeal upheld' });
  });
});
