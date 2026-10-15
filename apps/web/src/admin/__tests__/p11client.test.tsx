// Phase-10 backends wired into the client: re-auth modal, Private Content Viewer, revoked appointments.
import { act, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { emitDispatch } from '../../store/events';
import { usePlatformStaff } from '../../lounge/shell/platformStaff';
import { asStaff, http, HttpResponse, openAdmin, readJson, server } from './helpers';

const reauthRequired = () =>
  HttpResponse.json(
    { code: 900004, message: 'Please re-confirm your identity to continue' },
    { status: 403 },
  );

const page = {
  channel: { id: '55', kind: 1, private: true, recipients: [{ id: '2', username: 'bob' }] },
  deleted_retention_days: 30,
  items: [
    {
      id: '3',
      author_id: '2',
      author_name: 'bob',
      content: 'this was removed',
      attachment_count: 0,
      created_at_ms: 1_700_000_000_000,
      deleted_at_ms: 1_700_000_100_000,
      edits: [],
      visibility: 'user_deleted',
    },
    {
      id: '2',
      author_id: '2',
      author_name: 'bob',
      content: 'fixed text',
      attachment_count: 1,
      created_at_ms: 1_699_999_000_000,
      edited_at_ms: 1_699_999_500_000,
      edits: [{ edited_at_ms: 1_699_999_500_000, previous_content: 'typo text' }],
      visibility: 'visible',
    },
  ],
  next_before: null,
};

describe('Private Content Viewer (ADM-COM-003)', () => {
  it('is only offered with READ_PRIVATE_CONTENT', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    openAdmin('dashboard');
    await screen.findByRole('navigation');
    expect(screen.queryByRole('link', { name: 'Private content' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Private content' })).not.toBeInTheDocument();
  });

  it('asks for a second factor, retries the read and shows deleted messages and edit history', async () => {
    asStaff({ perms: ['READ_PRIVATE_CONTENT'] });
    let reads = 0;
    let proof: unknown;
    let reason: string | null = null;
    server.use(
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900, required: true }),
      ),
      http.post('*/api/v1/admin/reauth', async ({ request }) => {
        proof = await readJson(request);
        return HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900, required: true });
      }),
      http.get('*/api/v1/admin/private/channels/55/messages', ({ request }) => {
        reads += 1;
        reason = new URL(request.url).searchParams.get('reason');
        return reads === 1 ? reauthRequired() : HttpResponse.json(page);
      }),
    );
    openAdmin('private-content');
    expect(await screen.findByText(/Access is logged/)).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText('Channel ID'), '55');
    await userEvent.type(screen.getByLabelText(/Reason/), 'Report 1234');
    await userEvent.click(screen.getByRole('button', { name: 'Read messages' }));

    const dialog = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    await userEvent.type(within(dialog).getByLabelText(/authenticator app/), '123456');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));

    expect(await screen.findByText('this was removed')).toBeInTheDocument();
    expect(proof).toEqual({ method: 'totp', code: '123456' });
    expect(reads).toBe(2);
    expect(reason).toBe('Report 1234');
    expect(screen.getByText('Deleted by the author')).toBeInTheDocument();
    expect(screen.getByText('Edit history (1)')).toBeInTheDocument();
    expect(screen.getByText('typo text')).toBeInTheDocument();
    expect(screen.getByText(/older than 30 days are gone for good/)).toBeInTheDocument();
  });

  it('gives up quietly when the re-auth dialog is cancelled', async () => {
    asStaff({ perms: ['READ_PRIVATE_CONTENT'] });
    server.use(
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900, required: true }),
      ),
      http.get('*/api/v1/admin/private/channels/55/messages', reauthRequired),
    );
    openAdmin('private-content');
    await userEvent.type(await screen.findByLabelText('Channel ID'), '55');
    await userEvent.type(screen.getByLabelText(/Reason/), 'why');
    await userEvent.click(screen.getByRole('button', { name: 'Read messages' }));
    const dialog = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByText('this was removed')).not.toBeInTheDocument());
    expect(await screen.findByRole('alert')).toBeInTheDocument();
  });

  it('tells staff without a second factor to enrol instead of showing a code box', async () => {
    asStaff({ perms: ['READ_PRIVATE_CONTENT'] });
    server.use(
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: false, window_secs: 900, required: true }),
      ),
      http.get('*/api/v1/admin/private/channels/55/messages', reauthRequired),
    );
    openAdmin('private-content');
    await userEvent.type(await screen.findByLabelText('Channel ID'), '55');
    await userEvent.type(screen.getByLabelText(/Reason/), 'why');
    await userEvent.click(screen.getByRole('button', { name: 'Read messages' }));
    const dialog = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    expect(await within(dialog).findByText(/Set up two-factor/)).toBeInTheDocument();
    expect(within(dialog).queryByRole('button', { name: 'Confirm' })).not.toBeInTheDocument();
  });

  it('lists the direct messages of an account and opens one', async () => {
    asStaff({ perms: ['READ_PRIVATE_CONTENT'] });
    server.use(
      http.get('*/api/v1/admin/private/accounts/9/dms', () => HttpResponse.json([page.channel])),
      http.get('*/api/v1/admin/private/channels/55/messages', () => HttpResponse.json(page)),
    );
    openAdmin('private-content');
    await userEvent.click(await screen.findByRole('radio', { name: 'Account DMs' }));
    await userEvent.type(screen.getByLabelText('Account ID'), '9');
    await userEvent.type(screen.getByLabelText(/Reason/), 'why');
    await userEvent.click(screen.getByRole('button', { name: 'Find channels' }));
    await userEvent.click(await screen.findByRole('button', { name: 'Read messages' }));
    expect(await screen.findByText('fixed text')).toBeInTheDocument();
  });
});

