import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  RouterProvider,
} from '@tanstack/react-router';
import { createAppRouter } from '../../router';
import { http, HttpResponse, server } from '../../test/server';
import { clearCsrf } from '../../api/client';
import { VerifyEmailBanner } from '../../components/VerifyEmailBanner';
import { VerifyEmail } from './VerifyEmail';

function at(path: string) {
  const router = createAppRouter(createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
  return router;
}

describe('2FA offers only the server-listed methods (Q83)', () => {
  beforeEach(() => clearCsrf());

  it('starts in recovery mode with no toggle when only recovery is offered', async () => {
    sessionStorage.setItem('sr.mfaMethods', JSON.stringify(['recovery']));
    at('/login/2fa');
    expect(await screen.findByLabelText('Recovery code')).toBeInTheDocument();
    expect(screen.queryByLabelText('Digit 1')).toBeNull();
    expect(screen.queryByRole('button', { name: 'Use an authenticator code' })).toBeNull();
  });

  it('hides the recovery toggle when only TOTP is offered', async () => {
    sessionStorage.setItem('sr.mfaMethods', JSON.stringify(['totp']));
    at('/login/2fa');
    expect(await screen.findByLabelText('Digit 1')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Use a recovery code' })).toBeNull();
  });

  it('login stores the methods from mfa_required and the step reads them', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login', () =>
        HttpResponse.json({ status: 'mfa_required', methods: ['recovery'] }),
      ),
    );
    const user = userEvent.setup();
    const router = at('/login');
    await user.type(await screen.findByLabelText('Username or email'), 'sam');
    await user.type(screen.getByLabelText('Password'), 'hunter2hunter2');
    await user.click(screen.getByRole('button', { name: /step inside/i }));
    await waitFor(() => expect(router.state.location.pathname).toBe('/login/2fa'));
    expect(JSON.parse(sessionStorage.getItem('sr.mfaMethods') ?? '[]')).toEqual(['recovery']);
    expect(await screen.findByLabelText('Recovery code')).toBeInTheDocument();
  });
});

describe('verify email (ENT-UI-020)', () => {
  function renderVerify(token: string) {
    const root = createRootRoute({ component: Outlet });
    const route = createRoute({
      getParentRoute: () => root,
      path: '/verify-email/$token',
      component: VerifyEmail,
    });
    const router = createRouter({
      routeTree: root.addChildren([route]),
      history: createMemoryHistory({ initialEntries: [`/verify-email/${token}`] }),
    });
    render(<RouterProvider router={router} />);
  }

  it('confirms the token once', async () => {
    let calls = 0;
    let body: unknown;
    server.use(
      http.post('*/api/v1/sr/auth/verify-email', async ({ request }) => {
        calls += 1;
        body = await request.json();
        return new HttpResponse(null, { status: 204 });
      }),
    );
    renderVerify('tok9');
    expect(await screen.findByText(/Your email is verified/)).toBeInTheDocument();
    expect(body).toEqual({ token: 'tok9' });
    expect(calls).toBe(1);
  });

  it('explains an expired or used link', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/verify-email', () =>
        HttpResponse.json(
          { errors: { token: { _errors: [{ message: 'nope' }] } } },
          { status: 400 },
        ),
      ),
    );
    renderVerify('old');
    expect(await screen.findByRole('alert')).toHaveTextContent('expired or was already used');
  });
});

describe('verify-email banner', () => {
  beforeEach(() => clearCsrf());

  it('renders nothing for a verified member', () => {
    const { container } = render(<VerifyEmailBanner verified />);
    expect(container).toBeEmptyDOMElement();
  });

  it('resends the link on request', async () => {
    let calls = 0;
    server.use(
      http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'c' })),
      http.post('*/api/v1/sr/auth/verify-email/resend', () => {
        calls += 1;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    render(<VerifyEmailBanner verified={false} />);
    await user.click(screen.getByRole('button', { name: 'Resend link' }));
    expect(await screen.findByText('Check your inbox for the link.')).toBeInTheDocument();
    expect(calls).toBe(1);
  });
});
