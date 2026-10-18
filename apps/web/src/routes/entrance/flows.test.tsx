import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { warmLoungeRoute } from '../../test/warmRoutes';

warmLoungeRoute();

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('invite check (landing)', () => {
  it('disables the button until 10 characters, formats input, then goes to /register on valid', async () => {
    server.use(
      http.get('*/api/v1/sr/invites/ABCDE-FGHJK', () => HttpResponse.json({ valid: true })),
    );
    const user = userEvent.setup();
    const router = at('/');
    const input = await screen.findByLabelText('Your invitation');
    const button = screen.getByRole('button', { name: /check invitation/i });
    expect(button).toBeDisabled();
    await user.type(input, 'abcdefghjk');
    expect(input).toHaveValue('ABCDE-FGHJK');
    expect(button).toBeEnabled();
    await user.keyboard('{Enter}');
    expect(await screen.findByText("You're on the list.")).toBeInTheDocument();
    expect(sessionStorage.getItem('sr.invite')).toBe('ABCDE-FGHJK');
    await waitFor(() => expect(router.state.location.pathname).toBe('/register'), {
      timeout: 2000,
    });
    expect(router.state.location.search).toEqual({});
  });

  it.each([
    [{ valid: false }, 200, "That invitation doesn't open this door."],
    [{ valid: false, reason: 'closed' }, 200, "The door's closed for now. We're at capacity."],
    [
      { message: 'slow down', retry_after: 30 },
      429,
      'Too many tries. Take a breath and try again later.',
    ],
  ])('shows the right message for %j', async (body, status, text) => {
    server.use(http.get('*/api/v1/sr/invites/:code', () => HttpResponse.json(body, { status })));
    const user = userEvent.setup();
    at('/');
    await user.type(await screen.findByLabelText('Your invitation'), 'ABCDEFGHJK');
    await user.click(screen.getByRole('button', { name: /check invitation/i }));
    expect(await screen.findByText(text)).toBeInTheDocument();
    expect(sessionStorage.getItem('sr.invite')).toBeNull();
  });

  it('/join/:code prefills and auto-checks', async () => {
    server.use(
      http.get('*/api/v1/sr/invites/ABCDE-FGHJK', () => HttpResponse.json({ valid: false })),
    );
    at('/join/abcde-fghjk');
    expect(await screen.findByText("That invitation doesn't open this door.")).toBeInTheDocument();
    expect(screen.getByLabelText('Your invitation')).toHaveValue('ABCDE-FGHJK');
  });
});

describe('login', () => {
  const fill = async (user: ReturnType<typeof userEvent.setup>) => {
    await user.type(await screen.findByLabelText('Username or email'), 'sam');
    await user.type(screen.getByLabelText('Password'), 'hunter2hunter2');
    await user.click(screen.getByRole('button', { name: /step inside/i }));
  };

  it('validates empty fields client-side without calling the API', async () => {
    const user = userEvent.setup();
    at('/login');
    await user.click(await screen.findByRole('button', { name: /step inside/i }));
    expect(screen.getByText('Enter your username or email.')).toBeInTheDocument();
    expect(screen.getByText('Enter your password.')).toBeInTheDocument();
  });

  it('logs in and heads to ?next', async () => {
    server.use(http.post('*/api/v1/sr/auth/login', () => HttpResponse.json({ status: 'ok' })));
    const user = userEvent.setup();
    const router = at('/login?next=/invite/abc');
    await fill(user);
    await waitFor(() => expect(router.state.location.pathname).toBe('/invite/abc'));
  });

  it('ignores an off-site next', async () => {
    server.use(http.post('*/api/v1/sr/auth/login', () => HttpResponse.json({ status: 'ok' })));
    const user = userEvent.setup();
    const router = at('/login?next=//evil.example');
    await fill(user);
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
  });

  it('shows the generic error on 401', async () => {
    server.use(http.post('*/api/v1/sr/auth/login', () => HttpResponse.json({}, { status: 401 })));
    const user = userEvent.setup();
    at('/login');
    await fill(user);
    expect(await screen.findByRole('alert')).toHaveTextContent("That doesn't match our list.");
    expect(screen.getByRole('button', { name: /step inside/i })).toBeEnabled();
  });

  it('shows the lockout message on 429', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ retry_after: 600 }, { status: 429 }),
      ),
    );
    const user = userEvent.setup();
    at('/login');
    await fill(user);
    expect(await screen.findByRole('alert')).toHaveTextContent(
      'Too many attempts. Try again in 10 minutes.',
    );
  });

  it('moves to the 2FA step when required', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ status: 'mfa_required', methods: ['totp', 'recovery'] }),
      ),
    );
    const user = userEvent.setup();
    const router = at('/login');
    await fill(user);
    await waitFor(() => expect(router.state.location.pathname).toBe('/login/2fa'));
  });

  it('shows the Discord-unlinked message and has no register button', async () => {
    at('/login?error=discord_unlinked');
    expect(await screen.findByRole('alert')).toHaveTextContent('No SIDE ROOM account is linked');
    expect(screen.queryByRole('link', { name: /register|sign up/i })).toBeNull();
  });

  it('redirects logged-in members to their last section', async () => {
    server.use(http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1' })));
    const router = at('/login');
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
  });
});

