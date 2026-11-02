// XC-VIS-003 S3: the link-outcome pages (/login/link, /verify-email, /email-change/cancel) say in their headline
// what happened, so a screen reader does not announce "Signing you in." over an expired-link warning.
import { render, screen } from '@testing-library/react';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { createPublicRouter } from '../../test/publicRouter';
import { http, HttpResponse, server } from '../../test/server';
import { nav } from '../../lib/enterApp';

beforeEach(() => {
  vi.spyOn(nav, 'assign').mockImplementation(() => {});
  vi.spyOn(nav, 'replace').mockImplementation(() => {});
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf-test' })));
});
afterEach(() => vi.restoreAllMocks());

function at(path: string) {
  const router = createPublicRouter(path, createMemoryHistory({ initialEntries: [path] }));
  render(<RouterProvider router={router} />);
}

const h1 = (name: string) => screen.findByRole('heading', { level: 1, name });
const never = () => new Promise<never>(() => {});

describe('login link headline', () => {
  it('reads "Signing you in." while pending', async () => {
    server.use(http.post('*/api/v1/sr/auth/login-link', never));
    at('/login/link?token=abc');
    expect(await h1('Signing you in.')).toBeInTheDocument();
  });

  it('says the link did not work when it has no token', async () => {
    at('/login/link');
    expect(await h1("This sign-in link didn't work")).toBeInTheDocument();
  });

  it('says the link did not work when it has expired', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/login-link', () =>
        HttpResponse.json({ message: 'Invalid' }, { status: 400 }),
      ),
    );
    at('/login/link?token=old');
    expect(await h1("This sign-in link didn't work")).toBeInTheDocument();
  });

  it('keeps the pending headline when the server is unreachable', async () => {
    server.use(http.post('*/api/v1/sr/auth/login-link', () => HttpResponse.error()));
    at('/login/link?token=abc');
    expect(await screen.findByText(/reach/i)).toBeInTheDocument();
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Signing you in.');
  });
});

describe('verify email headline', () => {
  it('reads "Address confirmed" when done', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/verify-email', () => new HttpResponse(null, { status: 204 })),
    );
    at('/verify-email/tok');
    expect(await h1('Address confirmed')).toBeInTheDocument();
  });

  it('reads "This link has expired" for a used or old token', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/verify-email', () =>
        HttpResponse.json({ errors: { token: 'expired' } }, { status: 400 }),
      ),
    );
    at('/verify-email/tok');
    expect(await h1('This link has expired')).toBeInTheDocument();
  });

  it('reads "Confirming your address." while pending', async () => {
    server.use(http.post('*/api/v1/sr/auth/verify-email', never));
    at('/verify-email/tok');
    expect(await h1('Confirming your address.')).toBeInTheDocument();
  });
});

describe('email change cancel headline', () => {
  it('reads "Change cancelled" when done', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/email/cancel', () => new HttpResponse(null, { status: 204 })),
    );
    at('/email-change/cancel?token=tok');
    expect(await h1('Change cancelled')).toBeInTheDocument();
  });

  it('reads "This link has expired" for an invalid token', async () => {
    server.use(
      http.post('*/api/v1/sr/auth/email/cancel', () =>
        HttpResponse.json({ message: 'Invalid' }, { status: 400 }),
      ),
    );
    at('/email-change/cancel?token=tok');
    expect(await h1('This link has expired')).toBeInTheDocument();
  });

  it('reads "This link didn\'t work" without a token', async () => {
    at('/email-change/cancel');
    expect(await h1("This link didn't work")).toBeInTheDocument();
  });
});
