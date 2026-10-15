// Phase-12 admin UI: appointment offers and delegations, announcement composer, account actions, dashboard
// stats and stored alerts.
import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
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

const roles = [
  {
    id: 'r1',
    name: 'Moderator',
    platform_permissions: String((1 << 12) | (1 << 11) | 1),
    position: 3,
  },
];
const owner = {
  account_id: '1',
  username: 'owner',
  appointed_at_ms: 1_700_000_000_000,
  is_owner: true,
  platform_permissions: '0',
  reason: '',
  roles: [],
};
const bob = {
  ...owner,
  account_id: '7',
  username: 'bob',
  is_owner: false,
  roles: [roles[0]],
  platform_permissions: String(1 << 12),
};
const offer = (over: Record<string, unknown> = {}) => ({
  id: '90',
  account_id: '8',
  username: 'carol',
  offered_by: '1',
  offered_by_name: 'owner',
  role_ids: ['r1'],
  role_names: ['Moderator'],
  reason: 'helps a lot',
  state: 'pending',
  created_at_ms: 1_700_000_000_000,
  expires_at_ms: 1_700_600_000_000,
  ...over,
});

function staffServer(extra: Parameters<typeof server.use> = []) {
  server.use(
    http.get('*/api/v1/admin/staff', () => HttpResponse.json([owner, bob])),
    http.get('*/api/v1/admin/staff/roles', () => HttpResponse.json(roles)),
    http.get('*/api/v1/admin/staff/delegations', () => HttpResponse.json([])),
    ...extra,
  );
}