describe('2FA', () => {
  it('auto-submits the sixth digit (with trust device) and finishes', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/2fa', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const user = userEvent.setup();
    const router = at('/login/2fa');
    await user.click(await screen.findByLabelText('Trust this device for 30 days'));
    await user.click(screen.getByLabelText('Digit 1'));
    await user.keyboard('123456');
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
    expect(body).toEqual({ method: 'totp', code: '123456', trust_device: true });
  });

  it('shows an error and clears the code on a wrong TOTP', async () => {
    server.use(http.post('*/api/v1/sr/auth/2fa', () => HttpResponse.json({}, { status: 401 })));
    const user = userEvent.setup();
    at('/login/2fa');
    await user.click(await screen.findByLabelText('Digit 1'));
    await user.keyboard('000000');
    expect(await screen.findByRole('alert')).toHaveTextContent("That code didn't match");
    expect(screen.getByLabelText('Digit 1')).toHaveValue('');
  });

  it('accepts a pasted code', async () => {
    server.use(http.post('*/api/v1/sr/auth/2fa', () => HttpResponse.json({ status: 'ok' })));
    const user = userEvent.setup();
    const router = at('/login/2fa');
    await user.click(await screen.findByLabelText('Digit 1'));
    await user.paste('654321');
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
  });

  it('switches to a recovery code', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/2fa', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ status: 'ok' });
      }),
    );
    const user = userEvent.setup();
    const router = at('/login/2fa');
    await user.click(await screen.findByRole('button', { name: 'Use a recovery code' }));
    await user.type(screen.getByLabelText('Recovery code'), 'abcd-1234');
    await user.click(screen.getByRole('button', { name: /step inside/i }));
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
    expect(body).toEqual({ method: 'recovery', code: 'abcd-1234', trust_device: false });
  });
});

