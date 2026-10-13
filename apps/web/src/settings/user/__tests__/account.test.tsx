import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { fieldError, openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

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
    // No endpoint exists to edit these, and the page says so.
    expect(screen.getByText(/Changing your username, display name or email/)).toBeInTheDocument();
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

  it('keeps change-password disabled (no endpoint) and offers the emailed reset link', async () => {
    signedIn();
    accountEndpoints();
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/forgot', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    openSettings('account');
    expect(await screen.findByText(/no change-password endpoint/)).toBeInTheDocument();
    expect(screen.getByLabelText('Current password')).toBeDisabled();
    expect(screen.getByLabelText('New password')).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Change password' })).toBeDisabled();
    await userEvent.click(screen.getByRole('button', { name: 'Email me a reset link' }));
    expect(await screen.findByText('Reset link sent')).toBeInTheDocument();
    expect(body).toEqual({ email: 'alice@example.com' });
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
    openSettings('account');
    expect(await screen.findByText('alice#0')).toBeInTheDocument();
  });

  it('shows "no Discord linked" when there is none', async () => {
    signedIn();
    accountEndpoints();
    openSettings('account');
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
    openSettings('account');
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
    openSettings('account');
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
    const router = openSettings('account');
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
