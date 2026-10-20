// M-25 step c: the Admin data-request queue (MOD-DSR): due dates and overdue marking, the one extension, the
// runs, and the section being hidden without MANAGE_TAKEDOWNS.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { asStaff, http, HttpResponse, nav, openAdmin, readJson, server } from './helpers';

const DAY = 86_400_000;
const NOW = Date.now();

const CATALOG = [
  { id: 'person_record', label: 'Person record', reason: 'Links the accounts of one person.' },
  { id: 'invoices', label: 'Invoices', reason: 'Kept for accounting duties.' },
];

const row = (over: Record<string, unknown> = {}) => ({
  id: '900',
  kind: 'export',
  person_id: '500',
  account_id: '100',
  username: 'alice',
  state: 'open',
  note: 'email of 3 Oct',
  received_at_ms: NOW - 5 * DAY,
  due_at_ms: NOW + 25 * DAY,
  extended_until_ms: null,
  extension_reason: null,
  effective_due_ms: NOW + 25 * DAY,
  overdue: false,
  verified_by: null,
  verified_at_ms: null,
  export_id: null,
  outcome: null,
  retained: [],
  completed_at_ms: null,
  created_by: '1',
  ...over,
});

const page = (items: unknown[]) => ({ items, deadline_days: 30, retained_catalog: CATALOG });

describe('Admin data requests', () => {
  it('lists the queue with due dates and marks an overdue request', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    server.use(
      http.get('*/api/v1/admin/data-requests', () =>
        HttpResponse.json(
          page([
            row({
              id: '901',
              username: 'bob',
              kind: 'erasure',
              state: 'verified',
              overdue: true,
              effective_due_ms: NOW - 3 * DAY,
              due_at_ms: NOW - 3 * DAY,
            }),
            row(),
          ]),
        ),
      ),
    );
    openAdmin('data-requests');
    const list = await screen.findByRole('list', { name: 'Data requests' });
    const [bob, alice] = within(list).getAllByRole('listitem');
    expect(within(bob!).getByText('Overdue')).toBeInTheDocument();
    expect(within(bob!).getByText(/Overdue by 3 days/)).toBeInTheDocument();
    expect(within(alice!).queryByText('Overdue')).toBeNull();
    expect(within(alice!).getByText(/due in 24 days|due in 25 days/)).toBeInTheDocument();
    expect(screen.getByText(/30 days today/)).toBeInTheDocument();
  });

  it('is hidden without MANAGE_TAKEDOWNS', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_ACCOUNTS'] });
    server.use(
      http.get('*/api/v1/admin/stats', () => HttpResponse.json({})),
      http.get('*/api/v1/admin/system', () => HttpResponse.json({})),
    );
    openAdmin('dashboard');
    await within(await screen.findByRole('navigation', { name: /settings|admin/i })).findByText(
      'Accounts',
    );
    expect(within(nav()).queryByText('Data requests')).toBeNull();
  });

  it('verifies with a reason, then extends once and hides the button afterwards', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let current = row();
    const calls: unknown[] = [];
    server.use(
      http.get('*/api/v1/admin/data-requests', () => HttpResponse.json(page([current]))),
      http.post('*/api/v1/admin/data-requests/900/verify', async ({ request }) => {
        calls.push(['verify', await readJson(request)]);
        current = row({ state: 'verified', verified_at_ms: NOW });
        return HttpResponse.json(current);
      }),
      http.post('*/api/v1/admin/data-requests/900/extend', async ({ request }) => {
        calls.push(['extend', await readJson(request)]);
        current = row({
          state: 'verified',
          extended_until_ms: NOW + 55 * DAY,
          extension_reason: 'many systems involved',
          effective_due_ms: NOW + 55 * DAY,
        });
        return HttpResponse.json(current);
      }),
    );
    openAdmin('data-requests');
    await userEvent.click(
      await screen.findByRole('button', {
        name: /Verify the identity behind the request of alice/,
      }),
    );
    await userEvent.type(screen.getByLabelText('How it was verified'), 'reply from account email');
    const vd = await screen.findByRole('dialog', { name: 'Record identity check' });
    await userEvent.click(within(vd).getByRole('button', { name: 'Verify identity' }));
    expect(
      await screen.findByRole('button', { name: /Run the request of alice/ }),
    ).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole('button', { name: /Extend the deadline of the request of alice/ }),
    );
    const dialog = await screen.findByRole('dialog', { name: 'Extend the deadline' });
    await userEvent.type(
      within(dialog).getByLabelText('Why it needs more time'),
      'many systems involved',
    );
    await userEvent.click(within(dialog).getByRole('button', { name: 'Extend deadline' }));
    expect(await screen.findByText(/Extended to .*many systems involved/)).toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: /Extend the deadline of the request of alice/ }),
    ).toBeNull();
    expect(calls).toEqual([
      ['verify', { reason: 'reply from account email' }],
      ['extend', { days: 30, reason: 'many systems involved' }],
    ]);
  });

  it('names the records an erasure keeps before running it', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let ran = 0;
    let current = row({ kind: 'erasure', state: 'verified' });
    server.use(
      http.get('*/api/v1/admin/data-requests', () => HttpResponse.json(page([current]))),
      http.post('*/api/v1/admin/data-requests/900/run', () => {
        ran += 1;
        current = row({
          kind: 'erasure',
          state: 'completed',
          outcome: 'Account erased.',
          retained: ['person_record', 'invoices'],
          completed_at_ms: NOW,
        });
        return HttpResponse.json(current);
      }),
    );
    openAdmin('data-requests');
    await userEvent.click(await screen.findByRole('button', { name: /Run the request of alice/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Erase the account' });
    expect(within(dialog).getByText(/These records stay/)).toBeInTheDocument();
    expect(within(dialog).getByText('Person record.')).toBeInTheDocument();
    expect(within(dialog).getByText('Invoices.')).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole('button', { name: 'Erase account' }));
    expect(await screen.findByText('Request run.')).toBeInTheDocument();
    expect(ran).toBe(1);
    // The closed row names what stayed (the page is filtered to open requests, so show closed too).
  });

  it('enters a new request for an account', async () => {
    asStaff({ perms: ['STAFF_BASIC', 'MANAGE_TAKEDOWNS'] });
    let body: unknown;
    server.use(
      http.get('*/api/v1/admin/data-requests', () => HttpResponse.json(page([]))),
      http.post('*/api/v1/admin/data-requests', async ({ request }) => {
        body = await readJson(request);
        return HttpResponse.json(row(), { status: 201 });
      }),
    );
    openAdmin('data-requests');
    await userEvent.click(await screen.findByRole('button', { name: 'Enter a request' }));
    const dialog = await screen.findByRole('dialog', { name: 'Enter a data request' });
    const confirm = within(dialog).getByRole('button', { name: 'Enter request' });
    expect(confirm).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText('Account id'), '100');
    await userEvent.type(within(dialog).getByLabelText('Note'), 'email of 3 Oct');
    await userEvent.click(confirm);
    expect(await screen.findByText('Request entered.')).toBeInTheDocument();
    expect(body).toEqual({ kind: 'export', account_id: '100', note: 'email of 3 Oct' });
  });
});