describe('register', () => {
  beforeEach(() => sessionStorage.setItem('sr.invite', 'ABCDE-FGHJK'));

  const fillValid = async (user: ReturnType<typeof userEvent.setup>) => {
    await user.type(await screen.findByLabelText('Email'), 'sam@example.com');
    await user.type(screen.getByLabelText('Username'), 'sam');
    await user.type(screen.getByLabelText('Password'), 'correct Horse 9 battery!');
    await user.type(screen.getByLabelText('Confirm password'), 'correct Horse 9 battery!');
    await user.click(screen.getByRole('checkbox'));
  };

  it('validates before submitting', async () => {
    const user = userEvent.setup();
    at('/register');
    await user.type(await screen.findByLabelText('Email'), 'nope');
    await user.type(screen.getByLabelText('Password'), 'short');
    await user.click(screen.getByRole('button', { name: /open the door/i }));
    expect(screen.getByText('Enter a valid email address.')).toBeInTheDocument();
    expect(screen.getByText('Use at least 10 characters.')).toBeInTheDocument();
    expect(screen.getByText("Those passwords don't match.")).toBeInTheDocument();
    expect(screen.getByText('Please confirm you have read the house notes.')).toBeInTheDocument();
  });

  it('checks username availability live', async () => {
    server.use(
      http.get('*/api/v1/sr/usernames/check', ({ request }) =>
        HttpResponse.json({
          available: new URL(request.url).searchParams.get('username') !== 'sam',
        }),
      ),
    );
    const user = userEvent.setup();
    at('/register');
    await user.type(await screen.findByLabelText('Username'), 'sam');
    await user.click(screen.getByRole('button', { name: /open the door/i }));
    expect(await screen.findByText('That username is taken.')).toBeInTheDocument();
  });

  it('submits, clears the held code and enters the lounge', async () => {
    let body: Record<string, unknown> = {};
    server.use(
      http.get('*/api/v1/sr/usernames/check', () => HttpResponse.json({ available: true })),
      http.post('*/api/v1/sr/register', async ({ request }) => {
        body = (await request.json()) as Record<string, unknown>;
        return HttpResponse.json({ id: '1' });
      }),
    );
    const user = userEvent.setup();
    const router = at('/register');
    await fillValid(user);
    await user.click(screen.getByRole('button', { name: /open the door/i }));
    await waitFor(() => expect(router.state.location.pathname.startsWith('/channels')).toBe(true));
    expect(body).toMatchObject({ code: 'ABCDE-FGHJK', email: 'sam@example.com', username: 'sam' });
    expect(sessionStorage.getItem('sr.invite')).toBeNull();
  });

  it('maps server field errors inline', async () => {
    server.use(
      http.get('*/api/v1/sr/usernames/check', () => HttpResponse.json({ available: true })),
      http.post('*/api/v1/sr/register', () =>
        HttpResponse.json(
          { errors: { email: { _errors: [{ message: 'Email already in use.' }] } } },
          { status: 422 },
        ),
      ),
    );
    const user = userEvent.setup();
    at('/register');
    await fillValid(user);
    await user.click(screen.getByRole('button', { name: /open the door/i }));
    expect(await screen.findByText('Email already in use.')).toBeInTheDocument();
  });

  it('prefills from a pending Discord link, and unlinks', async () => {
    server.use(
      http.get('*/api/v1/sr/auth/discord/pending', () =>
        HttpResponse.json({ id: '9', username: 'Sam#Cool', email: 'sam@discord.test' }),
      ),
      http.get('*/api/v1/sr/usernames/check', () => HttpResponse.json({ available: true })),
      http.delete(
        '*/api/v1/sr/auth/discord/pending',
        () => new HttpResponse(null, { status: 204 }),
      ),
    );
    const user = userEvent.setup();
    at('/register');
    expect(await screen.findByText(/Linked to Discord as @Sam#Cool/)).toBeInTheDocument();
    expect(screen.getByLabelText('Email')).toHaveValue('sam@discord.test');
    expect(screen.getByLabelText('Username')).toHaveValue('samcool');
    await user.click(screen.getByRole('button', { name: 'Unlink Discord' }));
    expect(screen.getByRole('link', { name: /continue with discord/i })).toBeInTheDocument();
  });
});

describe('password reset', () => {
  it('never reveals whether the email exists', async () => {
    server.use(http.post('*/api/v1/sr/auth/forgot', () => new HttpResponse(null, { status: 204 })));
    const user = userEvent.setup();
    at('/forgot');
    await user.type(await screen.findByLabelText('Email'), 'who@example.com');
    await user.click(screen.getByRole('button', { name: /send the link/i }));
    expect(await screen.findByRole('status')).toHaveTextContent(
      'If that address is on the list, a link is on its way.',
    );
  });

  it('resets with a token', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/reset', async ({ request }) => {
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    at('/reset/tok123');
    await user.type(await screen.findByLabelText('New password'), 'correct Horse 9 battery!');
    await user.type(screen.getByLabelText('Confirm new password'), 'correct Horse 9 battery!');
    await user.click(screen.getByRole('button', { name: /set password/i }));
    expect(await screen.findByRole('status')).toHaveTextContent('Password changed');
    expect(body).toEqual({ token: 'tok123', password: 'correct Horse 9 battery!' });
  });

  it("shows the server's password verdict and keeps the link usable", async () => {
    server.use(
      http.post('*/api/v1/sr/auth/reset', () =>
        HttpResponse.json(
          {
            code: 50035,
            message: 'Invalid Form Body',
            errors: { password: { _errors: [{ code: 'weak', message: 'Too easy to guess.' }] } },
          },
          { status: 400 },
        ),
      ),
    );
    const user = userEvent.setup();
    at('/reset/tok123');
    await user.type(await screen.findByLabelText('New password'), 'correct Horse 9 battery!');
    await user.type(screen.getByLabelText('Confirm new password'), 'correct Horse 9 battery!');
    await user.click(screen.getByRole('button', { name: /set password/i }));
    expect(await screen.findByText('Too easy to guess.')).toBeInTheDocument();
    expect(screen.queryByText(/expired/)).toBeNull();
  });

  it('explains an expired link', async () => {
    server.use(http.post('*/api/v1/sr/auth/reset', () => HttpResponse.json({}, { status: 410 })));
    const user = userEvent.setup();
    at('/reset/old');
    await user.type(await screen.findByLabelText('New password'), 'correct Horse 9 battery!');
    await user.type(screen.getByLabelText('Confirm new password'), 'correct Horse 9 battery!');
    await user.click(screen.getByRole('button', { name: /set password/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent('expired');
  });
});
