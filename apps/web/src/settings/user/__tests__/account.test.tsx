import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { fieldError, ME, openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

const session = (id: string, current = false) => ({
  id,
  device_name: `Device ${id}`,
  created_at: '2026-01-01T00:00:00Z',
  last_seen_at: '2026-01-02T00:00:00Z',
  expires_at: '2026-02-01T00:00:00Z',
  current,
});

function accountEndpoints(
  extra: { sessions?: unknown[]; trusted?: unknown[]; connections?: unknown[] } = {},
) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () =>
      HttpResponse.json({ ...PROFILE, connected_accounts: extra.connections ?? [] }),
    ),
    http.get('*/api/v1/sr/auth/sessions', () =>
      HttpResponse.json(extra.sessions ?? [session('10', true), session('11')]),
    ),
    http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json(extra.trusted ?? [])),
  );
}

beforeEach(resetPrefs);

describe('My Account', () => {
  it('shows identity, masks and reveals the email, and shows verification state', async () => {
    signedIn();
    accountEndpoints();
    openSettings('account');
    expect(await screen.findByText('alice')).toBeInTheDocument();
    expect(screen.getByText('Verified')).toBeInTheDocument();
    expect(screen.queryByText('alice@example.com')).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: 'Reveal' }));
    expect(screen.getByText('alice@example.com')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Resend verification/ })).toBeNull();
    expect(screen.getByRole('button', { name: /Edit\s+username/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /Edit\s+display name/ })).toBeInTheDocument();
  });

  it('resends the verification email and reports a cooldown', async () => {
    signedIn({ verified: false });
    accountEndpoints();
    let calls = 0;
    server.use(
      http.post('*/api/v1/sr/auth/verify-email/resend', () => {
        calls += 1;
        return calls === 1
          ? new HttpResponse(null, { status: 204 })
          : HttpResponse.json({ message: 'slow down' }, { status: 429 });
      }),
    );
    openSettings('account');
    expect(await screen.findByText('Not verified')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Resend verification email' }));
    expect(await screen.findByText('Check your inbox for the link.')).toBeInTheDocument();
    expect(calls).toBe(1);
  });

  it('shows the cooldown message when the server rate limits the resend', async () => {
    signedIn({ verified: false });
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/verify-email/resend', () =>
        HttpResponse.json({ message: 'slow' }, { status: 429, headers: { 'retry-after': '0' } }),
      ),
    );
    openSettings('account');
    await userEvent.click(await screen.findByRole('button', { name: 'Resend verification email' }));
    expect(await screen.findByText(/sent a moment ago/)).toBeInTheDocument();
  });

  it('changes the password after checking the current one, and offers the emailed reset link', async () => {
    signedIn();
    accountEndpoints();
    let changed: unknown;
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/password', async ({ request }) => {
        changed = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
      http.post('*/api/v1/sr/auth/forgot', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.type(await screen.findByLabelText('Current password'), 'old-secret-123');
    await user.type(screen.getByLabelText('New password'), 'a-much-longer-passphrase-9');
    await user.click(screen.getByRole('switch', { name: /Sign out my other devices/ }));
    await user.click(screen.getByRole('button', { name: 'Change password' }));
    expect(await screen.findByText('Password changed')).toBeInTheDocument();
    expect(changed).toEqual({
      current_password: 'old-secret-123',
      new_password: 'a-much-longer-passphrase-9',
      revoke_other_sessions: false,
    });
    expect(screen.getByLabelText('Current password')).toHaveValue('');
    await user.click(screen.getByRole('button', { name: 'Email me a reset link' }));
    expect(await screen.findByText('Reset link sent')).toBeInTheDocument();
    expect(body).toEqual({ email: 'alice@example.com' });
  });

  it('shows a wrong current password on its field and a weak new password on the other', async () => {
    signedIn();
    accountEndpoints();
    let n = 0;
    server.use(
      http.post('*/api/v1/sr/auth/password', () =>
        n++ === 0
          ? HttpResponse.json(fieldError('current_password', 'That is not your password.'), {
              status: 400,
            })
          : HttpResponse.json(fieldError('new_password', 'That password is too easy to guess.'), {
              status: 400,
            }),
      ),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.type(await screen.findByLabelText('Current password'), 'nope');
    await user.type(screen.getByLabelText('New password'), 'password1234');
    await user.click(screen.getByRole('button', { name: 'Change password' }));
    expect(await screen.findByText('That is not your password.')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Change password' }));
    expect(await screen.findByText('That password is too easy to guess.')).toBeInTheDocument();
  });

  it('asks for both passwords before sending anything', async () => {
    signedIn();
    accountEndpoints();
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Change password' }));
    expect(await screen.findByText('Enter your current password.')).toBeInTheDocument();
    await user.type(screen.getByLabelText('Current password'), 'abc');
    await user.click(screen.getByRole('button', { name: 'Change password' }));
    expect(await screen.findByText('Enter a new password.')).toBeInTheDocument();
  });

  it('changes the username with the password and refreshes the account', async () => {
    let name = 'alice';
    accountEndpoints();
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'c' })),
      http.get('*/api/v1/users/@me', () => HttpResponse.json({ ...ME, username: name })),
    );
    let sent: unknown;
    server.use(
      http.put('*/api/v1/sr/auth/username', async ({ request }) => {
        sent = await request.json();
        name = 'alice_2';
        return HttpResponse.json({ username: 'alice_2' });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: /Edit\s+username/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Change your username' });
    expect(await axeViolations(dialog)).toEqual([]);
    const field = within(dialog).getByLabelText('Username');
    await user.clear(field);
    await user.type(field, 'Alice_2');
    await user.type(within(dialog).getByLabelText('Current password'), 'hunter2hunter2');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(sent).toEqual({ username: 'alice_2', password: 'hunter2hunter2' }));
    expect(await screen.findByText('alice_2')).toBeInTheDocument();
  });

  it('refuses a bad username before asking the server, and shows the server refusal', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.put('*/api/v1/sr/auth/username', () =>
        HttpResponse.json(fieldError('username', 'You can change your username twice a day.'), {
          status: 429,
        }),
      ),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: /Edit\s+username/ }));
    const dialog = await screen.findByRole('dialog', { name: 'Change your username' });
    const field = within(dialog).getByLabelText('Username');
    await user.clear(field);
    await user.type(field, 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(await within(dialog).findByText(/2 to 32/)).toBeInTheDocument();
    await user.clear(field);
    await user.type(field, 'fresh_name');
    await user.type(within(dialog).getByLabelText('Current password'), 'hunter2hunter2');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(
      await within(dialog).findByText('You can change your username twice a day.'),
    ).toBeInTheDocument();
  });

  it('changes the display name, and an empty name goes back to the username', async () => {
    signedIn();
    accountEndpoints();
    const sent: unknown[] = [];
    server.use(
      http.patch('*/api/v1/users/@me', async ({ request }) => {
        sent.push(await request.json());
        return HttpResponse.json({});
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: /Edit\s+display name/ }));
    let dialog = await screen.findByRole('dialog', { name: 'Change your display name' });
    const field = within(dialog).getByLabelText('Display name');
    await user.clear(field);
    await user.type(field, 'Ali');
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(sent).toEqual([{ global_name: 'Ali' }]));
    await user.click(await screen.findByRole('button', { name: /Edit\s+display name/ }));
    dialog = await screen.findByRole('dialog', { name: 'Change your display name' });
    await user.clear(within(dialog).getByLabelText('Display name'));
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    await waitFor(() => expect(sent).toEqual([{ global_name: 'Ali' }, { global_name: null }]));
  });

  it('reports a failure to send the reset link', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/forgot', () =>
        HttpResponse.json({ message: 'Mail is down.' }, { status: 500 }),
      ),
    );
    openSettings('account');
    await userEvent.click(await screen.findByRole('button', { name: 'Email me a reset link' }));
    expect(await screen.findByText('Mail is down.')).toBeInTheDocument();
  });

  it('enrolls TOTP: QR and key, a wrong code is rejected, then recovery codes appear once', async () => {
    signedIn();
    accountEndpoints();
    const codes = Array.from({ length: 10 }, (_, i) => `AAAA-000${i}`);
    let attempts = 0;
    server.use(
      http.post('*/api/v1/sr/auth/totp/enroll', () =>
        HttpResponse.json({
          otpauth_uri:
            'otpauth://totp/SIDE%20ROOM:alice?secret=JBSWY3DPEHPK3PXP&issuer=SIDE%20ROOM',
          manual_entry_key: 'JBSW Y3DP EHPK 3PXP',
        }),
      ),
      http.post('*/api/v1/sr/auth/totp/verify', async ({ request }) => {
        attempts += 1;
        const { code } = (await request.json()) as { code: string };
        return code === '123456'
          ? HttpResponse.json({ recovery_codes: codes })
          : HttpResponse.json(fieldError('code', "That code didn't match. Try the next one."), {
              status: 400,
            });
      }),
    );
    const createUrl = vi.fn(() => 'blob:codes');
    Object.assign(URL, { createObjectURL: createUrl, revokeObjectURL: vi.fn() });
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {});

    const user = userEvent.setup();
    const clip = vi.spyOn(navigator.clipboard, 'writeText');
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Enable authenticator app' }));
    const dialog = await screen.findByRole('dialog', { name: 'Enable authenticator app' });
    expect(await within(dialog).findByRole('img', { name: /QR code/ })).toBeInTheDocument();
    expect(within(dialog).getByLabelText('Manual entry key')).toHaveTextContent(
      'JBSW Y3DP EHPK 3PXP',
    );

    // Client validation first, no request.
    await user.type(within(dialog).getByLabelText('Authentication code'), '12');
    await user.click(within(dialog).getByRole('button', { name: 'Verify and enable' }));
    expect(
      await within(dialog).findByText('Enter the 6-digit code from your app.'),
    ).toBeInTheDocument();
    expect(attempts).toBe(0);

    await user.clear(within(dialog).getByLabelText('Authentication code'));
    await user.type(within(dialog).getByLabelText('Authentication code'), '000000');
    await user.click(within(dialog).getByRole('button', { name: 'Verify and enable' }));
    expect(await within(dialog).findByText(/code didn't match/)).toBeInTheDocument();

    await user.clear(within(dialog).getByLabelText('Authentication code'));
    await user.type(within(dialog).getByLabelText('Authentication code'), '123456');
    await user.click(within(dialog).getByRole('button', { name: 'Verify and enable' }));
    const list = await screen.findByRole('list', { name: 'Recovery codes' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(10);

    await user.click(screen.getByRole('button', { name: 'Copy codes' }));
    expect(clip).toHaveBeenCalledWith(codes.join('\n'));
    await user.click(screen.getByRole('button', { name: 'Download codes' }));
    expect(createUrl).toHaveBeenCalled();
    expect(click).toHaveBeenCalled();
    click.mockRestore();

    await user.click(screen.getByRole('button', { name: 'I have saved them' }));
    await waitFor(() => expect(screen.queryByRole('dialog', { name: /recovery/i })).toBeNull());
  });

  it('shows a server error when TOTP setup cannot start', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/totp/enroll', () =>
        HttpResponse.json(
          { message: 'Two-factor authentication is already enabled.' },
          { status: 400 },
        ),
      ),
    );
    openSettings('account');
    await userEvent.click(await screen.findByRole('button', { name: 'Enable authenticator app' }));
    expect(
      await screen.findByText('Two-factor authentication is already enabled.'),
    ).toBeInTheDocument();
  });

  it('regenerates recovery codes after a password check, and shows a wrong-password error', async () => {
    signedIn({ mfa_enabled: true });
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/recovery-codes', async ({ request }) => {
        const { password } = (await request.json()) as { password: string };
        return password === 'right-password'
          ? HttpResponse.json({ recovery_codes: ['NEW-0001', 'NEW-0002'] })
          : HttpResponse.json({ message: "That password doesn't match." }, { status: 403 });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Regenerate recovery codes' }));
    const dialog = await screen.findByRole('dialog', { name: 'Regenerate recovery codes' });
    await user.click(within(dialog).getByRole('button', { name: 'Regenerate' }));
    expect(await within(dialog).findByText('Enter your password.')).toBeInTheDocument();
    await user.type(within(dialog).getByLabelText('Current password'), 'nope');
    await user.click(within(dialog).getByRole('button', { name: 'Regenerate' }));
    expect(await within(dialog).findByText("That password doesn't match.")).toBeInTheDocument();
    await user.clear(within(dialog).getByLabelText('Current password'));
    await user.type(within(dialog).getByLabelText('Current password'), 'right-password');
    await user.click(within(dialog).getByRole('button', { name: 'Regenerate' }));
    expect(await screen.findByText('NEW-0001')).toBeInTheDocument();
  });

  it('removes TOTP with the password (DELETE with a body)', async () => {
    signedIn({ mfa_enabled: true });
    accountEndpoints();
    let body: unknown;
    server.use(
      http.delete('*/api/v1/sr/auth/totp', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    openSettings('account');
    await user.click(await screen.findByRole('button', { name: 'Remove authenticator' }));
    const dialog = await screen.findByRole('dialog', { name: 'Remove authenticator' });
    await user.type(within(dialog).getByLabelText('Current password'), 'pw');
    await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(body).toEqual({ password: 'pw' }));
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Remove authenticator' })).toBeNull(),
    );
  });

  it('shows linked Discord status from the profile connections', async () => {
    signedIn();
    accountEndpoints({
      connections: [{ type: 'discord', id: '9', name: 'alice#0', verified: true }],
    });
    openSettings('connections');
    expect(await screen.findByText('alice#0')).toBeInTheDocument();
  });

  it('shows "no Discord linked" when there is none', async () => {
    signedIn();
    accountEndpoints();
    openSettings('connections');
    expect(await screen.findByText('No Discord account is linked.')).toBeInTheDocument();
  });

  it('lists trusted devices and revokes one; a server error is shown', async () => {
    signedIn();
    accountEndpoints({
      trusted: [
        {
          id: '5',
          device_name: 'Laptop',
          created_at: '2026-01-01T00:00:00Z',
          expires_at: '2026-03-01T00:00:00Z',
        },
        {
          id: '6',
          device_name: 'Phone',
          created_at: '2026-01-01T00:00:00Z',
          expires_at: '2026-03-01T00:00:00Z',
        },
      ],
    });
    server.use(
      http.delete(
        '*/api/v1/sr/auth/trusted-devices/5',
        () => new HttpResponse(null, { status: 204 }),
      ),
      http.delete('*/api/v1/sr/auth/trusted-devices/6', () =>
        HttpResponse.json({ message: 'Unknown session.' }, { status: 404 }),
      ),
    );
    const user = userEvent.setup();
    openSettings('devices');
    expect((await screen.findAllByText('Laptop')).length).toBeGreaterThan(0);
    await user.click(screen.getByRole('button', { name: /Revoke\s+Laptop/ }));
    await waitFor(() => expect(screen.queryAllByText('Laptop')).toHaveLength(0));
    await user.click(screen.getByRole('button', { name: /Revoke\s+Phone/ }));
    expect(await screen.findByText('Unknown session.')).toBeInTheDocument();
    expect(screen.getAllByText('Phone').length).toBeGreaterThan(0);
  });

  it('lists sessions, marks this device, and revokes another', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.delete('*/api/v1/sr/auth/sessions/11', () => new HttpResponse(null, { status: 204 })),
    );
    openSettings('devices');
    expect((await screen.findAllByText('Device 10')).length).toBeGreaterThan(0);
    expect(screen.getByText('This device')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /Log out\s+Device 10/ })).toBeNull();
    await userEvent.click(screen.getByRole('button', { name: /Log out\s+Device 11/ }));
    await waitFor(() => expect(screen.queryAllByText('Device 11')).toHaveLength(0));
  });

  it('logs out everywhere after confirming, then goes to sign-in', async () => {
    const s = signedIn();
    accountEndpoints();
    let called = false;
    server.use(
      http.post('*/api/v1/sr/auth/logout-all', () => {
        called = true;
        s.endSession();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    const router = openSettings('devices');
    await user.click(await screen.findByRole('button', { name: 'Log out everywhere' }));
    const dialog = await screen.findByRole('dialog', { name: 'Log out everywhere?' });
    expect(called).toBe(false);
    await user.click(within(dialog).getByRole('button', { name: 'Log out everywhere' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/login'));
    expect(called).toBe(true);
  });

  it('shows an error with a retry when the account cannot be loaded', async () => {
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'c' })),
      http.get('*/api/v1/users/@me', () =>
        HttpResponse.json({ message: 'Boom.' }, { status: 500 }),
      ),
    );
    openSettings('account');
    expect(await screen.findByText("Couldn't load your account")).toBeInTheDocument();
    expect(screen.getByText(/Boom\./)).toBeInTheDocument();
  });

  it('sends an expired session to sign-in', async () => {
    server.use(
      http.get('*/api/v1/users/@me', () =>
        HttpResponse.json({ message: 'Unauthorized' }, { status: 401 }),
      ),
    );
    const router = openSettings('account');
    await waitFor(() => expect(router.state.location.pathname).toBe('/login'));
  });
});