describe('staff offers and delegations (ADM-STF-004)', () => {
  it('sends an offer instead of appointing, and lists it as pending', async () => {
    asStaff({ perms: [], owner: true });
    let sent: unknown;
    let offers = [] as ReturnType<typeof offer>[];
    staffServer([
      http.get('*/api/v1/admin/staff/offers', () => HttpResponse.json(offers)),
      http.post('*/api/v1/admin/staff/offers', async ({ request }) => {
        sent = await readJson(request);
        offers = [offer()];
        return HttpResponse.json(offer());
      }),
    ]);
    openAdmin('staff');
    const user = userEvent.setup();
    expect(await screen.findByText('No offers yet.')).toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: 'Send appointment offer' }));
    const dialog = await screen.findByRole('dialog', { name: 'Send an appointment offer' });
    await user.type(within(dialog).getByLabelText('Account id'), '8');
    await user.click(await within(dialog).findByRole('checkbox', { name: /Moderator/ }));
    await user.type(within(dialog).getByLabelText(/Reason/), 'helps a lot');
    await user.click(within(dialog).getByRole('button', { name: 'Send offer' }));
    await waitFor(() =>
      expect(sent).toEqual({ account_id: '8', role_ids: ['r1'], reason: 'helps a lot' }),
    );
    const list = await screen.findByRole('list', { name: 'Appointment offers' });
    expect(within(list).getByText('@carol')).toBeInTheDocument();
    expect(within(list).getByText('Pending')).toBeInTheDocument();
  });

  it('withdraws a pending offer after confirmation', async () => {
    asStaff({ perms: [], owner: true });
    let withdrawn = false;
    staffServer([
      http.get('*/api/v1/admin/staff/offers', () =>
        HttpResponse.json([offer({ state: withdrawn ? 'cancelled' : 'pending' })]),
      ),
      http.delete('*/api/v1/admin/staff/offers/90', () => {
        withdrawn = true;
        return HttpResponse.json(offer({ state: 'cancelled' }));
      }),
    ]);
    openAdmin('staff');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Withdraw the offer to carol' }));
    const dialog = await screen.findByRole('dialog', { name: /Withdraw the offer to @carol/ });
    await user.click(within(dialog).getByRole('button', { name: 'Withdraw offer' }));
    expect(await screen.findByText('Withdrawn')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Withdraw the offer to carol' })).toBeNull();
  });

  it('offers the direct appoint path to the Owner only', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'APPOINT_STAFF'] });
    staffServer([http.get('*/api/v1/admin/staff/offers', () => HttpResponse.json([]))]);
    openAdmin('staff');
    await screen.findByRole('button', { name: 'Send appointment offer' });
    expect(screen.queryByRole('button', { name: 'Appoint directly' })).toBeNull();
    // Delegations are the Owner's.
    expect(screen.queryByRole('heading', { name: 'Delegations' })).toBeNull();
    expect(screen.queryByText('Appoint directly (emergency)')).toBeNull();
  });

  it('keeps the Owner direct appoint in a closed emergency fold (G-80)', async () => {
    asStaff({ perms: [], owner: true });
    staffServer([http.get('*/api/v1/admin/staff/offers', () => HttpResponse.json([]))]);
    openAdmin('staff');
    const summary = await screen.findByText('Appoint directly (emergency)');
    const fold = summary.closest('details')!;
    expect(fold).not.toHaveAttribute('open');
    // The header carries the offer only.
    expect(screen.getAllByRole('button', { name: 'Send appointment offer' })).toHaveLength(1);
    await userEvent.setup().click(summary);
    expect(fold).toHaveAttribute('open');
    expect(within(fold).getByRole('button', { name: 'Appoint directly' })).toBeInTheDocument();
  });

  it('delegates appointing with a permission ceiling (Owner)', async () => {
    asStaff({ perms: [], owner: true });
    let sent: unknown;
    let url = '';
    staffServer([
      http.get('*/api/v1/admin/staff/offers', () => HttpResponse.json([])),
      http.put('*/api/v1/admin/staff/:id/delegation', async ({ request }) => {
        sent = await readJson(request);
        url = new URL(request.url).pathname;
        return HttpResponse.json({});
      }),
    ]);
    openAdmin('staff');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Delegate appointing' }));
    const dialog = await screen.findByRole('dialog', { name: 'Delegate appointing' });
    await user.click(within(dialog).getByRole('combobox', { name: 'Staff member' }));
    await user.click(await screen.findByRole('option', { name: '@bob' }));
    // Owner-only permissions are not offered as part of a ceiling.
    expect(within(dialog).queryByRole('checkbox', { name: /operator/ })).toBeNull();
    await user.click(within(dialog).getByRole('checkbox', { name: 'manage accounts' }));
    await user.click(within(dialog).getByRole('checkbox', { name: 'platform ban' }));
    await user.type(within(dialog).getByLabelText(/Reason/), 'covers weekends');
    await user.click(within(dialog).getByRole('button', { name: 'Save delegation' }));
    await waitFor(() =>
      expect(sent).toEqual({
        max_permissions: String((1 << 11) | (1 << 12)),
        reason: 'covers weekends',
      }),
    );
    expect(url).toMatch(/\/admin\/staff\/7\/delegation$/);
  });

  it('passes axe with offers and delegations', async () => {
    asStaff({ perms: [], owner: true });
    staffServer([http.get('*/api/v1/admin/staff/offers', () => HttpResponse.json([offer()]))]);
    openAdmin('staff');
    await screen.findByText('@carol');
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('announcement composer (ADM-PLT-009)', () => {
  const ann = (over: Record<string, unknown> = {}) => ({
    id: '5',
    title: 'Maintenance tonight',
    body: 'Short downtime at 22:00.',
    severity: 'warning',
    send_dm: false,
    created_by: '1',
    created_at_ms: 1_700_000_000_000,
    starts_at_ms: 1_700_000_000_000,
    ...over,
  });

  it('is only listed with MANAGE_FEATURES', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(http.get('*/api/v1/admin/stats', () => HttpResponse.json({})));
    openAdmin('dashboard');
    await screen.findByRole('navigation');
    expect(screen.queryByRole('button', { name: 'Announcements' })).toBeNull();
    expect(screen.queryByRole('link', { name: 'Announcements' })).toBeNull();
  });

  it('publishes a banner without messaging anyone by default', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    let sent: Record<string, unknown> | undefined;
    server.use(
      http.get('*/api/v1/admin/announcements', () => HttpResponse.json([])),
      http.post('*/api/v1/admin/announcements', async ({ request }) => {
        sent = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(ann());
      }),
    );
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New announcement' }));
    const dialog = await screen.findByRole('dialog', { name: 'New announcement' });
    await user.type(within(dialog).getByLabelText('Title'), 'Maintenance tonight');
    await user.type(within(dialog).getByLabelText('Message'), 'Short downtime at 22:00.');
    expect(within(dialog).queryByRole('alert')).toBeNull();
    await user.click(within(dialog).getByRole('button', { name: 'Publish' }));
    await waitFor(() => expect(sent).toBeDefined());
    expect(sent).toMatchObject({
      title: 'Maintenance tonight',
      body: 'Short downtime at 22:00.',
      severity: 'info',
      send_dm: false,
      ends_at_ms: null,
    });
    expect(await screen.findByText('Announcement published.')).toBeInTheDocument();
  });

  it('warns that a system DM goes to every active account before it can be published', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    let sent: Record<string, unknown> | undefined;
    server.use(
      http.get('*/api/v1/admin/announcements', () => HttpResponse.json([])),
      http.post('*/api/v1/admin/announcements', async ({ request }) => {
        sent = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(ann({ send_dm: true }));
      }),
    );
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New announcement' }));
    const dialog = await screen.findByRole('dialog', { name: 'New announcement' });
    await user.type(within(dialog).getByLabelText('Title'), 'Important');
    await user.type(within(dialog).getByLabelText('Message'), 'Please read.');
    await user.click(within(dialog).getByRole('checkbox', { name: /direct message/ }));
    const warning = within(dialog).getByRole('alert');
    expect(warning).toHaveTextContent('This messages every active account');
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(within(dialog).getByRole('button', { name: 'Publish and message everyone' }));
    await waitFor(() => expect(sent).toMatchObject({ send_dm: true }));
    expect(
      await screen.findByText(/message to every active account is going out/),
    ).toBeInTheDocument();
  });

  it('sends critical with the DM forced on and locked (G-83, D-192)', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    let sent: Record<string, unknown> | undefined;
    server.use(
      http.get('*/api/v1/admin/announcements', () => HttpResponse.json([])),
      http.post('*/api/v1/admin/announcements', async ({ request }) => {
        sent = (await readJson(request)) as Record<string, unknown>;
        return HttpResponse.json(ann({ severity: 'critical', send_dm: true }));
      }),
    );
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New announcement' }));
    const dialog = await screen.findByRole('dialog', { name: 'New announcement' });
    await user.type(within(dialog).getByLabelText('Title'), 'Outage');
    await user.type(within(dialog).getByLabelText('Message'), 'We are on it.');
    await user.click(within(dialog).getByRole('combobox', { name: 'Severity' }));
    await user.click(await screen.findByRole('option', { name: 'Critical' }));
    const box = within(dialog).getByRole('checkbox', { name: /direct message/ });
    expect(box).toBeChecked();
    expect(box).toBeDisabled();
    expect(within(dialog).getByRole('alert')).toHaveTextContent(
      'sends this text as a direct message to every active account on the platform',
    );
    await user.click(within(dialog).getByRole('button', { name: 'Publish and message everyone' }));
    await waitFor(() =>
      expect(sent).toMatchObject({ severity: 'critical', critical: true, send_dm: true }),
    );
  });

  it('tells staff that an ordinary DM skips people who turned it off', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    server.use(http.get('*/api/v1/admin/announcements', () => HttpResponse.json([])));
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New announcement' }));
    const dialog = await screen.findByRole('dialog', { name: 'New announcement' });
    const box = within(dialog).getByRole('checkbox', { name: /direct message/ });
    expect(box).not.toBeDisabled();
    await user.click(box);
    expect(within(dialog).getByRole('alert')).toHaveTextContent(
      'has not turned off platform announcements by direct message',
    );
  });

  it('rejects an end time in the past', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    server.use(http.get('*/api/v1/admin/announcements', () => HttpResponse.json([])));
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'New announcement' }));
    const dialog = await screen.findByRole('dialog', { name: 'New announcement' });
    await user.type(within(dialog).getByLabelText('Title'), 'x');
    await user.type(within(dialog).getByLabelText('Message'), 'y');
    await user.type(within(dialog).getByLabelText(/Stop showing at/), '2001-01-01T10:00');
    expect(await within(dialog).findByText('Pick a time in the future.')).toBeInTheDocument();
    expect(within(dialog).getByRole('button', { name: 'Publish' })).toBeDisabled();
  });

  it('ends an active announcement and offers reuse for editing', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    let ended = false;
    server.use(
      http.get('*/api/v1/admin/announcements', () =>
        HttpResponse.json([ann(ended ? { ended_at_ms: 1_700_000_500_000 } : {})]),
      ),
      http.delete('*/api/v1/admin/announcements/5', () => {
        ended = true;
        return HttpResponse.json(ann({ ended_at_ms: 1_700_000_500_000 }));
      }),
    );
    openAdmin('announcements');
    const user = userEvent.setup();
    await user.click(
      await screen.findByRole('button', {
        name: 'Reuse Maintenance tonight as a new announcement',
      }),
    );
    const reuse = await screen.findByRole('dialog', { name: 'New announcement' });
    expect(within(reuse).getByLabelText('Title')).toHaveValue('Maintenance tonight');
    await user.click(within(reuse).getByRole('button', { name: 'Cancel' }));

    await user.click(await screen.findByRole('button', { name: 'End Maintenance tonight' }));
    const dialog = await screen.findByRole('dialog', { name: /End "Maintenance tonight"/ });
    await user.click(within(dialog).getByRole('button', { name: 'End announcement' }));
    expect(await screen.findByText('Announcement ended.')).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'End Maintenance tonight' })).toBeNull(),
    );
  });

  it('passes axe', async () => {
    asStaff({ perms: ['MANAGE_FEATURES'] });
    server.use(http.get('*/api/v1/admin/announcements', () => HttpResponse.json([ann()])));
    openAdmin('announcements');
    await screen.findByText('Maintenance tonight');
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('account actions (ADM-PPL-003)', () => {
  async function openDrawer(flags = 0) {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/accounts', () =>
        HttpResponse.json({ items: [account({ flags })], next_after: null }),
      ),
      http.get('*/api/v1/admin/accounts/100', () => HttpResponse.json(detail({}, { flags }))),
    );
    openAdmin('accounts');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'View alice' }));
    await screen.findByRole('group', { name: 'Account actions' });
    return user;
  }

  it('renames, sending only the changed field', async () => {
    let sent: unknown;
    const user = await openDrawer();
    server.use(
      http.post('*/api/v1/admin/accounts/100/rename', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(account({ username: 'alice2' }));
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Rename' }));
    const dialog = await screen.findByRole('dialog', { name: /Rename @alice/ });
    const go = within(dialog).getByRole('button', { name: 'Rename' });
    expect(go).toBeDisabled();
    const name = within(dialog).getByLabelText('Username');
    await user.clear(name);
    await user.type(name, 'alice2');
    await user.type(within(dialog).getByLabelText(/Reason/), 'impersonation');
    await user.click(go);
    await waitFor(() => expect(sent).toEqual({ username: 'alice2', reason: 'impersonation' }));
    expect(await screen.findByText('Account renamed.')).toBeInTheDocument();
  });

  it('changes the email with a reason', async () => {
    let sent: unknown;
    const user = await openDrawer();
    server.use(
      http.post('*/api/v1/admin/accounts/100/change-email', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(account({ email: 'new@example.test', email_verified: false }));
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Change email' }));
    const dialog = await screen.findByRole('dialog', { name: /Change the email of @alice/ });
    const email = within(dialog).getByLabelText('New email address');
    await user.clear(email);
    await user.type(email, 'new@example.test');
    await user.type(within(dialog).getByLabelText(/Reason/), 'lost access');
    await user.click(within(dialog).getByRole('button', { name: 'Change email' }));
    await waitFor(() => expect(sent).toEqual({ email: 'new@example.test', reason: 'lost access' }));
    expect(await screen.findByText(/Email changed/)).toBeInTheDocument();
  });

  it('grants and revokes badges from the current flags', async () => {
    let sent: unknown;
    // Bit 8 (early member) is held.
    const user = await openDrawer(1 << 8);
    server.use(
      http.post('*/api/v1/admin/accounts/100/badges', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(account({ flags: 1 << 10 }));
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Badges' }));
    const dialog = await screen.findByRole('dialog', { name: /Badges for @alice/ });
    expect(within(dialog).getByRole('checkbox', { name: 'Early member' })).toBeChecked();
    await user.click(within(dialog).getByRole('checkbox', { name: 'Early member' }));
    await user.click(within(dialog).getByRole('checkbox', { name: 'Bug hunter' }));
    await user.type(within(dialog).getByLabelText(/Reason/), 'swapped');
    await user.click(within(dialog).getByRole('button', { name: 'Save badges' }));
    await waitFor(() =>
      expect(sent).toEqual({ grant: ['bug_hunter'], revoke: ['early_member'], reason: 'swapped' }),
    );
  });

  it('forces a password reset and retries after the second-factor dialog (withReauth)', async () => {
    let calls = 0;
    const user = await openDrawer();
    server.use(
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900, required: true }),
      ),
      http.post('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900, required: true }),
      ),
      http.post('*/api/v1/admin/accounts/100/force-password-reset', () => {
        calls += 1;
        return calls === 1
          ? HttpResponse.json({ code: 900004, message: 're-auth' }, { status: 403 })
          : HttpResponse.json({ sessions_revoked: 3 });
      }),
    );
    await user.click(screen.getByRole('button', { name: 'Force password reset' }));
    const dialog = await screen.findByRole('dialog', { name: /Force a password reset for @alice/ });
    await user.type(within(dialog).getByLabelText(/Reason/), 'phished');
    await user.click(within(dialog).getByRole('button', { name: 'Force password reset' }));
    const reauth = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    await user.type(within(reauth).getByLabelText(/authenticator app/), '123456');
    await user.click(within(reauth).getByRole('button', { name: 'Confirm' }));
    expect(await screen.findByText(/Password reset forced\. 3 sessions ended/)).toBeInTheDocument();
    expect(calls).toBe(2);
  });

  it('passes axe with the badges dialog open', async () => {
    const user = await openDrawer(1 << 9);
    await user.click(screen.getByRole('button', { name: 'Badges' }));
    await screen.findByRole('dialog', { name: /Badges for @alice/ });
    expect(await axeViolations(document.body)).toEqual([]);
  });
});

describe('dashboard stats and stored alerts (ADM-DASH-001, ADM-ALR-001)', () => {
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
    storage_bytes: 5 * 1024 ** 3,
    invites_open: 12,
  };

  it('shows storage and open invites, and hides voice tiles the server does not report', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(STATS)),
      http.get('*/api/v1/admin/alerts', () => HttpResponse.json([])),
    );
    openAdmin('dashboard');
    expect(await screen.findByText('Storage used')).toBeInTheDocument();
    expect(screen.getByText(/5(\.0)?\s?GB/)).toBeInTheDocument();
    expect(screen.getByText('Open invites')).toBeInTheDocument();
    expect(screen.queryByText('Voice rooms')).toBeNull();
  });

  it('shows voice rooms and egress when the server sends them', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    server.use(
      http.get('*/api/v1/admin/stats', () =>
        HttpResponse.json({ ...STATS, voice_rooms: 3, egress_bytes: 2048 }),
      ),
      http.get('*/api/v1/admin/alerts', () => HttpResponse.json([])),
    );
    openAdmin('dashboard');
    expect(await screen.findByText('Voice rooms')).toBeInTheDocument();
    expect(screen.getByText('Voice egress')).toBeInTheDocument();
  });

  it('lists recorded alerts and acknowledges one', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    let acked = false;
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json(STATS)),
      http.get('*/api/v1/admin/alerts', () =>
        HttpResponse.json([
          {
            id: '31',
            kind: 'disk_low',
            detail: 'Disk is 91% full',
            created_at_ms: 1_700_000_000_000,
            ...(acked ? { acked_at_ms: 1_700_000_100_000, acked_by: '1' } : {}),
          },
        ]),
      ),
      http.post('*/api/v1/admin/alerts/31/ack', () => {
        acked = true;
        return HttpResponse.json({});
      }),
    );
    openAdmin('dashboard');
    const user = userEvent.setup();
    const list = await screen.findByRole('list', { name: 'Recorded alerts' });
    expect(within(list).getByText(/Disk is 91% full/)).toBeInTheDocument();
    expect(await axeViolations(document.body)).toEqual([]);
    await user.click(within(list).getByRole('button', { name: 'Acknowledge the disk low alert' }));
    expect(await within(list).findByText('Acknowledged')).toBeInTheDocument();
    expect(within(list).queryByRole('button', { name: /Acknowledge the/ })).toBeNull();
  });
});
