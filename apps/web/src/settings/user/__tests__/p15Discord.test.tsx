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

  it('starts the link by asking the server for the authorize page', async () => {
    endpoints({ enabled: true, linked: false });
    let started = 0;
    server.use(
      http.post('*/api/v1/sr/auth/discord/link', () => {
        started += 1;
        // An unreachable host keeps jsdom from navigating while the call is proven.
        return HttpResponse.json({ url: 'about:blank' });
      }),
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    );
    const user = userEvent.setup();
    openSettings('account');
    expect(await screen.findByText('Not linked')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Link Discord' }));
    await waitFor(() => expect(started).toBe(1));
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