describe('Change email (D-167)', () => {
  const mfaError = (code: string) => ({
    code: 50035,
    message: 'Invalid Form Body',
    errors: { mfa: { _errors: [{ code, message: 'Enter a code.' }] } },
  });

  async function fillDetails(button: string) {
    await userEvent.click(await screen.findByRole('button', { name: 'Change email' }));
    const dialog = await screen.findByRole('dialog', { name: 'Change your email' });
    await userEvent.type(within(dialog).getByLabelText('New email'), 'new@example.com');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    await userEvent.click(within(dialog).getByRole('button', { name: button }));
    return dialog;
  }

  it('sends the link straight away when two-factor is off', async () => {
    signedIn();
    accountEndpoints();
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/email/change', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('account');
    await userEvent.click(await screen.findByRole('button', { name: 'Change email' }));
    const dialog = await screen.findByRole('dialog', { name: 'Change your email' });
    expect(await axeViolations(dialog)).toEqual([]);
    await userEvent.type(within(dialog).getByLabelText('New email'), 'new@example.com');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    await userEvent.click(within(dialog).getByRole('button', { name: 'Send link' }));
    expect(await screen.findByText('Check your new inbox')).toBeInTheDocument();
    expect(body).toEqual({ new_email: 'new@example.com', password: 'hunter22' });
  });

  it('asks for a second factor when two-factor is on, and tells TOTP from recovery codes', async () => {
    signedIn({ mfa_enabled: true });
    accountEndpoints();
    const bodies: Record<string, unknown>[] = [];
    server.use(
      http.post('*/api/v1/sr/auth/email/change', async ({ request }) => {
        const b = (await request.json()) as Record<string, unknown>;
        bodies.push(b);
        const code = (b.mfa as { code: string }).code;
        return code === '000000'
          ? HttpResponse.json(mfaError('invalid'), { status: 400 })
          : new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('account');
    await fillDetails('Continue');
    expect(bodies).toEqual([]);
    const step = await screen.findByRole('dialog', { name: 'Confirm it\u2019s you' });
    expect(await axeViolations(step)).toEqual([]);
    const field = within(step).getByLabelText('Authentication or recovery code');
    await userEvent.type(field, '000000');
    await userEvent.click(within(step).getByRole('button', { name: 'Send link' }));
    expect(await within(step).findByText('Enter a code.')).toBeInTheDocument();
    await userEvent.clear(field);
    await userEvent.type(field, 'ABCD-EFGH');
    await userEvent.click(within(step).getByRole('button', { name: 'Send link' }));
    expect(await screen.findByText('Check your new inbox')).toBeInTheDocument();
    expect(bodies[0]).toMatchObject({ mfa: { method: 'totp', code: '000000' } });
    expect(bodies[1]).toMatchObject({ mfa: { method: 'recovery', code: 'ABCD-EFGH' } });
  });

  it('moves to the code step when the server asks for one', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/email/change', () =>
        HttpResponse.json(mfaError('required'), { status: 400 }),
      ),
    );
    openSettings('account');
    await fillDetails('Send link');
    expect(await screen.findByLabelText('Authentication or recovery code')).toBeInTheDocument();
  });

  it('shows a refused address on the first step', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/email/change', () =>
        HttpResponse.json(fieldError('new_email', 'That is already your email.'), { status: 400 }),
      ),
    );
    openSettings('account');
    const dialog = await fillDetails('Send link');
    expect(await within(dialog).findByText('That is already your email.')).toBeInTheDocument();
  });
});

