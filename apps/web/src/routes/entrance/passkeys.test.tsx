import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { REQUEST_OPTIONS, stubWebAuthn, unstubWebAuthn } from '../../test/webauthnFake';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

afterEach(() => {
  unstubWebAuthn();
  sessionStorage.clear();
});

describe('passkey sign-in', () => {
  it('is not offered when the browser has no WebAuthn', async () => {
    at('/login');
    await screen.findByLabelText('Username or email');
    expect(screen.queryByRole('button', { name: 'Use a passkey' })).toBeNull();
  });

  it('runs begin, the browser prompt and finish, then enters', async () => {
    const { get } = stubWebAuthn();
    let finishBody: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/login/begin', async ({ request }) => {
        expect(await request.json()).toEqual({ second_factor: false });
        return HttpResponse.json({ ceremony_id: 'c1', options: REQUEST_OPTIONS });
      }),
      http.post('*/api/v1/sr/auth/passkeys/login/finish', async ({ request }) => {
        finishBody = await request.json();
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const user = userEvent.setup();
    const router = at('/login');
    await user.click(await screen.findByRole('button', { name: 'Use a passkey' }));
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
    expect(get).toHaveBeenCalledOnce();
    expect(finishBody).toMatchObject({
      ceremony_id: 'c1',
      credential: { rawId: 'AQID', response: { userHandle: 'DQ4' } },
    });
  });

  it('says so when the prompt is cancelled', async () => {
    stubWebAuthn({ reject: new DOMException('x', 'NotAllowedError') });
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/login/begin', () =>
        HttpResponse.json({ ceremony_id: 'c1', options: REQUEST_OPTIONS }),
      ),
    );
    const user = userEvent.setup();
    at('/login');
    await user.click(await screen.findByRole('button', { name: 'Use a passkey' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Passkey sign-in was cancelled.');
    expect(screen.getByRole('button', { name: 'Use a passkey' })).toBeEnabled();
  });

  it('shows a generic message when the server refuses the assertion', async () => {
    stubWebAuthn();
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/login/begin', () =>
        HttpResponse.json({ ceremony_id: 'c1', options: REQUEST_OPTIONS }),
      ),
      http.post('*/api/v1/sr/auth/passkeys/login/finish', () =>
        HttpResponse.json({ message: "That doesn't match our list." }, { status: 401 }),
      ),
    );
    const user = userEvent.setup();
    const router = at('/login');
    await user.click(await screen.findByRole('button', { name: 'Use a passkey' }));
    expect(await screen.findByRole('alert')).toHaveTextContent("That passkey didn't work");
    expect(router.state.location.pathname).toBe('/login');
  });
});

describe('passkey as the second step', () => {
  it('leads with the passkey when it is the only authenticator, and finishes through /2fa', async () => {
    sessionStorage.setItem('sr.mfaMethods', JSON.stringify(['passkey', 'recovery']));
    stubWebAuthn();
    let begin: unknown;
    let twoFa: { method: string; code: string; trust_device: boolean } | undefined;
    server.use(
      http.post('*/api/v1/sr/auth/passkeys/login/begin', async ({ request }) => {
        begin = await request.json();
        return HttpResponse.json({ ceremony_id: 'c9', options: REQUEST_OPTIONS });
      }),
      http.post('*/api/v1/sr/auth/2fa', async ({ request }) => {
        twoFa = (await request.json()) as typeof twoFa;
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const user = userEvent.setup();
    const router = at('/login/2fa');
    expect(await screen.findByText('Your passkey.')).toBeInTheDocument();
    expect(screen.queryByLabelText('Digit 1')).toBeNull();
    await user.click(screen.getByLabelText('Trust this device for 30 days'));
    await user.click(screen.getByRole('button', { name: 'Use your passkey' }));
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
    expect(begin).toEqual({ second_factor: true });
    expect(twoFa?.method).toBe('passkey');
    expect(twoFa?.trust_device).toBe(true);
    expect(JSON.parse(twoFa?.code ?? '{}')).toMatchObject({
      ceremony_id: 'c9',
      credential: { rawId: 'AQID' },
    });
  });

  it('offers the passkey next to the authenticator code and can switch to recovery', async () => {
    sessionStorage.setItem('sr.mfaMethods', JSON.stringify(['passkey', 'totp', 'recovery']));
    stubWebAuthn();
    const user = userEvent.setup();
    at('/login/2fa');
    expect(await screen.findByLabelText('Digit 1')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Use a passkey instead' })).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Use a recovery code' }));
    expect(await screen.findByLabelText('Recovery code')).toBeInTheDocument();
  });
});
