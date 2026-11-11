import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../../test/server';
import { openSettings, PROFILE, resetPrefs, signedIn } from './helpers';

function endpoints(status: { enabled: boolean; linked: boolean }) {
  server.use(
    http.get('*/api/v1/users/@me/profile', () =>
      HttpResponse.json({ ...PROFILE, connected_accounts: [] }),
    ),
    http.get('*/api/v1/sr/auth/sessions', () => HttpResponse.json([])),
    http.get('*/api/v1/sr/auth/trusted-devices', () => HttpResponse.json([])),
    http.get('*/api/v1/sr/auth/discord', () => HttpResponse.json(status)),
  );
}

beforeEach(() => {
  resetPrefs();
  signedIn();
});

describe('Account > Sign in with Discord (ENT-AUTH-011)', () => {
  it('is hidden when the server has no Discord sign-in and nothing is linked', async () => {
    endpoints({ enabled: false, linked: false });
    openSettings('account');
    await screen.findByText('alice');
    await waitFor(() => expect(screen.queryByText('Sign in with Discord')).toBeNull());
  });

  it('starts the link with the password (D-573), then asks the server for the authorize page', async () => {
    endpoints({ enabled: true, linked: false });
    const bodies: unknown[] = [];
    server.use(
      http.post('*/api/v1/sr/auth/discord/link', async ({ request }) => {
        bodies.push(await request.json());
        // An unreachable host keeps jsdom from navigating while the call is proven.
        return HttpResponse.json({ url: 'about:blank' });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    const user = userEvent.setup();
    openSettings('account');
    expect(await screen.findByText('Not linked')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Link Discord' }));
    // A bare click sends nothing: the password prompt comes first.
    const dialog = await screen.findByRole('dialog', { name: 'Link Discord' });
    expect(bodies).toHaveLength(0);
    await user.type(within(dialog).getByLabelText('Current password'), 'hunter2hunter2');
    await user.click(within(dialog).getByRole('button', { name: 'Continue' }));
    await waitFor(() => expect(bodies).toEqual([{ password: 'hunter2hunter2' }]));
  });

  it('asks for the second factor in the re-auth dialog when the server wants it, then retries', async () => {
    endpoints({ enabled: true, linked: false });
    const bodies: unknown[] = [];
    let proof: unknown = null;
    server.use(
      http.post('*/api/v1/sr/auth/discord/link', async ({ request }) => {
        bodies.push(await request.json());
        return proof
          ? HttpResponse.json({ url: 'about:blank' })
          : HttpResponse.json(
              {
                code: 50035,
                message: 'Invalid Form Body',
                errors: {
                  mfa: {
                    _errors: [
                      {
                        code: 'required',
                        message:
                          'Enter a code from your authenticator app, use a passkey, or enter a recovery code.',
                      },
                    ],
                  },
                },
              },
              { status: 400 },
            );
      }),
      http.get('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Access' }, { status: 403 }),
      ),
      http.post('*/api/v1/admin/reauth', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Access' }, { status: 403 }),
      ),
      http.get('*/api/v1/sr/auth/reauth', () =>
        HttpResponse.json({ fresh: false, enrolled: true, window_secs: 900 }),
      ),
      http.post('*/api/v1/sr/auth/reauth', async ({ request }) => {
        proof = await request.json();
        return HttpResponse.json({ fresh: true, enrolled: true, window_secs: 900 });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    const user = userEvent.setup();
    openSettings('account');
    expect(await screen.findByText('Not linked')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Link Discord' }));
    const dialog = await screen.findByRole('dialog', { name: 'Link Discord' });
    await user.type(within(dialog).getByLabelText('Current password'), 'hunter2hunter2');
    await user.click(within(dialog).getByRole('button', { name: 'Continue' }));
    const reauth = await screen.findByRole('dialog', { name: 'Confirm it is you' });
    expect(
      within(reauth).getByText(/Changing how you sign in needs your second factor/),
    ).toBeVisible();
    await user.type(within(reauth).getByLabelText('Code from your authenticator app'), '123456');
    await user.click(within(reauth).getByRole('button', { name: 'Confirm' }));
    await waitFor(() => expect(bodies).toHaveLength(2));
    expect(proof).toEqual({ method: 'totp', code: '123456' });
    expect(bodies[1]).toEqual({ password: 'hunter2hunter2' });
  });

  it('unlinks with the password', async () => {
    endpoints({ enabled: true, linked: true });
    let body: unknown = null;
    server.use(
      http.post('*/api/v1/sr/auth/discord/unlink', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    const user = userEvent.setup();
    openSettings('account');
    expect(await screen.findByText('Linked')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Unlink' }));
    const dialog = await screen.findByRole('dialog', { name: 'Unlink Discord' });
    await user.type(within(dialog).getByLabelText('Current password'), 'hunter2hunter2');
    await user.click(within(dialog).getByRole('button', { name: 'Unlink' }));
    await waitFor(() => expect(body).toEqual({ password: 'hunter2hunter2' }));
  });
});
