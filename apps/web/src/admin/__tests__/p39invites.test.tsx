// OPS-INV-001: Admin > Invites controls: Lock the Doors, the quotas, the prune job's last pass and the inactive
// subtrees.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const SLOW = { timeout: 10_000 };

const controls = (over: Record<string, unknown> = {}) => ({
  locked: false,
  default_credits: 5,
  max_outstanding: 20,
  unused_code_days: 30,
  last_prune: {
    ran_at_ms: 1_700_000_000_000,
    codes_revoked: 4,
    roots_marked: 2,
    roots_cleared: 0,
    graph_rows: 311,
  },
  inactive_roots: 1,
  ...over,
});

function serve(over: Record<string, unknown> = {}) {
  server.use(
    http.get('*/api/v1/admin/invites', () => HttpResponse.json({ items: [], next_after: null })),
    http.get('*/api/v1/admin/suspicion', () => HttpResponse.json([])),
    http.get('*/api/v1/admin/invites/controls', () => HttpResponse.json(controls(over))),
    http.get('*/api/v1/admin/invites/inactive', () =>
      HttpResponse.json({
        items: [
          {
            account_id: '42',
            username: 'ghost',
            invited_by_account_id: '7',
            marked_at_ms: 1_700_000_100_000,
            subtree_size: 3,
          },
        ],
        total: 1,
      }),
    ),
  );
}

describe('Admin > Invites controls (OPS-INV-001)', () => {
  it('shows the doors, the quotas, the last cleanup and the inactive subtrees', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_FEATURES'] });
    serve();
    openAdmin('invites');
    const card = await screen.findByRole('region', { name: 'Lock the Doors' }, SLOW);
    expect(await within(card).findByText('Open')).toBeInTheDocument();
    expect(within(card).getByText('Anyone with a valid invite can register.')).toBeInTheDocument();
    const quotas = within(card).getByLabelText('Invite quotas');
    expect(quotas).toHaveTextContent('Credits a new account starts with');
    expect(quotas).toHaveTextContent('Most unused invites one account may hold');
    expect(quotas).toHaveTextContent('Days before an unused invite is revoked');
    expect(card).toHaveTextContent('4 unused invites revoked, 2 inactive subtrees newly marked');
    expect(card).toHaveTextContent('The invite graph still has 311 entries');
    // The inactive subtree of ghost: display only, no action beside it.
    const list = await within(card).findByRole('list', { name: 'Inactive subtrees' });
    expect(within(list).getByText('@ghost')).toBeInTheDocument();
    expect(list).toHaveTextContent('Subtree of 3');
    expect(within(list).queryByRole('button')).toBeNull();
  });

  it('locks the doors with a reason and shows them closed', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_FEATURES'] });
    serve();
    let sent: unknown;
    server.use(
      http.put('*/api/v1/admin/invites/lock', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(controls({ locked: true }));
      }),
    );
    openAdmin('invites');
    const user = userEvent.setup();
    await user.click(await screen.findByRole('button', { name: 'Lock the doors' }, SLOW));
    const dialog = await screen.findByRole('dialog', { name: 'Lock the doors?' });
    const confirm = within(dialog).getByRole('button', { name: 'Lock the doors' });
    // A reason is required.
    expect(confirm).toBeDisabled();
    await user.type(within(dialog).getByRole('textbox'), 'wave of bad signups');
    await user.click(confirm);
    expect(await screen.findByText('The doors are locked.')).toBeInTheDocument();
    expect(sent).toEqual({ locked: true, reason: 'wave of bad signups' });
    expect(await screen.findByText('Closed')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Open the doors' })).toBeInTheDocument();
  });

  it('opens locked doors again', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES', 'MANAGE_FEATURES'] });
    serve({ locked: true });
    let sent: unknown;
    server.use(
      http.put('*/api/v1/admin/invites/lock', async ({ request }) => {
        sent = await readJson(request);
        return HttpResponse.json(controls({ locked: false }));
      }),
    );
    openAdmin('invites');
    const user = userEvent.setup();
    expect(await screen.findByText('Closed', {}, SLOW)).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Open the doors' }));
    const dialog = await screen.findByRole('dialog', { name: 'Open the doors?' });
    await user.type(within(dialog).getByRole('textbox'), 'incident over');
    await user.click(within(dialog).getByRole('button', { name: 'Open the doors' }));
    expect(await screen.findByText('The doors are open.')).toBeInTheDocument();
    expect(sent).toEqual({ locked: false, reason: 'incident over' });
  });

  it('offers no lock button to staff without the Manage features permission', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_INVITES'] });
    serve();
    openAdmin('invites');
    expect(await screen.findByText('Open', {}, SLOW)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Lock the doors' })).toBeNull();
    expect(
      screen.getByText('Locking the doors needs the Manage features permission.'),
    ).toBeInTheDocument();
  });
});