describe('Delete account (LNG-USET-006, D-124, G-82)', () => {
  async function openDialog() {
    await userEvent.click(await screen.findByRole('button', { name: 'Delete my account' }));
    return screen.findByRole('dialog', { name: 'Delete your account' });
  }

  it('states what is retained, wants the typed username and password, and signs out afterwards', async () => {
    const session = signedIn();
    accountEndpoints();
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/account/delete', async ({ request }) => {
        body = await request.json();
        // The server ends the session with the account.
        session.endSession();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const router = openSettings('account');
    const dialog = await openDialog();
    expect(within(dialog).getByText(/your Person record/)).toBeInTheDocument();
    expect(await axeViolations(dialog)).toEqual([]);
    // The button stays off until the username matches.
    const confirm = within(dialog).getByRole('button', { name: 'Delete my account for good' });
    expect(confirm).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText(/Type your username, alice/), 'ALICE');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    expect(confirm).toBeEnabled();
    await userEvent.click(confirm);
    await waitFor(() => expect(body).toEqual({ password: 'hunter22', username: 'ALICE' }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/login'));
  });

  it('asks for a code with two-factor on and tells authenticator codes from recovery codes', async () => {
    signedIn({ mfa_enabled: true });
    accountEndpoints();
    const bodies: unknown[] = [];
    server.use(
      http.post('*/api/v1/sr/auth/account/delete', async ({ request }) => {
        bodies.push(await request.json());
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('account');
    const dialog = await openDialog();
    await userEvent.type(within(dialog).getByLabelText(/Type your username/), 'alice');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Delete my account for good' }),
    );
    expect(
      await within(dialog).findByText(/Enter a code from your authenticator/),
    ).toBeInTheDocument();
    expect(bodies).toEqual([]);
    await userEvent.type(
      within(dialog).getByLabelText('Authenticator or recovery code'),
      '123 456',
    );
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Delete my account for good' }),
    );
    await waitFor(() =>
      expect(bodies).toEqual([
        {
          password: 'hunter22',
          username: 'alice',
          mfa: { method: 'totp', code: '123456' },
        },
      ]),
    );
  });

  it.each([
    ['staff', 'You are on the staff', 'Leave the Staff Server first'],
    ['owner', 'You are the Owner', 'Transfer ownership first'],
    ['guilds', 'You still own 1 server(s)', 'Transfer ownership or delete them first'],
  ])('explains the %s refusal and keeps the account', async (field, serverText, shown) => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/account/delete', () =>
        HttpResponse.json(fieldError(field, `${serverText}. ${shown}.`), { status: 400 }),
      ),
    );
    const router = openSettings('account');
    const dialog = await openDialog();
    await userEvent.type(within(dialog).getByLabelText(/Type your username/), 'alice');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'hunter22');
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Delete my account for good' }),
    );
    const alert = await within(dialog).findByRole('alert');
    expect(alert).toHaveTextContent(new RegExp(shown));
    expect(router.state.location.pathname).toBe('/settings/user/account');
  });

  it('shows a wrong password on the password field', async () => {
    signedIn();
    accountEndpoints();
    server.use(
      http.post('*/api/v1/sr/auth/account/delete', () =>
        HttpResponse.json(fieldError('password', 'That password is not right.'), { status: 400 }),
      ),
    );
    openSettings('account');
    const dialog = await openDialog();
    await userEvent.type(within(dialog).getByLabelText(/Type your username/), 'alice');
    await userEvent.type(within(dialog).getByLabelText('Current password'), 'nope');
    await userEvent.click(
      within(dialog).getByRole('button', { name: 'Delete my account for good' }),
    );
    expect(await within(dialog).findByText('That password is not right.')).toBeInTheDocument();
  });
});