describe('re-auth on other admin actions', () => {
  it('retries a refused action after the dialog (set staff roles)', async () => {
    asStaff({ perms: ['APPOINT_STAFF'], owner: true });
    let tries = 0;
    server.use(
      http.get('*/api/v1/admin/staff', () =>
        HttpResponse.json([
          {
            account_id: '8',
            username: 'sam',
            display_name: 'Sam',
            is_owner: false,
            roles: [],
            appointed_at_ms: 1,
            platform_permissions: '0',
          },
        ]),
      ),
      http.get('*/api/v1/admin/staff/roles', () => HttpResponse.json([])),
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900, required: true }),
      ),
      http.post('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900, required: true }),
      ),
      http.delete('*/api/v1/admin/staff/8', () => {
        tries += 1;
        return tries === 1 ? reauthRequired() : new HttpResponse(null, { status: 204 });
      }),
    );
    openAdmin('staff');
    await userEvent.click(
      await screen.findByRole('button', { name: 'Remove sam from staff' }, { timeout: 4000 }),
    );
    const confirm = await screen.findByRole('dialog', { name: /Remove @sam from staff/ });
    await userEvent.click(within(confirm).getByRole('button', { name: 'Remove from staff' }));
    const dialog = await screen.findByRole(
      'dialog',
      { name: 'Confirm it is you' },
      { timeout: 3000 },
    );
    await userEvent.type(within(dialog).getByLabelText(/authenticator app/), '654321');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(tries).toBe(2));
  });
});

describe('revoked appointment (ADM-STF-005)', () => {
  it('closes /admin when the server pushes SR_STAFF_ALERT with revoked', async () => {
    asStaff({ perms: ['STAFF_BASIC'] });
    const { router } = openAdmin('dashboard');
    await screen.findByRole('navigation');
    usePlatformStaff.setState({ staff: true, loaded: true });
    act(() => emitDispatch('SR_STAFF_ALERT', { kind: 'disk_low', detail: 'x' }));
    expect(router.state.location.pathname).toMatch(/^\/admin/);
    act(() => emitDispatch('SR_STAFF_ALERT', { kind: 'staff_revoked', revoked: true }));
    await waitFor(() => expect(router.state.location.pathname).not.toMatch(/^\/admin/));
    expect(usePlatformStaff.getState().staff).toBe(false);
  });
});
